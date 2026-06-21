//! OS-side OAuth orchestration (shell-only).
//!
//! This module holds the OS-bound half of the OAuth2 round-trip: opening the system
//! browser to the authorize URL, and the `complete_oauth` entry point that the Plan-03
//! `nxm://oauth/callback` deep-link handler will call with the returned code. The
//! security-sensitive PKCE/CSRF construction and the token exchange itself live in the
//! headless `crates/nexus` (`nexus::build_authorize_url` / `nexus::exchange_code`); this
//! module only does the browser launch and threads the keyring store on success.
//!
//! Plan 01 (this slice) lands the browser-open + the `complete_oauth` code path. The
//! deep-link plugin that delivers the code is wired in Plan 03.

use crate::keyring;

/// A pending OAuth round-trip: the CSRF state + PKCE verifier the shell must hold in
/// memory between opening the browser and receiving the `nxm://oauth/callback` code.
/// Stored in `AppState`; never persisted.
#[derive(Debug, Clone)]
pub struct PendingOAuth {
    /// CSRF state to match against the callback's `state`.
    pub csrf_state: String,
    /// PKCE verifier to feed into the code exchange. In-memory only.
    pub pkce_verifier: String,
}

/// Errors from the shell OAuth orchestration.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// The headless nexus client failed (URL build / code exchange / api-key validate).
    #[error("{0}")]
    Nexus(#[from] nexus::NexusError),
    /// The keyring store/clear failed (includes the NEXUS-02 no-backend hard-fail).
    #[error("{0}")]
    Keyring(#[from] keyring::KeyringError),
    /// The system browser could not be opened for the authorize step.
    #[error("could not open the system browser: {0}")]
    Browser(String),
}

/// Open the authorize URL in the user's default system browser.
///
/// Uses the `webbrowser` crate (no Tauri plugin / capability needed). Failures surface
/// as `AuthError::Browser` so the UI can fall back to the API-key path.
pub fn open_authorize_url(url: &str) -> Result<(), AuthError> {
    webbrowser::open(url).map_err(|e| AuthError::Browser(e.to_string()))?;
    Ok(())
}

/// Complete the OAuth round-trip: validate CSRF + exchange the code for tokens via the
/// headless client, store the refresh token in the keyring, and return the in-memory
/// access token to the caller (the shell caches it in `AppState`, never on disk).
///
/// The Plan-03 deep-link handler calls this with the `code`+`state` from
/// `nxm://oauth/callback`. `pending` carries the CSRF state + PKCE verifier captured at
/// `login_oauth_start` time.
pub async fn complete_oauth(
    client_id: &str,
    redirect: &str,
    pending: &PendingOAuth,
    code: &str,
    state: &str,
) -> Result<String, AuthError> {
    let tokens = nexus::exchange_code(
        client_id,
        redirect,
        nexus::TOKEN_BASE,
        code,
        state,                  // returned state
        &pending.csrf_state,    // expected state
        &pending.pkce_verifier,
    )
    .await?;

    // The long-lived refresh token (if issued) goes to the keyring; the short-lived
    // access token is returned to be held in memory only (NEXUS-02 / D-Auth).
    if let Some(refresh) = tokens.refresh.as_deref() {
        keyring::store_refresh_token(refresh)?;
    }
    Ok(tokens.access)
}
