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

    /// A caller tried to disable or reorder a protected / implicitly-active master (the
    /// game master, hardcoded DLC, or a Creation-Club plugin libloot keeps active without a
    /// `*` line). The reversibility + safety guarantee (SFLO-03) is enforced in the ENGINE,
    /// not just the UI: `apply_load_order` rejects the request with this typed error before
    /// the libloot call. The protected set is derived purely from libloot's
    /// `Game::is_plugin_active` (see `loot::protected_plugins`) — never a hard-coded name
    /// list. Holds the offending plugin name.
    #[error("cannot reorder or disable protected master: {0}")]
    ProtectedMaster(String),
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
