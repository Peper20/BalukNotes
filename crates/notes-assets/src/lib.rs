//! The built client of `app/` (`npm run build` -> `app/dist`): the shell page
//! and the `assets/` files for the server (`notes-server`).
//!
//! A debug build reads the files from disk (rebuild the client, reload the
//! page); a release build embeds them (it cannot be built without `app/dist`,
//! see `build.rs`).

use std::borrow::Cow;

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../app/dist/"]
#[allow_missing = true]
struct Dist;

/// A file of the built client.
#[derive(Debug, Clone)]
pub struct File {
    pub data: Cow<'static, [u8]>,
    /// MIME type by extension.
    pub mime: String,
}

fn get(path: &str) -> Option<File> {
    Dist::get(path).map(|f| File { mime: f.metadata.mimetype().to_owned(), data: f.data })
}

/// The client's shell page (`index.html`); `None` if the client is not built.
pub fn index_html() -> Option<File> {
    get("index.html")
}

/// The file `assets/{path}` (`baluk.css`, `index-XXXXXXXX.js`, ...).
pub fn asset(path: &str) -> Option<File> {
    get(&format!("assets/{path}"))
}
