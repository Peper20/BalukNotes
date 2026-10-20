//! Research prototype E4 (`docs/research/E4.md`): the core in WASM.
//!
//! One thread, no disk: the host (a Web Worker) hands the vault files and the
//! Typst package files over as bytes, the core keeps them in memory
//! ([`MemStorage`]) and builds a note on request. [`Core`] is plain Rust (its
//! tests run natively); the JS binding is [`wasm::WasmCore`], compiled only
//! for `wasm32`.

use std::path::PathBuf;
use std::sync::Arc;

use notes_core::figures::FigureOptions;
use notes_core::{LibrarySource, Notes, NotesConfig, SharedAssets};
use notes_store::note_id::NoteId;
use notes_store::storage::MemStorage;

/// The core over a vault in memory.
#[derive(Debug)]
pub struct Core {
    vault: Arc<MemStorage>,
    packages: Arc<MemStorage>,
    notes: Option<Notes>,
}

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}

impl Core {
    /// An empty vault and no packages.
    pub fn new() -> Self {
        Self { vault: Arc::new(MemStorage::new()), packages: Arc::new(MemStorage::new()), notes: None }
    }

    /// Adds or replaces a vault file (`Сеть/SSH.typ`).
    pub fn add_file(&self, path: &str, data: &[u8]) {
        self.vault.write(path, data);
    }

    /// Adds a Typst package file (`preview/cetz/0.4.2/src/lib.typ`).
    pub fn add_package_file(&self, path: &str, data: &[u8]) {
        self.packages.write(path, data);
    }

    /// Opens the vault: loads the embedded fonts and builds the themes. Once;
    /// files added later are seen by the next build.
    pub fn open(&mut self) -> Result<(), String> {
        if self.notes.is_some() {
            return Ok(());
        }
        let config = NotesConfig {
            vault: PathBuf::from("memory"),
            library: LibrarySource::Embedded,
            font_dirs: Vec::new(),
            cache: None,
            trash: None,
        };
        let shared = SharedAssets { packages: Some(self.packages.clone()), ..SharedAssets::default() };
        let notes = Notes::with_storage_shared(self.vault.clone(), &config, &shared).map_err(|e| e.to_string())?;
        self.notes = Some(notes);
        Ok(())
    }

    /// The note page as the server answers it (`NotePage` in JSON): built
    /// in every theme, one after another.
    pub fn page(&mut self, note: &str) -> Result<String, String> {
        self.open()?;
        let notes = self.notes.as_ref().ok_or("the vault is not open")?;
        let id = NoteId::new(note).map_err(|e| e.to_string())?;
        let page = notes.page(&id, FigureOptions::default()).map_err(|e| e.to_string())?;
        serde_json::to_string(&*page).map_err(|e| e.to_string())
    }

    /// The notes and books of the vault (their paths), sorted.
    pub fn notes(&mut self) -> Result<Vec<String>, String> {
        self.open()?;
        let notes = self.notes.as_ref().ok_or("the vault is not open")?;
        let mut ids: Vec<String> =
            notes.entries().map_err(|e| e.to_string())?.into_iter().map(|e| e.id.as_str().to_owned()).collect();
        ids.sort();
        Ok(ids)
    }

    /// Forgets what Typst memoized (there is no idle thread to do it here).
    pub fn release_memory(&self) {
        if let Some(notes) = &self.notes {
            notes.release_memory();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(core: &mut Core, note: &str) -> serde_json::Value {
        serde_json::from_str(&core.page(note).unwrap()).unwrap()
    }

    /// A vault and packages from memory: a note builds in every theme, a
    /// package comes from the package files, a missing one is a build error
    /// of the note, an unknown note is an error.
    #[test]
    fn builds_from_memory() {
        let mut core = Core::new();
        core.add_file("Привет.typ", "= Привет\n\nТекст $x^2$.".as_bytes());
        core.add_file("Пакет.typ", "#import \"@preview/oxifmt:1.0.0\": answer\nОтвет: #answer".as_bytes());
        core.add_file("Нет пакета.typ", br#"#import "@preview/cetz:0.4.2""#);
        core.add_package_file(
            "preview/oxifmt/1.0.0/typst.toml",
            b"[package]\nname = \"oxifmt\"\nversion = \"1.0.0\"\nentrypoint = \"lib.typ\"\n",
        );
        core.add_package_file("preview/oxifmt/1.0.0/lib.typ", b"#let answer = 42");
        assert_eq!(core.notes().unwrap(), ["Нет пакета", "Пакет", "Привет"]);

        let hello = page(&mut core, "Привет");
        assert!(hello["rendered"]["body"].as_str().unwrap().contains("Привет"), "{hello}");
        assert_eq!(hello["errors"], serde_json::json!([]));

        let package = page(&mut core, "Пакет");
        assert!(package["rendered"]["body"].as_str().unwrap().contains("42"), "{package}");

        let missing = page(&mut core, "Нет пакета");
        assert!(missing["rendered"].is_null());
        assert!(missing["errors"].to_string().contains("cetz"), "{missing}");

        assert!(core.page("Нет такой").is_err());
        core.release_memory();
    }
}

/// The JS binding.
#[cfg(target_family = "wasm")]
pub mod wasm {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = console, js_name = error)]
        fn console_error(message: &str);
    }

    /// A Rust panic aborts the module (wasm32 has no unwinding): its message
    /// goes to the console before that.
    #[wasm_bindgen(start)]
    pub fn start() {
        std::panic::set_hook(Box::new(|info| console_error(&info.to_string())));
    }

    /// The core for JS.
    #[wasm_bindgen]
    #[derive(Debug, Default)]
    pub struct WasmCore(super::Core);

    #[wasm_bindgen]
    impl WasmCore {
        #[wasm_bindgen(constructor)]
        pub fn new() -> Self {
            Self::default()
        }

        #[wasm_bindgen(js_name = addFile)]
        pub fn add_file(&self, path: &str, data: &[u8]) {
            self.0.add_file(path, data);
        }

        #[wasm_bindgen(js_name = addPackageFile)]
        pub fn add_package_file(&self, path: &str, data: &[u8]) {
            self.0.add_package_file(path, data);
        }

        pub fn open(&mut self) -> Result<(), JsError> {
            self.0.open().map_err(|e| JsError::new(&e))
        }

        /// `NotePage` in JSON.
        pub fn page(&mut self, note: &str) -> Result<String, JsError> {
            self.0.page(note).map_err(|e| JsError::new(&e))
        }

        pub fn notes(&mut self) -> Result<Vec<String>, JsError> {
            self.0.notes().map_err(|e| JsError::new(&e))
        }

        #[wasm_bindgen(js_name = releaseMemory)]
        pub fn release_memory(&self) {
            self.0.release_memory();
        }
    }
}
