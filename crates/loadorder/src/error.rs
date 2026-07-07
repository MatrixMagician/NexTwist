//! The load-order / plugin-management error type.
//!
//! `thiserror` enum per the locked error-design decision (libs use thiserror;
//! anyhow is reserved for the app/Tauri boundary — NEVER anyhow here). Mirrors the
//! shape of `DeployError`: wraps store failures, I/O failures (with the offending
//! path), libloot failures (flattened to a string at the crate boundary so the
//! libloot error type never leaks into NexTwist's public surface), and the
//! Linux-seam invariant the wrapper refuses to proceed through.

use std::path::PathBuf;

use nextwist_core::StoreError;
use thiserror::Error;

/// Errors from the plugin / load-order layer.
#[derive(Debug, Error)]
pub enum LoadOrderError {
    /// A persistence-layer failure surfaced from `store`.
    #[error("store error: {0}")]
    Store(#[from] StoreError),

    /// An I/O error while touching a real filesystem path (e.g. creating the
    /// Proton-prefix AppData parent dirs before constructing the libloot game).
    #[error("i/o error for {path}: {source}")]
    Io {
        /// Path involved in the failed operation.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A libloot operation failed (game construction, load, sort, set/save). The
    /// libloot error is flattened to its display string so the libloot error type
    /// never crosses NexTwist's crate boundary.
    #[error("libloot error: {0}")]
    Loot(String),

    /// No local AppData path could be resolved for the Proton prefix — the Linux
    /// seam (Pitfall 1). NexTwist must ALWAYS supply the prefix AppData path via
    /// `with_local_path`; this guards against an empty/unresolved prefix root.
    #[error("no local AppData path resolved for the Proton prefix: {0}")]
    NoLocalAppData(PathBuf),

    /// A masterlist HTTP fetch failed (network/TLS/HTTP-status). NON-fatal at the
    /// callsite: the masterlist layer falls back to a bundled CC0 snapshot, so this
    /// surfaces only when BOTH the network and the bundled fallback are unavailable.
    #[error("masterlist fetch failed: {0}")]
    Network(String),

    /// An unsupported game has no LOOT masterlist slug (the allow-list rejected the
    /// AppID before any fetch was attempted).
    #[error("unsupported game for masterlist (appid {0})")]
    UnsupportedGame(u32),
}

impl LoadOrderError {
    /// Construct a [`LoadOrderError::Io`] tagged with the offending path.
    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        LoadOrderError::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}
