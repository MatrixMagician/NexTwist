//! Download adapter (NEXUS-03/06) — the ONLY place a Tauri type touches the download
//! flow. Per the Anti-Pattern-4 contract (see `commands/mod.rs`): no business logic
//! lives here. The adapter:
//!
//! 1. resolves the managed game + the session auth (OAuth bearer or the keyring API key),
//! 2. asks the headless `nexus` client for the REST v1 download link + REST v1 file metadata,
//! 3. streams the file to a staging-adjacent temp path via `nexus::download_to`, wrapping
//!    the headless `Fn(u64, Option<u64>)` progress callback into
//!    `window.emit("download://progress", …)` (the single Tauri-type touch point),
//! 4. hands the finished archive to `extract::install_archive` VERBATIM (NEXUS-06 — the
//!    exact terminus `commands/mods.rs` uses for a local archive), then
//! 5. persists the mod (`store.add_mod`) + its Nexus provenance (`store.add_nexus_source`).
//!
//! A `NexusError::Redeem` (expired free-user link) is surfaced as a distinct "expired
//! link" string, NOT a failed-download row (UI-SPEC §C.3). Errors map via `boundary_err`.

use std::path::PathBuf;

use nexus::{CancelFlag, NexusAuth, NexusClient};
use serde::Serialize;
use tauri::{Emitter, State};
use tokio::sync::Mutex;

use crate::commands::{appid_for_domain, boundary_err, require_game};
use crate::state::AppState;

/// The per-item progress payload emitted on `download://progress`. Mirrors the
/// `DownloadItem` fields the frontend renders (snake_case crosses the IPC boundary).
#[derive(Debug, Clone, Serialize)]
struct ProgressEvent {
    /// The UI download id (echoes the caller-supplied id).
    id: String,
    /// Bytes downloaded so far.
    downloaded: u64,
    /// Total bytes (Content-Length) if known.
    total: Option<u64>,
    /// One of: "downloading" | "extracting" | "done" | "failed".
    state: String,
    /// Human-readable failure reason when `state == "failed"`; else `None`.
    reason: Option<String>,
}

/// The result of a completed download: the staged mod + its persisted provenance ids.
#[derive(Debug, Clone, Serialize)]
pub struct DownloadResult {
    /// The local `managed_mod` row id created for the staged mod.
    pub mod_id: i64,
    /// The mod's display name (from the REST v1 file-info metadata).
    pub display_name: String,
    /// Root of the staged tree the deploy engine will use.
    pub staging_root: PathBuf,
}

/// Start an in-app download of a NexusMods file and stage it as an ordinary `ManagedMod`.
///
/// `id` is the UI-assigned download id (used for progress events + cancellation).
/// `key`/`expires` are `None` for a Premium direct download and `Some` for a free-user
/// `nxm://` redemption.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri command: the arg list is the IPC contract.
pub async fn start_download(
    state: State<'_, Mutex<AppState>>,
    window: tauri::Window,
    id: String,
    appid: u32,
    game_domain: String,
    nexus_mod_id: u64,
    file_id: u64,
    key: Option<String>,
    expires: Option<String>,
) -> Result<DownloadResult, String> {
    // Delegate to the shared core so the in-app (Premium) path and the `nxm://` free-user
    // redemption run the EXACT same stream→extract→stage flow (no parallel download path).
    run_download_to_window(
        &state,
        &window,
        &id,
        appid,
        &game_domain,
        nexus_mod_id,
        file_id,
        key,
        expires,
    )
    .await
}

/// Drive a full download keyed by raw coordinates, emitting progress to `window`.
///
/// This is the shared core both the IPC [`start_download`] command and the `nxm://`
/// deep-link router (`commands::nexus::handle_nxm_url`) call — so the free-user redemption
/// reuses the EXACT Plan-02 stream→extract→stage path (no parallel download flow). It
/// resolves the session auth itself (OAuth bearer or the keyring API key), registers a
/// cancel flag, runs the flow, and emits the terminal `download://progress` event (`done`,
/// `failed`, or `expired`). Returns the staged result, or the failure reason string.
///
/// `key`/`expires` are `None` for a Premium direct download and `Some` for a free-user
/// `nxm://` redemption.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_download_to_window(
    state: &State<'_, Mutex<AppState>>,
    window: &tauri::Window,
    id: &str,
    appid: u32,
    game_domain: &str,
    nexus_mod_id: u64,
    file_id: u64,
    key: Option<String>,
    expires: Option<String>,
) -> Result<DownloadResult, String> {
    // BUG 2 fix: a Retry of an `nxm://`-originated row reaches the IPC `start_download`
    // command with `appid == 0`, because the backend created that row entirely server-side
    // (`route_download` resolved the AppID from the domain) and the secret-free arrival
    // event never carried an AppID back to the frontend. Recover the AppID from the
    // (non-secret) `game_domain` using the SAME allow-list `route_download` uses, so a
    // Retry resolves the managed game instead of failing with "game 0 is not managed".
    // A genuine premium download always supplies a real AppID, so this only kicks in for
    // the `appid == 0` sentinel.
    let appid = if appid == 0 {
        appid_for_domain(game_domain).ok_or_else(|| {
            format!("cannot retry: '{game_domain}' is not a game NexTwist manages")
        })?
    } else {
        appid
    };

    let game = require_game(state, appid).await?;

    // Resolve session auth + the shared rate limiter + register a cancel flag — lock held
    // only briefly.
    let (auth, limiter, cancel) = {
        let mut guard = state.lock().await;
        let auth = match guard.access_token.clone() {
            Some(tok) => NexusAuth::Bearer(tok),
            None => {
                let api_key = crate::keyring::load_refresh_token()
                    .map_err(boundary_err)?
                    .ok_or_else(|| "not logged in: no NexusMods session".to_string())?;
                NexusAuth::ApiKey(api_key)
            }
        };
        // WR-03: clone the ONE process-wide limiter so this download coordinates its
        // budget + backoff with every other in-flight NexusMods request.
        let limiter = guard.rate_limiter.clone();
        let cancel = CancelFlag::new();
        guard.downloads.insert(id.to_string(), cancel.clone());
        (auth, limiter, cancel)
    };

    let result = run_download(
        state,
        window,
        id,
        &game,
        game_domain,
        nexus_mod_id,
        file_id,
        key.as_deref(),
        expires.as_deref(),
        auth,
        limiter,
        &cancel,
    )
    .await;

    state.lock().await.downloads.remove(id);

    match result {
        Ok(res) => {
            emit_progress(
                window,
                id,
                res.staging_size_hint,
                res.staging_size_hint_total,
                "done",
                None,
            );
            Ok(res.result)
        }
        Err(DownloadFailure { reason, is_redeem, retry_after }) => {
            // WR-02: a rate-limit is transient and auto-recoverable — surface it as a
            // distinct "ratelimited" state (which drives the WR-01 UI notice and a paused,
            // retryable row), NOT a terminal "failed" row. An expired free-user link is
            // surfaced as "expired" (UI-SPEC §C.3). Everything else is a real "failed".
            let state_label = if retry_after.is_some() {
                "ratelimited"
            } else if is_redeem {
                "expired"
            } else {
                "failed"
            };
            emit_progress(window, id, 0, None, state_label, Some(reason.clone()));
            Err(reason)
        }
    }
}

/// Cancel an in-flight download by id. Idempotent: an unknown id is a no-op.
#[tauri::command]
pub async fn cancel_download(state: State<'_, Mutex<AppState>>, id: String) -> Result<(), String> {
    if let Some(flag) = state.lock().await.downloads.get(&id) {
        flag.cancel();
    }
    Ok(())
}

/// A typed download failure carrying whether it was a redeemable (expired-link) error
/// and, for a rate-limit, the retry-after seconds (WR-02).
struct DownloadFailure {
    reason: String,
    is_redeem: bool,
    /// `Some(secs)` when the failure was a `NexusError::RateLimited` — the transient,
    /// auto-recoverable case the UI shows as a paused "rate limited" row, not a failure.
    retry_after: Option<u64>,
}

/// RAII cleanup for the untrusted partial download archive (CR-01).
///
/// While the temp `.nextwist-dl-*.archive` exists in the deploy-trusted staging dir, this
/// guard ensures it is unlinked when it goes out of scope — on success (after the explicit
/// `remove_file`, where Drop is a no-op), on any `?` early-return, on a cancel, or on a
/// panic. This guarantees partially-written, untrusted bytes never linger in staging.
struct TempArchive(PathBuf);

impl Drop for TempArchive {
    fn drop(&mut self) {
        // Best-effort, synchronous unlink on any exit path. Already-removed (the success
        // path) is fine — `remove_file` on a missing file just errors and is ignored.
        let _ = std::fs::remove_file(&self.0);
    }
}

/// What `run_download` returns on success: the result DTO plus the final byte counts so
/// the caller can emit the terminal "done" progress event.
struct RunOk {
    result: DownloadResult,
    staging_size_hint: u64,
    staging_size_hint_total: Option<u64>,
}

#[allow(clippy::too_many_arguments)]
async fn run_download(
    state: &State<'_, Mutex<AppState>>,
    window: &tauri::Window,
    id: &str,
    game: &nextwist_core::Game,
    game_domain: &str,
    nexus_mod_id: u64,
    file_id: u64,
    key: Option<&str>,
    expires: Option<&str>,
    auth: NexusAuth,
    limiter: std::sync::Arc<nexus::RateLimiter>,
    cancel: &CancelFlag,
) -> Result<RunOk, DownloadFailure> {
    // WR-03: build the client with the SHARED process-wide limiter (not a fresh one) so
    // parallel downloads honour one budget + one backoff deadline.
    let client =
        NexusClient::with_limiter(nexus::NEXUS_API_BASE, auth, limiter).map_err(fail)?;

    // 1. REST v1 download link (premium omits key/expires; free passes them).
    let links = client
        .download_link(game_domain, nexus_mod_id, file_id, key, expires)
        .await
        .map_err(fail)?;
    let link = links
        .into_iter()
        .next()
        .ok_or_else(|| DownloadFailure {
            reason: "no download link returned".into(),
            is_redeem: false,
            retry_after: None,
        })?;

    // 2. REST v1 file-info metadata (version + display name) for the provenance record + label.
    let meta = client
        .mod_file_metadata(game_domain, nexus_mod_id, file_id)
        .await
        .map_err(fail)?;

    // 3. Stream to a staging-adjacent temp path (same dir as the staging root so the
    //    later extract's final move is a cheap rename). The temp file is the untrusted
    //    archive; extract validates it before anything lands in staging.
    let staging_dir = &game.staging_dir;
    tokio::fs::create_dir_all(staging_dir)
        .await
        .map_err(|e| DownloadFailure {
            reason: format!("could not create staging dir {}: {e}", staging_dir.display()),
            is_redeem: false,
            retry_after: None,
        })?;
    let archive_path = staging_dir.join(format!(".nextwist-dl-{id}.archive"));
    // CR-01 (BLOCKER): RAII-guard the untrusted partial archive so it is unlinked on
    // EVERY exit from the download/extract window — a chunk/transport/IO error, an
    // extract failure, a cancel, or a panic. A leftover `.nextwist-dl-*.archive` is
    // partially-written, untrusted bytes inside the deploy-trusted staging dir; it must
    // never be orphaned there. On success the explicit `remove_file` below already
    // unlinks it (the guard's Drop is then a harmless best-effort no-op).
    let _archive_guard = TempArchive(archive_path.clone());

    let id_owned = id.to_string();
    let win = window.clone();
    // Capture the last-seen Content-Length across the await via a Send-safe atomic
    // (`u64::MAX` is the "unknown total" sentinel) so the closure stays `Send`.
    let total_seen = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(u64::MAX));
    let total_for_cb = total_seen.clone();
    let written = client
        .download(&link.uri, &archive_path, cancel, move |downloaded, total| {
            if let Some(t) = total {
                total_for_cb.store(t, std::sync::atomic::Ordering::Relaxed);
            }
            emit_progress(&win, &id_owned, downloaded, total, "downloading", None);
        })
        .await
        .map_err(fail)?;
    let total_hint = match total_seen.load(std::sync::atomic::Ordering::Relaxed) {
        u64::MAX => None,
        t => Some(t),
    };

    // 4. Reuse the extract→staging pipeline VERBATIM (NEXUS-06). The downloaded archive
    //    is indistinguishable from a local one here; extract enforces zip-slip/symlink/`..`
    //    defenses identically. Stage under a per-mod subdir of the game's staging dir.
    emit_progress(window, id, written, total_hint, "extracting", None);
    let staging_root = staging_dir.join(sanitize(&meta.display_name));
    let staged = extract::install_archive(&archive_path, &staging_root).map_err(fail)?;
    // The validated tree is staged; remove the downloaded archive (no longer needed).
    let _ = tokio::fs::remove_file(&archive_path).await;

    // 5. Persist the mod + its Nexus provenance via the store facade.
    let managed = nextwist_core::ManagedMod {
        id: 0,
        name: meta.display_name.clone(),
        staging_root: staged.staging_root.clone(),
        enabled: false,
        rank: 1,
    };
    let mod_id = {
        let guard = state.lock().await;
        let mod_id = guard.store.add_mod(game.appid, &managed).map_err(fail)?;
        guard
            .store
            .add_nexus_source(&nextwist_core::NexusSource {
                mod_id,
                nexus_mod_id,
                file_id,
                version: meta.version.clone(),
                display_name: meta.display_name.clone(),
            })
            .map_err(fail)?;
        mod_id
    };

    Ok(RunOk {
        result: DownloadResult {
            mod_id,
            display_name: meta.display_name,
            staging_root: staged.staging_root,
        },
        staging_size_hint: written,
        staging_size_hint_total: total_hint,
    })
}

/// Map any headless error into a `DownloadFailure`, flagging the redeemable (expired
/// free-user link) case and, for a rate-limit, the retry-after seconds (WR-02).
fn fail<E: Into<NexusErrorLike>>(e: E) -> DownloadFailure {
    let like = e.into();
    DownloadFailure {
        reason: like.reason,
        is_redeem: like.is_redeem,
        retry_after: like.retry_after,
    }
}

/// A tiny error-shape bridge so `fail` accepts both `NexusError` and `ExtractError`.
struct NexusErrorLike {
    reason: String,
    is_redeem: bool,
    /// `Some(secs)` for a `NexusError::RateLimited` (WR-02); `None` otherwise.
    retry_after: Option<u64>,
}

impl From<nexus::NexusError> for NexusErrorLike {
    fn from(e: nexus::NexusError) -> Self {
        let is_redeem = matches!(e, nexus::NexusError::Redeem(_));
        // WR-02: carry the retry-after seconds so the shell can surface the transient,
        // auto-recoverable rate-limit state instead of a terminal failure.
        let retry_after = match &e {
            nexus::NexusError::RateLimited(secs) => Some(*secs),
            _ => None,
        };
        NexusErrorLike { reason: e.to_string(), is_redeem, retry_after }
    }
}

impl From<extract::ExtractError> for NexusErrorLike {
    fn from(e: extract::ExtractError) -> Self {
        NexusErrorLike { reason: e.to_string(), is_redeem: false, retry_after: None }
    }
}

impl From<nextwist_core::StoreError> for NexusErrorLike {
    fn from(e: nextwist_core::StoreError) -> Self {
        NexusErrorLike { reason: e.to_string(), is_redeem: false, retry_after: None }
    }
}

/// Emit a `download://progress` event. The ONLY place a Tauri type meets the flow.
fn emit_progress(
    window: &tauri::Window,
    id: &str,
    downloaded: u64,
    total: Option<u64>,
    state: &str,
    reason: Option<String>,
) {
    let _ = window.emit(
        "download://progress",
        ProgressEvent {
            id: id.to_string(),
            downloaded,
            total,
            state: state.to_string(),
            reason,
        },
    );
}

/// Sanitize a display name into a single safe staging-subdir component (no separators,
/// no traversal). The full path-traversal defense still lives in `extract`; this only
/// keeps the staging subdir name well-formed.
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "nexus-mod".to_string()
    } else {
        trimmed.to_string()
    }
}
