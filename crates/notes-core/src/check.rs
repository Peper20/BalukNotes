//! Vault check: compile errors, broken links (with anchors) and errors in
//! the folder files `_folder.toml`.
//!
//! Obsidian did not check anchors `[[Note#Heading]]`; here a link is whole
//! only if both the note and the section in it exist.

use serde::Serialize;

use crate::diag::Diagnostic;
use crate::figures::FigureOptions;
use crate::notes::Notes;
use crate::render::{LinkRef, slug};
use crate::vault::NoteId;
use crate::{Error, Result};

/// Result of a check: notes and folders with problems.
#[derive(Debug, Serialize)]
pub struct Report {
    pub notes: Vec<NoteReport>,
    /// Folders with an error in `_folder.toml`.
    pub folders: Vec<FolderProblem>,
}

/// An error in a folder file: the folder is shown by its own name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FolderProblem {
    /// File from the vault root: `Сеть/_folder.toml`.
    pub file: String,
    pub error: String,
}

/// Problems of one note.
#[derive(Debug, Serialize)]
pub struct NoteReport {
    pub id: NoteId,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
    pub broken_links: Vec<BrokenLink>,
}

/// A link to a missing note or section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrokenLink {
    pub target: String,
    pub anchor: Option<String>,
    pub reason: String,
}

impl Report {
    /// No compile errors, broken links or folder errors (warnings are
    /// allowed).
    pub fn is_clean(&self) -> bool {
        self.folders.is_empty() && self.notes.iter().all(|n| n.errors.is_empty() && n.broken_links.is_empty())
    }

    /// The total in one line: `notes: 19, errors: 1, warnings: 2, broken links: 3`
    /// (folder errors count as errors). `notes check` prints it; the
    /// expected total of the fixture is in `tests/vault/README.md`.
    pub fn summary(&self) -> String {
        let count = |f: fn(&NoteReport) -> usize| self.notes.iter().map(f).sum::<usize>();
        format!(
            "notes: {}, errors: {}, warnings: {}, broken links: {}",
            self.notes.len(),
            count(|n| n.errors.len()) + self.folders.len(),
            count(|n| n.warnings.len()),
            count(|n| n.broken_links.len()),
        )
    }
}

/// Checks the whole vault.
pub fn check(notes: &Notes) -> Result<Report> {
    let reports = notes.entries()?.into_iter().map(|entry| note_report(notes, entry.id)).collect::<Result<_>>()?;
    Ok(Report { notes: reports, folders: folder_problems(notes, |_| true)? })
}

/// Checks one note or book (links against the whole vault).
pub fn check_note(notes: &Notes, id: &NoteId) -> Result<Report> {
    // Folders on the way to the note: the tree shows their names for it.
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
    // Built: the links of the built page (with computed paths); not built:
    // from the source index (literal `#see` and the links of the last
    // successful build), so a failed note gets its links checked too.
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

/// What is wrong with the link, or `None` if it is fine.
fn link_problem(notes: &Notes, target: &str, anchor: Option<&str>) -> Result<Option<String>> {
    let Ok(id) = NoteId::new(target) else {
        return Ok(Some("invalid path".into()));
    };
    let page = match notes.page(&id, FigureOptions::default()) {
        Ok(page) => page,
        Err(Error::NotFound(_)) => return Ok(Some("no such note".into())),
        Err(e) => return Err(e),
    };
    let Some(anchor) = anchor else { return Ok(None) };
    let Some(rendered) = &page.rendered else {
        return Ok(Some("the target note does not build, the anchor cannot be checked".into()));
    };
    let wanted = slug(anchor);
    let found = rendered.headings.iter().any(|h| h.anchor == wanted || h.id == anchor);
    Ok((!found).then(|| format!("\"{target}\" has no section \"{anchor}\"")))
}
