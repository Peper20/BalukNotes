//! The device and its sync preference: what the core and the device part both
//! need without the settings schema (which is built from the library themes and
//! stays in `notes-core`).

use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

/// The device the defaults of device settings are picked for: what is fine on
/// a computer is not on a phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Desktop,
    Phone,
}

impl Platform {
    /// The build platform.
    pub fn current() -> Self {
        if cfg!(any(target_os = "android", target_os = "ios")) { Self::Phone } else { Self::Desktop }
    }

    /// The default for this platform.
    pub fn pick<T>(self, desktop: T, phone: T) -> T {
        match self {
            Self::Desktop => desktop,
            Self::Phone => phone,
        }
    }
}

/// Who wins when a file changed on this device and on the server (sync, the
/// device setting `device.sync_prefer`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPrefer {
    /// This device's edit (the computer that writes first).
    Local,
    /// The server's version (the devices that receive).
    Remote,
}

impl SyncPrefer {
    /// The default of a platform: a computer writes first, a phone receives.
    pub fn default_for(platform: Platform) -> Self {
        platform.pick(Self::Local, Self::Remote)
    }

    /// Setting value: `local`, `remote`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "local" => Some(Self::Local),
            "remote" => Some(Self::Remote),
            _ => None,
        }
    }
}

/// `device.sync_prefer` from a settings file, without the schema (and so
/// without a core: the `notes sync` commands do not open one). A missing or
/// unreadable file, or a bad value, gives the platform's default.
pub fn stored_sync_prefer(path: &Path, platform: Platform) -> SyncPrefer {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Map<String, Value>>(&bytes).ok())
        .and_then(|values| values.get("device.sync_prefer")?.as_str().and_then(SyncPrefer::from_key))
        .unwrap_or_else(|| SyncPrefer::default_for(platform))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_prefer_reads_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        fs::write(&file, r#"{"device.sync_prefer": "remote"}"#).unwrap();
        assert_eq!(stored_sync_prefer(&file, Platform::Desktop), SyncPrefer::Remote);
        assert_eq!(stored_sync_prefer(&dir.path().join("none.json"), Platform::Desktop), SyncPrefer::Local);
        assert_eq!(stored_sync_prefer(&dir.path().join("none.json"), Platform::Phone), SyncPrefer::Remote);
        fs::write(&file, r#"{"device.sync_prefer": "local"}"#).unwrap();
        assert_eq!(stored_sync_prefer(&file, Platform::Phone), SyncPrefer::Local);
        fs::write(&file, r#"{"device.sync_prefer": "sideways"}"#).unwrap();
        assert_eq!(stored_sync_prefer(&file, Platform::Desktop), SyncPrefer::Local);
    }
}
