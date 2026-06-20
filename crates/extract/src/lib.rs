//! `nextwist-extract` — the untrusted-archive → validated staging-tree transform.
//!
//! This crate is a pure transform: it takes the bytes of a user-supplied local mod
//! archive (`.zip` / `.7z` / `.rar`) and produces a validated, read-only per-mod
//! staging tree. It has NO knowledge of Steam, deployment, or Tauri — it depends
//! only on `core` types.
//!
//! ## Security model (the Phase 1 centerpiece)
//!
//! Untrusted third-party archive content is the entire threat surface of this
//! phase. Every archive entry is routed through the single shared
//! [`validate::validate_entry`] before its bytes are written:
//!
//! * symlink entries are rejected outright (CVE-2025-29787 write-through vector);
//! * absolute paths and parent-directory escape components are rejected;
//! * the canonicalized destination parent is asserted to stay under the
//!   extraction root.
//!
//! Extraction always happens into a temporary directory; only after the whole
//! archive validates is the tree moved into the staging root and marked
//! read-only. `.rar` is handled exclusively by spawning a system `unrar`/`7z`
//! binary (archive path passed as an argv element — never a shell string), so no
//! non-free RAR code is ever bundled.

use std::path::{Path, PathBuf};

pub mod rar;
pub mod sevenz;
pub mod staging;
pub mod validate;
pub mod zip;

pub use staging::{install_archive, StagedMod};
pub use validate::{validate_entry, ExtractError};

/// The archive formats NexTwist can install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    /// PKZIP `.zip`.
    Zip,
    /// 7-Zip `.7z`.
    SevenZip,
    /// RAR `.rar` (handled via a system tool only).
    Rar,
}

impl ArchiveFormat {
    /// Detect the archive format from the file extension and leading magic bytes.
    ///
    /// Extension is used as a hint; the magic bytes are authoritative when they
    /// match a known signature, so a mislabeled archive is handled by content.
    pub fn detect(path: &Path) -> Result<ArchiveFormat, ExtractError> {
        if let Some(fmt) = magic_format(path)? {
            return Ok(fmt);
        }
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("zip") => Ok(ArchiveFormat::Zip),
            Some("7z") => Ok(ArchiveFormat::SevenZip),
            Some("rar") => Ok(ArchiveFormat::Rar),
            other => Err(ExtractError::UnsupportedFormat(
                other.unwrap_or("<none>").to_string(),
            )),
        }
    }
}

/// Sniff the leading bytes of `path` for a known archive signature.
fn magic_format(path: &Path) -> Result<Option<ArchiveFormat>, ExtractError> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| ExtractError::io(path, e))?;
    let mut head = [0u8; 8];
    let n = f.read(&mut head).map_err(|e| ExtractError::io(path, e))?;
    let head = &head[..n];
    // PK\x03\x04 (local file header) / PK\x05\x06 (empty) / PK\x07\x08 (spanned).
    if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") || head.starts_with(b"PK\x07\x08") {
        return Ok(Some(ArchiveFormat::Zip));
    }
    // 7z signature: 37 7A BC AF 27 1C.
    if head.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return Ok(Some(ArchiveFormat::SevenZip));
    }
    // RAR 4.x "Rar!\x1A\x07\x00" and RAR 5.x "Rar!\x1A\x07\x01\x00".
    if head.starts_with(b"Rar!\x1A\x07") {
        return Ok(Some(ArchiveFormat::Rar));
    }
    Ok(None)
}

/// Recursively mark every regular file under `root` read-only.
///
/// This preserves the staging-integrity safety invariant (the deploy engine relies
/// on staged files being immutable so a deployed hardlink can't be mutated through
/// the staging copy). Directories are left writable so the tree can be traversed
/// and, if needed, removed during purge bookkeeping.
pub(crate) fn mark_tree_readonly(root: &Path) -> Result<(), ExtractError> {
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|e| {
            ExtractError::Decode(format!("walking staged tree failed: {e}"))
        })?;
        if entry.file_type().is_file() {
            let p = entry.path();
            let mut perms = std::fs::metadata(p)
                .map_err(|e| ExtractError::io(p, e))?
                .permissions();
            perms.set_readonly(true);
            std::fs::set_permissions(p, perms).map_err(|e| ExtractError::io(p, e))?;
        }
    }
    Ok(())
}

/// List every regular file under `root`, returned as paths relative to `root`.
pub(crate) fn list_files_rel(root: &Path) -> Result<Vec<PathBuf>, ExtractError> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|e| {
            ExtractError::Decode(format!("walking staged tree failed: {e}"))
        })?;
        if entry.file_type().is_file() {
            let rel = entry
                .path()
                .strip_prefix(root)
                .map_err(|e| ExtractError::Decode(format!("strip_prefix failed: {e}")))?
                .to_path_buf();
            files.push(rel);
        }
    }
    files.sort();
    Ok(files)
}
