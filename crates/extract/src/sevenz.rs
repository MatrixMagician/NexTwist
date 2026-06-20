//! `.7z` extraction with per-entry validation via sevenz-rust2's `ArchiveReader`.
//!
//! We use the low-level [`ArchiveReader::for_each_entries`] interposition closure
//! so every entry path is routed through the SAME shared
//! [`crate::validate::validate_entry`] as the zip path BEFORE its bytes are
//! decoded and written. The 7z container has no per-entry unix mode the way zip
//! does, so symlink defense here relies on the path-component + canonicalization
//! checks (a 7z entry cannot express a symlink target the way a zip symlink entry
//! can); traversal/absolute entries are rejected identically to the zip path.

use std::io;
use std::path::Path;

use sevenz_rust2::{ArchiveReader, Password};

use crate::validate::{validate_entry, ExtractError};

/// Extract `archive` (a `.7z`) into the already-created temp dir `temp_root`,
/// validating every entry before writing it.
pub fn extract_7z(archive: &Path, temp_root: &Path) -> Result<(), ExtractError> {
    let mut reader = ArchiveReader::open(archive, Password::empty())
        .map_err(|e| ExtractError::Decode(format!("open 7z: {e}")))?;

    // The closure must return sevenz_rust2::Error to abort iteration, so we stash
    // our richer ExtractError here and surface it after iteration unwinds.
    let mut pending: Option<ExtractError> = None;

    let result = reader.for_each_entries(|entry, rdr| {
        let raw_name = Path::new(entry.name()).to_path_buf();

        if entry.is_directory() {
            // Validate the directory path via a sentinel child so the shared
            // validator creates and bounds-checks the directory under the root.
            let probe = raw_name.join(".nextwist-dir-probe");
            if let Err(e) = validate_entry(&probe, temp_root, false) {
                pending = Some(e);
                return Err(sevenz_rust2::Error::Other("entry rejected by validator".into()));
            }
            return Ok(true);
        }

        // Regular file: validate (7z has no symlink-entry concept here; the path
        // checks still apply), then stream the decoded bytes to the safe dest.
        let dest = match validate_entry(&raw_name, temp_root, false) {
            Ok(d) => d,
            Err(e) => {
                pending = Some(e);
                return Err(sevenz_rust2::Error::Other("entry rejected by validator".into()));
            }
        };
        let mut out = match std::fs::File::create(&dest) {
            Ok(f) => f,
            Err(e) => {
                pending = Some(ExtractError::io(&dest, e));
                return Err(sevenz_rust2::Error::Other("create dest failed".into()));
            }
        };
        if let Err(e) = io::copy(rdr, &mut out) {
            pending = Some(ExtractError::io(&dest, e));
            return Err(sevenz_rust2::Error::Other("write dest failed".into()));
        }
        Ok(true)
    });

    // Our own validation/I/O error takes precedence over the generic sevenz error.
    if let Some(e) = pending {
        return Err(e);
    }
    result.map_err(|e| ExtractError::Decode(format!("7z extraction failed: {e}")))?;
    Ok(())
}
