//! Индекс исходников хранилища: ссылки (обратные ссылки, граф) и
//! содержание заметок ([`crate::outline`]: название, теги, разделы — для
//! быстрого перехода и поиска).
//!
//! Ссылки берутся **из исходников, без компиляции**: парсер Typst находит
//! вызовы `#see("путь", anchor: "…")` с буквальными строками. Так весь граф
//! строится за миллисекунды, а не за время сборки всех заметок (у книги —
//! десятки секунд). Ссылка с вычисляемым путём (`#see(путь-из-переменной)`) в
//! индекс не попадёт — Claude Code пишет ссылки буквально, а `notes check`
//! проверяет ссылки по собранным страницам, так что расхождение будет видно.
//!
//! Файл разбирается заново, только если изменились его время изменения или
//! размер. Заметке принадлежит её файл; книге — все `.typ` в её папке.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::SystemTime;

use parking_lot::Mutex;
use serde::Serialize;
use typst::syntax::{SyntaxNode, ast};

use crate::Result;
use crate::outline::{Outline, parse_outline};
use crate::render::LinkRef;
use crate::vault::{Entry, NoteId, NoteKind, Vault};

/// Функция ссылки из `baluk/links.typ`.
const LINK_FN: &str = "see";
const ANCHOR_ARG: &str = "anchor";

/// Ссылка на заметку из другой заметки.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Backlink {
    pub from: NoteId,
    pub anchor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Node {
    pub id: String,
    /// `None` — заметки нет (на неё ссылаются, но её не написали).
    pub kind: Option<NoteKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// Сколько ссылок (с разными якорями) ведёт по этому ребру.
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// Файл, разобранный при таком `(время, размер)`.
#[derive(Debug)]
struct Parsed {
    stamp: (Option<SystemTime>, u64),
    links: Vec<LinkRef>,
    outline: Outline,
}

#[derive(Debug, Default)]
pub struct SourceIndex {
    /// Путь файла в хранилище → разбор.
    files: Mutex<HashMap<String, Parsed>>,
}

/// Ссылки всех заметок хранилища на данный момент.
#[derive(Debug)]
pub struct Snapshot {
    entries: Vec<Entry>,
    /// Заметка → её ссылки (без повторов, в порядке появления).
    links: BTreeMap<NoteId, Vec<LinkRef>>,
    /// Заметка → содержание (у книги — всех файлов по порядку).
    outlines: BTreeMap<NoteId, Outline>,
}

impl SourceIndex {
    /// Обходит хранилище и возвращает ссылки всех заметок. Разбираются
    /// только изменившиеся файлы.
    pub fn snapshot(&self, vault: &Vault) -> Result<Snapshot> {
        let entries = vault.entries()?;
        let mut files = self.files.lock();
        let mut seen = BTreeSet::new();
        let mut links = BTreeMap::new();
        let mut outlines = BTreeMap::new();
        for entry in &entries {
            let mut own: Vec<LinkRef> = Vec::new();
            let mut outline = Outline::default();
            for path in vault.files_of(entry)? {
                let meta = vault.storage().stat(&path).map_err(|e| vault.io_error(&path, e))?;
                let stamp = (meta.modified, meta.len);
                if files.get(&path).is_none_or(|p| p.stamp != stamp) {
                    let text = vault.read_text(&path)?;
                    let parsed = Parsed { stamp, links: parse_links(&text), outline: parse_outline(&text) };
                    files.insert(path.clone(), parsed);
                }
                let parsed = &files[&path];
                for link in &parsed.links {
                    if !own.contains(link) {
                        own.push(link.clone());
                    }
                }
                // Книга: название и теги — из главного файла (где шаблон), разделы — всех глав.
                outline.title = outline.title.or_else(|| parsed.outline.title.clone());
                if outline.tags.is_empty() {
                    outline.tags.clone_from(&parsed.outline.tags);
                }
                outline.sections.extend(parsed.outline.sections.iter().cloned());
                seen.insert(path);
            }
            links.insert(entry.id.clone(), own);
            outlines.insert(entry.id.clone(), outline);
        }
        // Удалённые файлы — из кэша вон.
        files.retain(|path, _| seen.contains(path));
        Ok(Snapshot { entries, links, outlines })
    }
}

impl Snapshot {
    /// Содержание заметки: название, теги, разделы.
    pub fn outline(&self, id: &NoteId) -> Option<&Outline> {
        self.outlines.get(id)
    }

    /// Теги заметки по пути (`None` — нет такой заметки).
    pub fn tags_of(&self, id: &str) -> Option<&[String]> {
        self.outlines.get(&NoteId::new(id).ok()?).map(|o| o.tags.as_slice())
    }

    /// Все заметки с содержанием.
    pub fn outlines(&self) -> impl Iterator<Item = (&Entry, &Outline)> {
        self.entries.iter().filter_map(|e| self.outlines.get(&e.id).map(|o| (e, o)))
    }

    /// Ссылки заметки (как написаны в исходнике).
    pub fn outgoing(&self, id: &NoteId) -> &[LinkRef] {
        self.links.get(id).map_or(&[], Vec::as_slice)
    }

    /// Есть ли в хранилище заметка с таким путём.
    pub fn exists(&self, target: &str) -> bool {
        self.entries.iter().any(|e| e.id.as_str() == target)
    }

    /// Кто ссылается на заметку (без ссылок на себя).
    pub fn backlinks(&self, id: &NoteId) -> Vec<Backlink> {
        let mut out = Vec::new();
        for (from, links) in &self.links {
            if from == id {
                continue;
            }
            for link in links.iter().filter(|l| l.target == id.as_str()) {
                out.push(Backlink { from: from.clone(), anchor: link.anchor.clone() });
            }
        }
        out
    }

    /// Граф: все заметки и те, на которые ссылаются, но которых нет.
    /// Рёбра — между заметками (якоря сливаются), без петель.
    pub fn graph(&self) -> Graph {
        let mut nodes: BTreeMap<String, Option<NoteKind>> =
            self.entries.iter().map(|e| (e.id.to_string(), Some(e.kind))).collect();
        let mut edges: BTreeMap<(String, String), usize> = BTreeMap::new();
        for (from, links) in &self.links {
            for link in links {
                if link.target == from.as_str() {
                    continue;
                }
                nodes.entry(link.target.clone()).or_insert(None);
                *edges.entry((from.to_string(), link.target.clone())).or_default() += 1;
            }
        }
        Graph {
            nodes: nodes.into_iter().map(|(id, kind)| Node { id, kind }).collect(),
            edges: edges.into_iter().map(|((from, to), count)| Edge { from, to, count }).collect(),
        }
    }
}

/// Все `see("…", anchor: "…")` с буквальными строками, без повторов.
pub fn parse_links(text: &str) -> Vec<LinkRef> {
    let root = typst::syntax::parse(text);
    let mut out = Vec::new();
    visit(&root, &mut out);
    out
}

fn visit(node: &SyntaxNode, out: &mut Vec<LinkRef>) {
    if let Some(call) = node.cast::<ast::FuncCall>()
        && let ast::Expr::Ident(name) = call.callee()
        && name.as_str() == LINK_FN
    {
        let mut target = None;
        let mut anchor = None;
        for arg in call.args().items() {
            match arg {
                ast::Arg::Pos(ast::Expr::Str(s)) if target.is_none() => target = Some(s.get().to_string()),
                ast::Arg::Named(n) if n.name().as_str() == ANCHOR_ARG => {
                    if let ast::Expr::Str(s) = n.expr() {
                        anchor = Some(s.get().to_string());
                    }
                }
                _ => {}
            }
        }
        if let Some(target) = target {
            let link = LinkRef { target, anchor };
            if !out.contains(&link) {
                out.push(link);
            }
        }
    }
    for child in node.children() {
        visit(child, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(target: &str, anchor: Option<&str>) -> LinkRef {
        LinkRef { target: target.into(), anchor: anchor.map(Into::into) }
    }

    #[test]
    fn finds_literal_links_only() {
        let text = r#"
#import "/_baluk/lib.typ": *
Текст #see("Сеть/SSH") и #see("Сеть/SSH", anchor: "Смена порта")[порт].
#let target = "Сеть/UFW"
#see(target)
#definition[внутри блока — #see("Матан")]
#see("Сеть/SSH")
// #see("закомментировано")
`#see("в коде")`
"#;
        assert_eq!(
            parse_links(text),
            [link("Сеть/SSH", None), link("Сеть/SSH", Some("Смена порта")), link("Матан", None)]
        );
    }

    #[test]
    fn backlinks_and_graph() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let write = |f: &str, text: &str| {
            let p = root.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        };
        write("A.typ", r#"#see("B") #see("B", anchor: "x") #see("A") #see("Нет")"#);
        write("B.typ", "");
        write("Книга/main.typ", r#"#include "01.typ""#);
        write("Книга/01.typ", r#"#see("B")"#);

        let vault = Vault::open(root).unwrap();
        let index = SourceIndex::default();
        let snap = index.snapshot(&vault).unwrap();
        let b = NoteId::new("B").unwrap();
        let from: Vec<_> = snap.backlinks(&b).into_iter().map(|l| (l.from.to_string(), l.anchor)).collect();
        assert_eq!(from, [("A".into(), None), ("A".into(), Some("x".into())), ("Книга".into(), None)]);
        assert!(snap.backlinks(&NoteId::new("A").unwrap()).is_empty(), "ссылка на себя — не обратная");

        let g = snap.graph();
        let nodes: Vec<_> = g.nodes.iter().map(|n| (n.id.as_str(), n.kind)).collect();
        assert_eq!(
            nodes,
            [("A", Some(NoteKind::Note)), ("B", Some(NoteKind::Note)), ("Книга", Some(NoteKind::Book)), ("Нет", None)]
        );
        let edges: Vec<_> = g.edges.iter().map(|e| (e.from.as_str(), e.to.as_str(), e.count)).collect();
        assert_eq!(edges, [("A", "B", 2), ("A", "Нет", 1), ("Книга", "B", 1)]);

        // Правка файла видна без перезапуска; удалённый файл уходит из кэша.
        write("B.typ", r#"#see("A")"#);
        fs::remove_file(root.join("Книга/01.typ")).unwrap();
        let snap = index.snapshot(&vault).unwrap();
        assert_eq!(snap.backlinks(&NoteId::new("A").unwrap()).len(), 1);
        assert_eq!(snap.backlinks(&b).len(), 2);
        assert_eq!(index.files.lock().len(), 3);
    }
}
