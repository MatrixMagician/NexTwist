//! Shared application state held behind a `tokio::Mutex` and `manage`d by Tauri.
//!
//! The only mutable resource the command layer touches is the persistence [`Store`]
//! (the headless safety core owns everything else). Keeping the state this thin is the
//! point: business logic lives in the headless crates, never here (Anti-Pattern 4).

use std::collections::HashMap;
use std::path::PathBuf;

use nexus::CancelFlag;
use store::Store;

use crate::auth::PendingOAuth;

/// The redirect URI registered for the NexusMods OAuth public client.
pub const OAUTH_REDIRECT: &str = "nxm://oauth/callback";

/// Process-wide app state. `Store` is the WAL SQLite handle from `crates/store`; the
/// resolved app-data paths are kept so command adapters can derive default staging
/// locations without re-resolving the OS dirs each call.
///
/// The NexusMods auth spine adds in-memory-only auth state: the short-lived OAuth
/// access token, a pending OAuth round-trip (CSRF + PKCE verifier between browser-open
/// and callback), and a cached `UserInfo`. The long-lived refresh token / API key is
/// NEVER held here — it lives only in the OS keyring (NEXUS-02).
pub struct AppState {
    /// The persistence store (registry / manifest / journal / vanilla ledger).
    pub store: Store,
    /// OS app-data directory NexTwist owns (DB + per-game `originals/` vanilla store).
    pub data_dir: PathBuf,
    /// The public OAuth client id shipped with the app (PKCE → no secret). Empty until
    /// a client is registered under the Nexus Acceptable Use Policy; the API-key paste
    /// fallback works regardless (NEXUS-01 / RESEARCH Pitfall 3).
    pub oauth_client_id: String,
    /// Short-lived OAuth access token — in memory only, never persisted (NEXUS-02).
    pub access_token: Option<String>,
    /// A pending OAuth round-trip awaiting the `nxm://oauth/callback` code (Plan 03).
    pub pending_oauth: Option<PendingOAuth>,
    /// The currently logged-in user, cached for the account panel.
    pub user: Option<nexus::UserInfo>,
    /// In-flight downloads' cancellation flags, keyed by the UI download id. A
    /// `cancel_download` command trips the matching flag; the streaming loop in
    /// `crates/nexus` checks it once per chunk and aborts (NEXUS-03 Cancel affordance).
    pub downloads: HashMap<String, CancelFlag>,
}

impl AppState {
    /// Build the app state: ensure the app-data dir exists and open the store DB under it.
    pub fn init(data_dir: PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let store = Store::open(&data_dir.join("nextwist.db"))?;
        Ok(Self {
            store,
            data_dir,
            // No registered OAuth client yet (release task); the API-key fallback is the
            // works-today login path. Set from config/env when registration lands.
            oauth_client_id: String::new(),
            access_token: None,
            pending_oauth: None,
            user: None,
            downloads: HashMap::new(),
        })
    }
}
