//! Проверка хранилища: ошибки компиляции, битые ссылки (с якорями) и
//! ошибки в файлах папок `_folder.toml`.
//!
//! Obsidian не проверял якоря `[[Заметка#Заголовок]]` — здесь ссылка
//! считается целой, только если есть и заметка, и раздел в ней.

use serde::Serialize;

use crate::diag::Diagnostic;
use crate::figures::FigureOptions;
use crate::notes::Notes;
use crate::render::{LinkRef, slug};
use crate::vault::NoteId;
use crate::{Error, Result};

#[derive(Debug, Serialize)]
pub struct Report {
    pub notes: Vec<NoteReport>,
    /// Папки с ошибкой в `_folder.toml`.
    pub folders: Vec<FolderProblem>,
}

/// Ошибка в файле папки: папка показывается своим именем.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FolderProblem {
    /// Файл от корня хранилища: `Сеть/_folder.toml`.
    pub file: String,
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct NoteReport {
    pub id: NoteId,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
    pub broken_links: Vec<BrokenLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokenLink {
    pub target: String,
    pub anchor: Option<String>,
    pub reason: String,
}

impl Report {
    /// Нет ошибок компиляции, битых ссылок и ошибок папок (предупреждения
    /// допустимы).
    pub fn is_clean(&self) -> bool {
        self.folders.is_empty() && self.notes.iter().all(|n| n.errors.is_empty() && n.broken_links.is_empty())
    }

    /// Итог одной строкой: `заметок: 19, ошибок: 1, предупреждений: 2, битых ссылок: 3`
    /// (ошибки папок — в числе ошибок). Его печатает `notes check`;
    /// ожидаемый итог фикстуры — в `tests/vault/README.md`.
    pub fn summary(&self) -> String {
        let count = |f: fn(&NoteReport) -> usize| self.notes.iter().map(f).sum::<usize>();
        format!(
            "заметок: {}, ошибок: {}, предупреждений: {}, битых ссылок: {}",
            self.notes.len(),
            count(|n| n.errors.len()) + self.folders.len(),
            count(|n| n.warnings.len()),
            count(|n| n.broken_links.len()),
        )
    }
}

/// Проверка всего хранилища.
pub fn check(notes: &Notes) -> Result<Report> {
    let mut out = Vec::new();
    for entry in notes.entries()? {
        out.push(note_report(notes, entry.id)?);
    }
    Ok(Report { notes: out, folders: folder_problems(notes, |_| true)? })
}

/// Проверка одной заметки или книги (ссылки — по всему хранилищу).
pub fn check_note(notes: &Notes, id: &NoteId) -> Result<Report> {
    // Папки на пути к заметке — их названия она показывает в дереве.
    let mine: Vec<&str> = crate::folders::ancestors(id.as_str()).collect();
    Ok(Report { notes: vec![note_report(notes, id.clone())?], folders: folder_problems(notes, |p| mine.contains(&p))? })
}

fn folder_problems(notes: &Notes, wanted: impl Fn(&str) -> bool) -> Result<Vec<FolderProblem>> {
    let index = notes.index()?;
    Ok(index
        .folders()
        .filter(|(path, _)| wanted(path))
        .filter_map(|(path, f)| {
            let error = f.error.clone()?;
            Some(FolderProblem { file: format!("{path}/{}", crate::folders::FOLDER_FILE), error })
        })
        .collect())
}

fn note_report(notes: &Notes, id: NoteId) -> Result<NoteReport> {
    let page = notes.page(&id, FigureOptions::default())?;
    // Собралась — ссылки собранной страницы (с вычисляемыми путями);
    // нет — из индекса исходников (буквальные `#see` и ссылки прошлой
    // удачной сборки): ссылки проверяются и у несобравшейся заметки.
    let links: Vec<LinkRef> = match &page.rendered {
        Some(rendered) if page.errors.is_empty() => rendered.links.clone(),
        _ => notes.index()?.outgoing(&id).to_vec(),
    };
    let mut broken = Vec::new();
    for link in &links {
        if let Some(reason) = link_problem(notes, &link.target, link.anchor.as_deref())? {
            broken.push(BrokenLink { target: link.target.clone(), anchor: link.anchor.clone(), reason });
        }
    }
    Ok(NoteReport { id, errors: page.errors.clone(), warnings: page.warnings.clone(), broken_links: broken })
}

/// Что не так со ссылкой, или `None`, если всё в порядке.
fn link_problem(notes: &Notes, target: &str, anchor: Option<&str>) -> Result<Option<String>> {
    let Ok(id) = NoteId::new(target) else {
        return Ok(Some("недопустимый путь".into()));
    };
    let page = match notes.page(&id, FigureOptions::default()) {
        Ok(page) => page,
        Err(Error::NotFound(_)) => return Ok(Some("нет такой заметки".into())),
        Err(e) => return Err(e),
    };
    let Some(anchor) = anchor else { return Ok(None) };
    let Some(rendered) = &page.rendered else {
        return Ok(Some("заметка-цель не собирается — якорь не проверить".into()));
    };
    let wanted = slug(anchor);
    let found = rendered.headings.iter().any(|h| h.anchor == wanted || h.id == anchor);
    Ok((!found).then(|| format!("в «{target}» нет раздела «{anchor}»")))
}
