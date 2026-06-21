//! NexusMods auth command adapters — thin IPC boundary over the headless `crates/nexus`
//! client + the shell keyring / OAuth orchestration.
//!
//! Per the Anti-Pattern-4 contract (see `commands/mod.rs`): each command locks the
//! shared state, calls one headless/keyring/auth function, maps the error to a `String`,
//! and returns. No HTTP, no file loops, no business logic here. A token or key is NEVER
//! returned to the UI — only a `UserInfo`.

use tauri::State;
use tokio::sync::Mutex;

use crate::auth;
use crate::commands::boundary_err;
use crate::keyring;
use crate::state::{AppState, OAUTH_REDIRECT};

/// Log in with a manual NexusMods personal API key (the works-today fallback while OAuth
/// client registration is pending — NEXUS-01). Validates the key against the live API,
/// stores it in the keyring (NEXUS-02), caches the `UserInfo`, and returns it. The key
/// itself never crosses back to the UI.
#[tauri::command]
pub async fn login_with_api_key(
    state: State<'_, Mutex<AppState>>,
    key: String,
) -> Result<nexus::UserInfo, String> {
    // Validate first (headless, mockable) — a bad key fails here before anything is stored.
    let info = nexus::validate_api_key(nexus::API_BASE, &key)
        .await
        .map_err(boundary_err)?;

    // Persist ONLY through the keyring (NEXUS-02 hard-fail-no-plaintext). If no backend,
    // this surfaces the NoKeyringBackend string the UI keys its destructive banner on.
    keyring::store_refresh_token(&key).map_err(boundary_err)?;

    let mut guard = state.lock().await;
    guard.user = Some(info.clone());
    // An API-key session has no OAuth access token; the key lives only in the keyring.
    guard.access_token = None;
    Ok(info)
}

/// Begin the OAuth2+PKCE login: build the authorize URL (headless), stash the CSRF +
/// PKCE verifier in memory, and open the system browser. The `nxm://oauth/callback`
/// code is delivered by the Plan-03 deep-link handler, which calls `auth::complete_oauth`.
/// Returns the authorize URL (also opened in the browser) so the UI can show a fallback link.
#[tauri::command]
pub async fn login_oauth_start(state: State<'_, Mutex<AppState>>) -> Result<String, String> {
    let client_id = {
        let guard = state.lock().await;
        guard.oauth_client_id.clone()
    };
    if client_id.is_empty() {
        // No registered client yet (RESEARCH Pitfall 3) — steer the UI to the key fallback.
        return Err(
            "OAuth login is not yet available (no registered client). Use an API key instead."
                .to_string(),
        );
    }

    let req = nexus::build_authorize_url(&client_id, OAUTH_REDIRECT).map_err(boundary_err)?;

    {
        let mut guard = state.lock().await;
        guard.pending_oauth = Some(auth::PendingOAuth {
            csrf_state: req.csrf_state.clone(),
            pkce_verifier: req.pkce_verifier.clone(),
        });
    }

    auth::open_authorize_url(&req.authorize_url).map_err(boundary_err)?;
    Ok(req.authorize_url)
}

/// Log out: clear the keyring entry and the in-memory access token + cached user
/// (NEXUS-01 / D-Auth). Idempotent — clearing when already logged out succeeds.
#[tauri::command]
pub async fn logout(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    keyring::clear_refresh_token().map_err(boundary_err)?;
    let mut guard = state.lock().await;
    guard.access_token = None;
    guard.pending_oauth = None;
    guard.user = None;
    Ok(())
}

/// Return the currently logged-in user, or `None` if logged out. Reads the cached
/// `UserInfo`; the UI uses this to render the logged-in vs logged-out panel on load.
#[tauri::command]
pub async fn account_info(state: State<'_, Mutex<AppState>>) -> Result<Option<nexus::UserInfo>, String> {
    let guard = state.lock().await;
    Ok(guard.user.clone())
}
