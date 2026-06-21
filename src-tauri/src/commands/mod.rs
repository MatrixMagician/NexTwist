//! Thin Tauri command adapters — the ONLY job of this layer is to cross the IPC
//! boundary and delegate to the headless safety core.
//!
//! Anti-Pattern 4 (RESEARCH.md / ARCHITECTURE.md): no business logic, no file loops,
//! no path resolution lives here. Every `#[tauri::command]` below: locks the shared
//! state, calls exactly one headless-crate function, maps the typed error to a
//! `String` at the boundary (the webview only speaks JSON/strings), and returns. All
//! validation and safety logic lives in the (tested) `steam`/`extract`/`deploy` crates.

pub mod conflicts;
pub mod deploy;
pub mod downloads;
pub mod fomod;
pub mod games;
pub mod mods;
pub mod nexus;
pub mod plugins;
pub mod profiles;

use tauri::State;
use tokio::sync::Mutex;

use crate::state::AppState;

/// Map any headless-crate error into the `String` the IPC boundary returns. Keeping
/// this in one place ensures every adapter maps errors identically (and trivially).
pub(crate) fn boundary_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Map a NexusMods game domain (the `nxm://` host) to a managed Steam AppID.
///
/// The `nxm://` link carries the game *domain* (e.g. `skyrimspecialedition`), but the
/// download flow needs the Steam AppID to resolve the managed game's staging dir. This is
/// the small, fixed v1 Bethesda allow-list (mirrors the frontend `SUPPORTED` list); a
/// domain outside it returns `None` and the caller rejects it rather than guessing. Kept
/// here (not in the headless crate) because it is a shell-side registry concern, not pure
/// client logic. Shared by the `nxm://` router (`nexus::route_download`) AND the download
/// core (`downloads::run_download_to_window`, which uses it to recover the AppID when a
/// Retry of an nxm-originated row arrives with `appid == 0`) so there is ONE definition.
pub(crate) fn appid_for_domain(domain: &str) -> Option<u32> {
    match domain {
        "skyrimspecialedition" => Some(489830),
        "fallout4" => Some(377160),
        _ => None,
    }
}

/// Look up a managed game by AppID (a single `store.get_game` call) or surface a clear
/// "not managed" boundary error. Shared by the mods/deploy adapters so neither inlines
/// a store lookup; this is not business logic, just the registry read every op needs.
pub(crate) async fn require_game(
    state: &State<'_, Mutex<AppState>>,
    appid: u32,
) -> Result<nextwist_core::Game, String> {
    state
        .lock()
        .await
        .store
        .get_game(appid)
        .map_err(boundary_err)?
        .ok_or_else(|| format!("game {appid} is not managed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BUG 2: the known Bethesda domains map to their managed Steam AppIDs. This is the
    /// allow-list `route_download` AND the `appid == 0` Retry-recovery path both depend on.
    #[test]
    fn appid_for_domain_maps_known_games() {
        assert_eq!(appid_for_domain("skyrimspecialedition"), Some(489830));
        assert_eq!(appid_for_domain("fallout4"), Some(377160));
    }

    /// BUG 2: an unmanaged/unknown domain returns `None` — the Retry path turns this into a
    /// clear error rather than guessing an AppID (and the router emits the §C.3 Warning).
    #[test]
    fn appid_for_domain_rejects_unknown_domain() {
        assert_eq!(appid_for_domain("morrowind"), None);
        assert_eq!(appid_for_domain(""), None);
    }

    /// BUG 2: the exact recovery `run_download_to_window` performs — when the IPC arg is the
    /// `appid == 0` sentinel (a Retry of an nxm-originated row, which never received an
    /// AppID frontend-side), resolve the real AppID from the non-secret `game_domain`. A
    /// known domain yields the managed AppID; an unknown one yields `None` (→ clear error).
    #[test]
    fn appid_zero_resolves_from_game_domain() {
        let recover = |appid: u32, domain: &str| -> Option<u32> {
            if appid == 0 {
                appid_for_domain(domain)
            } else {
                Some(appid)
            }
        };
        // appid == 0 + a known domain → the managed AppID.
        assert_eq!(recover(0, "skyrimspecialedition"), Some(489830));
        assert_eq!(recover(0, "fallout4"), Some(377160));
        // appid == 0 + an unknown domain → None (the caller surfaces a clear error).
        assert_eq!(recover(0, "unknowngame"), None);
        // A real AppID is passed through untouched (a normal premium download).
        assert_eq!(recover(489830, "skyrimspecialedition"), Some(489830));
    }
}
