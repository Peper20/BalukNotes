//! Small file helpers shared by modules.

use std::fs;
use std::path::Path;

use crate::{Error, Result};

/// Writes through a temporary file and a rename: a failure halfway through
/// does not leave a half-written file.
pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = Path::new(&tmp);
    fs::write(tmp, data).map_err(|e| Error::io(tmp, e))?;
    fs::rename(tmp, path).map_err(|e| Error::io(path, e))
}
