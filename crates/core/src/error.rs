//! Domain error enums.
//!
//! Per the locked error-design decision (CONTEXT.md): libraries use `thiserror`
//! enums; `anyhow` is reserved for the app/Tauri boundary only. Each crate that
//! grows its own failure modes adds a variant here or its own `thiserror` type;
//! the shared ones live here so callers can match across crate boundaries.

use std::path::PathBuf;

use thiserror::Error;

/// Errors originating from the persistence layer (`store`).
///
/// The store deliberately does NOT leak `rusqlite::Error` in its public surface —
/// it maps DB failures into these domain variants so callers never depend on the
/// concrete SQL backend.
#[derive(Debug, Error)]
pub enum StoreError {
    /// The underlying database failed (open, query, or transaction).
    #[error("database error: {0}")]
    Db(String),

    /// A schema migration failed to apply.
    #[error("migration error: {0}")]
    Migration(String),

    /// A value persisted in the DB could not be parsed back into a domain type
    /// (e.g. an unknown deploy-method token).
    #[error("corrupt persisted value: {0}")]
    Corrupt(String),

    /// An I/O error while touching the DB file or its directory.
    #[error("i/o error for {path}: {source}")]
    Io {
        /// Path involved.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

/// Errors originating from core-level operations shared across crates.
#[derive(Debug, Error)]
pub enum CoreError {
    /// A path was expected to be relative to a deploy/staging root but was not.
    #[error("path is not within the expected root: {0}")]
    PathEscape(PathBuf),

    /// A required value was missing or malformed.
    #[error("invalid input: {0}")]
    Invalid(String),
}
