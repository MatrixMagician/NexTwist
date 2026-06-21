//! The NexusMods-client error type.
//!
//! `thiserror` enum per the locked error-design decision (libs use thiserror;
//! anyhow is reserved for the app/Tauri boundary — NEVER anyhow here). Mirrors the
//! shape of `LoadOrderError`: wraps store failures, I/O failures (with the offending
//! path), and flattens external-library errors (reqwest, oauth2) to a `String` at the
//! crate boundary so those error types never leak into NexTwist's public surface.

use std::path::PathBuf;

use nextwist_core::StoreError;
use thiserror::Error;

/// Errors from the NexusMods client layer (auth, metadata, download, rate limiting).
#[derive(Debug, Error)]
pub enum NexusError {
    /// A persistence-layer failure surfaced from `store` (Nexus provenance writes).
    #[error("store error: {0}")]
    Store(#[from] StoreError),

    /// An I/O error while touching a real filesystem path (e.g. writing a downloaded
    /// archive to a staging-adjacent path).
    #[error("i/o error for {path}: {source}")]
    Io {
        /// Path involved in the failed operation.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A plain HTTP/transport failure (network/TLS/HTTP-status). The reqwest error is
    /// flattened to its display string so the reqwest error type never crosses the
    /// crate boundary (mirrors `LoadOrderError::Loot(String)`).
    #[error("http error: {0}")]
    Http(String),

    /// An authentication failure: OAuth2 token exchange, a rejected API key (401), a
    /// CSRF/state mismatch, or any oauth2-crate error. Flattened to a `String` so the
    /// oauth2 error type never crosses the crate boundary. NEVER carries a secret.
    #[error("auth error: {0}")]
    Auth(String),

    /// The client backed off to honour the NexusMods rate limit. The payload is the
    /// number of seconds the caller should wait before retrying (NEXUS-05).
    ///
    /// Constructed by the rate limiter (Plan 02); declared now as part of the stable
    /// error contract the download slices build on.
    #[allow(dead_code)] // wired in Plan 02 (governor rate limiter / streaming download)
    #[error("rate limited; retry after {0}s")]
    RateLimited(u64),

    /// Free-user download-link redemption failed (an expired/invalid `key`+`expires`
    /// from an `nxm://` link). Distinct from `Http` so the UI can surface the
    /// "link expired — re-open from the website" hint rather than a download error.
    ///
    /// Constructed by the download-link path (Plan 02); declared now as part of the
    /// stable error contract.
    #[allow(dead_code)] // wired in Plan 02 (free-user nxm:// redemption)
    #[error("download-link redemption failed: {0}")]
    Redeem(String),
}

impl NexusError {
    /// Construct a [`NexusError::Io`] tagged with the offending path.
    ///
    /// Used by the streaming-download path (Plan 02); declared now to mirror the
    /// `LoadOrderError::io` constructor convention.
    #[allow(dead_code)] // wired in Plan 02 (streaming download writes to a staging path)
    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        NexusError::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}
