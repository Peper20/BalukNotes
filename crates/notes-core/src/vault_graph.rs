//! Граф хранилища для показа: фильтр и раскладка — **в одном месте** для
//! всех, кто его рисует:
//!
//! - страница графа и главная клиента (`POST /api/graph/layout`);
//! - заметка — `#vault-graph(…)` из `baluk/graph.typ`: библиотека читает
//!   виртуальный файл `/_vault/graph/<фильтр>.json` (его отдаёт поставщик
//!   [`GraphData`] из реестра [`crate::vault_data`]), CeTZ рисует граф в
//!   PDF и HTML, а клиент оживляет его по тем же координатам.
//!
//! Раскладка — детерминированная силовая модель: отталкивание в радиусе через
//! решётку соседей, рёбра — пружины, слабое притяжение к центру; затем узлы с
//! подписями раздвигаются, пока подписи не перестанут наезжать. Одинаковый
//! граф — одинаковая картинка.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::graph::{ChapterOf, Edge, Graph, Node, Snapshot, SourceIndex};
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

/// Группа узла — папка верхнего уровня.
pub fn group_of(id: &str) -> &str {
    id.split_once('/').map_or(ROOT_GROUP, |(g, _)| g)
}

/// Группа вершины: у главы — группа её книги.
fn node_group(n: &Node) -> &str {
    group_of(n.chapter.as_ref().map_or(&n.id, |c| &c.book))
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
    /// Книги — корнем и главами вокруг него, а не одной вершиной.
    pub chapters: bool,
    /// Силы раскладки (настройки вида на странице графа); граф в заметке -
    /// по умолчанию.
    pub forces: Forces,
}

impl Default for GraphFilter {
    fn default() -> Self {
        Self {
            folders: vec![],
            hidden: vec![],
            tag: None,
            missing: true,
            orphans: true,
            around: None,
            depth: 1,
            chapters: false,
            forces: Forces::default(),
        }
    }
}

/// Силы раскладки в процентах от обычных (по умолчанию - раскладка как без
/// них, бит в бит).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct Forces {
    /// Насколько выражены папки: узлы папки держатся вместе, чужие папки
    /// отталкивают сильнее (между папками просвет); 0 - папки держатся
    /// вместе только начальными местами и связями.
    pub clusters: u32,
    /// Притяжение к центру: меньше - граф просторнее.
    pub center: u32,
    /// Отталкивание узлов.
    pub repel: u32,
    /// Притяжение связанных узлов (жёсткость рёбер).
    pub links: u32,
}

impl Default for Forces {
    fn default() -> Self {
        Self { clusters: 0, center: 100, repel: 100, links: 100 }
    }
}

impl Forces {
    fn scale(percent: u32) -> f64 {
        f64::from(percent) / 100.0
    }
}

/// Узел на месте.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlacedNode {
    pub id: String,
    /// `None` — заметки нет.
    pub kind: Option<NoteKind>,
    /// Глава книги: открывается книга на этой главе.
    pub chapter: Option<ChapterOf>,
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

/// Подграф по фильтру. `has_tag(вершина, тег)` — есть ли тег у заметки (у
/// книги — и у любой её главы; у главы — её или книги).
pub fn filter(graph: &Graph, has_tag: impl Fn(&Node, &str) -> bool, f: &GraphFilter) -> Graph {
    let center = f.around.as_deref();
    let near = center.map(|c| neighbourhood(graph, c, f.depth));
    let keep = |n: &Node| {
        let group = node_group(n);
        Some(n.id.as_str()) == center
            || (near.as_ref().is_none_or(|near| near.contains(&n.id))
                && (f.folders.is_empty() || f.folders.iter().any(|g| g == group))
                && !f.hidden.iter().any(|g| g == group)
                && (f.missing || n.kind.is_some())
                && f.tag.as_ref().is_none_or(|t| has_tag(n, t)))
    };
    let mut nodes: Vec<_> = graph.nodes.iter().filter(|n| keep(n)).cloned().collect();
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
/// Силы: отталкивание всех пар `EDGE²/d` (Барнс - Хат: далёкая клетка
/// квадродерева — одно тело в центре масс), пружины рёбер к длине [`EDGE`],
/// притяжение к центру. Притяжение подобрано так, что на узел приходится
/// около [`AREA`] площади при любом числе узлов: большой граф не сжимается в
/// кашу подписей и не расползается. Начало — [`initial_positions`]. Без
/// случайности и с тем же порядком сумм — одинаковый вход даёт одинаковые
/// координаты бит в бит.
///
/// `groups` - номер группы (папки) узла, для [`Forces::clusters`]; пустой -
/// без групп.
pub fn layout(
    n: usize,
    links: &[(usize, usize)],
    groups: &[usize],
    boxes: Option<&[NodeBox]>,
    forces: Forces,
) -> Vec<(f64, f64)> {
    layout_with(n, links, &[], groups, boxes, forces)
}

/// [`layout`] с добавочными пружинами `tight` (книга - глава): короче
/// ([`TIGHT_EDGE`]) и жёстче ([`TIGHT_SPRING`]) обычных.
fn layout_with(
    n: usize,
    links: &[(usize, usize)],
    tight: &[(usize, usize)],
    groups: &[usize],
    boxes: Option<&[NodeBox]>,
    forces: Forces,
) -> Vec<(f64, f64)> {
    let mut pos = layout_forces(n, links, tight, groups, forces);
    if let Some(boxes) = boxes {
        separate(&mut pos, boxes);
    }
    pos
}

/// Площадь раскладки на узел: подпись в среднем ~130 x 35.
const AREA: f64 = 12_000.0;
/// Притяжение к центру `CENTER_PULL * r`: равновесие с отталкиванием
/// `EDGE²/d` — диск плотности `π EDGE² / CENTER_PULL` на узел.
const CENTER_PULL: f64 = std::f64::consts::PI * EDGE * EDGE / AREA;
/// Барнс - Хат: клетка размера `s` на расстоянии `d` — одно тело, если `s/d`
/// меньше этого.
const THETA: f64 = 0.9;
const ITERATIONS: usize = 60;
/// Наибольший шаг узла: сначала, потом убывает до ~2.
const START_TEMP: f64 = 40.0;
const COOLING: f64 = 0.95;
/// Притяжение к папке при `clusters` = 100 %, в долях [`CENTER_PULL`].
const CLUSTER_PULL: f64 = 4.0 * CENTER_PULL;
/// Добавочное отталкивание чужой папки при `clusters` = 100 %, в долях
/// обычного (папка - одно тело в её середине).
const GROUP_REPEL: f64 = 1.0;

/// Длина и жёсткость пружины «книга - глава» (в долях обычной; сама связь
/// тоже ребро графа — пружины складываются).
const TIGHT_EDGE: f64 = 0.4 * EDGE;
const TIGHT_SPRING: f64 = 10.0;

fn layout_forces(
    n: usize,
    links: &[(usize, usize)],
    tight: &[(usize, usize)],
    groups: &[usize],
    forces: Forces,
) -> Vec<(f64, f64)> {
    let mut pos = initial_positions(n);
    let mut temp = START_TEMP;
    let mut tree = QuadTree::default();
    let mut stack = Vec::new();
    let mut force = vec![(0.0, 0.0); n];
    let repel = Forces::scale(forces.repel);
    let spring = Forces::scale(forces.links) / 2.0;
    let center = CENTER_PULL * Forces::scale(forces.center);
    let cluster = CLUSTER_PULL * Forces::scale(forces.clusters);
    let group_repel = Forces::scale(forces.clusters) * GROUP_REPEL;
    let clustered = cluster > 0.0 && groups.len() == n;
    let mut middles = Vec::new();
    for _ in 0..ITERATIONS {
        tree.build(&pos);
        for (i, f) in force.iter_mut().enumerate() {
            let (x, y) = tree.repulsion(i, pos[i], &mut stack);
            *f = (x * repel, y * repel);
        }
        for &(a, b) in links {
            let (dx, dy) = (pos[a].0 - pos[b].0, pos[a].1 - pos[b].1);
            let d = dx.hypot(dy).max(1.0);
            let f = (d - EDGE) / d * spring;
            force[a].0 -= dx * f;
            force[a].1 -= dy * f;
            force[b].0 += dx * f;
            force[b].1 += dy * f;
        }
        for &(a, b) in tight {
            let (dx, dy) = (pos[a].0 - pos[b].0, pos[a].1 - pos[b].1);
            let d = dx.hypot(dy).max(1.0);
            let f = (d - TIGHT_EDGE) / d * spring * TIGHT_SPRING;
            force[a].0 -= dx * f;
            force[a].1 -= dy * f;
            force[b].0 += dx * f;
            force[b].1 += dy * f;
        }
        if clustered {
            group_middles(&pos, groups, &mut middles);
            // Только из-за края круга площади папки: отставшие подтягиваются,
            // а плотнее, чем [`AREA`] на узел, папка не становится (иначе
            // подписи наезжают).
            // Чужие папки отталкивают сильнее - между папками просвет.
            for ((f, p), &g) in force.iter_mut().zip(&pos).zip(groups) {
                for (h, &(mx, my, radius, mass)) in middles.iter().enumerate() {
                    let (dx, dy) = (mx - p.0, my - p.1);
                    let d = dx.hypot(dy).max(1.0);
                    let k = if h == g {
                        (d - radius).max(0.0) / d * cluster
                    } else {
                        -mass * EDGE * EDGE / (d * d) * group_repel
                    };
                    f.0 += dx * k;
                    f.1 += dy * k;
                }
            }
        }
        for (p, f) in pos.iter_mut().zip(&mut force) {
            f.0 -= p.0 * center;
            f.1 -= p.1 * center;
            let len = f.0.hypot(f.1);
            if len > 0.0 {
                let m = len.min(temp) / len;
                p.0 += f.0 * m;
                p.1 += f.1 * m;
            }
        }
        temp *= COOLING;
    }
    pos
}

/// Середины групп: `middles[g]` - центр масс узлов группы `g`, радиус
/// круга площадью [`AREA`] на её узел и число узлов.
#[allow(clippy::cast_precision_loss, reason = "узлов - тысячи")]
fn group_middles(pos: &[(f64, f64)], groups: &[usize], middles: &mut Vec<(f64, f64, f64, f64)>) {
    let count = groups.iter().max().map_or(0, |g| g + 1);
    let mut sum = vec![(0.0, 0.0, 0usize); count];
    for (p, &g) in pos.iter().zip(groups) {
        sum[g].0 += p.0;
        sum[g].1 += p.1;
        sum[g].2 += 1;
    }
    middles.clear();
    middles.extend(sum.iter().map(|&(x, y, k)| {
        let k = k.max(1) as f64;
        (x / k, y / k, (k * AREA / std::f64::consts::PI).sqrt(), k)
    }));
}

/// Начальные места: узлы по порядку вдоль кривой Гильберта, квадрат площади
/// [`AREA`] на узел. Узлы идут по путям (`Graph` упорядочен по имени), а
/// отрезок кривой — компактное пятно: папки начинают кучками, а не
/// вперемешку, и шагов нужно немного.
#[allow(clippy::cast_precision_loss, reason = "узлов — тысячи")]
fn initial_positions(n: usize) -> Vec<(f64, f64)> {
    let mut side = 1;
    while side * side < n {
        side *= 2;
    }
    let step = (n as f64 * AREA).sqrt() / side as f64;
    let half = side as f64 * step / 2.0;
    (0..n)
        .map(|k| {
            let (x, y) = hilbert(side, k * side * side / n);
            ((x as f64 + 0.5) * step - half, (y as f64 + 0.5) * step - half)
        })
        .collect()
}

/// Точка `d` кривой Гильберта на решётке `side` x `side` (`side` — степень двойки).
fn hilbert(side: usize, mut d: usize) -> (usize, usize) {
    let (mut x, mut y) = (0, 0);
    let mut s = 1;
    while s < side {
        let rx = 1 & (d / 2);
        let ry = 1 & (d ^ rx);
        if ry == 0 {
            if rx == 1 {
                x = s - 1 - x;
                y = s - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        x += s * rx;
        y += s * ry;
        d /= 4;
        s *= 2;
    }
    (x, y)
}

/// Квадродерево для отталкивания (Барнс - Хат). Клетки хранятся плоско;
/// у листа — отрезок `order` с его узлами (больше одного — только узлы в
/// одной точке).
#[derive(Debug, Default)]
struct QuadTree {
    cells: Vec<Cell>,
    order: Vec<usize>,
    pos: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, Copy)]
struct Cell {
    /// Центр масс и масса (число узлов).
    x: f64,
    y: f64,
    mass: f64,
    size: f64,
    /// Первая из четырёх дочерних клеток подряд; 0 — лист.
    kids: usize,
    /// Узлы листа: `order[from..to]`.
    from: usize,
    to: usize,
}

/// Переставить `items` так, чтобы сначала шли подходящие под `pred`; их число.
fn partition(items: &mut [usize], pred: impl Fn(usize) -> bool) -> usize {
    let mut k = 0;
    for i in 0..items.len() {
        if pred(items[i]) {
            items.swap(i, k);
            k += 1;
        }
    }
    k
}

/// Глубже — узлы почти в одной точке: лист с несколькими узлами.
const MAX_DEPTH: usize = 40;

impl QuadTree {
    fn build(&mut self, pos: &[(f64, f64)]) {
        self.cells.clear();
        self.order.clear();
        self.order.extend(0..pos.len());
        self.pos.clear();
        self.pos.extend_from_slice(pos);
        if pos.is_empty() {
            return;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y) in pos {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        let size = (x1 - x0).max(y1 - y0).max(1.0);
        self.cells.push(Cell { x: 0.0, y: 0.0, mass: 0.0, size, kids: 0, from: 0, to: pos.len() });
        self.split(0, x0, y0, 0);
    }

    /// Разбить клетку `c` с левым верхним углом `(x0, y0)` и посчитать её
    /// центр масс.
    #[allow(clippy::cast_precision_loss, reason = "узлов — тысячи")]
    fn split(&mut self, c: usize, x0: f64, y0: f64, depth: usize) {
        let Cell { size, from, to, .. } = self.cells[c];
        if to - from > 1 && depth < MAX_DEPTH {
            let half = size / 2.0;
            let (mx, my) = (x0 + half, y0 + half);
            // Сначала верхние (`y < my`), в каждой половине — сначала левые.
            let pos = &self.pos;
            let items = &mut self.order[from..to];
            let top = partition(items, |i| pos[i].1 < my);
            let (upper, lower) = items.split_at_mut(top);
            let bounds = [
                from,
                from + partition(upper, |i| pos[i].0 < mx),
                from + top,
                from + top + partition(lower, |i| pos[i].0 < mx),
                to,
            ];
            let kids = self.cells.len();
            for q in 0..4 {
                self.cells.push(Cell {
                    x: 0.0,
                    y: 0.0,
                    mass: 0.0,
                    size: half,
                    kids: 0,
                    from: bounds[q],
                    to: bounds[q + 1],
                });
            }
            self.cells[c].kids = kids;
            let (mut sx, mut sy, mut m) = (0.0, 0.0, 0.0);
            for q in 0..4 {
                let k = kids + q;
                if self.cells[k].to > self.cells[k].from {
                    let (qx, qy) = (x0 + half * (q % 2) as f64, y0 + half * (q / 2) as f64);
                    self.split(k, qx, qy, depth + 1);
                    let kid = self.cells[k];
                    sx += kid.x * kid.mass;
                    sy += kid.y * kid.mass;
                    m += kid.mass;
                }
            }
            let cell = &mut self.cells[c];
            (cell.x, cell.y, cell.mass) = (sx / m, sy / m, m);
        } else {
            let (mut sx, mut sy) = (0.0, 0.0);
            for &i in &self.order[from..to] {
                sx += self.pos[i].0;
                sy += self.pos[i].1;
            }
            let m = (to - from) as f64;
            let cell = &mut self.cells[c];
            (cell.x, cell.y, cell.mass) = (sx / m, sy / m, m);
        }
    }

    /// Сумма отталкивания `EDGE²/d` от всех узлов, кроме `i`, на точку `p`;
    /// `stack` — место для обхода.
    fn repulsion(&self, i: usize, p: (f64, f64), stack: &mut Vec<usize>) -> (f64, f64) {
        let mut f = (0.0, 0.0);
        let mut push = |dx: f64, dy: f64, mass: f64| {
            let d2 = (dx * dx + dy * dy).max(1.0);
            let k = mass * EDGE * EDGE / d2;
            f.0 += dx * k;
            f.1 += dy * k;
        };
        stack.clear();
        stack.push(0);
        while let Some(c) = stack.pop() {
            let cell = self.cells[c];
            if cell.mass == 0.0 {
                continue;
            }
            let (dx, dy) = (p.0 - cell.x, p.1 - cell.y);
            if cell.kids == 0 {
                for &j in &self.order[cell.from..cell.to] {
                    if j != i {
                        // Узлы в одной точке: расталкивает детерминированный сдвиг по номерам.
                        let (dx, dy) = (p.0 - self.pos[j].0, p.1 - self.pos[j].1);
                        let (dx, dy) =
                            if dx == 0.0 && dy == 0.0 { (if i < j { -1.0 } else { 1.0 }, 0.0) } else { (dx, dy) };
                        push(dx, dy, 1.0);
                    }
                }
            } else if cell.size * cell.size < THETA * THETA * (dx * dx + dy * dy) {
                push(dx, dy, cell.mass);
            } else {
                stack.extend((cell.kids..cell.kids + 4).rev());
            }
        }
        f
    }
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
pub fn place(graph: &Graph, groups: Vec<String>, center: Option<String>, forces: Forces) -> GraphLayout {
    let index: HashMap<&str, usize> = graph.nodes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();
    let pair = |e: &Edge| Some((*index.get(e.from.as_str())?, *index.get(e.to.as_str())?));
    let links: Vec<(usize, usize)> = graph.edges.iter().filter_map(pair).collect();
    // Книга и главы — кластер: рёбра «книга - глава» короче и жёстче ссылок.
    let tight: Vec<(usize, usize)> = graph.edges.iter().filter(|e| e.chapter).filter_map(pair).collect();
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
            let book = n.kind == Some(NoteKind::Book) && n.chapter.is_none();
            #[allow(clippy::cast_precision_loss, reason = "не больше 8")]
            let r = if book { 11.0 } else { 5.5 } + d.min(8) as f64 * 0.7;
            NodeBox { r, label: label_width(&n.title, if book { LABEL_SIZE * 1.1 } else { LABEL_SIZE }) }
        })
        .collect();
    let node_groups: Vec<usize> =
        graph.nodes.iter().map(|n| groups.iter().position(|g| g == node_group(n)).unwrap_or(0)).collect();
    let pos = layout_with(graph.nodes.len(), &links, &tight, &node_groups, Some(&boxes), forces);
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
            chapter: n.chapter.clone(),
            name: n.title.clone(),
            group: node_group(n).to_owned(),
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
/// та же раскладка, считать её заново незачем. Ключ — хэш самого графа и
/// сил, поэтому кэш не устаревает: изменилось хранилище — другой ключ.
/// Переключатели страницы `/graph`, версия заметки с `#vault-graph`
/// ([`GraphData`]) и её сборка берут готовую.
#[derive(Debug, Default)]
pub struct Layouts {
    /// Недавние — в начале.
    entries: parking_lot::Mutex<std::collections::VecDeque<(u64, Arc<GraphLayout>)>>,
}

impl Layouts {
    /// Раскладка графа (см. [`place`]) — из кэша или посчитанная.
    pub fn place(
        &self,
        graph: &Graph,
        groups: Vec<String>,
        center: Option<String>,
        forces: Forces,
    ) -> Arc<GraphLayout> {
        let key = {
            let json = serde_json::to_vec(&(graph, &groups, &center, forces)).unwrap_or_default();
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
        let layout = Arc::new(place(graph, groups, center, forces));
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
        layouts.place(&shown, groups, f.around.clone(), f.forces)
    }

    /// Граф хранилища по фильтру, разложенный для рисования.
    pub fn graph_layout(&self, f: &GraphFilter) -> GraphLayout {
        let (shown, groups) = self.shown(f);
        place(&shown, groups, f.around.clone(), f.forces)
    }

    /// Показанный граф по фильтру и все группы хранилища.
    fn shown(&self, f: &GraphFilter) -> (Graph, Vec<String>) {
        let full = self.graph_of(f.chapters);
        let mut groups: Vec<String> = Vec::new();
        for n in &full.nodes {
            let g = node_group(n);
            if !groups.iter().any(|x| x == g) {
                groups.push(g.to_owned());
            }
        }
        let has_tag = |n: &Node, tag: &str| match &n.chapter {
            Some(c) => self.chapter_has_tag(&c.book, &c.anchor, tag),
            None => self.has_tag(&n.id, tag),
        };
        (filter(&full, has_tag, f), groups)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::time::Instant;

    use super::*;
    use crate::graph::Node;

    #[derive(Debug)]
    struct BenchGraph {
        links: Vec<(usize, usize)>,
        boxes: Vec<NodeBox>,
        groups: Vec<usize>,
        degree: Vec<usize>,
    }

    #[derive(Debug)]
    struct Quality {
        edge_mean: f64,
        area_per_node: f64,
        overlaps: usize,
        intra_group: f64,
        group_gap: f64,
        isolated_radius: f64,
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_precision_loss)]
    fn synthetic_graph(n: usize) -> BenchGraph {
        let groups_count = (n / 120).clamp(4, 40);
        let mut groups = vec![0usize; n];
        for (i, group) in groups.iter_mut().enumerate() {
            *group = i.saturating_mul(groups_count) / n.max(1);
        }
        let mut edges = BTreeSet::new();
        let mut group_nodes: Vec<Vec<usize>> = vec![Vec::new(); groups_count];
        for (i, &group) in groups.iter().enumerate() {
            group_nodes[group].push(i);
        }
        // Локальная связность внутри папки: цепочка + короткие перемычки.
        for nodes in &group_nodes {
            for pair in nodes.windows(2) {
                edges.insert((pair[0], pair[1]));
            }
            for pair in nodes.windows(4) {
                edges.insert((pair[0], pair[3]));
            }
        }
        // Межпапочные связи: кольцо центров групп.
        let centers: Vec<usize> = group_nodes.iter().filter_map(|nodes| nodes.first().copied()).collect();
        for i in 0..centers.len() {
            let a = centers[i];
            let b = centers[(i + 1) % centers.len()];
            if a != b {
                edges.insert((a.min(b), a.max(b)));
            }
        }
        // Несколько хабов с широкими связями по всему хранилищу.
        let hubs: Vec<usize> = [0usize, n / 7, n / 3, n / 2].into_iter().filter(|&i| i < n).collect();
        for &hub in &hubs {
            for step in [17usize, 41, 89, 157] {
                let mut v = hub + step;
                while v < n {
                    if groups[v] != groups[hub] {
                        edges.insert((hub.min(v), hub.max(v)));
                    }
                    v += step;
                }
            }
        }
        // Изолированные заметки: как в реальном хранилище, часть узлов без ссылок.
        for i in (19..n).step_by(53) {
            if i % 7 == 0 {
                edges.retain(|&(a, b)| a != i && b != i);
            }
        }
        let links: Vec<(usize, usize)> = edges.into_iter().collect();
        let mut degree = vec![0usize; n];
        for &(a, b) in &links {
            degree[a] += 1;
            degree[b] += 1;
        }
        let boxes: Vec<NodeBox> = (0..n)
            .map(|i| {
                let text_len = 8 + (i * 17 + 11) % 26;
                let label = "x".repeat(text_len);
                let r = 5.5 + (degree[i].min(10) as f64) * 0.35;
                NodeBox { r, label: label_width(&label, LABEL_SIZE) }
            })
            .collect();
        BenchGraph { links, boxes, groups, degree }
    }

    #[allow(clippy::cast_precision_loss, reason = "узлов тысячи, метрики дробные")]
    fn quality(pos: &[(f64, f64)], graph: &BenchGraph) -> Quality {
        let edge_len: Vec<f64> =
            graph.links.iter().map(|&(a, b)| (pos[a].0 - pos[b].0).hypot(pos[a].1 - pos[b].1)).collect();
        let edge_mean = edge_len.iter().sum::<f64>() / edge_len.len().max(1) as f64;
        let bounds =
            pos.iter().zip(&graph.boxes).map(|(&p, &b)| box_rect(p, b)).fold(None, |acc: Option<[f64; 4]>, r| {
                Some(acc.map_or(r, |a| [a[0].min(r[0]), a[1].min(r[1]), a[2].max(r[2]), a[3].max(r[3])]))
            });
        let [x0, y0, x1, y1] = bounds.unwrap_or([0.0, 0.0, 1.0, 1.0]);
        let area_per_node = ((x1 - x0) * (y1 - y0)) / pos.len().max(1) as f64;

        let mut overlaps = 0usize;
        for i in 0..pos.len() {
            for j in i + 1..pos.len() {
                let [a0, a1, a2, a3] = box_rect(pos[i], graph.boxes[i]);
                let [b0, b1, b2, b3] = box_rect(pos[j], graph.boxes[j]);
                if a2.min(b2) > a0.max(b0) && a3.min(b3) > a1.max(b1) {
                    overlaps += 1;
                }
            }
        }

        let groups_count = graph.groups.iter().copied().max().map_or(0, |x| x + 1);
        let mut centroids = vec![(0.0, 0.0, 0usize); groups_count];
        for (i, &(x, y)) in pos.iter().enumerate() {
            let g = graph.groups[i];
            centroids[g].0 += x;
            centroids[g].1 += y;
            centroids[g].2 += 1;
        }
        let mut centroid_pos = vec![(0.0, 0.0); groups_count];
        for (g, &(sx, sy, c)) in centroids.iter().enumerate() {
            if c > 0 {
                centroid_pos[g] = (sx / c as f64, sy / c as f64);
            }
        }
        let mut intra = 0.0;
        for (i, &(x, y)) in pos.iter().enumerate() {
            let c = centroid_pos[graph.groups[i]];
            intra += (x - c.0).hypot(y - c.1);
        }
        let intra_group = intra / pos.len().max(1) as f64;

        let mut gap_sum = 0.0;
        let mut gap_n = 0usize;
        for i in 0..groups_count {
            for j in i + 1..groups_count {
                if centroids[i].2 == 0 || centroids[j].2 == 0 {
                    continue;
                }
                gap_sum += (centroid_pos[i].0 - centroid_pos[j].0).hypot(centroid_pos[i].1 - centroid_pos[j].1);
                gap_n += 1;
            }
        }
        let group_gap = gap_sum / gap_n.max(1) as f64;

        let (cx, cy) = (
            pos.iter().map(|p| p.0).sum::<f64>() / pos.len().max(1) as f64,
            pos.iter().map(|p| p.1).sum::<f64>() / pos.len().max(1) as f64,
        );
        let isolated: Vec<f64> = pos
            .iter()
            .enumerate()
            .filter_map(|(i, &(x, y))| (graph.degree[i] == 0).then_some((x - cx).hypot(y - cy)))
            .collect();
        let isolated_radius = isolated.iter().sum::<f64>() / isolated.len().max(1) as f64;
        Quality { edge_mean, area_per_node, overlaps, intra_group, group_gap, isolated_radius }
    }

    /// Время раскладки и раздвигания подписей (мс) и качество.
    fn bench_current(graph: &BenchGraph) -> (f64, f64, Quality) {
        let started = Instant::now();
        let mut pos = layout_forces(graph.boxes.len(), &graph.links, &[], &graph.groups, Forces::default());
        let forces = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        separate(&mut pos, &graph.boxes);
        let separated = started.elapsed().as_secs_f64() * 1000.0;
        (forces, separated, quality(&pos, graph))
    }

    fn node(id: &str, kind: Option<NoteKind>) -> Node {
        Node { id: id.into(), kind, title: id.rsplit('/').next().unwrap_or(id).into(), chapter: None }
    }

    fn edge(from: &str, to: &str, count: usize) -> Edge {
        Edge { from: from.into(), to: to.into(), count, chapter: false }
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

    fn tags(n: &Node, tag: &str) -> bool {
        let tags: &[&str] = match n.id.as_str() {
            "Сеть/A" => &["сеть"],
            "Сеть/B" => &["сеть", "ssh"],
            _ => &[],
        };
        tags.contains(&tag)
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
    #[ignore = "инструментальный замер производительности; запускать в release с --ignored --nocapture"]
    fn layout_bench_synthetic() {
        for n in [100usize, 300, 1000, 5000] {
            let graph = synthetic_graph(n);
            let (forces, separated, q) = bench_current(&graph);
            println!(
                "layout_bench n={n} total_ms={:.1} layout_ms={:.1} separate_ms={:.1} edge_mean={:.0} area_per_node={:.0} overlaps={} intra_group={:.0} group_gap={:.0} isolated_radius={:.0}",
                forces + separated,
                forces,
                separated,
                q.edge_mean,
                q.area_per_node,
                q.overlaps,
                q.intra_group,
                q.group_gap,
                q.isolated_radius,
            );
        }
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
        let pos = layout(7, &RING, &[], None, Forces::default());
        assert_eq!(pos, layout(7, &RING, &[], None, Forces::default()));
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
        let pos = layout(names.len(), &star, &[], Some(&boxes), Forces::default());
        assert_eq!(pos, layout(names.len(), &star, &[], Some(&boxes), Forces::default()));
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
        let f = Forces::default();
        let a = layouts.place(&g, vec![], None, f);
        assert!(Arc::ptr_eq(&a, &layouts.place(&g, vec![], None, f)), "тот же граф — из кэша");
        assert_eq!(*a, place(&g, vec![], None, f));
        let other = layouts.place(&g, vec![], Some("D".into()), f);
        assert!(!Arc::ptr_eq(&a, &other), "другой центр — другая раскладка");
        let mut g2 = sample();
        g2.edges.pop();
        assert!(!Arc::ptr_eq(&a, &layouts.place(&g2, vec![], None, f)), "другой граф — заново");
        let looser = layouts.place(&g, vec![], None, Forces { repel: 200, ..f });
        assert!(!Arc::ptr_eq(&a, &looser), "другие силы — заново");
    }

    #[test]
    fn layout_handles_empty_single_and_without_links() {
        assert!(layout(0, &[], &[], Some(&[]), Forces::default()).is_empty());
        assert_eq!(layout(1, &[], &[], None, Forces::default()), vec![(0.0, 0.0)]);
        let pos = layout(32, &[], &[], None, Forces::default());
        assert_eq!(pos.len(), 32);
        assert!(pos.iter().all(|(x, y)| x.is_finite() && y.is_finite()));
    }

    #[test]
    fn layout_is_bit_identical_for_generated_graph() {
        let graph = synthetic_graph(480);
        let a = layout(480, &graph.links, &[], Some(&graph.boxes), Forces::default());
        let b = layout(480, &graph.links, &[], Some(&graph.boxes), Forces::default());
        let bits = |v: &[(f64, f64)]| v.iter().map(|p| (p.0.to_bits(), p.1.to_bits())).collect::<Vec<_>>();
        assert_eq!(bits(&a), bits(&b));
    }

    #[test]
    fn generated_labels_do_not_overlap() {
        let mut graph = synthetic_graph(180);
        for b in &mut graph.boxes {
            b.label = label_width("узел", LABEL_SIZE);
        }
        let pos = layout(180, &graph.links, &[], Some(&graph.boxes), Forces::default());
        let q = quality(&pos, &graph);
        assert_eq!(q.overlaps, 0);
    }

    #[test]
    fn forces_do_what_they_say() {
        let graph = synthetic_graph(300);
        let q = |forces| quality(&layout(300, &graph.links, &graph.groups, Some(&graph.boxes), forces), &graph);
        let base = q(Forces::default());
        let clusters = q(Forces { clusters: 100, ..Forces::default() });
        let repel = q(Forces { repel: 200, ..Forces::default() });
        let center = q(Forces { center: 50, ..Forces::default() });
        let links = q(Forces { links: 500, ..Forces::default() });
        let apart = |q: &Quality| q.group_gap / q.intra_group;
        assert!(apart(&clusters) > apart(&base) * 1.4, "папки - врозь");
        assert!(repel.area_per_node > base.area_per_node * 1.3, "просторнее");
        assert!(center.area_per_node > base.area_per_node * 1.3, "слабее к центру - просторнее");
        assert!(links.edge_mean < base.edge_mean * 0.9, "связи короче");
        for q in [clusters, repel, center, links] {
            assert_eq!(q.overlaps, 0);
        }
    }

    #[test]
    fn placed_nodes_and_bounds() {
        let g = sample();
        let l = place(&g, vec!["Сеть".into(), "Мат".into(), ROOT_GROUP.into()], None, Forces::default());
        let c = l.nodes.iter().find(|n| n.id == "Мат/C").unwrap();
        assert_eq!((c.name.as_str(), c.group.as_str(), c.degree), ("C", "Мат", 1));
        assert!(c.r > l.nodes.iter().find(|n| n.id == "E").unwrap().r, "книга крупнее");
        for n in &l.nodes {
            assert!(l.bounds[0] <= n.x && n.x <= l.bounds[2] && l.bounds[1] <= n.y && n.y <= l.bounds[3]);
        }
    }
}
