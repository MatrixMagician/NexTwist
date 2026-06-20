//! `.zip` extraction with zip-slip + symlink-entry defense.
//!
//! Every entry is routed through [`crate::validate::validate_entry`] before any
//! bytes are written. Symlink entries (detected via the unix mode bits) are
//! rejected — they are the CVE-2025-29787 write-through vector. The `zip` crate
//! is pinned at `>= 8` (the CVE was fixed in 2.3.0), but we still validate every
//! entry explicitly rather than trusting the decoder.

use std::io;
use std::path::Path;

use zip::ZipArchive;

use crate::validate::{validate_entry, ExtractError};

/// Unix mode mask selecting the file-type bits, and the symlink file-type value.
const S_IFMT: u32 = 0o170000;
const S_IFLNK: u32 = 0o120000;

/// Extract `archive` into the (already created) temp directory `temp_root`.
///
/// Each entry is validated before its bytes land. Directory entries create the
/// directory; symlink entries are rejected; regular files are streamed to the
/// validated destination. Returns on the first unsafe entry without writing it.
pub fn extract_zip(archive: &Path, temp_root: &Path) -> Result<(), ExtractError> {
    let file = std::fs::File::open(archive).map_err(|e| ExtractError::io(archive, e))?;
    let mut zip =
        ZipArchive::new(file).map_err(|e| ExtractError::Decode(format!("open zip: {e}")))?;

    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| ExtractError::Decode(format!("read zip entry {i}: {e}")))?;

        // Validate the RAW declared name, not the post-sanitized `enclosed_name()`.
        // `enclosed_name()` silently strips a leading separator / prefix (so an
        // absolute entry becomes relative), which would let an absolute-path entry
        // through as "accepted" rather than rejected. The shared validator must see
        // the entry exactly as the archive declared it so it can reject absolute
        // and escape entries explicitly.
        let raw_name = Path::new(entry.name()).to_path_buf();

        // Detect symlink entries from the unix mode bits so the shared validator
        // can reject them uniformly with the other formats.
        let is_symlink = entry
            .unix_mode()
            .map(|mode| mode & S_IFMT == S_IFLNK)
            .unwrap_or(false);

        if is_symlink {
            return Err(ExtractError::SymlinkEntry(raw_name));
        }

        if entry.is_dir() {
            // A directory entry: validate via a sentinel child so the shared
            // validator (which creates and checks the *parent*) guards the
            // directory path and creates the directory itself under the root.
            let probe = raw_name.join(".nextwist-dir-probe");
            validate_entry(&probe, temp_root, false)?;
            continue;
        }

        // Regular file.
        let dest = validate_entry(&raw_name, temp_root, false)?;
        let mut out =
            std::fs::File::create(&dest).map_err(|e| ExtractError::io(&dest, e))?;
        io::copy(&mut entry, &mut out).map_err(|e| ExtractError::io(&dest, e))?;
    }

    Ok(())
}
