//! The config file and the data directory: one rule for every part of the app
//! (`notes-typst`, `notes-hub`, ...), so that all of them find the same
//! directory.
//!
//! The data directory is, in this order: the explicit one (`--data`,
//! `NOTES_DATA`), `data = "..."` of the config file
//! (`~/.config/baluk-notes/config.toml`), `~/.local/share/baluk-notes`. In the
//! config file `~/` is the home directory and a relative path is relative to
//! the file; a key other than `data` is an error.
//!
//! Well-known files of the data directory: [`users_file`], [`sessions_file`],
//! [`hub_dir`].

use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The app's directory name: `~/.config/<APP>`, `~/.local/share/<APP>`.
pub const APP: &str = "baluk-notes";

/// Errors of the config module.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("config {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("user data directory not found: pass --data")]
    NoDataDir,
}

/// The result of the config module.
pub type Result<T> = std::result::Result<T, Error>;

/// The config file (`~/.config/baluk-notes/config.toml`).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Data directory; `~/` is home, a relative path is relative to the config file.
    pub data: Option<PathBuf>,
}

/// Where the config file is (it may not exist).
#[must_use]
pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join(APP).join("config.toml"))
}

/// The config file of the user; no file means an empty config.
///
/// # Errors
/// The file exists but cannot be read or is not a config.
pub fn load() -> Result<Config> {
    match config_path() {
        Some(path) => load_from(&path),
        None => Ok(Config::default()),
    }
}

/// The config file at `path`; no file means an empty config.
///
/// # Errors
/// The file exists but cannot be read or is not a config.
pub fn load_from(path: &Path) -> Result<Config> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(source) => return Err(Error::Io { path: path.to_owned(), source }),
    };
    let mut config: Config = toml::from_str(&text).map_err(|source| Error::Parse { path: path.to_owned(), source })?;
    if let Some(data) = config.data.take() {
        config.data = Some(match data.strip_prefix("~") {
            Ok(rest) => dirs::home_dir().unwrap_or_default().join(rest),
            Err(_) => path.parent().unwrap_or(Path::new("")).join(data),
        });
    }
    Ok(config)
}

/// The data directory: explicit, from the config file, or the standard one.
///
/// # Errors
/// The config file is broken, or there is no standard directory and no other
/// choice.
pub fn data_dir(explicit: Option<&Path>) -> Result<PathBuf> {
    resolve(explicit, &load()?, dirs::data_dir())
}

/// [`data_dir`] with the inputs given (`standard`: the user's data directory
/// the app directory goes into).
///
/// # Errors
/// [`Error::NoDataDir`]: nothing is explicit or configured and there is no
/// standard directory.
pub fn resolve(explicit: Option<&Path>, config: &Config, standard: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(dir) = explicit.or(config.data.as_deref()) {
        return Ok(dir.to_owned());
    }
    standard.map(|dir| dir.join(APP)).ok_or(Error::NoDataDir)
}

/// The users of the storage server (`notes_store::accounts`).
#[must_use]
pub fn users_file(data: &Path) -> PathBuf {
    data.join("users.json")
}

/// The sign-in sessions (`notes_store::sessions`).
#[must_use]
pub fn sessions_file(data: &Path) -> PathBuf {
    data.join("sessions.json")
}

/// The vaults of the storage server: `<hub>/<login>/<vault>/`.
#[must_use]
pub fn hub_dir(data: &Path) -> PathBuf {
    data.join("hub")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_file(text: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conf/config.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        (dir, path)
    }

    #[test]
    fn no_file_is_an_empty_config() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_from(&dir.path().join("none.toml")).unwrap();
        assert!(config.data.is_none());
    }

    #[test]
    fn data_paths_in_the_file() {
        let (dir, path) = config_file("data = \"notes-data\"\n");
        assert_eq!(load_from(&path).unwrap().data, Some(dir.path().join("conf/notes-data")));
        let (_dir, path) = config_file("data = \"/srv/notes\"\n");
        assert_eq!(load_from(&path).unwrap().data, Some(PathBuf::from("/srv/notes")));
        let (_dir, path) = config_file("data = \"~/notes-data\"\n");
        let home = dirs::home_dir().unwrap_or_default();
        assert_eq!(load_from(&path).unwrap().data, Some(home.join("notes-data")));
        let (_dir, path) = config_file("");
        assert!(load_from(&path).unwrap().data.is_none());
    }

    #[test]
    fn broken_files_are_errors() {
        let (_dir, path) = config_file("data = \"x\"\nother = 1\n");
        let e = load_from(&path).unwrap_err();
        assert!(matches!(e, Error::Parse { .. }), "{e}");
        assert!(e.to_string().contains("config.toml") && e.to_string().contains("other"), "{e}");
        let (_dir, path) = config_file("data = [");
        assert!(matches!(load_from(&path), Err(Error::Parse { .. })));
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(load_from(dir.path()), Err(Error::Io { .. })), "a directory is not a config");
    }

    #[test]
    fn data_dir_order() {
        let (_dir, path) = config_file("data = \"/from/config\"\n");
        let config = load_from(&path).unwrap();
        let standard = Some(PathBuf::from("/home/u/.local/share"));
        let explicit = Path::new("/from/flag");
        assert_eq!(resolve(Some(explicit), &config, standard.clone()).unwrap(), explicit);
        assert_eq!(resolve(None, &config, standard.clone()).unwrap(), Path::new("/from/config"));
        assert_eq!(resolve(None, &Config::default(), standard).unwrap(), Path::new("/home/u/.local/share/baluk-notes"));
        assert!(matches!(resolve(None, &Config::default(), None), Err(Error::NoDataDir)));
        assert_eq!(resolve(Some(explicit), &Config::default(), None).unwrap(), explicit);
    }

    #[test]
    fn well_known_files() {
        let data = Path::new("/d");
        assert_eq!(users_file(data), Path::new("/d/users.json"));
        assert_eq!(sessions_file(data), Path::new("/d/sessions.json"));
        assert_eq!(hub_dir(data), Path::new("/d/hub"));
    }
}
