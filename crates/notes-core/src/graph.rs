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
//!
//! Вычисляемые пути индекс берёт из собранных страниц: ссылки последней
//! удачной сборки заметки ([`SourceIndex::set_built`], из кэша страниц)
//! дописываются к найденным в исходнике — так они попадают в обратные
//! ссылки и граф. Пока заметка не пересобрана, её прежние ссылки остаются.
//!
//! С наблюдателем файлов ([`crate::watch::Changes`], сервер) индекс не
//! обходит хранилище, пока в нём ничего не менялось: разбор всех заметок
//! берётся из памяти (и на всякий случай обновляется не реже [`MAX_AGE`]).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use parking_lot::Mutex;
use serde::Serialize;
use typst::syntax::{SyntaxNode, ast};

use crate::Result;
use crate::outline::{Outline, parse_outline};
use crate::render::LinkRef;
use crate::vault::{Entry, NoteId, NoteKind, Vault};
use crate::watch::Changes;

/// С наблюдателем — обходить хранилище хотя бы так часто (страховка от
/// потерянного события).
pub const MAX_AGE: Duration = Duration::from_secs(600);

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

/// Ссылки последней удачной сборки заметки (`None` — не собиралась).
pub type BuiltLinks = Box<dyn Fn(&NoteId) -> Option<Vec<LinkRef>> + Send + Sync>;

#[derive(Default)]
pub struct SourceIndex {
    /// Путь файла в хранилище → разбор.
    files: Mutex<HashMap<String, Parsed>>,
    /// Откуда брать ссылки собранных страниц.
    built: OnceLock<BuiltLinks>,
    /// Наблюдатель файлов: пока счётчик тот же, обход не нужен.
    changes: OnceLock<Arc<Changes>>,
    /// Последний обход: при каком счётчике, когда и что нашёл.
    walked: Mutex<Option<(u64, Instant, Arc<Walk>)>>,
}

/// Разбор всех заметок хранилища — итог обхода.
#[derive(Debug)]
struct Walk {
    entries: Arc<Vec<Entry>>,
    /// Ссылки из исходников.
    links: BTreeMap<NoteId, Vec<LinkRef>>,
    outlines: Arc<BTreeMap<NoteId, Outline>>,
}

impl std::fmt::Debug for SourceIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceIndex").field("files", &self.files.lock().len()).finish_non_exhaustive()
    }
}

/// Ссылки всех заметок хранилища на данный момент.
#[derive(Debug)]
pub struct Snapshot {
    entries: Arc<Vec<Entry>>,
    /// Заметка → её ссылки (без повторов, в порядке появления).
    links: BTreeMap<NoteId, Vec<LinkRef>>,
    /// Заметка → содержание (у книги — всех файлов по порядку).
    outlines: Arc<BTreeMap<NoteId, Outline>>,
}

impl SourceIndex {
    /// Дополнять ссылки из исходников ссылками собранных страниц (один раз).
    pub fn set_built(&self, built: BuiltLinks) {
        if self.built.set(built).is_err() {
            tracing::warn!("индекс ссылок: источник собранных страниц уже задан");
        }
    }

    /// Не обходить хранилище, пока `changes` не сообщит об изменении (один раз).
    pub fn set_changes(&self, changes: Arc<Changes>) {
        if self.changes.set(changes).is_err() {
            tracing::warn!("индекс ссылок: наблюдатель уже задан");
        }
    }

    /// Ссылки и содержание всех заметок. Обходит хранилище (разбираются
    /// только изменившиеся файлы) — или, с наблюдателем, берёт прошлый обход,
    /// если с тех пор ничего не менялось.
    pub fn snapshot(&self, vault: &Vault) -> Result<Snapshot> {
        let walk = self.walk(vault)?;
        let mut links = walk.links.clone();
        // Ссылки собранных страниц: вычисляемые пути — после буквальных.
        if let Some(built) = self.built.get() {
            for (id, own) in &mut links {
                for link in built(id).unwrap_or_default() {
                    if !own.contains(&link) {
                        own.push(link);
                    }
                }
            }
        }
        Ok(Snapshot { entries: walk.entries.clone(), links, outlines: walk.outlines.clone() })
    }

    fn walk(&self, vault: &Vault) -> Result<Arc<Walk>> {
        let watched = self.changes.get().filter(|c| c.watching());
        // Счётчик — до обхода: событие во время обхода сделает его устаревшим.
        let seq = watched.map(|c| c.seq());
        if let (Some(seq), Some((at_seq, at, walk))) = (seq, &*self.walked.lock())
            && *at_seq == seq
            && at.elapsed() < MAX_AGE
        {
            return Ok(walk.clone());
        }
        let walk = Arc::new(self.walk_files(vault)?);
        if let Some(seq) = seq {
            *self.walked.lock() = Some((seq, Instant::now(), walk.clone()));
        }
        Ok(walk)
    }

    /// Обход хранилища: разбираются только изменившиеся файлы.
    fn walk_files(&self, vault: &Vault) -> Result<Walk> {
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
        Ok(Walk { entries: Arc::new(entries), links, outlines: Arc::new(outlines) })
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

    #[test]
    fn with_watcher_walks_only_after_change() {
        use crate::storage::MemStorage;
        let mem = Arc::new(MemStorage::new());
        mem.write("A.typ", r#"#see("B")"#);
        mem.write("B.typ", "");
        let vault = Vault::new(mem.clone());
        let changes = Arc::new(Changes::default());
        assert!(changes.start(&*mem));
        let index = SourceIndex::default();
        index.set_changes(changes.clone());
        let b = NoteId::new("B").unwrap();
        assert_eq!(index.snapshot(&vault).unwrap().backlinks(&b).len(), 1);
        let first = index.walked.lock().as_ref().map(|w| Arc::as_ptr(&w.2)).unwrap();
        index.snapshot(&vault).unwrap();
        let again = index.walked.lock().as_ref().map(|w| Arc::as_ptr(&w.2)).unwrap();
        assert_eq!(first, again, "без изменений — прошлый обход");

        mem.write("C.typ", r#"#see("B")"#);
        assert_eq!(index.snapshot(&vault).unwrap().backlinks(&b).len(), 2, "изменение — новый обход");

        // Без наблюдателя — обход всегда.
        changes.stop();
        mem.write("D.typ", r#"#see("B")"#);
        assert_eq!(index.snapshot(&vault).unwrap().backlinks(&b).len(), 3);
    }

    #[test]
    fn built_links_are_merged() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("A.typ"), r#"#let t = "B"; #see(t) #see("C")"#).unwrap();
        std::fs::write(dir.path().join("B.typ"), "").unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let index = SourceIndex::default();
        let a = NoteId::new("A").unwrap();
        assert!(index.snapshot(&vault).unwrap().backlinks(&NoteId::new("B").unwrap()).is_empty());
        index.set_built(Box::new(|id: &NoteId| (id.as_str() == "A").then(|| vec![link("C", None), link("B", None)])));
        let snap = index.snapshot(&vault).unwrap();
        assert_eq!(snap.outgoing(&a), [link("C", None), link("B", None)], "вычисляемая — после буквальных");
        assert_eq!(snap.backlinks(&NoteId::new("B").unwrap()).len(), 1);
        assert_eq!(snap.graph().edges.len(), 2);
    }
}
