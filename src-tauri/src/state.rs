//! Shared application state held behind a `tokio::Mutex` and `manage`d by Tauri.
//!
//! The only mutable resource the command layer touches is the persistence [`Store`]
//! (the headless safety core owns everything else). Keeping the state this thin is the
//! point: business logic lives in the headless crates, never here (Anti-Pattern 4).

use std::path::PathBuf;

use store::Store;

/// Process-wide app state. `Store` is the WAL SQLite handle from `crates/store`; the
/// resolved app-data paths are kept so command adapters can derive default staging
/// locations without re-resolving the OS dirs each call.
pub struct AppState {
    /// The persistence store (registry / manifest / journal / vanilla ledger).
    pub store: Store,
    /// OS app-data directory NexTwist owns (DB + per-game `originals/` vanilla store).
    pub data_dir: PathBuf,
}

impl AppState {
    /// Build the app state: ensure the app-data dir exists and open the store DB under it.
    pub fn init(data_dir: PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let store = Store::open(&data_dir.join("nextwist.db"))?;
        Ok(Self { store, data_dir })
    }
}
