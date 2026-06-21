//! NexusMods auth command adapters — thin IPC boundary over the headless `crates/nexus`
//! client + the shell keyring / OAuth orchestration.
//!
//! Per the Anti-Pattern-4 contract (see `commands/mod.rs`): each command locks the
//! shared state, calls one headless/keyring/auth function, maps the error to a `String`,
//! and returns. No HTTP, no file loops, no business logic here. A token or key is NEVER
//! returned to the UI — only a `UserInfo`.

use nexus::{NxmLink, NxmLinkKind};
use serde::Serialize;
use tauri::{Emitter, Manager, State};
use tokio::sync::Mutex;

use crate::auth;
use crate::commands::boundary_err;
use crate::keyring;
use crate::state::{AppState, OAUTH_REDIRECT};

/// Map a NexusMods game domain (the `nxm://` host) to a managed Steam AppID.
///
/// The `nxm://` link carries the game *domain* (e.g. `skyrimspecialedition`), but the
/// download flow needs the Steam AppID to resolve the managed game's staging dir. This is
/// the small, fixed v1 Bethesda allow-list (mirrors the frontend `SUPPORTED` list); a
/// domain outside it is rejected rather than guessed. Kept here (not in the headless crate)
/// because it is a shell-side registry concern, not pure client logic.
fn appid_for_domain(domain: &str) -> Option<u32> {
    match domain {
        "skyrimspecialedition" => Some(489830),
        "fallout4" => Some(377160),
        _ => None,
    }
}

/// The `nxm://` arrival toast payload (UI-SPEC §C.1). Emitted on `nxm://arrival` so the UI
/// can show the non-blocking "Download started from NexusMods" Success toast. Carries NO
/// secret — only the UI download id + display domain (never the key/expires/code).
#[derive(Debug, Clone, Serialize)]
struct NxmArrival {
    /// The UI download id of the new row this arrival started.
    id: String,
}

/// The `nxm://` expired/invalid-link payload (UI-SPEC §C.3). Emitted on `nxm://expired`
/// so the UI shows the Warning notice instead of a stuck Failed row. Carries no secret.
#[derive(Debug, Clone, Serialize)]
struct NxmExpired {
    /// A human-readable, secret-free reason for the Warning notice.
    reason: String,
}

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

/// Route one incoming `nxm://` URL (NXM-01 / NEXUS-04) — the deep-link handler's only job.
///
/// This is a THIN router: ALL parsing lives in the headless `nexus::NxmLink::parse`, ALL
/// download logic lives in Plan-02's `run_download_to_window`, and the OAuth code-exchange
/// lives in Plan-01's `auth::complete_oauth`. Here we only parse, discriminate, and dispatch.
///
/// SECURITY (V5/V7): the URL is untrusted OS input. We never shell out, never interpolate
/// link content into a command, and never log the url/key/expires/code — only a coarse,
/// secret-free outcome. A malformed/expired link emits the UI Warning, never a stuck row.
///
/// Runs on the Tauri runtime (the deep-link plugin invokes `on_open_url` there). Spawns the
/// async work so the synchronous `on_open_url` callback returns immediately.
pub fn handle_nxm_url(app: &tauri::AppHandle, url: &str) {
    match NxmLink::parse(url) {
        Ok(NxmLinkKind::Download(link)) => route_download(app, link),
        Ok(NxmLinkKind::OAuthCallback { code, state }) => route_oauth_callback(app, code, state),
        Err(_e) => {
            // A malformed/spoofed/expired link → the UI Warning (UI-SPEC §C.3), not a
            // stuck Failed row. We do NOT log `_e` (it could echo link content — V7).
            emit_expired(
                app,
                "This download link has expired. Re-open it from the NexusMods website.",
            );
        }
    }
}

/// Dispatch a download `nxm://` link to the shared Plan-02 download core.
fn route_download(app: &tauri::AppHandle, link: NxmLink) {
    let Some(appid) = appid_for_domain(&link.game_domain) else {
        // Unknown/unmanaged game domain — surface the Warning rather than guess an AppID.
        emit_expired(
            app,
            "This download is for a game NexTwist doesn't manage yet.",
        );
        return;
    };

    // A fresh UI row id; the arrival toast + the new downloads row both key off it.
    let id = format!("nxm-{}-{}", link.mod_id, link.file_id);

    // Confirm the arrival immediately (UI-SPEC §C.1) — the row then streams via events.
    let _ = app.emit("nxm://arrival", NxmArrival { id: id.clone() });

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        let state = app.state::<Mutex<AppState>>();
        // Reuse the EXACT Plan-02 stream→extract→stage flow (NEXUS-04 free-user redemption):
        // the parsed key/expires are passed straight through, never interpreted here.
        let _ = crate::commands::downloads::run_download_to_window(
            &state,
            &window.as_ref().window(),
            &id,
            appid,
            &link.game_domain,
            link.mod_id,
            link.file_id,
            link.key,
            link.expires,
        )
        .await;
        // Success/failure/expired is already emitted on `download://progress` by the core;
        // an expired free-user redemption surfaces the §C.3 Warning there.
    });
}

/// Dispatch an `nxm://oauth/callback` to the Plan-01 OAuth code-exchange, closing the loop.
fn route_oauth_callback(app: &tauri::AppHandle, code: String, state_param: String) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<Mutex<AppState>>();
        // Pull the pending round-trip (CSRF + PKCE verifier) + client/redirect from state.
        let (client_id, pending) = {
            let guard = state.lock().await;
            (guard.oauth_client_id.clone(), guard.pending_oauth.clone())
        };
        let Some(pending) = pending else {
            // No login in progress — ignore a stray callback (defensive; never logged).
            return;
        };
        // `complete_oauth` validates state == csrf (T-03-13) before exchanging the code,
        // and stores the refresh token in the keyring. We never log code/state (V7).
        match auth::complete_oauth(&client_id, OAUTH_REDIRECT, &pending, &code, &state_param).await
        {
            Ok(access) => {
                let mut guard = state.lock().await;
                guard.access_token = Some(access);
                guard.pending_oauth = None;
            }
            Err(_e) => {
                // Surface a generic auth-failure Warning; do NOT log `_e` (may carry detail).
                emit_expired(&app, "NexusMods login failed. Try again, or use an API key.");
            }
        }
    });
}

/// Emit the secret-free expired/invalid-link Warning to the main window (UI-SPEC §C.3).
fn emit_expired(app: &tauri::AppHandle, reason: &str) {
    let _ = app.emit("nxm://expired", NxmExpired { reason: reason.to_string() });
}
