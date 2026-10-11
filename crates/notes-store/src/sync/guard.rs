//! The guard against mass deletion (architecture §9): a round that would
//! delete an unusually large part of the vault, on the server or on this
//! device, stops before it deletes anything and asks the caller.
//!
//! A deletion is a change like any other, so emptying a folder by hand (a slip
//! of `rm -rf`, an unmounted disk, a folder moved away) would delete the files
//! everywhere. The guard only decides; the engine ([`super::engine`]) calls it
//! with the paths its plan would delete and stops with
//! [`super::Error::DeletionsHeld`].

use std::fmt;

use serde::{Deserialize, Serialize};

use super::protocol::hash_hex;

/// At least this many files must go (and more than [`MIN_SHARE_PERCENT`] of the
/// vault) for the guard to hold a round.
pub const MIN_FILES: usize = 10;

/// The share of the vault, in percent, that must be exceeded.
pub const MIN_SHARE_PERCENT: usize = 20;

/// The smallest vault in which deleting every file is held: emptying 1 file
/// of 1 is an ordinary edit.
const MIN_EMPTIED: usize = 2;

/// What a round does with deletions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Deletions {
    /// Hold a round that deletes too much ([`Held`]).
    #[default]
    Guard,
    /// Let through the held set with this fingerprint ([`Held::fingerprint`]);
    /// any other set that is too large is held again.
    Confirmed(String),
    /// Do not delete on the server what is missing here: download it again
    /// from the server instead.
    Restore,
}

/// Where the held deletions would happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    /// The files are gone from this device, and the round would delete them
    /// on the server.
    Server,
    /// The files are gone from the server, and the round would remove them
    /// from this device.
    Device,
}

/// A set of deletions the guard holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Held {
    pub side: Side,
    /// How many files would be deleted.
    pub count: usize,
    /// How many files the vault has (as of the last round).
    pub total: usize,
    /// Identifies this very set of files; a confirmation names it.
    pub fingerprint: String,
}

impl fmt::Display for Held {
    /// `120 of 130 files are gone from this device and would be deleted on the server`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (count, total) = (self.count, self.total);
        match self.side {
            Side::Server => {
                write!(f, "{count} of {total} files are gone from this device and would be deleted on the server")
            }
            Side::Device => {
                write!(f, "{count} of {total} files are gone from the server and would be removed from this device")
            }
        }
    }
}

/// Is deleting `count` of `total` files too much.
#[must_use]
pub fn is_mass(count: usize, total: usize) -> bool {
    let emptied = count == total && total >= MIN_EMPTIED;
    emptied || (count > MIN_FILES && count * 100 > total * MIN_SHARE_PERCENT)
}

/// Checks the deletions on `side`, paths in any order. `None`: let them
/// through.
#[must_use]
pub fn check(mode: &Deletions, side: Side, paths: &[&str], total: usize) -> Option<Held> {
    if !is_mass(paths.len(), total) {
        return None;
    }
    let held = Held { side, count: paths.len(), total, fingerprint: fingerprint(side, paths) };
    match mode {
        Deletions::Confirmed(fingerprint) if *fingerprint == held.fingerprint => None,
        _ => Some(held),
    }
}

fn fingerprint(side: Side, paths: &[&str]) -> String {
    let mut sorted = paths.to_vec();
    sorted.sort_unstable();
    let prefix = match side {
        Side::Server => "server",
        Side::Device => "device",
    };
    let mut text = prefix.to_owned();
    for path in sorted {
        text.push('\n');
        text.push_str(path);
    }
    hash_hex(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold() {
        // A few notes of a small vault, or a sliver of a big one: ordinary.
        assert!(!is_mass(3, 5));
        assert!(!is_mass(10, 12));
        assert!(!is_mass(20, 2000));
        assert!(!is_mass(1, 1), "the only file");
        // Many files and a large share.
        assert!(is_mass(11, 20));
        assert!(is_mass(401, 2000));
        assert!(!is_mass(400, 2000), "exactly 20 % is not over it");
        // Everything, in a vault of more than one file.
        assert!(is_mass(2, 2));
        assert!(is_mass(5, 5));
    }

    #[test]
    fn confirmation_names_the_set() {
        let paths: Vec<String> = (0..12).map(|n| format!("n{n}.typ")).collect();
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        let held = check(&Deletions::Guard, Side::Server, &refs, 20).unwrap();
        assert_eq!((held.count, held.total), (12, 20));
        // The order does not matter, the side and the files do.
        let mut reversed = refs.clone();
        reversed.reverse();
        assert_eq!(check(&Deletions::Guard, Side::Server, &reversed, 20).unwrap().fingerprint, held.fingerprint);
        assert_ne!(check(&Deletions::Guard, Side::Device, &refs, 20).unwrap().fingerprint, held.fingerprint);
        let confirmed = Deletions::Confirmed(held.fingerprint.clone());
        assert!(check(&confirmed, Side::Server, &refs, 20).is_none());
        // One more file is another set.
        let mut more = refs.clone();
        more.push("extra.typ");
        assert!(check(&confirmed, Side::Server, &more, 21).is_some());
        // Restore does not let the deletions of the other side through.
        assert!(check(&Deletions::Restore, Side::Device, &refs, 20).is_some());
        // A small set needs no confirmation.
        assert!(check(&Deletions::Guard, Side::Server, &refs[..3], 20).is_none());
    }
}
