//! Кадры (`div.k-frames`, `baluk/frames.typ`): общие части — один раз.
//!
//! Кадры одного рисунка обычно повторяют оси, рамку, подписи и неизменные
//! кривые — по 4–8 КБ на кадр. Проход после [`crate::figures`] (он уже
//! склеил темы и округлил координаты, поэтому одинаковое и в тексте
//! одинаково) находит поддеревья SVG, встречающиеся в группе кадров не
//! меньше двух раз, и переносит их в скрытый
//! `<svg class="k-frames-defs"><defs>` в начале `div.k-frames`; на их месте
//! остаётся `<use xlink:href="#kd…"/>`. Как с глифами: вид тот же, CSS-
//! переменные цветов наследуются через `<use>`.
//!
//! Общие части лежат внутри `div.k-frames`, а не в наборе страницы: глава
//! книги ([`crate::book`]) режется по тексту, и рисунок уходит в главу
//! целиком. `id` — хэш поддерева: две заметки на одной странице с
//! одинаковыми частями не конфликтуют.
//!
//! Разбор — по тегам, как в [`crate::figures`]; непонятное (вложенные
//! `<svg>`, несбалансированные теги) остаётся как есть.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::figures::tokens;
use crate::version::StableHasher;

const GROUP_OPEN: &str = r#"<div class="k-frames""#;

/// Поддерево короче этого не выносится: `<use>` сам около 30 байт.
const MIN_LEN: usize = 96;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// Групп кадров на странице.
    pub groups: usize,
    /// Вынесено поддеревьев (разных).
    pub shared: usize,
    pub before: usize,
    pub after: usize,
}

/// Выносит общие части кадров каждой группы `div.k-frames` в `<defs>`.
pub fn share(body: &str) -> (String, Stats) {
    let mut stats = Stats { before: body.len(), ..Stats::default() };
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find(GROUP_OPEN) {
        out.push_str(&rest[..start]);
        let group = &rest[start..];
        let Some(len) = div_len(group) else {
            out.push_str(group);
            rest = "";
            break;
        };
        stats.groups += 1;
        let (text, shared) = share_group(&group[..len]);
        stats.shared += shared;
        out.push_str(&text);
        rest = &group[len..];
    }
    out.push_str(rest);
    stats.after = out.len();
    (out, stats)
}

/// Длина элемента `<div …>…</div>` от начала `s` (с учётом вложенных `div`).
fn div_len(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut pos = 0;
    loop {
        let next_open = s[pos..].find("<div").map(|i| pos + i);
        let next_close = s[pos..].find("</div>").map(|i| pos + i);
        match (next_open, next_close) {
            (Some(o), Some(c)) if o < c => {
                depth += 1;
                pos = o + 4;
            }
            (_, Some(c)) => {
                depth = depth.checked_sub(1)?;
                pos = c + "</div>".len();
                if depth == 0 {
                    return Some(pos);
                }
            }
            _ => return None,
        }
    }
}

/// Один `div.k-frames`: текст с общими частями в `<defs>` и сколько их.
fn share_group(group: &str) -> (String, usize) {
    // SVG группы: (начало, конец) в тексте.
    let mut svgs = Vec::new();
    let mut pos = 0;
    while let Some(i) = group[pos..].find("<svg") {
        let start = pos + i;
        let Some(len) = group[start..].find("</svg>") else { return (group.to_owned(), 0) };
        let end = start + len + "</svg>".len();
        if group[start + 4..end].contains("<svg") {
            return (group.to_owned(), 0); // вложенный SVG — не наш формат
        }
        svgs.push((start, end));
        pos = end;
    }
    if svgs.len() < 2 {
        return (group.to_owned(), 0);
    }

    let parsed: Vec<Vec<&str>> = svgs.iter().map(|&(s, e)| tokens(&group[s..e])).collect();
    let trees: Option<Vec<Vec<Node>>> = parsed.iter().map(|t| subtrees(t)).collect();
    let Some(trees) = trees else { return (group.to_owned(), 0) };

    // Сколько раз каждое поддерево встречается в группе.
    let mut count: HashMap<String, usize> = HashMap::new();
    for (toks, nodes) in parsed.iter().zip(&trees) {
        for n in nodes.iter().filter(|n| n.eligible) {
            *count.entry(toks[n.start..=n.end].concat()).or_default() += 1;
        }
    }

    let mut defs: Vec<(String, String)> = Vec::new();
    let mut ids: HashMap<String, String> = HashMap::new();
    let mut out = String::with_capacity(group.len());
    let mut last = 0;
    for ((&(s, e), toks), nodes) in svgs.iter().zip(&parsed).zip(&trees) {
        out.push_str(&group[last..s]);
        last = e;
        // Узел, начинающийся с токена (узлы идут по порядку начала).
        let mut at = vec![None; toks.len()];
        for n in nodes {
            at[n.start] = Some(*n);
        }
        let mut svg = String::with_capacity(e - s);
        let mut i = 0;
        while i < toks.len() {
            // Самое внешнее общее поддерево, начинающееся здесь.
            let hit =
                at[i].filter(|n| n.eligible && count.get(&toks[n.start..=n.end].concat()).is_some_and(|&c| c >= 2));
            if let Some(n) = hit {
                let text = toks[n.start..=n.end].concat();
                let id = ids.entry(text.clone()).or_insert_with(|| {
                    let id = format!("kd{}", &StableHasher::new().str(&text).hex()[..12]);
                    defs.push((id.clone(), text));
                    id
                });
                let _ = write!(svg, r##"<use xlink:href="#{id}"/>"##);
                i = n.end + 1;
            } else {
                svg.push_str(toks[i]);
                i += 1;
            }
        }
        out.push_str(&svg);
    }
    out.push_str(&group[last..]);
    if defs.is_empty() {
        return (group.to_owned(), 0);
    }

    // Скрытый набор общих частей — сразу после открывающего тега группы.
    let open_end = out.find('>').map_or(0, |i| i + 1);
    let mut sprite = String::from(
        r#"<svg class="k-frames-defs" aria-hidden="true" width="0" height="0" style="position: absolute" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><defs>"#,
    );
    for (id, text) in &defs {
        sprite.push_str(&with_id(text, id));
    }
    sprite.push_str("</defs></svg>");
    out.insert_str(open_end, &sprite);
    (out, defs.len())
}

/// Поддерево в токенах SVG: первый и последний токен включительно.
#[derive(Debug, Clone, Copy)]
struct Node {
    start: usize,
    end: usize,
    /// Можно выносить: не корень, не внутри `<defs>`, без `id`, не короткое.
    eligible: bool,
}

/// Все поддеревья (элементы) SVG по порядку начала. Несбалансированные
/// теги — `None`.
fn subtrees(toks: &[&str]) -> Option<Vec<Node>> {
    let mut nodes = Vec::new();
    // Открытые элементы: (имя, номер токена, номер узла).
    let mut stack: Vec<(&str, usize, usize)> = Vec::new();
    let mut defs_depth = 0usize;
    let mut bytes = 0usize;
    let mut offsets = Vec::with_capacity(toks.len() + 1);
    for t in toks {
        offsets.push(bytes);
        bytes += t.len();
    }
    offsets.push(bytes);
    for (i, t) in toks.iter().enumerate() {
        if !t.starts_with('<') || t.starts_with("<!") || t.starts_with("<?") {
            continue;
        }
        if let Some(name) = t.strip_prefix("</") {
            let name = name.trim_end_matches('>').trim();
            let (open, start, node) = stack.pop()?;
            if open != name {
                return None;
            }
            if name == "defs" {
                defs_depth -= 1;
            }
            let n: &mut Node = &mut nodes[node];
            n.end = i;
            n.eligible &= offsets[i + 1] - offsets[start] >= MIN_LEN && !has_id(&toks[start..=i]);
            continue;
        }
        let name = tag_name(t);
        let self_closing = t.ends_with("/>");
        let eligible = !stack.is_empty() && defs_depth == 0 && name != "defs";
        nodes.push(Node { start: i, end: i, eligible });
        if self_closing {
            let n = nodes.last_mut().expect("только что добавлен");
            n.eligible &= t.len() >= MIN_LEN && !t.contains(" id=\"");
        } else {
            if name == "defs" {
                defs_depth += 1;
            }
            stack.push((name, i, nodes.len() - 1));
        }
    }
    stack.is_empty().then_some(nodes)
}

fn tag_name(tag: &str) -> &str {
    let inner = tag.trim_start_matches('<');
    let end = inner.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(inner.len());
    &inner[..end]
}

fn has_id(toks: &[&str]) -> bool {
    toks.iter().any(|t| t.starts_with('<') && t.contains(" id=\""))
}

/// Поддерево с `id` на корневом теге.
fn with_id(text: &str, id: &str) -> String {
    let name_end = text.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(text.len());
    format!(r#"{} id="{id}"{}"#, &text[..name_end], &text[name_end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    const AXES: &str = r#"<path fill="none" stroke-width="0.6" transform="translate(11.77 45.64)" d="M 0 0 h 120.69" style="stroke: var(--kf0)"/>"#;

    fn frame(curve: &str) -> String {
        format!(
            r#"<div class="k-frames-item"><div class="k-frame k-fig"><svg viewBox="0 0 10 10"><g>{AXES}<g transform="translate(1 2)">{AXES}</g><path d="{curve}"/></g></svg></div></div>"#
        )
    }

    fn group(frames: &[&str]) -> String {
        let items: String = frames.iter().map(|c| frame(c)).collect();
        format!(
            r#"<p>до</p><div class="k-frames" data-k-frames="{{}}"><div class="k-frames-stack">{items}</div></div><p>после</p>"#
        )
    }

    #[test]
    fn common_parts_move_to_defs() {
        let html = group(&["M 0 0 h 1", "M 0 0 h 2", "M 0 0 h 3"]);
        let (out, stats) = share(&html);
        assert_eq!((stats.groups, stats.shared), (1, 2), "ось и группа с осью: {out}");
        assert!(stats.after < stats.before);
        assert_eq!(out.matches(r#"d="M 0 0 h 120.69""#).count(), 2, "ось — в двух общих частях: {out}");
        assert!(out.starts_with(r#"<p>до</p><div class="k-frames" data-k-frames="{}"><svg class="k-frames-defs""#));
        assert!(out.ends_with("</div></div><p>после</p>"));
        for curve in ["h 1", "h 2", "h 3"] {
            assert!(out.contains(curve), "свои части кадров остаются");
        }
        // Самое внешнее общее: <g transform> с осью внутри — одним <use>.
        let items = &out[out.find("k-frames-stack").unwrap()..];
        assert_eq!(items.matches("<use xlink:href=\"#kd").count(), 6, "{items}");
    }

    #[test]
    fn nothing_shared_is_untouched() {
        let one = group(&["M 0 0 h 1"]);
        assert_eq!(share(&one).0, one, "один кадр — нечего делить");
        let text = "<p>без кадров</p>";
        assert_eq!(share(text), (text.to_owned(), Stats { before: text.len(), after: text.len(), ..Stats::default() }));
        let broken = r#"<div class="k-frames"><svg><g></svg><svg><g></svg></div>"#;
        assert_eq!(share(broken).0, broken, "несбалансированные теги — как есть");
    }

    #[test]
    fn ids_and_defs_are_not_shared() {
        let with_id =
            format!(r#"<svg><g><path id="p" d="{}"/><defs><path d="{0}"/></defs></g></svg>"#, "M 0 0 ".repeat(30));
        let html = format!(r#"<div class="k-frames">{with_id}{with_id}</div>"#);
        assert_eq!(share(&html).0, html);
    }

    #[test]
    fn same_part_gets_same_id_everywhere() {
        let a = share(&group(&["M 0 0 h 1", "M 0 0 h 2"])).0;
        let b = share(&group(&["M 0 0 h 5", "M 0 0 h 6"])).0;
        let id = |s: &str| s[s.find("id=\"kd").unwrap()..][4..20].to_owned();
        assert_eq!(id(&a), id(&b));
    }
}
