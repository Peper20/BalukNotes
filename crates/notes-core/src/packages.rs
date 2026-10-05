//! Typst packages: a note imports only whitelisted packages ([`ALLOWED`],
//! with versions) and those the user allowed on top of it (the device
//! setting `device.packages`). Others are a build error: a package is
//! someone else's code, run on every build of the note (user's decision).
//!
//! The version of a note that read a package (allowed or not) includes the
//! allowed list: vault data `/_vault/packages/policy` ([`PolicyData`]).
//! Allowing a package rebuilds the notes that read packages (with the
//! styling library - all of them: it imports CeTZ), others stay. The list
//! changes rarely.

use std::collections::BTreeSet;
use std::sync::Arc;

use parking_lot::RwLock;
use typst::syntax::package::PackageSpec;

/// Packages notes import without asking: the styling library draws with
/// them (CeTZ), plus what they pull in themselves. Review and extend: good
/// looking notes matter more (`docs/roadmap.md`).
pub const ALLOWED: &[&str] = &["@preview/cetz:0.4.2", "@preview/oxifmt:1.0.0"];

/// Prefix of the vault data.
pub const DATA_PREFIX: &str = "packages";
/// Vault data file with the fingerprint of the list.
pub const POLICY_FILE: &str = "packages/policy";

/// Which packages may be imported: the whitelist and the extra allowed ones.
#[derive(Debug, Default)]
pub struct PackagePolicy {
    extra: RwLock<BTreeSet<String>>,
}

impl PackagePolicy {
    /// Sets the packages allowed on top of the whitelist
    /// (`@namespace/name:version`); `true` if the list changed.
    pub fn set_extra(&self, extra: &[String]) -> bool {
        let next: BTreeSet<String> = extra.iter().cloned().collect();
        let mut current = self.extra.write();
        let changed = *current != next;
        *current = next;
        changed
    }

    /// Whether the package may be imported; if not, the error text for the note.
    pub fn check(&self, spec: &PackageSpec) -> Result<(), String> {
        let name = spec.to_string();
        if ALLOWED.contains(&name.as_str()) || self.extra.read().contains(&name) {
            return Ok(());
        }
        Err(format!(
            "package {name} is not whitelisted ({}); allow it in the setting \"Пакеты Typst сверх белого списка\" \
             (it is someone else's code: only from an author you trust)",
            ALLOWED.join(", ")
        ))
    }

    /// Fingerprint for the note version: the extra allowed packages.
    fn describe(&self) -> String {
        self.extra.read().iter().cloned().collect::<Vec<_>>().join("\n")
    }
}

/// Package list from the setting: `@namespace/name:version` separated by
/// spaces or commas. The error names the malformed package.
pub fn parse_list(text: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for item in text.split([' ', ',', '\n', '\t']).filter(|s| !s.is_empty()) {
        let spec: PackageSpec = item
            .parse()
            .map_err(|e| format!("\"{item}\": {e}; expected @namespace/name:version, e.g. @preview/fletcher:0.5.8"))?;
        let name = spec.to_string();
        if !out.contains(&name) {
            out.push(name);
        }
    }
    Ok(out)
}

/// Provider of `/_vault/packages/policy`, the allowed list: only for the
/// version of notes that read packages.
#[derive(Debug)]
pub struct PolicyData(pub Arc<PackagePolicy>);

impl crate::vault_data::DataProvider for PolicyData {
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        match path {
            "policy" => Ok(self.0.describe().into_bytes()),
            _ => Err(format!("no vault data \"{DATA_PREFIX}/{path}\"")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(s: &str) -> PackageSpec {
        s.parse().unwrap()
    }

    #[test]
    fn whitelist_and_extra() {
        let policy = PackagePolicy::default();
        assert!(policy.check(&spec("@preview/cetz:0.4.2")).is_ok());
        let err = policy.check(&spec("@preview/cetz:0.4.1")).unwrap_err();
        assert!(err.starts_with("package @preview/cetz:0.4.1 is not whitelisted"), "{err}");
        assert!(policy.check(&spec("@preview/fletcher:0.5.8")).is_err());
        assert!(policy.set_extra(&["@preview/fletcher:0.5.8".into()]));
        assert!(!policy.set_extra(&["@preview/fletcher:0.5.8".into()]), "the same list is no change");
        assert!(policy.check(&spec("@preview/fletcher:0.5.8")).is_ok());
        assert!(policy.check(&spec("@preview/fletcher:0.5.7")).is_err(), "the version is exact");
    }

    #[test]
    fn list_from_setting() {
        assert_eq!(
            parse_list(" @preview/fletcher:0.5.8,@local/my:1.0.0  @preview/fletcher:0.5.8").unwrap(),
            ["@preview/fletcher:0.5.8", "@local/my:1.0.0"]
        );
        assert_eq!(parse_list("").unwrap(), Vec::<String>::new());
        assert!(parse_list("fletcher").unwrap_err().starts_with("\"fletcher\""));
        assert!(parse_list("@preview/fletcher").is_err(), "no version");
    }
}
