//! Заметки хранилища: список, страница (книга — по главам), версия, ссылки,
//! превью, PDF, удаление и подсказки прогреву.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use notes_core::NoteId;
use notes_core::book::{Select, chapter_page};
use notes_core::search::{Preview, tagged_chapters};

use crate::AppState;
use crate::api::{
    ErrorResponse, FolderListItem, LinksResponse, NoteListItem, OutgoingLink, VersionResponse, WarmRequest,
};
use crate::error::{ApiResult, blocking};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/vaults/{vault}/notes", get(list_notes))
        .route("/api/vaults/{vault}/folders", get(list_folders))
        .route("/api/vaults/{vault}/notes/{*id}", get(note).delete(delete))
        .route("/api/vaults/{vault}/version/{*id}", get(version))
        .route("/api/vaults/{vault}/links/{*id}", get(links))
        .route("/api/vaults/{vault}/preview/{*id}", get(preview))
        .route("/api/vaults/{vault}/pdf/{*id}", get(pdf))
        .route("/api/vaults/{vault}/warm", post(warm))
}

async fn list_notes(State(s): State<AppState>, Path(vault): Path<String>) -> ApiResult<Json<Vec<NoteListItem>>> {
    let notes = s.vault(vault).await?.notes.clone();
    let list = blocking(move || {
        let index = notes.index()?;
        Ok(index
            .outlines()
            .map(|(e, o)| NoteListItem {
                id: e.id.clone(),
                kind: e.kind,
                name: e.id.name().to_owned(),
                folder: e.id.parent().to_owned(),
                title: o.title.clone().unwrap_or_else(|| e.id.name().to_owned()),
                tags: o.tags.clone(),
                chapters: tagged_chapters(o),
            })
            .collect())
    })
    .await?;
    Ok(Json(list))
}

async fn list_folders(State(s): State<AppState>, Path(vault): Path<String>) -> ApiResult<Json<Vec<FolderListItem>>> {
    let notes = s.vault(vault).await?.notes.clone();
    let list = blocking(move || {
        let index = notes.index()?;
        Ok(index
            .folders()
            .map(|(path, _)| FolderListItem { path: path.to_owned(), title: index.folder_title(path) })
            .collect())
    })
    .await?;
    Ok(Json(list))
}

#[derive(Debug, serde::Deserialize)]
struct PreviewQuery {
    anchor: Option<String>,
}

async fn preview(
    State(s): State<AppState>,
    Path((vault, id)): Path<(String, String)>,
    Query(q): Query<PreviewQuery>,
) -> ApiResult<Json<Preview>> {
    let id = NoteId::new(id)?;
    let notes = s.vault(vault).await?.notes.clone();
    Ok(Json(blocking(move || notes.preview(&id, q.anchor.as_deref())).await?))
}

#[derive(Debug, serde::Deserialize)]
struct NoteQuery {
    chapter: Option<usize>,
    anchor: Option<String>,
}

/// Заметка. С `chapter` или `anchor` книга приходит одной главой (с
/// оглавлением книги в `book`); не книга — целиком, как без них.
async fn note(
    State(s): State<AppState>,
    Path((vault, id)): Path<(String, String)>,
    Query(q): Query<NoteQuery>,
) -> ApiResult<Response> {
    let id = NoteId::new(id)?;
    let vault = s.vault(vault).await?;
    let (notes, opts) = (vault.notes.clone(), vault.figure_options(&s.settings));
    let by_chapter = q.chapter.is_some() || q.anchor.is_some();
    let page = blocking(move || {
        let page = notes.page(&id, opts)?;
        let select = Select { chapter: q.chapter, anchor: q.anchor.as_deref() };
        Ok(if by_chapter { chapter_page(&page, select).map(Arc::new) } else { None }.unwrap_or(page))
    })
    .await?;
    Ok(Json(page).into_response())
}

/// Удалить заметку или книгу (папку целиком) — в корзину.
async fn delete(State(s): State<AppState>, Path((vault, id)): Path<(String, String)>) -> ApiResult<StatusCode> {
    let id = NoteId::new(id)?;
    let notes = s.vault(vault).await?.notes.clone();
    blocking(move || notes.delete(&id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn version(
    State(s): State<AppState>,
    Path((vault, id)): Path<(String, String)>,
) -> ApiResult<Json<VersionResponse>> {
    let id = NoteId::new(id)?;
    let vault = s.vault(vault).await?;
    let (notes, opts) = (vault.notes.clone(), vault.figure_options(&s.settings));
    let version = blocking(move || notes.version(&id, opts)).await?;
    Ok(Json(VersionResponse { version }))
}

async fn links(State(s): State<AppState>, Path((vault, id)): Path<(String, String)>) -> ApiResult<Json<LinksResponse>> {
    let id = NoteId::new(id)?;
    let notes = s.vault(vault).await?.notes.clone();
    blocking(move || {
        notes.vault().entry(&id)?;
        let snap = notes.index()?;
        let outgoing = snap
            .outgoing(&id)
            .iter()
            .map(|l| OutgoingLink {
                target: l.target.clone(),
                anchor: l.anchor.clone(),
                exists: snap.exists(&l.target),
            })
            .collect();
        Ok(Json(LinksResponse { outgoing, backlinks: snap.backlinks(&id) }))
    })
    .await
}

#[derive(Debug, serde::Deserialize)]
struct PdfQuery {
    theme: Option<String>,
}

async fn pdf(
    State(s): State<AppState>,
    Path((vault, id)): Path<(String, String)>,
    Query(q): Query<PdfQuery>,
) -> ApiResult<Response> {
    let id = NoteId::new(id)?;
    let notes = s.vault(vault).await?.notes.clone();
    let theme = q.theme.unwrap_or_else(|| notes.themes().names().first().cloned().unwrap_or_default());
    let name = id.name().to_owned();
    let result = blocking(move || notes.pdf(&id, &theme)).await?;
    Ok(match result {
        Ok(bytes) => {
            // Имя файла в заголовке — по RFC 5987 (кириллица).
            let disposition = format!("inline; filename*=UTF-8''{}.pdf", percent(&name));
            ([(header::CONTENT_TYPE, "application/pdf".to_owned()), (header::CONTENT_DISPOSITION, disposition)], bytes)
                .into_response()
        }
        Err(errors) => (StatusCode::UNPROCESSABLE_ENTITY, Json(ErrorResponse { error: "не собралось".into(), errors }))
            .into_response(),
    })
}

/// Процентное кодирование всего, кроме букв, цифр и `-._~`.
fn percent(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// Подсказка прогреву; хранилище становится активным (прогревается оно).
async fn warm(
    State(s): State<AppState>,
    Path(vault): Path<String>,
    Json(req): Json<WarmRequest>,
) -> ApiResult<StatusCode> {
    let vault = s.vault(vault).await?;
    s.vaults.activate(&vault.name);
    vault.notes.hint_warm(req.ids.iter().filter_map(|id| NoteId::new(id).ok()).collect());
    Ok(StatusCode::NO_CONTENT)
}
