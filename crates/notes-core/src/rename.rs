//! Переименование заметки, книги или папки из интерфейса (решение
//! пользователя): меняются и название, и имя файла (папки); ссылки `#see` на
//! неё в других заметках переписываются — не молча: клиент сначала
//! показывает план ([`RenamePlan`]: новый путь и какие заметки поправятся),
//! а применяет по подтверждению.
//!
//! - **Название** — в самом файле: у заметки и книги — `title: […]` шаблона
//!   (`note.with`, `book.with` в `main.typ`); нет `title` — дописывается,
//!   нет шаблона — название и есть имя файла. У папки — `_folder.toml`
//!   (создаётся, если имя файла не передаёт название).
//! - **Имя файла** — из названия, как у `notes new` ([`file_name`]): занятое
//!   — с номером, само переименовываемое занятым не считается.
//! - **Ссылки** — буквальные `#see("путь")` во всех `.typ` хранилища (и в
//!   главах книг, и в самой заметке): путь заметки (у папки — всё, что в
//!   ней) меняется на новый, якорь и подпись остаются. Вычисляемые пути и
//!   `#import`/`#include` чужих файлов по абсолютному пути не переписываются.
//!
//! Файлы меняются по одному, не транзакцией: сбой посередине оставит часть
//! правок (что успело — видно в журнале сервера).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use typst::syntax::{LinkedNode, SyntaxKind, ast};

use crate::folders::FOLDER_FILE;
use crate::new_note::{file_name, free_id, markup};
use crate::storage::is_typ;
use crate::vault::{BOOK_MAIN, NoteId, NoteKind, Vault};
use crate::{Error, Result};

/// Функция ссылки из `baluk/links.typ`.
const LINK_FN: &str = "see";
/// Шаблоны, у которых название — `title: […]`.
const TEMPLATES: &[&str] = &["note", "book"];

/// Что переименовывают.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum RenameKind {
    /// Заметка или книга (путь — её `id`).
    Note,
    /// Папка хранилища.
    Folder,
}

/// Заметка, в которой перепишутся ссылки.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LinkRewrite {
    pub note: NoteId,
    /// Сколько ссылок в ней.
    pub count: usize,
}

/// Что сделает переименование (или сделало).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RenamePlan {
    pub kind: RenameKind,
    /// Прежний путь.
    pub from: String,
    /// Новый путь: имя — из названия; совпадает с `from` — меняется только название.
    pub to: String,
    /// Новое название (пробелы схлопнуты).
    pub title: String,
    /// Другие заметки со ссылками сюда (у папки — на то, что в ней), по алфавиту.
    pub links: Vec<LinkRewrite>,
}

/// Новое содержимое файла (путь — прежний, до переноса).
struct Edit {
    path: String,
    text: String,
}

/// План переименования и правки файлов.
struct Prepared {
    plan: RenamePlan,
    edits: Vec<Edit>,
    /// Файл `_folder.toml`, которого ещё нет (путь — прежний).
    create: Option<Edit>,
    /// Что переносится: файл заметки или каталог.
    moved: Option<(String, String)>,
}

/// План переименования: ничего не меняет.
pub fn plan(vault: &Vault, kind: RenameKind, from: &NoteId, title: &str) -> Result<RenamePlan> {
    Ok(prepare(vault, kind, from, title)?.plan)
}

/// Переименовать. Результат — план и изменённые пути (прежние и новые) — для
/// наблюдателя.
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
    tracing::info!("переименовано: {} -> {} («{}»)", prepared.plan.from, prepared.plan.to, prepared.plan.title);
    Ok((prepared.plan, changed))
}

fn prepare(vault: &Vault, kind: RenameKind, from: &NoteId, title: &str) -> Result<Prepared> {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let refuse = |reason: &str| Error::Rename { id: from.to_string(), reason: reason.to_owned() };
    if title.is_empty() {
        return Err(refuse("пустое название"));
    }
    let storage = vault.storage();
    let parent = from.parent();
    // Что переносится и где название.
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
                return Err(refuse("это книга: её название — в main.typ"));
            }
            (from.to_string(), "", format!("{from}/{FOLDER_FILE}"))
        }
    };
    let to = match free_id(vault, parent, &file_name(&title), Some(from.name()))? {
        Ok(id) => id,
        Err(reason) => return Err(refuse(&reason)),
    };

    // Ссылки: прежний путь → новый (у папки и книги — и всё, что в ней).
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

    // Название.
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

/// Файл `.typ` → заметка, которой он принадлежит (у главы — книга).
fn owners(vault: &Vault) -> Result<BTreeMap<String, NoteId>> {
    let mut out = BTreeMap::new();
    for entry in vault.entries()? {
        for file in vault.files_of(&entry)? {
            out.insert(file, entry.id.clone());
        }
    }
    Ok(out)
}

/// Переписать пути буквальных `see("путь")`, для которых `retarget` даёт
/// новый. Результат — текст и сколько ссылок переписано.
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

/// Новое название в шаблоне (`note.with`/`book.with`): заменить
/// `title: […]` или дописать первым аргументом. Шаблона нет — `None`.
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
            // `note.with(` — дописать сразу после скобки.
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

/// Первый потомок вида `kind` (в глубину).
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

/// Заменить куски текста (не пересекаются).
fn splice(text: &str, mut spans: Vec<(std::ops::Range<usize>, String)>) -> String {
    spans.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut out = text.to_owned();
    for (range, new) in spans {
        out.replace_range(range, &new);
    }
    out
}

/// Строка Typst в кавычках.
fn typst_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Базовая строка TOML в кавычках.
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
        assert!(mem.read("Сеть/SSH.typ").is_ok(), "план ничего не меняет");

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
        assert_eq!(plan.to, "Сеть/UFW", "своё имя — не занято");
        let plan = plan_ok(&vault, "Сеть/UFW", "ufw");
        assert_eq!(plan.to, "Сеть/ufw", "регистр — тоже переименование");
        apply(&vault, RenameKind::Note, &id("Сеть/UFW"), "ufw").unwrap();
        assert!(text(&mem, "Сеть/ufw.typ").contains("title: [ufw],"));
        // Занято другой — с номером.
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
        assert_eq!(text(&mem, "Одетая.typ"), "Текст без шаблона, #see(\"Сеть/SSH\").", "нет шаблона — только имя");
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
        assert_eq!(notes, ["Голая", "Книга"], "ссылки внутри папки — не чужие");
        apply(&vault, RenameKind::Folder, &id("Сеть"), "Сети: основы").unwrap();
        assert!(text(&mem, "Сети основы/SSH.typ").contains("#see(\"Сети основы/UFW\")"));
        assert!(text(&mem, "Книга/01.typ").contains("#see(\"Сети основы/UFW\") и #see(\"Сети основы\")"));
        let meta = parse_folder(&text(&mem, "Сети основы/_folder.toml")).unwrap();
        assert_eq!(meta.title.as_deref(), Some("Сети: основы"), "название — в _folder.toml");
        assert!(mem.read("Сеть/SSH.typ").is_err());

        // Имя передаёт название — файл не нужен; уже есть — правится.
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
