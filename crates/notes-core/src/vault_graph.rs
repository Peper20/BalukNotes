//! Граф хранилища для показа: фильтр и раскладка — **в одном месте** для
//! всех, кто его рисует:
//!
//! - страница графа и главная клиента (`POST /api/graph/layout`);
//! - заметка — `#vault-graph(…)` из `baluk/graph.typ`: библиотека читает
//!   виртуальный файл `/_vault/graph/<фильтр>.json` (его отдаёт поставщик
//!   [`GraphData`] из реестра [`crate::vault_data`]), CeTZ рисует граф в
//!   PDF и HTML, а клиент оживляет его по тем же координатам.
//!
//! Раскладка — простая силовая модель без библиотек: узлы отталкиваются,
//! рёбра — пружины, слабое притяжение к центру; затем узлы с подписями
//! раздвигаются, пока подписи не перестанут наезжать. Детерминированно:
//! одинаковый граф — одинаковая картинка. O(n²) на шаг — для сотен узлов
//! миллисекунды.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::graph::{Edge, Graph, Snapshot, SourceIndex};
use crate::vault::{NoteKind, Vault};
use crate::vault_data::DataProvider;

/// Группа заметок в корне хранилища.
pub const ROOT_GROUP: &str = "в корне";

/// Кегль подписи и её отступ от кружка (в единицах раскладки) — те же, что
/// при отрисовке.
pub const LABEL_SIZE: f64 = 11.0;
pub const LABEL_GAP: f64 = 2.0;
/// Зазор между прямоугольниками узлов после раздвигания.
const PAD: f64 = 4.0;
/// Желаемая длина ребра.
const EDGE: f64 = 70.0;
/// Дальше этого узлы не отталкиваются: иначе несвязанные заметки улетают на
/// края, а связная часть сжимается в точку.
const CUTOFF2: f64 = (4.0 * EDGE) * (4.0 * EDGE);
/// С какого числа узлов пары ищутся по решётке: у маленького графа перебор
/// всех пар быстрее (итог один и тот же, бит в бит).
const GRID_FROM: usize = 500;

/// Группа узла — папка верхнего уровня.
pub fn group_of(id: &str) -> &str {
    id.split_once('/').map_or(ROOT_GROUP, |(g, _)| g)
}

/// Что показать. Пустой `folders` — все папки.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct GraphFilter {
    /// Только эти группы (папки верхнего уровня; корень — [`ROOT_GROUP`]).
    pub folders: Vec<String>,
    /// Скрытые группы.
    pub hidden: Vec<String>,
    /// Только заметки с этим тегом.
    pub tag: Option<String>,
    /// Показывать несуществующие заметки (на них ссылаются, но их нет).
    pub missing: bool,
    /// Показывать заметки без связей.
    pub orphans: bool,
    /// Только соседи этой заметки на `depth` шагов (в обе стороны); она
    /// сама остаётся, даже если её скрыл бы другой фильтр.
    pub around: Option<String>,
    pub depth: u32,
}

impl Default for GraphFilter {
    fn default() -> Self {
        Self { folders: vec![], hidden: vec![], tag: None, missing: true, orphans: true, around: None, depth: 1 }
    }
}

/// Узел на месте.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlacedNode {
    pub id: String,
    /// `None` — заметки нет.
    pub kind: Option<NoteKind>,
    /// Подпись: название заметки.
    pub name: String,
    pub group: String,
    pub x: f64,
    pub y: f64,
    /// Радиус кружка: книга крупнее, связи прибавляют.
    pub r: f64,
    /// Связей в показанном графе.
    pub degree: usize,
}

/// Граф, готовый к рисованию.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GraphLayout {
    pub nodes: Vec<PlacedNode>,
    pub edges: Vec<Edge>,
    /// Все группы хранилища по порядку — для цветов: фильтр не перекрашивает узлы.
    pub groups: Vec<String>,
    /// Границы нарисованного с подписями: `[x0, y0, x1, y1]`.
    pub bounds: [f64; 4],
    /// Заметка в центре («соседи заметки»).
    pub center: Option<String>,
}

/// Заметки не дальше `depth` рёбер от `start` (направление не важно).
pub fn neighbourhood(graph: &Graph, start: &str, depth: u32) -> HashSet<String> {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in &graph.edges {
        adj.entry(&e.from).or_default().push(&e.to);
        adj.entry(&e.to).or_default().push(&e.from);
    }
    let mut seen = HashSet::from([start.to_owned()]);
    let mut layer = vec![start];
    for _ in 0..depth {
        let mut next = Vec::new();
        for id in layer {
            for m in adj.get(id).into_iter().flatten() {
                if seen.insert((*m).to_owned()) {
                    next.push(*m);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        layer = next;
    }
    seen
}

/// Подграф по фильтру. `tags` — теги заметки по пути.
pub fn filter<'t>(graph: &Graph, tags: impl Fn(&str) -> &'t [String], f: &GraphFilter) -> Graph {
    let center = f.around.as_deref();
    let near = center.map(|c| neighbourhood(graph, c, f.depth));
    let keep = |id: &str, missing: bool| {
        let group = group_of(id);
        Some(id) == center
            || (near.as_ref().is_none_or(|n| n.contains(id))
                && (f.folders.is_empty() || f.folders.iter().any(|g| g == group))
                && !f.hidden.iter().any(|g| g == group)
                && (f.missing || !missing)
                && f.tag.as_ref().is_none_or(|t| tags(id).contains(t)))
    };
    let mut nodes: Vec<_> = graph.nodes.iter().filter(|n| keep(&n.id, n.kind.is_none())).cloned().collect();
    let ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let edges: Vec<_> =
        graph.edges.iter().filter(|e| ids.contains(e.from.as_str()) && ids.contains(e.to.as_str())).cloned().collect();
    if !f.orphans {
        let linked: HashSet<&str> = edges.iter().flat_map(|e| [e.from.as_str(), e.to.as_str()]).collect();
        nodes.retain(|n| linked.contains(n.id.as_str()) || Some(n.id.as_str()) == center);
    }
    Graph { nodes, edges }
}

/// Место узла: радиус кружка и ширина подписи под ним.
#[derive(Debug, Clone, Copy)]
pub struct NodeBox {
    pub r: f64,
    pub label: f64,
}

/// Ширина подписи — по числу знаков (без замера шрифтом: раскладка одна везде).
#[allow(clippy::cast_precision_loss, reason = "число знаков подписи — десятки")]
pub fn label_width(text: &str, size: f64) -> f64 {
    text.chars().count() as f64 * size * 0.58
}

/// Прямоугольник узла с подписью: `[левый, верхний, правый, нижний]`.
pub fn box_rect((x, y): (f64, f64), b: NodeBox) -> [f64; 4] {
    let half = b.r.max(b.label / 2.0);
    [x - half, y - b.r, x + half, y + b.r + LABEL_GAP + LABEL_SIZE * 1.2]
}

/// Раскладка: координаты узлов по порядку `n` узлов, рёбра — пары номеров.
///
/// Отталкивание — только в пределах [`CUTOFF2`], поэтому пары ищутся по
/// решётке с клеткой не меньше этого радиуса (соседние клетки), а не все со
/// всеми. Силы на узел суммируются в том же порядке, что и перебором всех
/// пар (партнёры по возрастанию номера, те же выражения): результат совпадает
/// с ним бит в бит — картинка графа от ускорения не меняется.
#[allow(clippy::cast_precision_loss, reason = "узлов — тысячи")]
pub fn layout(n: usize, links: &[(usize, usize)], boxes: Option<&[NodeBox]>) -> Vec<(f64, f64)> {
    let mut pos: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
            (a.cos() * 100.0, a.sin() * 100.0)
        })
        .collect();
    // Клетка чуть больше радиуса: пары на самой границе тоже в соседних клетках.
    let cell = CUTOFF2.sqrt() + 1.0;
    let mut t = 30.0;
    for _ in 0..400 {
        let mut force = vec![(0.0, 0.0); n];
        // Пары (i, j), j > i, по возрастанию j — как при переборе всех.
        let mut pair = |i: usize, j: usize| {
            let (dx, dy) = (pos[i].0 - pos[j].0, pos[i].1 - pos[j].1);
            let d2 = (dx * dx + dy * dy).max(1.0);
            if d2 > CUTOFF2 {
                return;
            }
            let f = EDGE * EDGE / d2 * (1.0 - d2 / CUTOFF2);
            force[i].0 += dx * f;
            force[i].1 += dy * f;
            force[j].0 -= dx * f;
            force[j].1 -= dy * f;
        };
        if n < GRID_FROM {
            for i in 0..n {
                for j in i + 1..n {
                    pair(i, j);
                }
            }
        } else {
            let grid = Grid::new(&pos, cell, cell);
            // Соседи клетки (она и восемь вокруг) — один раз на клетку.
            let mut around: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
            for i in 0..n {
                let c = grid.cell(pos[i]);
                let list = around.entry(c).or_insert_with(|| {
                    let mut v = Vec::new();
                    grid.after(usize::MAX, c, &mut v);
                    v
                });
                for &j in &list[list.partition_point(|&j| j <= i)..] {
                    pair(i, j);
                }
            }
        }
        for &(a, b) in links {
            let (dx, dy) = (pos[a].0 - pos[b].0, pos[a].1 - pos[b].1);
            let d = dx.hypot(dy).max(1.0);
            let f = (d - EDGE) / d / 2.0;
            force[a].0 -= dx * f;
            force[a].1 -= dy * f;
            force[b].0 += dx * f;
            force[b].1 += dy * f;
        }
        for (p, f) in pos.iter_mut().zip(&mut force) {
            f.0 -= p.0 * 0.02;
            f.1 -= p.1 * 0.02;
            let len = f.0.hypot(f.1);
            if len > 0.0 {
                let m = len.min(t) / len;
                p.0 += f.0 * m;
                p.1 += f.1 * m;
            }
        }
        t *= 0.985;
    }
    if let Some(boxes) = boxes {
        separate(&mut pos, boxes);
    }
    pos
}

/// Решётка для поиска соседей: клетка → номера узлов по возрастанию.
struct Grid {
    w: f64,
    h: f64,
    cells: HashMap<(i64, i64), Vec<usize>>,
}

impl Grid {
    fn new(pos: &[(f64, f64)], w: f64, h: f64) -> Self {
        let mut grid = Self { w, h, cells: HashMap::new() };
        for (i, &p) in pos.iter().enumerate() {
            grid.cells.entry(grid.cell(p)).or_default().push(i);
        }
        grid
    }

    #[allow(clippy::cast_possible_truncation, reason = "координаты раскладки — тысячи единиц")]
    fn cell(&self, (x, y): (f64, f64)) -> (i64, i64) {
        ((x / self.w).floor() as i64, (y / self.h).floor() as i64)
    }

    /// Узлы с номером больше `i` (`usize::MAX` — все) в клетке `(cx, cy)` и
    /// восьми соседних, по возрастанию номера.
    fn after(&self, i: usize, (cx, cy): (i64, i64), out: &mut Vec<usize>) {
        out.clear();
        for x in cx - 1..=cx + 1 {
            for y in cy - 1..=cy + 1 {
                if let Some(v) = self.cells.get(&(x, y)) {
                    // Клетки — возрастающие отрезки: слияние, а не сортировка.
                    let from = if i == usize::MAX { 0 } else { v.partition_point(|&j| j <= i) };
                    out.extend_from_slice(&v[from..]);
                }
            }
        }
        out.sort_unstable();
    }

    /// Узел переехал.
    fn moved(&mut self, node: usize, from: (f64, f64), to: (f64, f64)) {
        let (old, new) = (self.cell(from), self.cell(to));
        if old == new {
            return;
        }
        // Порядок номеров в клетке сохраняется.
        if let Some(list) = self.cells.get_mut(&old)
            && let Ok(at) = list.binary_search(&node)
        {
            list.remove(at);
        }
        let list = self.cells.entry(new).or_default();
        let at = list.binary_search(&node).unwrap_or_else(|at| at);
        list.insert(at, node);
    }
}

/// Раздвигает пересекающиеся прямоугольники узлов: каждую пару — поровну по
/// оси с меньшим перекрытием. Нескольких десятков проходов хватает, чтобы
/// подписи перестали наезжать; связи при этом почти не меняют вид.
///
/// Пары — в том же порядке, что перебором всех (`i < j` по возрастанию), и
/// с теми же сдвигами, но кандидаты `j` берутся из решётки по текущим
/// местам: клетка шире двух самых широких прямоугольников, так что
/// пересекающиеся узлы всегда в соседних клетках. Сдвинулся `i` — кандидаты
/// ищутся заново.
fn separate(pos: &mut [(f64, f64)], boxes: &[NodeBox]) {
    let n = pos.len();
    let half = boxes.iter().map(|b| b.r.max(b.label / 2.0)).fold(0.0, f64::max);
    let radius = boxes.iter().map(|b| b.r).fold(0.0, f64::max);
    let width = 2.0 * half + PAD + 1.0;
    let height = 2.0 * radius + LABEL_GAP + LABEL_SIZE * 1.2 + PAD + 1.0;
    let mut grid = Grid::new(pos, width, height);
    let mut near = Vec::new();
    for _ in 0..80 {
        let mut moved = false;
        for i in 0..n {
            grid.after(i, grid.cell(pos[i]), &mut near);
            let mut next = 0;
            while next < near.len() {
                let j = near[next];
                next += 1;
                let [a0, a1, a2, a3] = box_rect(pos[i], boxes[i]);
                let [b0, b1, b2, b3] = box_rect(pos[j], boxes[j]);
                let ox = a2.min(b2) - a0.max(b0) + PAD;
                let oy = a3.min(b3) - a1.max(b1) + PAD;
                if ox <= 0.0 || oy <= 0.0 {
                    continue;
                }
                moved = true;
                let (pi, pj) = (pos[i], pos[j]);
                // Одинаковые центры — первый влево/вверх: детерминированно.
                if ox < oy {
                    let s = if pos[i].0 <= pos[j].0 { -ox / 2.0 } else { ox / 2.0 };
                    pos[i].0 += s;
                    pos[j].0 -= s;
                } else {
                    let s = if pos[i].1 <= pos[j].1 { -oy / 2.0 } else { oy / 2.0 };
                    pos[i].1 += s;
                    pos[j].1 -= s;
                }
                grid.moved(i, pi, pos[i]);
                grid.moved(j, pj, pos[j]);
                // i сдвинулся — соседи другие: дальше — кандидаты больше j.
                grid.after(j, grid.cell(pos[i]), &mut near);
                next = 0;
            }
        }
        if !moved {
            return;
        }
    }
}

/// Разложить граф: радиусы, подписи, координаты, границы. `groups` — все
/// группы хранилища (порядок цветов).
pub fn place(graph: &Graph, groups: Vec<String>, center: Option<String>) -> GraphLayout {
    let index: HashMap<&str, usize> = graph.nodes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();
    let links: Vec<(usize, usize)> =
        graph.edges.iter().filter_map(|e| Some((*index.get(e.from.as_str())?, *index.get(e.to.as_str())?))).collect();
    let mut degree = vec![0usize; graph.nodes.len()];
    for &(a, b) in &links {
        degree[a] += 1;
        degree[b] += 1;
    }
    let boxes: Vec<NodeBox> = graph
        .nodes
        .iter()
        .zip(&degree)
        .map(|(n, &d)| {
            let book = n.kind == Some(NoteKind::Book);
            #[allow(clippy::cast_precision_loss, reason = "не больше 8")]
            let r = if book { 11.0 } else { 5.5 } + d.min(8) as f64 * 0.7;
            NodeBox { r, label: label_width(&n.title, if book { LABEL_SIZE * 1.1 } else { LABEL_SIZE }) }
        })
        .collect();
    let pos = layout(graph.nodes.len(), &links, Some(&boxes));
    let bounds = pos.iter().zip(&boxes).map(|(&p, &b)| box_rect(p, b)).fold(None, |acc: Option<[f64; 4]>, r| {
        Some(acc.map_or(r, |a| [a[0].min(r[0]), a[1].min(r[1]), a[2].max(r[2]), a[3].max(r[3])]))
    });
    let nodes = graph
        .nodes
        .iter()
        .zip(pos)
        .zip(boxes.iter().zip(degree))
        .map(|((n, (x, y)), (b, degree))| PlacedNode {
            id: n.id.clone(),
            kind: n.kind,
            name: n.title.clone(),
            group: group_of(&n.id).to_owned(),
            x,
            y,
            r: b.r,
            degree,
        })
        .collect();
    GraphLayout { nodes, edges: graph.edges.clone(), groups, bounds: bounds.unwrap_or([0.0, 0.0, 1.0, 1.0]), center }
}

/// Префикс графа в данных хранилища: `/_vault/graph/…`.
pub const DATA_PREFIX: &str = "graph";

/// Поставщик `/_vault/graph/<фильтр>.json` — граф по фильтру.
#[derive(Debug)]
pub struct GraphData {
    pub vault: Vault,
    pub index: Arc<SourceIndex>,
    pub layouts: Arc<Layouts>,
}

/// Отпечаток — по самому ответу (по умолчанию): правка заметки, не
/// изменившая граф, заметку с графом не пересобирает.
impl DataProvider for GraphData {
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        data_file(|| self.index.snapshot(&self.vault).map_err(|e| e.to_string()), &self.layouts, path)
    }
}

/// Файл `/_vault/graph/<path>` для заметки: `<фильтр>.json`, где фильтр —
/// JSON [`GraphFilter`] с экранированными `%`, `/`, `\\` (`baluk/graph.typ`).
/// Ответ — [`GraphLayout`] в JSON.
pub fn data_file(
    snapshot: impl FnOnce() -> Result<Snapshot, String>,
    layouts: &Layouts,
    path: &str,
) -> Result<Vec<u8>, String> {
    let query = path
        .strip_suffix(".json")
        .ok_or_else(|| format!("нет данных хранилища «{DATA_PREFIX}/{path}» (граф — {DATA_PREFIX}/<фильтр>.json)"))?;
    let filter: GraphFilter =
        serde_json::from_str(&unescape(query)).map_err(|e| format!("граф: неверный фильтр ({e})"))?;
    let layout = snapshot()?.graph_layout_cached(&filter, layouts);
    serde_json::to_vec(&*layout).map_err(|e| e.to_string())
}

/// Обратное к экранированию в `baluk/graph.typ`: `%25` → `%`, `%2F` → `/`, `%5C` → `\\`.
fn unescape(s: &str) -> String {
    s.replace("%2F", "/").replace("%5C", "\\").replace("%25", "%")
}

/// Сколько раскладок помнить.
const LAYOUTS: usize = 32;

/// Кэш раскладок: тот же показанный граф (узлы, рёбра, группы, центр) —
/// та же раскладка, считать её заново незачем. Ключ — хэш самого графа,
/// поэтому кэш не устаревает: изменилось хранилище — другой ключ.
/// Переключатели страницы `/graph`, версия заметки с `#vault-graph`
/// ([`GraphData`]) и её сборка берут готовую.
#[derive(Debug, Default)]
pub struct Layouts {
    /// Недавние — в начале.
    entries: parking_lot::Mutex<std::collections::VecDeque<(u64, Arc<GraphLayout>)>>,
}

impl Layouts {
    /// Раскладка графа (см. [`place`]) — из кэша или посчитанная.
    pub fn place(&self, graph: &Graph, groups: Vec<String>, center: Option<String>) -> Arc<GraphLayout> {
        let key = {
            let json = serde_json::to_vec(&(graph, &groups, &center)).unwrap_or_default();
            crate::version::StableHasher::new().bytes(&json).finish()
        };
        {
            let mut entries = self.entries.lock();
            if let Some(k) = entries.iter().position(|(h, _)| *h == key) {
                let hit = entries.remove(k).expect("есть");
                entries.push_front(hit.clone());
                return hit.1;
            }
        }
        let layout = Arc::new(place(graph, groups, center));
        let mut entries = self.entries.lock();
        entries.push_front((key, layout.clone()));
        entries.truncate(LAYOUTS);
        layout
    }
}

impl Snapshot {
    /// Граф хранилища по фильтру, разложенный для рисования; раскладка — из
    /// кэша `layouts`, если такой граф уже раскладывали.
    pub fn graph_layout_cached(&self, f: &GraphFilter, layouts: &Layouts) -> Arc<GraphLayout> {
        let (shown, groups) = self.shown(f);
        layouts.place(&shown, groups, f.around.clone())
    }

    /// Граф хранилища по фильтру, разложенный для рисования.
    pub fn graph_layout(&self, f: &GraphFilter) -> GraphLayout {
        let (shown, groups) = self.shown(f);
        place(&shown, groups, f.around.clone())
    }

    /// Показанный граф по фильтру и все группы хранилища.
    fn shown(&self, f: &GraphFilter) -> (Graph, Vec<String>) {
        let full = self.graph();
        let mut groups: Vec<String> = Vec::new();
        for n in &full.nodes {
            let g = group_of(&n.id);
            if !groups.iter().any(|x| x == g) {
                groups.push(g.to_owned());
            }
        }
        let empty: &[String] = &[];
        (filter(&full, |id| self.tags_of(id).unwrap_or(empty), f), groups)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Node;

    fn node(id: &str, kind: Option<NoteKind>) -> Node {
        Node { id: id.into(), kind, title: id.rsplit('/').next().unwrap_or(id).into() }
    }

    fn edge(from: &str, to: &str, count: usize) -> Edge {
        Edge { from: from.into(), to: to.into(), count }
    }

    /// A → B → C, D → B, E — без связей, «Нет» — ссылка на несуществующую заметку.
    fn sample() -> Graph {
        Graph {
            nodes: vec![
                node("Сеть/A", Some(NoteKind::Note)),
                node("Сеть/B", Some(NoteKind::Note)),
                node("Мат/C", Some(NoteKind::Book)),
                node("D", Some(NoteKind::Note)),
                node("E", Some(NoteKind::Note)),
                node("Нет", None),
            ],
            edges: vec![
                edge("Сеть/A", "Сеть/B", 1),
                edge("Сеть/B", "Мат/C", 2),
                edge("D", "Сеть/B", 1),
                edge("D", "Нет", 1),
            ],
        }
    }

    fn tags(id: &str) -> &'static [String] {
        static NET: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| vec!["сеть".into()]);
        static SSH: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| vec!["сеть".into(), "ssh".into()]);
        match id {
            "Сеть/A" => &NET,
            "Сеть/B" => &SSH,
            _ => &[],
        }
    }

    fn ids(g: &Graph) -> Vec<&str> {
        let mut v: Vec<_> = g.nodes.iter().map(|n| n.id.as_str()).collect();
        v.sort_unstable();
        v
    }

    fn sorted(set: HashSet<String>) -> Vec<String> {
        let mut v: Vec<_> = set.into_iter().collect();
        v.sort();
        v
    }

    #[test]
    fn group_is_top_folder() {
        assert_eq!(group_of("Сеть/SSH/Ключи"), "Сеть");
        assert_eq!(group_of("Начало"), ROOT_GROUP);
    }

    #[test]
    fn neighbours_both_directions() {
        let g = sample();
        assert_eq!(sorted(neighbourhood(&g, "Сеть/A", 1)), ["Сеть/A", "Сеть/B"]);
        assert_eq!(sorted(neighbourhood(&g, "Сеть/A", 2)), ["D", "Мат/C", "Сеть/A", "Сеть/B"]);
    }

    #[test]
    fn filters() {
        let g = sample();
        let f = GraphFilter::default();
        assert_eq!(filter(&g, tags, &f).nodes, g.nodes);
        let no_net = filter(&g, tags, &GraphFilter { hidden: vec!["Сеть".into()], ..f.clone() });
        assert_eq!(ids(&no_net), ["D", "E", "Мат/C", "Нет"]);
        assert_eq!(no_net.edges, [edge("D", "Нет", 1)]);
        let only_net = filter(&g, tags, &GraphFilter { folders: vec!["Сеть".into()], ..f.clone() });
        assert_eq!(ids(&only_net), ["Сеть/A", "Сеть/B"]);
        assert_eq!(ids(&filter(&g, tags, &GraphFilter { tag: Some("ssh".into()), ..f.clone() })), ["Сеть/B"]);
        let linked = filter(&g, tags, &GraphFilter { missing: false, orphans: false, ..f });
        assert_eq!(ids(&linked), ["D", "Мат/C", "Сеть/A", "Сеть/B"]);
    }

    #[test]
    fn center_stays() {
        let g = sample();
        let f = GraphFilter {
            around: Some("E".into()),
            orphans: false,
            hidden: vec![ROOT_GROUP.into()],
            ..GraphFilter::default()
        };
        assert_eq!(ids(&filter(&g, tags, &f)), ["E"]);
        let f = GraphFilter { around: Some("D".into()), ..GraphFilter::default() };
        assert_eq!(ids(&filter(&g, tags, &f)), ["D", "Нет", "Сеть/B"]);
    }

    const RING: [(usize, usize); 4] = [(0, 1), (1, 2), (2, 0), (0, 3)];

    #[test]
    fn layout_is_deterministic_and_keeps_orphans_near() {
        let pos = layout(7, &RING, None);
        assert_eq!(pos, layout(7, &RING, None));
        let dist = |p: usize, q: usize| (pos[p].0 - pos[q].0).hypot(pos[p].1 - pos[q].1);
        let cluster = dist(0, 1).max(dist(1, 2)).max(dist(0, 3));
        for lone in 4..7 {
            assert!(dist(lone, 0) < cluster * 8.0, "несвязанные не улетают");
        }
        assert!(dist(0, 1) > 20.0);
    }

    #[test]
    fn labels_do_not_overlap() {
        let names = [
            "Интеграл Эйлера–Пуассона",
            "Ряд Тейлора",
            "Бинарный поиск",
            "Сортировка пузырьком",
            "SSH",
            "Git",
            "Итоги",
        ];
        let star: Vec<_> = (1..names.len()).map(|i| (0, i)).collect();
        let boxes: Vec<_> = names.iter().map(|n| NodeBox { r: 6.0, label: label_width(n, LABEL_SIZE) }).collect();
        let pos = layout(names.len(), &star, Some(&boxes));
        assert_eq!(pos, layout(names.len(), &star, Some(&boxes)));
        for i in 0..names.len() {
            for j in i + 1..names.len() {
                let [a0, a1, a2, a3] = box_rect(pos[i], boxes[i]);
                let [b0, b1, b2, b3] = box_rect(pos[j], boxes[j]);
                let overlap = a2.min(b2) > a0.max(b0) && a3.min(b3) > a1.max(b1);
                assert!(!overlap, "{} × {}", names[i], names[j]);
            }
        }
    }

    #[test]
    fn data_file_path() {
        assert_eq!(unescape("a%2Fb%5Cc%25252F"), "a/b\\c%252F");
        let layouts = Layouts::default();
        let err = data_file(|| Err("не нужен".into()), &layouts, "{}.txt").unwrap_err();
        assert!(err.contains("graph/"), "{err}");
        let err = data_file(|| Err("не нужен".into()), &layouts, "{нет.json").unwrap_err();
        assert!(err.contains("фильтр"), "{err}");
    }

    #[test]
    fn layouts_are_cached_by_graph() {
        let layouts = Layouts::default();
        let g = sample();
        let a = layouts.place(&g, vec![], None);
        assert!(Arc::ptr_eq(&a, &layouts.place(&g, vec![], None)), "тот же граф — из кэша");
        assert_eq!(*a, place(&g, vec![], None));
        let other = layouts.place(&g, vec![], Some("D".into()));
        assert!(!Arc::ptr_eq(&a, &other), "другой центр — другая раскладка");
        let mut g2 = sample();
        g2.edges.pop();
        assert!(!Arc::ptr_eq(&a, &layouts.place(&g2, vec![], None)), "другой граф — заново");
    }

    /// Перебор всех пар — эталон: решётка даёт то же бит в бит.
    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn grid_layout_matches_all_pairs() {
        let n = GRID_FROM + 40;
        let links: Vec<_> = (1..n).map(|i| (i, (i * 7919) % i)).collect();
        let boxes: Vec<_> = (0..n).map(|i| NodeBox { r: 5.5, label: 20.0 + (i % 5) as f64 * 15.0 }).collect();
        let grid = layout(n, &links, Some(&boxes));
        // Эталон: layout без решётки на тех же данных.
        let mut pos: Vec<(f64, f64)> = (0..n)
            .map(|i| {
                let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
                (a.cos() * 100.0, a.sin() * 100.0)
            })
            .collect();
        let mut t = 30.0;
        for _ in 0..400 {
            let mut force = vec![(0.0, 0.0); n];
            for i in 0..n {
                for j in i + 1..n {
                    let (dx, dy) = (pos[i].0 - pos[j].0, pos[i].1 - pos[j].1);
                    let d2 = (dx * dx + dy * dy).max(1.0);
                    if d2 > CUTOFF2 {
                        continue;
                    }
                    let f = EDGE * EDGE / d2 * (1.0 - d2 / CUTOFF2);
                    force[i].0 += dx * f;
                    force[i].1 += dy * f;
                    force[j].0 -= dx * f;
                    force[j].1 -= dy * f;
                }
            }
            for &(a, b) in &links {
                let (dx, dy) = (pos[a].0 - pos[b].0, pos[a].1 - pos[b].1);
                let d = dx.hypot(dy).max(1.0);
                let f = (d - EDGE) / d / 2.0;
                force[a].0 -= dx * f;
                force[a].1 -= dy * f;
                force[b].0 += dx * f;
                force[b].1 += dy * f;
            }
            for (p, f) in pos.iter_mut().zip(&mut force) {
                f.0 -= p.0 * 0.02;
                f.1 -= p.1 * 0.02;
                let len = f.0.hypot(f.1);
                if len > 0.0 {
                    let m = len.min(t) / len;
                    p.0 += f.0 * m;
                    p.1 += f.1 * m;
                }
            }
            t *= 0.985;
        }
        for _ in 0..80 {
            let mut moved = false;
            for i in 0..n {
                for j in i + 1..n {
                    let [a0, a1, a2, a3] = box_rect(pos[i], boxes[i]);
                    let [b0, b1, b2, b3] = box_rect(pos[j], boxes[j]);
                    let ox = a2.min(b2) - a0.max(b0) + PAD;
                    let oy = a3.min(b3) - a1.max(b1) + PAD;
                    if ox <= 0.0 || oy <= 0.0 {
                        continue;
                    }
                    moved = true;
                    if ox < oy {
                        let s = if pos[i].0 <= pos[j].0 { -ox / 2.0 } else { ox / 2.0 };
                        pos[i].0 += s;
                        pos[j].0 -= s;
                    } else {
                        let s = if pos[i].1 <= pos[j].1 { -oy / 2.0 } else { oy / 2.0 };
                        pos[i].1 += s;
                        pos[j].1 -= s;
                    }
                }
            }
            if !moved {
                break;
            }
        }
        let bits = |v: &[(f64, f64)]| v.iter().map(|p| (p.0.to_bits(), p.1.to_bits())).collect::<Vec<_>>();
        assert!(bits(&grid) == bits(&pos), "решётка изменила раскладку");
    }

    #[test]
    fn placed_nodes_and_bounds() {
        let g = sample();
        let l = place(&g, vec!["Сеть".into(), "Мат".into(), ROOT_GROUP.into()], None);
        let c = l.nodes.iter().find(|n| n.id == "Мат/C").unwrap();
        assert_eq!((c.name.as_str(), c.group.as_str(), c.degree), ("C", "Мат", 1));
        assert!(c.r > l.nodes.iter().find(|n| n.id == "E").unwrap().r, "книга крупнее");
        for n in &l.nodes {
            assert!(l.bounds[0] <= n.x && n.x <= l.bounds[2] && l.bounds[1] <= n.y && n.y <= l.bounds[3]);
        }
    }
}
