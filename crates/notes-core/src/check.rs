//! Проверка хранилища: ошибки компиляции и битые ссылки (с якорями).
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
    /// Нет ошибок компиляции и битых ссылок (предупреждения допустимы).
    pub fn is_clean(&self) -> bool {
        self.notes.iter().all(|n| n.errors.is_empty() && n.broken_links.is_empty())
    }

    /// Итог одной строкой: `заметок: 19, ошибок: 1, предупреждений: 2, битых ссылок: 3`.
    /// Его печатает `notes check`; ожидаемый итог фикстуры — в `tests/vault/README.md`.
    pub fn summary(&self) -> String {
        let count = |f: fn(&NoteReport) -> usize| self.notes.iter().map(f).sum::<usize>();
        format!(
            "заметок: {}, ошибок: {}, предупреждений: {}, битых ссылок: {}",
            self.notes.len(),
            count(|n| n.errors.len()),
            count(|n| n.warnings.len()),
            count(|n| n.broken_links.len()),
        )
    }
}

pub fn check(notes: &Notes) -> Result<Report> {
    let mut out = Vec::new();
    for entry in notes.entries()? {
        let page = notes.page(&entry.id, FigureOptions::default())?;
        // Собралась — ссылки собранной страницы (с вычисляемыми путями);
        // нет — из индекса исходников (буквальные `#see` и ссылки прошлой
        // удачной сборки): ссылки проверяются и у несобравшейся заметки.
        let links: Vec<LinkRef> = match &page.rendered {
            Some(rendered) if page.errors.is_empty() => rendered.links.clone(),
            _ => notes.index()?.outgoing(&entry.id).to_vec(),
        };
        let mut broken = Vec::new();
        for link in &links {
            if let Some(reason) = link_problem(notes, &link.target, link.anchor.as_deref())? {
                broken.push(BrokenLink { target: link.target.clone(), anchor: link.anchor.clone(), reason });
            }
        }
        out.push(NoteReport {
            id: entry.id,
            errors: page.errors.clone(),
            warnings: page.warnings.clone(),
            broken_links: broken,
        });
    }
    Ok(Report { notes: out })
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
