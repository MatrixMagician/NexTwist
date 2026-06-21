//! The FOMOD-engine error type.
//!
//! `thiserror` enum per the locked error-design decision (libs use thiserror; anyhow
//! is reserved for the app/Tauri boundary — NEVER anyhow here). Mirrors the shape of
//! `NexusError`/`LoadOrderError`: it wraps store failures, I/O failures (with the
//! offending path), and **flattens the external `quick_xml::DeError` to a `String`** at
//! the crate boundary so the quick-xml error type never leaks into NexTwist's public
//! surface.
//!
//! The [`FomodError::MalformedSchema`] variant is the locked "fail clearly, never
//! mis-install" outcome: any genuinely unsupported/contradictory FOMOD construct
//! surfaces here rather than producing a silent, wrong install plan.

use std::path::PathBuf;

use nextwist_core::StoreError;
use thiserror::Error;

/// Errors from the FOMOD engine (locate, parse, condition-eval, resolve).
#[derive(Debug, Error)]
pub enum FomodError {
    /// A persistence-layer failure surfaced from `store`. Declared as part of the
    /// stable error contract; the headless engine itself never writes, but the apply
    /// path (Plan 02) that consumes a resolved plan records provenance through `store`.
    #[error("store error: {0}")]
    Store(#[from] StoreError),

    /// An I/O error while touching a real filesystem path (e.g. reading the located
    /// `ModuleConfig.xml`, or walking the extracted tree to find `fomod/`).
    #[error("i/o error for {path}: {source}")]
    Io {
        /// Path involved in the failed operation.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// XML deserialization failed. The `quick_xml::DeError` is flattened to its display
    /// string so the quick-xml error type never crosses the crate boundary (mirrors
    /// `NexusError::Http(String)` flattening reqwest). A well-formed-but-not-FOMOD or a
    /// broken-XML document lands here.
    #[error("FOMOD xml parse error: {0}")]
    Xml(String),

    /// `fomod/ModuleConfig.xml` could not be located (case-insensitively) inside the
    /// extracted/staged tree.
    #[error("no fomod/ModuleConfig.xml found under {0}")]
    ConfigNotFound(PathBuf),

    /// The XML parsed but describes a genuinely unsupported / contradictory construct
    /// (the locked "fail clearly, never mis-install" outcome). Carries a human-readable
    /// description of the offending construct.
    #[error("malformed or unsupported FOMOD construct: {0}")]
    MalformedSchema(String),

    /// A resolved file-install referenced a `source` path that does not exist inside the
    /// staged tree (case-insensitive resolution failed). Surfaced rather than silently
    /// dropped so a pinned/recorded choice that no longer matches is never mis-installed.
    #[error("FOMOD source not found in staged tree: {0}")]
    MissingSource(String),
}

impl FomodError {
    /// Construct a [`FomodError::Io`] tagged with the offending path.
    ///
    /// Mirrors the `NexusError::io` / `LoadOrderError::io` constructor convention so the
    /// path context is always attached at the I/O site.
    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        FomodError::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}
