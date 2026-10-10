//! Small file helpers shared by modules.

use std::path::Path;

use crate::{Error, Result};

/// Writes through a temporary file and a rename: a failure halfway through
/// does not leave a half-written file. The work is
/// [`notes_store::fsutil::write_atomic`], here only the core error type.
pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    notes_store::fsutil::write_atomic(path, data).map_err(|e| Error::io(path, e))
}
