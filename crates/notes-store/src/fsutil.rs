//! Small file helpers shared by modules.

use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

/// Writes through a temporary file and a rename: a failure halfway through
/// does not leave a half-written file. Directories on the way appear by
/// themselves.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = Path::new(&tmp);
    fs::write(tmp, data)?;
    fs::rename(tmp, path)
}

/// As [`write_atomic`], and only the user can read the file (secrets: password
/// hashes, sessions). On other systems than Unix the mode is the default one.
pub fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = Path::new(&tmp);
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    io::Write::write_all(&mut options.open(tmp)?, data)?;
    fs::rename(tmp, path)
}

/// What identifies a version of a file on disk: the modification time, the
/// size and (on Unix) the inode - a rename-based write (see [`write_atomic`])
/// always changes the inode. Compared to see that another process changed the
/// file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
    inode: u64,
}

impl FileStamp {
    /// The stamp of the file; `None` if it does not exist.
    ///
    /// # Errors
    /// Any I/O error except "not found".
    pub fn of(path: &Path) -> io::Result<Option<Self>> {
        match fs::metadata(path) {
            Ok(meta) => Ok(Some(Self { modified: meta.modified().ok(), len: meta.len(), inode: inode(&meta) })),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(unix)]
fn inode(meta: &fs::Metadata) -> u64 {
    std::os::unix::fs::MetadataExt::ino(meta)
}

#[cfg(not(unix))]
fn inode(_: &fs::Metadata) -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_whole_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b/file.json");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        let secret = dir.path().join("secret.json");
        write_private(&secret, b"x").unwrap();
        write_private(&secret, b"y").unwrap();
        assert_eq!(fs::read(&secret).unwrap(), b"y");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&secret).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert!(!dir.path().join("secret.json.tmp").exists());
    }

    #[test]
    fn stamps_follow_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.json");
        assert_eq!(FileStamp::of(&path).unwrap(), None);
        write_atomic(&path, b"one").unwrap();
        let first = FileStamp::of(&path).unwrap();
        assert!(first.is_some());
        assert_eq!(FileStamp::of(&path).unwrap(), first, "unchanged file, same stamp");
        // The same size, written again: still a different version.
        write_atomic(&path, b"two").unwrap();
        assert_ne!(FileStamp::of(&path).unwrap(), first);
    }
}
