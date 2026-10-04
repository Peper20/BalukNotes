//! API responses built by the server itself (the rest are core types).
//!
//! With the `ts` feature they are all exported to TypeScript for the `app/` client:
//! `cd app && npm run types`.

use notes_core::diag::Diagnostic;
use notes_core::graph::Backlink;
use notes_core::search::TaggedChapter;
use notes_core::settings::Schema;
use notes_core::{NoteId, NoteKind, VaultName};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// `GET /api/vaults`, the answer to `POST /api/vaults`. There is no default
/// vault: the client (the last opened one) or the user decides which to open.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct VaultsResponse {
    /// All vaults in alphabetical order (there may be none).
    pub vaults: Vec<VaultName>,
    /// Whether a new one can be created (not when the server runs on one
    /// vault: `notes serve --vault <path>`).
    pub can_create: bool,
}

/// `POST /api/vaults`: a new empty vault.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CreateVaultRequest {
    pub name: String,
}

/// `PATCH /api/vaults/{vault}`: the new vault name.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RenameVaultRequest {
    pub name: String,
}

/// An item of `GET /api/vaults/{vault}/notes`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct NoteListItem {
    pub id: NoteId,
    pub kind: NoteKind,
    /// The last path segment, the file name: `SSH` for `Network/SSH`.
    pub name: String,
    /// The folder: `Network`; an empty string for the root.
    pub folder: String,
    /// The display title: from the template (`title: [...]`), otherwise the file name.
    pub title: String,
    /// The note tags; for a book, those of its root (`main.typ`), inherited by every chapter.
    pub tags: Vec<String>,
    /// The book chapters with their own tags.
    pub chapters: Vec<TaggedChapter>,
}

/// An item of `GET /api/vaults/{vault}/folders`: a folder with notes or an
/// empty one (`Vault::folders`).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FolderListItem {
    /// The path from the root: `Network/Linux`.
    pub path: String,
    /// The display title: from `_folder.toml`, otherwise the folder name.
    pub title: String,
}

/// `POST /api/vaults/{vault}/rename`: rename a note (a book) or a folder;
/// without `apply`, only the plan (the new path, which links get rewritten).
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RenameRequest {
    pub kind: notes_core::rename::RenameKind,
    /// The note or folder path.
    pub id: String,
    /// The new title.
    pub title: String,
    #[serde(default)]
    pub apply: bool,
}

/// `POST /api/vaults/{vault}/warm`: what to build ahead first (notes in tabs,
/// recent ones). Unknown and invalid paths are skipped.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct WarmRequest {
    pub ids: Vec<String>,
}

/// `GET /api/vaults/{vault}/version/{id}`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct VersionResponse {
    pub version: String,
}

/// A link from a note and whether its target exists.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct OutgoingLink {
    pub target: String,
    pub anchor: Option<String>,
    pub exists: bool,
}

/// `GET /api/vaults/{vault}/links/{id}`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LinksResponse {
    pub outgoing: Vec<OutgoingLink>,
    pub backlinks: Vec<Backlink>,
}

/// `GET /api/settings`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SettingsResponse {
    pub schema: Schema,
    /// Key -> a number, a string or a flag (the type is in the schema).
    #[cfg_attr(feature = "ts", ts(type = "Record<string, number | string | boolean>"))]
    pub values: Map<String, Value>,
}

/// `GET`/`PUT /api/vaults/{vault}/settings`: settings for the vault - the
/// result (`values`) and where it comes from: shared by all (`shared`) and set
/// only in this vault (`own`).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct VaultSettingsResponse {
    pub schema: Schema,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, number | string | boolean>"))]
    pub values: Map<String, Value>,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, number | string | boolean>"))]
    pub shared: Map<String, Value>,
    #[cfg_attr(feature = "ts", ts(type = "Record<string, number | string | boolean>"))]
    pub own: Map<String, Value>,
}

/// An error body. `errors` are compile errors (for a PDF), otherwise empty.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ErrorResponse {
    pub error: String,
    pub errors: Vec<Diagnostic>,
}

/// A change of vault files (batched, see `notes_core::watch`): time for the
/// client to check the note version.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChangeEvent {
    /// Grows with every change.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub seq: u64,
    /// The changed vault files; empty means unknown (check everything).
    pub paths: Vec<String>,
}

/// The answer to `GET /api/vaults/{vault}/events?after=<seq>` (long polling, see `events`).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct EventsResponse {
    /// The server watches the files: waiting for changes makes sense. If not,
    /// changes come only on the button.
    pub watching: bool,
    /// The number of the last change: `after` of the next request.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub seq: u64,
    /// Changes after `after`; empty if there were none (or no `after` in the request).
    pub changes: Vec<ChangeEvent>,
}
