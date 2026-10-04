//! Renaming a note, a book or a folder from the interface (the user's
//! decision): both the title and the file (folder) name change; `#see` links
//! to it in other notes get rewritten, not silently: the client first shows
//! the plan ([`RenamePlan`]: the new path and which notes get fixed) and
//! applies it on confirmation.
//!
//! - **Title**: in the file itself; for a note or a book, `title: [...]` of
//!   the template (`note.with`, `book.with` in `main.typ`); without `title` it
//!   is added; without a template the title is the file name. For a folder,
//!   `_folder.toml` (created if the file name does not carry the title).
//! - **File name**: from the title, as in `notes new` ([`file_name`]); a taken
//!   name gets a number; the renamed item itself does not count as taken.
//! - **Links**: literal `#see("path")` in every `.typ` of the vault (in book
//!   chapters and in the note itself too): the note path (for a folder,
//!   everything in it) changes to the new one; the anchor and the label stay.
//!   Computed paths and `#import`/`#include` of other files by absolute path
//!   are not rewritten.
//!
//! Files change one by one, not in a transaction: a failure halfway leaves
//! part of the edits (the server log shows what got done).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use typst::syntax::{LinkedNode, SyntaxKind, ast};

use crate::folders::FOLDER_FILE;
use crate::new_note::{file_name, free_id, markup};
use crate::storage::is_typ;
use crate::vault::{BOOK_MAIN, NoteId, NoteKind, Vault};
use crate::{Error, Result};

/// The link function from `baluk/links.typ`.
const LINK_FN: &str = "see";
/// Templates whose title is `title: [...]`.
const TEMPLATES: &[&str] = &["note", "book"];

/// What is renamed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum RenameKind {
    /// A note or a book (the path is its `id`).
    Note,
    /// A vault folder.
    Folder,
}

/// A note whose links get rewritten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LinkRewrite {
    pub note: NoteId,
    /// How many links it has.
    pub count: usize,
}

/// What the rename will do (or did).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RenamePlan {
    pub kind: RenameKind,
    /// The old path.
    pub from: String,
    /// The new path: the name comes from the title; equal to `from` means only the title changes.
    pub to: String,
    /// The new title (whitespace collapsed).
    pub title: String,
    /// Other notes with links here (for a folder, to what is in it), in alphabetical order.
    pub links: Vec<LinkRewrite>,
}

/// New contents of a file (the path is the old one, before the move).
struct Edit {
    path: String,
    text: String,
}

/// The rename plan and the file edits.
struct Prepared {
    plan: RenamePlan,
    edits: Vec<Edit>,
    /// A `_folder.toml` that does not exist yet (the path is the old one).
    create: Option<Edit>,
    /// What moves: the note file or a directory.
    moved: Option<(String, String)>,
}

/// The rename plan: changes nothing.
pub fn plan(vault: &Vault, kind: RenameKind, from: &NoteId, title: &str) -> Result<RenamePlan> {
    Ok(prepare(vault, kind, from, title)?.plan)
}

/// Renames. Returns the plan and the changed paths (old and new) for the
/// watcher.
pub fn apply(vault: &Vault, kind: RenameKind, from: &NoteId, title: &str) -> Result<(RenamePlan, Vec<String>)> {
    let prepared = prepare(vault, kind, from, title)?;
    let storage = vault.storage();
    let mut changed = Vec::new();
    for edit in &prepared.edits {
        storage.rewrite(&edit.path, edit.text.as_bytes()).map_err(|e| vault.io_error(&edit.path, e))?;
        changed.push(edit.path.clone());
    }
    if let Some(edit) = &prepared.create {
        storage.create(&edit.path, edit.text.as_bytes()).map_err(|e| vault.io_error(&edit.path, e))?;
        changed.push(edit.path.clone());
    }
    if let Some((src, dst)) = &prepared.moved {
        storage.rename(src, dst).map_err(|e| vault.io_error(src, e))?;
        changed.extend([src.clone(), dst.clone()]);
    }
    tracing::info!("renamed: {} -> {} (\"{}\")", prepared.plan.from, prepared.plan.to, prepared.plan.title);
    Ok((prepared.plan, changed))
}

fn prepare(vault: &Vault, kind: RenameKind, from: &NoteId, title: &str) -> Result<Prepared> {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let refuse = |reason: &str| Error::Rename { id: from.to_string(), reason: reason.to_owned() };
    if title.is_empty() {
        return Err(refuse("empty title"));
    }
    let storage = vault.storage();
    let parent = from.parent();
    // What moves and where the title is.
    let (moved_from, moved_ext, title_file) = match kind {
        RenameKind::Note => {
            let entry = vault.entry(from)?;
            let main = entry.main.to_string_lossy().replace('\\', "/");
            match entry.kind {
                NoteKind::Note => (format!("{from}.typ"), ".typ", main),
                NoteKind::Book => (from.to_string(), "", main),
            }
        }
        RenameKind::Folder => {
            if !storage.stat(from.as_str()).is_ok_and(|m| m.is_dir) {
                return Err(Error::NotFound(from.to_string()));
            }
            if storage.stat(&format!("{from}/{BOOK_MAIN}")).is_ok() {
                return Err(refuse("this is a book: its title is in main.typ"));
            }
            (from.to_string(), "", format!("{from}/{FOLDER_FILE}"))
        }
    };
    let to = match free_id(vault, parent, &file_name(&title), Some(from.name()))? {
        Ok(id) => id,
        Err(reason) => return Err(refuse(&reason)),
    };

    // Links: the old path -> the new one (for a folder or a book, everything in it too).
    let retarget = |target: &str| -> Option<String> {
        if target == from.as_str() {
            return Some(to.to_string());
        }
        let rest = target.strip_prefix(from.as_str())?.strip_prefix('/')?;
        (kind == RenameKind::Folder).then(|| format!("{to}/{rest}"))
    };
    let owner = owners(vault)?;
    let inside = |path: &str| path == moved_from || path.starts_with(&format!("{moved_from}/"));
    let files = storage.list().map_err(|e| vault.io_error("", e))?;
    let mut texts: BTreeMap<String, String> = BTreeMap::new();
    let mut counts: BTreeMap<NoteId, usize> = BTreeMap::new();
    for file in files.iter().filter(|f| is_typ(f)) {
        let text = vault.read_text(file)?;
        let (new, count) = rewrite_links(&text, &retarget);
        if count == 0 {
            continue;
        }
        if !inside(file)
            && let Some(note) = owner.get(file)
        {
            *counts.entry(note.clone()).or_default() += count;
        }
        texts.insert(file.clone(), new);
    }

    // The title.
    let mut create = None;
    match kind {
        RenameKind::Note => {
            let text = match texts.get(&title_file) {
                Some(text) => text.clone(),
                None => vault.read_text(&title_file)?,
            };
            if let Some(new) = set_title(&text, &title) {
                texts.insert(title_file, new);
            }
        }
        RenameKind::Folder => {
            let toml = format!("title = {}\n", toml_string(&title));
            if storage.stat(&title_file).is_ok() {
                texts.insert(title_file, toml);
            } else if to.name() != title {
                create = Some(Edit { path: title_file, text: toml });
            }
        }
    }

    let moved_to = format!("{to}{moved_ext}");
    let plan = RenamePlan {
        kind,
        from: from.to_string(),
        to: to.to_string(),
        title,
        links: counts.into_iter().map(|(note, count)| LinkRewrite { note, count }).collect(),
    };
    let edits = texts.into_iter().map(|(path, text)| Edit { path, text }).collect();
    Ok(Prepared { plan, edits, create, moved: (moved_from != moved_to).then_some((moved_from, moved_to)) })
}

/// A `.typ` file -> the note it belongs to (a book for a chapter).
fn owners(vault: &Vault) -> Result<BTreeMap<String, NoteId>> {
    let mut out = BTreeMap::new();
    for entry in vault.entries()? {
        for file in vault.files_of(&entry)? {
            out.insert(file, entry.id.clone());
        }
    }
    Ok(out)
}

/// Rewrites the paths of literal `see("path")` for which `retarget` gives a
/// new one. Returns the text and how many links were rewritten.
fn rewrite_links(text: &str, retarget: &dyn Fn(&str) -> Option<String>) -> (String, usize) {
    let root = typst::syntax::parse(text);
    let mut spans = Vec::new();
    collect_links(&LinkedNode::new(&root), retarget, &mut spans);
    let count = spans.len();
    (splice(text, spans), count)
}

fn collect_links(
    node: &LinkedNode,
    retarget: &dyn Fn(&str) -> Option<String>,
    out: &mut Vec<(std::ops::Range<usize>, String)>,
) {
    if let Some(call) = node.cast::<ast::FuncCall>()
        && let ast::Expr::Ident(name) = call.callee()
        && name.as_str() == LINK_FN
        && let Some(args) = node.children().find(|c| c.kind() == SyntaxKind::Args)
        && let Some(target) = args.children().find(|c| c.kind() == SyntaxKind::Str)
        && let Some(s) = target.cast::<ast::Str>()
        && let Some(new) = retarget(&s.get())
    {
        out.push((target.range(), typst_string(&new)));
    }
    for child in node.children() {
        collect_links(&child, retarget, out);
    }
}

/// A new title in the template (`note.with`/`book.with`): replaces
/// `title: [...]` or adds it as the first argument. `None` without a template.
fn set_title(text: &str, title: &str) -> Option<String> {
    let root = typst::syntax::parse(text);
    let block = format!("[{}]", markup(title));
    let mut found = None;
    find_template(&LinkedNode::new(&root), &block, &mut found);
    found.map(|span| splice(text, vec![span]))
}

fn find_template(node: &LinkedNode, block: &str, out: &mut Option<(std::ops::Range<usize>, String)>) {
    if out.is_some() {
        return;
    }
    if let Some(rule) = node.cast::<ast::ShowRule>()
        && let ast::Expr::FuncCall(call) = rule.transform()
        && let ast::Expr::FieldAccess(access) = call.callee()
        && let ast::Expr::Ident(name) = access.target()
        && TEMPLATES.contains(&name.as_str())
        && access.field().as_str() == "with"
        && let Some(args) = find_kind(node, SyntaxKind::Args)
    {
        let title = args.children().find(|c| c.cast::<ast::Named>().is_some_and(|n| n.name().as_str() == "title"));
        *out = Some(if let Some(value) = title.and_then(|named| named.children().next_back()) {
            (value.range(), block.to_owned())
        } else {
            // `note.with(`: add it right after the parenthesis.
            let at =
                args.children().find(|c| c.kind() == SyntaxKind::LeftParen).map_or(args.offset(), |p| p.range().end);
            (at..at, format!("title: {block}, "))
        });
        return;
    }
    for child in node.children() {
        find_template(&child, block, out);
    }
}

/// The first descendant of kind `kind` (depth first).
fn find_kind<'a>(node: &LinkedNode<'a>, kind: SyntaxKind) -> Option<LinkedNode<'a>> {
    for child in node.children() {
        if child.kind() == kind {
            return Some(child);
        }
        if let Some(found) = find_kind(&child, kind) {
            return Some(found);
        }
    }
    None
}

/// Replaces pieces of text (they do not overlap).
fn splice(text: &str, mut spans: Vec<(std::ops::Range<usize>, String)>) -> String {
    spans.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut out = text.to_owned();
    for (range, new) in spans {
        out.replace_range(range, &new);
    }
    out
}

/// A quoted Typst string.
fn typst_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// A quoted TOML basic string.
fn toml_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::folders::parse_folder;
    use crate::storage::{MemStorage, Storage};

    fn id(s: &str) -> NoteId {
        NoteId::new(s).unwrap()
    }

    fn text(mem: &MemStorage, path: &str) -> String {
        String::from_utf8(mem.read(path).unwrap()).unwrap()
    }

    const HEAD: &str = "#import \"/_baluk/lib.typ\": *\n";

    fn setup() -> (Arc<MemStorage>, Vault) {
        let mem = Arc::new(MemStorage::new());
        mem.write(
            "Сеть/SSH.typ",
            format!("{HEAD}#show: note.with(title: [SSH], tags: (\"сеть\",))\nСм. #see(\"Сеть/UFW\").\n"),
        );
        mem.write(
            "Сеть/UFW.typ",
            format!("{HEAD}#show: note.with(\n  title: [UFW],\n)\n#see(\"Сеть/SSH\", anchor: \"Порт\")[порт], #see(\"Сеть/SSH\")\n"),
        );
        mem.write("Книга/main.typ", format!("{HEAD}#show: book.with(title: [Книга])\n#include \"01.typ\"\n"));
        mem.write("Книга/01.typ", "= Глава\nСм. #see(\"Сеть/UFW\") и #see(\"Сеть\").\n");
        mem.write("Голая.typ", "Текст без шаблона, #see(\"Сеть/SSH\").");
        let vault = Vault::new(mem.clone());
        (mem, vault)
    }

    #[test]
    fn note_title_file_and_links() {
        let (mem, vault) = setup();
        let plan = plan(&vault, RenameKind::Note, &id("Сеть/SSH"), "  SSH:  основы ").unwrap();
        assert_eq!((plan.to.as_str(), plan.title.as_str()), ("Сеть/SSH основы", "SSH: основы"));
        let notes: Vec<(&str, usize)> = plan.links.iter().map(|l| (l.note.as_str(), l.count)).collect();
        assert_eq!(notes, [("Голая", 1), ("Сеть/UFW", 2)]);
        assert!(mem.read("Сеть/SSH.typ").is_ok(), "the plan changes nothing");

        apply(&vault, RenameKind::Note, &id("Сеть/SSH"), "SSH: основы").unwrap();
        assert!(mem.read("Сеть/SSH.typ").is_err());
        let ssh = text(&mem, "Сеть/SSH основы.typ");
        assert!(ssh.contains("note.with(title: [SSH: основы], tags: (\"сеть\",))"), "{ssh}");
        let ufw = text(&mem, "Сеть/UFW.typ");
        assert!(ufw.contains("#see(\"Сеть/SSH основы\", anchor: \"Порт\")[порт], #see(\"Сеть/SSH основы\")"), "{ufw}");
        assert!(text(&mem, "Голая.typ").contains("#see(\"Сеть/SSH основы\")"));
    }

    #[test]
    fn same_name_only_title_and_case() {
        let (mem, vault) = setup();
        let plan = plan(&vault, RenameKind::Note, &id("Сеть/UFW"), "UFW").unwrap();
        assert_eq!(plan.to, "Сеть/UFW", "its own name is not taken");
        let plan = plan_ok(&vault, "Сеть/UFW", "ufw");
        assert_eq!(plan.to, "Сеть/ufw", "a case change is a rename too");
        apply(&vault, RenameKind::Note, &id("Сеть/UFW"), "ufw").unwrap();
        assert!(text(&mem, "Сеть/ufw.typ").contains("title: [ufw],"));
        // Taken by another note: with a number.
        let plan = plan_ok(&vault, "Сеть/ufw", "SSH");
        assert_eq!(plan.to, "Сеть/SSH 2");
    }

    fn plan_ok(vault: &Vault, from: &str, title: &str) -> RenamePlan {
        plan(vault, RenameKind::Note, &id(from), title).unwrap()
    }

    #[test]
    fn note_without_template_and_title_arg() {
        let (mem, vault) = setup();
        apply(&vault, RenameKind::Note, &id("Голая"), "Одетая").unwrap();
        assert_eq!(text(&mem, "Одетая.typ"), "Текст без шаблона, #see(\"Сеть/SSH\").", "no template, only the name");
        mem.write("Без названия.typ", format!("{HEAD}#show: note.with(tags: (\"x\",))\n"));
        apply(&vault, RenameKind::Note, &id("Без названия"), "Есть [1]").unwrap();
        assert!(text(&mem, "Есть [1].typ").contains("note.with(title: [Есть \\[1\\]], tags: (\"x\",))"));
    }

    #[test]
    fn book_moves_as_folder() {
        let (mem, vault) = setup();
        mem.write("Сеть/SSH.typ", "#see(\"Книга\", anchor: \"Глава\")");
        let (plan, _) = apply(&vault, RenameKind::Note, &id("Книга"), "Том 1").unwrap();
        assert_eq!(plan.to, "Том 1");
        assert!(text(&mem, "Том 1/main.typ").contains("book.with(title: [Том 1])"));
        assert!(text(&mem, "Том 1/01.typ").contains("= Глава"));
        assert_eq!(text(&mem, "Сеть/SSH.typ"), "#see(\"Том 1\", anchor: \"Глава\")");
    }

    #[test]
    fn folder_moves_everything_inside() {
        let (mem, vault) = setup();
        let plan = plan(&vault, RenameKind::Folder, &id("Сеть"), "Сети: основы").unwrap();
        assert_eq!(plan.to, "Сети основы");
        let notes: Vec<&str> = plan.links.iter().map(|l| l.note.as_str()).collect();
        assert_eq!(notes, ["Голая", "Книга"], "links inside the folder are not foreign");
        apply(&vault, RenameKind::Folder, &id("Сеть"), "Сети: основы").unwrap();
        assert!(text(&mem, "Сети основы/SSH.typ").contains("#see(\"Сети основы/UFW\")"));
        assert!(text(&mem, "Книга/01.typ").contains("#see(\"Сети основы/UFW\") и #see(\"Сети основы\")"));
        let meta = parse_folder(&text(&mem, "Сети основы/_folder.toml")).unwrap();
        assert_eq!(meta.title.as_deref(), Some("Сети: основы"), "the title is in _folder.toml");
        assert!(mem.read("Сеть/SSH.typ").is_err());

        // A name that carries the title needs no file; an existing one is edited.
        mem.write("Папка/a.typ", "");
        apply(&vault, RenameKind::Folder, &id("Папка"), "Другая").unwrap();
        assert!(mem.read("Другая/_folder.toml").is_err());
        apply(&vault, RenameKind::Folder, &id("Сети основы"), "Сеть \"2\"").unwrap();
        let meta = parse_folder(&text(&mem, "Сеть 2/_folder.toml")).unwrap();
        assert_eq!(meta.title.as_deref(), Some("Сеть \"2\""));
    }

    #[test]
    fn refusals() {
        let (_, vault) = setup();
        assert!(matches!(plan(&vault, RenameKind::Note, &id("Нет"), "x"), Err(Error::NotFound(_))));
        assert!(matches!(plan(&vault, RenameKind::Note, &id("Сеть/SSH"), "  "), Err(Error::Rename { .. })));
        assert!(matches!(plan(&vault, RenameKind::Folder, &id("Книга"), "x"), Err(Error::Rename { .. })));
        assert!(matches!(plan(&vault, RenameKind::Folder, &id("Сеть/SSH"), "x"), Err(Error::NotFound(_))));
    }
}
