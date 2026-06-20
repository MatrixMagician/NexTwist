//! `.rar` extraction via a system `unrar`/`7z` binary (never bundled, never shell).
//!
//! Implemented in Task 2. Task 1 establishes the shared validator and the zip
//! path; this stub keeps the crate compiling and is replaced with a real
//! `std::process::Command` extractor (archive path as an argv element) in the
//! next task.

use std::path::Path;

use crate::validate::ExtractError;

/// Extract `archive` (a `.rar`) into `temp_root` via a system tool.
pub fn extract_rar(_archive: &Path, _temp_root: &Path) -> Result<(), ExtractError> {
    // TODO(Task 2): detect `unrar` then `7z` on PATH; spawn via
    // std::process::Command with the archive path + output dir as separate argv
    // elements (no shell string); re-validate the extracted tree; return
    // RarToolMissing when neither tool is present.
    Err(ExtractError::RarToolMissing)
}
