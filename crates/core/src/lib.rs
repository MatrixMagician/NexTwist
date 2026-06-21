//! `nextwist-core` — shared domain types and error enums.
//!
//! Pure, headless, dependency-light. Every other NexTwist crate links against the
//! types re-exported here. No I/O framework deps (rusqlite/tauri/reqwest) live in
//! this crate by design, so the safety-critical engine stays unit/property-testable
//! in CI without a webview.

pub mod error;
pub mod model;

pub use error::{CoreError, StoreError};
pub use model::{
    Collection, CollectionMod, DeployMethod, FileConflict, FileEntry, Game, ManagedMod,
    NexusSource, Plugin, PluginKind, Profile,
};
