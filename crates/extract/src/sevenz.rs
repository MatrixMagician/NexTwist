//! `.7z` extraction with per-entry validation via sevenz-rust2.
//!
//! Implemented in Task 2. Task 1 establishes the shared validator and the zip
//! path; this stub keeps the crate compiling and is replaced with a real
//! `ArchiveReader`-based, per-entry-validated extractor in the next task.

use std::path::Path;

use crate::validate::ExtractError;

/// Extract `archive` (a `.7z`) into `temp_root`, validating every entry.
pub fn extract_7z(_archive: &Path, _temp_root: &Path) -> Result<(), ExtractError> {
    // TODO(Task 2): implement via sevenz-rust2 ArchiveReader with per-entry
    // validate_entry() before decoding each entry's bytes.
    Err(ExtractError::UnsupportedFormat(
        "7z extraction not yet implemented (Task 2)".to_string(),
    ))
}
