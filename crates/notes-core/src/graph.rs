//! The source index of a vault: links (backlinks, the graph) and note
//! outlines ([`crate::outline`]: title, tags, sections - for quick
//! navigation and search).
//!
//! Links come **from the sources, without compiling**: the Typst parser
//! finds the calls `#see("path", anchor: "...")` with literal strings. So
//! the whole graph is built in milliseconds, not in the time of building all
//! notes (tens of seconds for a book). A link with a computed path
//! (`#see(path-from-a-variable)`) does not get into the index - Claude Code
//! writes links literally, and `notes check` checks links by the built
//! pages, so a mismatch shows.
//!
//! A file is parsed anew only if its modification time or size changed. A
//! note owns its file; a book owns all `.typ` in its folder. Folder titles
//! are their `_folder.toml` files ([`crate::folders`]), also by time and
//! size.
//!
//! The index takes computed paths from the built pages: the links of the
//! last successful build of a note ([`SourceIndex::set_built`], from the
//! page cache) are added to those found in the source, so they get into the
//! backlinks and the graph. Until the note is rebuilt, its old links stay.
//!
//! With the file watcher ([`crate::watch::Changes`], the server) the index
//! does not scan the vault while nothing in it changed: the parse of all
//! notes comes from memory (and is refreshed at least every [`MAX_AGE`],
//! just in case).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use parking_lot::Mutex;
use serde::Serialize;
use typst::syntax::{SyntaxNode, ast};

use crate::Result;
use crate::folders::{FOLDER_FILE, Folder, ancestors, parse_folder};
use crate::outline::{Outline, parse_outline};
use crate::render::LinkRef;
use crate::vault::{Entry, NoteId, NoteKind, Vault};
use crate::watch::Changes;

/// With the watcher, scan the vault at least this often (insurance against a
/// lost event).
pub const MAX_AGE: Duration = Duration::from_secs(600);

/// The link function from `baluk/links.typ`.
const LINK_FN: &str = "see";
/// The named argument of the link with the anchor.
const ANCHOR_ARG: &str = "anchor";

/// A link to a note from another note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Backlink {
    /// The linking note.
    pub from: NoteId,
    /// The link anchor as written.
    pub anchor: Option<String>,
    /// The section of the note the anchor leads to: the heading `id` (the
    /// heading HTML is in `Rendered::headings`) and its text (a formula as
    /// source); no anchor or the section was not found - `None`.
    pub section: Option<String>,
    pub heading: Option<String>,
}

/// A graph node: a note, a book, a book chapter or a missing note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Node {
    /// Note path; for a book chapter `<book>/.<number>` (a segment starting
    /// with `.` is impossible in a note path).
    pub id: String,
    /// `None`: the note does not exist (it is linked to but not written).
    pub kind: Option<NoteKind>,
    /// Title ([`Snapshot::title`]); for a chapter, its heading.
    pub title: String,
    /// A book chapter (the graph with chapters, [`Snapshot::graph_of`]).
    pub chapter: Option<ChapterOf>,
}

/// Whose chapter the node is and where it leads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChapterOf {
    /// Book path.
    pub book: String,
    /// `id` of the chapter heading on the book page.
    pub anchor: String,
}

/// A graph edge: links from one node to another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Edge {
    /// Source node.
    pub from: String,
    /// Target node.
    pub to: String,
    /// How many links (with different anchors) go along this edge.
    pub count: usize,
    /// Not a link but a book to its chapter (the graph with chapters).
    pub chapter: bool,
}

/// The link graph of a vault.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Graph {
    /// Nodes in the order of their ids.
    pub nodes: Vec<Node>,
    /// Edges in the order of (from, to).
    pub edges: Vec<Edge>,
}

/// A file parsed at this `(time, size)`.
#[derive(Debug)]
struct Parsed {
    stamp: (Option<SystemTime>, u64),
    links: Vec<LinkRef>,
    outline: Outline,
}

/// `_folder.toml` parsed at this `(time, size)`.
#[derive(Debug)]
struct ParsedFolder {
    stamp: (Option<SystemTime>, u64),
    folder: Folder,
}

/// Links of the last successful build of a note (`None` - not built).
pub type BuiltLinks = Box<dyn Fn(&NoteId) -> Option<Vec<LinkRef>> + Send + Sync>;

/// The source index of a vault, reparsing only changed files.
#[derive(Default)]
pub struct SourceIndex {
    /// File path in the vault -> its parse.
    files: Mutex<HashMap<String, Parsed>>,
    /// Folder -> its `_folder.toml` (only folders that have one).
    folder_files: Mutex<HashMap<String, ParsedFolder>>,
    /// Where to take the links of built pages from.
    built: OnceLock<BuiltLinks>,
    /// The file watcher: while the counter is the same, no scan is needed.
    changes: OnceLock<Arc<Changes>>,
    /// The last scan: at which counter, when and what it found.
    walked: Mutex<Option<(u64, Instant, Arc<Walk>)>>,
}

/// The parse of all notes of a vault, the result of a scan.
#[derive(Debug)]
struct Walk {
    entries: Arc<Vec<Entry>>,
    /// Links from the sources.
    links: BTreeMap<NoteId, Vec<LinkRef>>,
    outlines: Arc<BTreeMap<NoteId, Outline>>,
    folders: Arc<BTreeMap<String, Folder>>,
}

impl std::fmt::Debug for SourceIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceIndex").field("files", &self.files.lock().len()).finish_non_exhaustive()
    }
}

/// The links of all notes of a vault at a moment.
#[derive(Debug)]
pub struct Snapshot {
    entries: Arc<Vec<Entry>>,
    /// Note -> its links (no repeats, in order of appearance).
    links: BTreeMap<NoteId, Vec<LinkRef>>,
    /// Note -> outline (of a book - of all files in order).
    outlines: Arc<BTreeMap<NoteId, Outline>>,
    /// Folders that have notes (and all their ancestors) -> title.
    folders: Arc<BTreeMap<String, Folder>>,
}

impl SourceIndex {
    /// Adds the links of built pages to the links from sources (set once).
    pub fn set_built(&self, built: BuiltLinks) {
        if self.built.set(built).is_err() {
            tracing::warn!("link index: the source of built pages is already set");
        }
    }

    /// Do not scan the vault until `changes` reports a change (set once).
    pub fn set_changes(&self, changes: Arc<Changes>) {
        if self.changes.set(changes).is_err() {
            tracing::warn!("link index: the watcher is already set");
        }
    }

    /// Links and outlines of all notes. Scans the vault (only changed files
    /// are parsed) or, with the watcher, takes the previous scan if nothing
    /// changed since.
    pub fn snapshot(&self, vault: &Vault) -> Result<Snapshot> {
        let walk = self.walk(vault)?;
        let mut links = walk.links.clone();
        // Links of built pages: computed paths after literal ones.
        if let Some(built) = self.built.get() {
            for (id, own) in &mut links {
                for link in built(id).unwrap_or_default() {
                    if !own.contains(&link) {
                        own.push(link);
                    }
                }
            }
        }
        Ok(Snapshot {
            entries: walk.entries.clone(),
            links,
            outlines: walk.outlines.clone(),
            folders: walk.folders.clone(),
        })
    }

    fn walk(&self, vault: &Vault) -> Result<Arc<Walk>> {
        let watched = self.changes.get().filter(|c| c.watching());
        // The counter before the scan: an event during the scan makes it stale.
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

    /// Scans the vault: only changed files are parsed.
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
                // A book: title and tags from the main file (with the template), sections from all chapters.
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
        // Deleted files leave the cache.
        files.retain(|path, _| seen.contains(path));
        drop(files);
        let folders = self.walk_folders(vault, &entries)?;
        Ok(Walk { entries: Arc::new(entries), links, outlines: Arc::new(outlines), folders: Arc::new(folders) })
    }

    /// Vault folders (empty ones too) and their `_folder.toml` (only changed
    /// ones are reread).
    fn walk_folders(&self, vault: &Vault, entries: &[Entry]) -> Result<BTreeMap<String, Folder>> {
        let all = vault.folders()?;
        let paths: BTreeSet<&str> =
            entries.iter().flat_map(|e| ancestors(e.id.as_str())).chain(all.iter().map(String::as_str)).collect();
        let mut cache = self.folder_files.lock();
        let mut out = BTreeMap::new();
        for path in paths {
            let file = format!("{path}/{FOLDER_FILE}");
            let folder = match vault.storage().stat(&file) {
                Ok(meta) if !meta.is_dir => {
                    let stamp = (meta.modified, meta.len);
                    if cache.get(path).is_none_or(|p| p.stamp != stamp) {
                        let folder = match vault.read_text(&file) {
                            Ok(text) => match parse_folder(&text) {
                                Ok(meta) => Folder { title: meta.title, error: None },
                                Err(e) => Folder { title: None, error: Some(e) },
                            },
                            Err(e) => Folder { title: None, error: Some(e.to_string()) },
                        };
                        cache.insert(path.to_owned(), ParsedFolder { stamp, folder });
                    }
                    cache[path].folder.clone()
                }
                _ => Folder::default(),
            };
            out.insert(path.to_owned(), folder);
        }
        cache.retain(|path, _| out.get(path).is_some_and(|f| *f != Folder::default()));
        Ok(out)
    }
}

impl Snapshot {
    /// Note outline: title, tags, sections.
    pub fn outline(&self, id: &NoteId) -> Option<&Outline> {
        self.outlines.get(id)
    }

    /// Whether the note at the path has the tag: itself or a chapter of the book.
    pub fn has_tag(&self, id: &str, tag: &str) -> bool {
        NoteId::new(id).ok().and_then(|id| self.outlines.get(&id)).is_some_and(|o| o.has_tag(tag))
    }

    /// Whether a book chapter has the tag: at the book root or in the chapter
    /// itself (`anchor` is the `id` of its heading).
    pub fn chapter_has_tag(&self, book: &str, anchor: &str, tag: &str) -> bool {
        let Some(outline) = NoteId::new(book).ok().and_then(|id| self.outlines.get(&id)) else { return false };
        outline.tags.iter().any(|t| t == tag)
            || crate::search::section_at(outline, anchor)
                .is_some_and(|(i, _)| outline.sections[i].tags.iter().any(|t| t == tag))
    }

    /// Note title for display: from the template (`title: [...]`), otherwise
    /// the file name. For an unwritten note (only linked to), the last path
    /// segment.
    pub fn title(&self, id: &str) -> String {
        let fallback = || id.rsplit('/').next().unwrap_or(id).to_owned();
        let Ok(id) = NoteId::new(id) else { return fallback() };
        self.outlines.get(&id).and_then(|o| o.title.clone()).unwrap_or_else(fallback)
    }

    /// Folder title: from `_folder.toml`, otherwise its name.
    pub fn folder_title(&self, path: &str) -> String {
        self.folders
            .get(path)
            .and_then(|f| f.title.clone())
            .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).to_owned())
    }

    /// Vault folders (empty ones too), by path alphabetically.
    pub fn folders(&self) -> impl Iterator<Item = (&str, &Folder)> {
        self.folders.iter().map(|(path, f)| (path.as_str(), f))
    }

    /// All notes with outlines.
    pub fn outlines(&self) -> impl Iterator<Item = (&Entry, &Outline)> {
        self.entries.iter().filter_map(|e| self.outlines.get(&e.id).map(|o| (e, o)))
    }

    /// Note links (as written in the source).
    pub fn outgoing(&self, id: &NoteId) -> &[LinkRef] {
        self.links.get(id).map_or(&[], Vec::as_slice)
    }

    /// Whether the vault has a note at this path.
    pub fn exists(&self, target: &str) -> bool {
        self.entries.iter().any(|e| e.id.as_str() == target)
    }

    /// Who links to the note (no links to itself).
    pub fn backlinks(&self, id: &NoteId) -> Vec<Backlink> {
        let outline = self.outlines().find(|(e, _)| e.id == *id).map(|(_, o)| o);
        let mut out = Vec::new();
        for (from, links) in &self.links {
            if from == id {
                continue;
            }
            for link in links.iter().filter(|l| l.target == id.as_str()) {
                let at = outline.zip(link.anchor.as_deref()).and_then(|(o, a)| {
                    let (i, section) = crate::search::section_at(o, a)?;
                    Some((section, o.sections[i].heading.clone()))
                });
                let (section, heading) = at.map_or((None, None), |(s, h)| (Some(s), h));
                out.push(Backlink { from: from.clone(), anchor: link.anchor.clone(), section, heading });
            }
        }
        out
    }

    /// The graph: all notes and those linked to but missing. Edges are
    /// between notes (anchors merge), no loops.
    pub fn graph(&self) -> Graph {
        self.graph_of(false)
    }

    /// The graph; with `chapters` a book is not one node but a root with
    /// chapters around it (edges "book - chapter"). A link from a chapter
    /// goes from its node; a link to a book section (`anchor`) goes to the
    /// chapter with that section; links before the first chapter and without
    /// an anchor belong to the root.
    pub fn graph_of(&self, chapters: bool) -> Graph {
        let books: BTreeMap<&str, Vec<Chapter>> = if chapters {
            self.entries
                .iter()
                .filter(|e| e.kind == NoteKind::Book)
                .filter_map(|e| Some((e.id.as_str(), book_chapters(e.id.as_str(), self.outlines.get(&e.id)?))))
                .filter(|(_, c)| !c.is_empty())
                .collect()
        } else {
            BTreeMap::new()
        };
        let mut nodes: BTreeMap<String, Node> = self
            .entries
            .iter()
            .map(|e| {
                (
                    e.id.to_string(),
                    Node { id: e.id.to_string(), kind: Some(e.kind), title: self.title(e.id.as_str()), chapter: None },
                )
            })
            .collect();
        let mut edges: BTreeMap<(String, String), (usize, bool)> = BTreeMap::new();
        for (book, list) in &books {
            for c in list {
                let chapter = ChapterOf { book: (*book).to_owned(), anchor: c.anchor.clone() };
                let node = Node {
                    id: c.node.clone(),
                    kind: Some(NoteKind::Book),
                    title: c.title.clone(),
                    chapter: Some(chapter),
                };
                nodes.insert(c.node.clone(), node);
                edges.insert(((*book).to_owned(), c.node.clone()), (0, true));
            }
        }
        // Where a link leads: to a chapter (a book section), otherwise to the note.
        let target_of = |link: &LinkRef| -> String {
            let chapter = books.get(link.target.as_str()).and_then(|list| {
                let outline = self.outlines.get(&NoteId::new(&link.target).ok()?)?;
                let (section, _) = crate::search::section_at(outline, link.anchor.as_deref()?)?;
                list.iter().rev().find(|c| c.section <= section)
            });
            chapter.map_or_else(|| link.target.clone(), |c| c.node.clone())
        };
        for (from, links) in &self.links {
            let own = books.get(from.as_str());
            for link in links {
                // A chapter link goes from the chapter (a link in several chapters - from each).
                let mut sources: Vec<String> =
                    own.into_iter().flatten().filter(|c| c.links.contains(link)).map(|c| c.node.clone()).collect();
                if sources.is_empty() {
                    sources.push(from.to_string());
                }
                let to = target_of(link);
                if to == link.target && link.target == from.as_str() {
                    continue;
                }
                if !nodes.contains_key(&to) {
                    let title = self.title(&to);
                    nodes.insert(to.clone(), Node { id: to.clone(), kind: None, title, chapter: None });
                }
                for source in sources.into_iter().filter(|s| *s != to) {
                    edges.entry((source, to.clone())).or_insert((0, false)).0 += 1;
                }
            }
        }
        Graph {
            nodes: nodes.into_values().collect(),
            edges: edges.into_iter().map(|((from, to), (count, chapter))| Edge { from, to, count, chapter }).collect(),
        }
    }
}

/// A book chapter on the graph.
#[derive(Debug)]
struct Chapter {
    /// Node: `<book>/.<number>`.
    node: String,
    title: String,
    anchor: String,
    /// Section number of the chapter heading in the book outline.
    section: usize,
    /// Links of all sections of the chapter.
    links: Vec<LinkRef>,
}

/// Book chapters: first-level sections, with the links of their subsections.
fn book_chapters(book: &str, outline: &Outline) -> Vec<Chapter> {
    let ids = crate::search::section_ids(outline);
    let mut out: Vec<Chapter> = Vec::new();
    for (i, (s, id)) in outline.sections.iter().zip(ids).enumerate() {
        match (s.level, id) {
            (1, Some(anchor)) => out.push(Chapter {
                node: format!("{book}/.{}", out.len() + 1),
                title: s.heading.clone().unwrap_or_default(),
                anchor,
                section: i,
                links: s.links.clone(),
            }),
            _ => {
                if let Some(c) = out.last_mut().filter(|_| s.level != 1) {
                    for link in &s.links {
                        if !c.links.contains(link) {
                            c.links.push(link.clone());
                        }
                    }
                }
            }
        }
    }
    out
}

/// Prefix of titles in vault data: `/_vault/title/<path>`.
pub const TITLE_PREFIX: &str = "title";

/// Provider of `/_vault/title/<path>`: the note title ([`Snapshot::title`])
/// as text, shown by `#see("path")` without its own caption. An unwritten
/// note gives the last path segment (`notes check` shows the broken link,
/// the build does not fail). The fingerprint is by content: a note is
/// rebuilt only when the title of the target changes.
#[derive(Debug)]
pub struct TitleData {
    /// The vault.
    pub vault: Vault,
    /// Its source index.
    pub index: Arc<SourceIndex>,
}

impl crate::vault_data::DataProvider for TitleData {
    fn read(&self, path: &str) -> std::result::Result<Vec<u8>, String> {
        let snapshot = self.index.snapshot(&self.vault).map_err(|e| e.to_string())?;
        Ok(snapshot.title(path).into_bytes())
    }
}

/// All `see("...", anchor: "...")` with literal strings, no repeats.
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
        assert!(snap.backlinks(&NoteId::new("A").unwrap()).is_empty(), "a link to itself is not a backlink");

        let g = snap.graph();
        let nodes: Vec<_> = g.nodes.iter().map(|n| (n.id.as_str(), n.kind)).collect();
        assert_eq!(
            nodes,
            [("A", Some(NoteKind::Note)), ("B", Some(NoteKind::Note)), ("Книга", Some(NoteKind::Book)), ("Нет", None)]
        );
        let edges: Vec<_> = g.edges.iter().map(|e| (e.from.as_str(), e.to.as_str(), e.count)).collect();
        assert_eq!(edges, [("A", "B", 2), ("A", "Нет", 1), ("Книга", "B", 1)]);

        // A file edit shows without a restart; a deleted file leaves the cache.
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
        assert_eq!(first, again, "no changes: the previous scan");

        mem.write("C.typ", r#"#see("B")"#);
        assert_eq!(index.snapshot(&vault).unwrap().backlinks(&b).len(), 2, "a change: a new scan");

        // Without the watcher: always a scan.
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
        assert_eq!(snap.outgoing(&a), [link("C", None), link("B", None)], "computed after literal");
        assert_eq!(snap.backlinks(&NoteId::new("B").unwrap()).len(), 1);
        assert_eq!(snap.graph().edges.len(), 2);
    }

    #[test]
    fn titles_of_notes_and_folders() {
        use crate::storage::MemStorage;
        let note = |title: &str| format!("#show: note.with(title: [{title}])");
        let mem = Arc::new(MemStorage::new());
        mem.write("Сеть/ssh.typ", note("SSH: основы"));
        mem.write("Сеть/без-названия.typ", "");
        mem.write("Сеть/_folder.toml", "title = \"Сети и протоколы\"");
        mem.write("Курсы/Глубже/Матан/main.typ", "#show: book.with(title: [Кратные интегралы])");
        mem.write("Курсы/Глубже/Матан/_folder.toml", "title = \"не читается: у книги название в main.typ\"");
        mem.write("Курсы/_folder.toml", "title = \"Учёба\"");
        mem.write("Курсы/Глубже/_folder.toml", "titel = \"опечатка\"");
        let vault = Vault::new(mem.clone());
        let index = SourceIndex::default();
        let snap = index.snapshot(&vault).unwrap();

        assert_eq!(snap.title("Сеть/ssh"), "SSH: основы");
        assert_eq!(snap.title("Сеть/без-названия"), "без-названия", "no title: the file name");
        assert_eq!(snap.title("Курсы/Глубже/Матан"), "Кратные интегралы");
        assert_eq!(snap.title("Нет/такой"), "такой", "unwritten: the last segment");
        assert_eq!(snap.title("../x"), "x", "an invalid path too");

        assert_eq!(snap.folder_title("Сеть"), "Сети и протоколы");
        assert_eq!(snap.folder_title("Курсы"), "Учёба", "a folder with subfolders only");
        assert_eq!(snap.folder_title("Курсы/Глубже"), "Глубже", "an error in the file: the folder name");
        let folders: Vec<&str> = snap.folders().map(|(p, _)| p).collect();
        assert_eq!(folders, ["Курсы", "Курсы/Глубже", "Сеть"], "a book is not a folder");
        let (_, broken) = snap.folders().find(|(p, _)| *p == "Курсы/Глубже").unwrap();
        assert!(broken.error.as_deref().is_some_and(|e| e.contains("titel")), "{broken:?}");

        let graph = snap.graph();
        let ssh = graph.nodes.iter().find(|n| n.id == "Сеть/ssh").unwrap();
        assert_eq!(ssh.title, "SSH: основы", "the graph labels by title");

        // An edit and a removal of `_folder.toml` show on the next scan.
        mem.write("Сеть/_folder.toml", "title = \"Сеть\"");
        mem.remove("Курсы/_folder.toml");
        let snap = index.snapshot(&vault).unwrap();
        assert_eq!(snap.folder_title("Сеть"), "Сеть");
        assert_eq!(snap.folder_title("Курсы"), "Курсы");
    }
}
