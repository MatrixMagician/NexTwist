//! The single shared per-entry path validator.
//!
//! THE Phase 1 security centerpiece. Every format handler (zip, 7z, system-rar
//! output) routes each archive entry through [`validate_entry`] BEFORE its bytes
//! are written. The threat surface is untrusted third-party archive content, so
//! this is the one code path that must hold:
//!
//! * reject any entry flagged as a symlink (the CVE-2025-29787 write-through
//!   vector) — symlink entries are never created and never followed;
//! * reject absolute entry paths and any entry whose name carries a
//!   parent-directory escape component;
//! * after joining the (validated) relative name to the extraction root, create
//!   the parent and re-canonicalize it, asserting it still resides under the
//!   canonicalized root (belt-and-braces defense in depth).
//!
//! Handlers MUST NOT implement their own ad-hoc checks: a single audited path is
//! the only way to guarantee the invariant holds uniformly across formats.

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

/// Errors raised while extracting an untrusted archive into staging.
#[derive(Debug, Error)]
pub enum ExtractError {
    /// An entry's path was unsafe (absolute, escapes the extraction root, or
    /// canonicalized outside the root). The string describes the reason.
    #[error("unsafe archive entry: {0}")]
    UnsafeEntry(String),

    /// An entry was flagged as a symbolic link. Such entries are rejected
    /// outright — they are the archive symlink write-through attack vector.
    #[error("symlink entry rejected: {0}")]
    SymlinkEntry(PathBuf),

    /// A `.rar` archive was supplied but neither a system `unrar` nor `7z`
    /// binary is available on `PATH`.
    #[error(
        "cannot extract .rar: no system 'unrar' or '7z' binary found on PATH. \
         Install one (e.g. 'sudo dnf install p7zip p7zip-plugins' or your distro's \
         'unrar' package) and retry. NexTwist does not bundle non-free RAR code."
    )]
    RarToolMissing,

    /// The supplied archive's format is not supported.
    #[error("unsupported archive format: {0}")]
    UnsupportedFormat(String),

    /// A system rar/7z extraction tool exited unsuccessfully.
    #[error("system archive tool '{tool}' failed: {message}")]
    ToolFailed {
        /// The tool that was invoked.
        tool: String,
        /// Captured stderr / failure detail.
        message: String,
    },

    /// An I/O error occurred while reading the archive or writing staging.
    #[error("i/o error for {path}: {source}")]
    Io {
        /// Path involved.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The archive's own decoder reported a failure.
    #[error("archive decode error: {0}")]
    Decode(String),
}

impl ExtractError {
    /// Convenience constructor for [`ExtractError::Io`].
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        ExtractError::Io {
            path: path.into(),
            source,
        }
    }
}

/// Validate one archive entry and return the safe absolute destination path
/// under `root`.
///
/// `name` is the raw entry path as carried in the archive. `root` is the
/// extraction root (a freshly created temp directory). `is_symlink` is the
/// format handler's determination of whether this entry is a symbolic link
/// (e.g. from the zip unix-mode bits).
///
/// On success the parent directory of the returned path has been created and
/// re-canonicalized under `root`. On failure NOTHING is written and an
/// [`ExtractError`] describing the rejection is returned.
pub fn validate_entry(
    name: &Path,
    root: &Path,
    is_symlink: bool,
) -> Result<PathBuf, ExtractError> {
    // 1. A symlink entry is never acceptable, regardless of where it points.
    if is_symlink {
        return Err(ExtractError::SymlinkEntry(name.to_path_buf()));
    }

    // 2. Obtain a safe relative path: reject absolute names and any escape
    //    component. We inspect components directly rather than string-matching
    //    so platform separators and odd encodings cannot slip past.
    let safe_rel = safe_relative(name)?;

    // 3. Join under the root and ensure the parent exists, then re-canonicalize
    //    the parent and assert containment. This catches any residual escape
    //    that survived the component check (e.g. via an intermediate symlink in
    //    the root path itself) and is the authoritative containment guarantee.
    let dest = root.join(&safe_rel);
    let parent = dest
        .parent()
        .ok_or_else(|| ExtractError::UnsafeEntry(format!("entry has no parent: {name:?}")))?;
    std::fs::create_dir_all(parent).map_err(|e| ExtractError::io(parent, e))?;

    let canon_parent = parent
        .canonicalize()
        .map_err(|e| ExtractError::io(parent, e))?;
    let canon_root = root
        .canonicalize()
        .map_err(|e| ExtractError::io(root, e))?;
    if !canon_parent.starts_with(&canon_root) {
        return Err(ExtractError::UnsafeEntry(format!(
            "entry destination escapes extraction root: {name:?}"
        )));
    }

    Ok(canon_parent.join(dest.file_name().ok_or_else(|| {
        ExtractError::UnsafeEntry(format!("entry has no file name: {name:?}"))
    })?))
}

/// Reduce a raw archive entry name to a safe relative path, rejecting absolute
/// paths, root/prefix components, and parent-directory escape components.
fn safe_relative(name: &Path) -> Result<PathBuf, ExtractError> {
    let mut out = PathBuf::new();
    for comp in name.components() {
        match comp {
            // A normal path segment is the only acceptable component.
            Component::Normal(seg) => out.push(seg),
            // The current-directory marker is harmless; drop it.
            Component::CurDir => {}
            // Everything else (root, drive/UNC prefix, or a parent-escape
            // marker) means the entry is trying to leave its sandbox.
            Component::RootDir | Component::Prefix(_) | Component::ParentDir => {
                return Err(ExtractError::UnsafeEntry(format!(
                    "entry path is absolute or escapes its root: {name:?}"
                )));
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(ExtractError::UnsafeEntry(format!(
            "entry path is empty after normalization: {name:?}"
        )));
    }
    Ok(out)
}
