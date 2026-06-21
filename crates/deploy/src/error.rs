//! The deploy-engine error type.
//!
//! `thiserror` enum per the locked error-design decision (libs use thiserror;
//! anyhow is reserved for the app/Tauri boundary). Wraps store failures, I/O
//! failures (with the offending path), and the safety-invariant violations the
//! engine refuses to proceed through.

use std::path::PathBuf;

use nextwist_core::StoreError;
use thiserror::Error;

/// Errors from the reversible-deployment engine.
#[derive(Debug, Error)]
pub enum DeployError {
    /// A persistence-layer failure surfaced from `store`.
    #[error("store error: {0}")]
    Store(#[from] StoreError),

    /// An I/O error while touching a real filesystem path.
    #[error("i/o error for {path}: {source}")]
    Io {
        /// Path involved in the failed operation.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A target path escaped the resolved deploy root (V4 access control). The
    /// engine never writes outside `<install_dir>/Data` or the app-data store.
    #[error("path escapes the deploy root: {0}")]
    PathEscape(PathBuf),

    /// The post-purge / post-recovery pristine check failed — the game folder is
    /// not byte-for-byte what provenance says it should be. Carries a human diff.
    #[error("pristine check failed: {0}")]
    NotPristine(String),

    /// A test-only injected abort fired mid-deploy (used by the crash-recovery
    /// centerpiece test to simulate a kill mid-operation).
    #[error("deploy aborted after {0} file operation(s) (injected)")]
    Aborted(usize),

    /// A profile-switch reconcile step failed outside the deploy/purge primitives —
    /// e.g. writing the target profile's `plugins.txt` (the libloot reason is wrapped)
    /// or reading the profile's membership/plugin state. The purge half having already
    /// completed means the game is pristine (or journal-recoverable), never unreversible.
    #[error("profile switch error: {0}")]
    Profile(String),
}

impl DeployError {
    /// Construct an [`DeployError::Io`] tagged with the offending path.
    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        DeployError::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}
