//! Steam root + app discovery (ENV-01).
//!
//! Enumerates Steam roots via steamlocate's `locate_all()` (which already covers the
//! Flatpak-relocated root on most systems) and additionally probes the explicit
//! Flatpak path. Snap is treated as LOW-confidence (RESEARCH.md Assumption A2): we do
//! NOT auto-detect Snap — Snap users rely on [`crate::add_game_by_folder`].
//!
//! Discovery is filtered to the supported Bethesda AppIDs (see [`crate::resolve`]).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::SteamError;
use crate::resolve::{SUPPORTED_APPIDS, is_supported};

/// An installed, supported game found during discovery.
///
/// Derives serde so the Tauri command layer (Plan 06) can return it across the IPC
/// boundary unchanged — the shape is a pure-data DTO with no I/O dependencies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedGame {
    /// Steam AppID (one of [`crate::SKYRIM_SE`] / [`crate::FALLOUT4`]).
    pub appid: u32,
    /// Human-readable name (from the app manifest, best-effort).
    pub name: String,
    /// The Steam library root that contains this app (parent of `steamapps`).
    pub library_path: PathBuf,
}

/// Scan all detectable Steam roots and return the installed supported games.
///
/// Covers native + Flatpak roots. Snap is deliberately excluded (A2, LOW confidence);
/// Snap users add their game with [`crate::add_game_by_folder`].
///
/// Returns an empty vec (not an error) when Steam is present but neither supported
/// game is installed. Returns [`SteamError::NoSteam`] only when no Steam root exists
/// at all.
pub fn detect_games() -> Result<Vec<DetectedGame>, SteamError> {
    // `locate_all` returns every Steam root steamlocate can find, including the
    // Flatpak-relocated one on systems where the registry/desktop hints point at it.
    let dirs = steamlocate::locate_all().map_err(|e| SteamError::Locate(e.to_string()))?;

    // Additionally probe the explicit Flatpak root, since `locate_all` can miss it on
    // headless/CI-like environments where the native registry hint is absent.
    let mut roots = dirs;
    if let Some(flatpak) = flatpak_steam_root() {
        if flatpak.is_dir()
            && !roots.iter().any(|d| d.path() == flatpak)
            && let Ok(extra) = steamlocate::SteamDir::from_dir(&flatpak)
        {
            roots.push(extra);
        }
    }

    // TODO(A2): Snap root (~/snap/steam/common/.steam/steam) is LOW confidence and is
    // intentionally NOT auto-detected here — Snap users use `add_game_by_folder`.

    if roots.is_empty() {
        return Err(SteamError::NoSteam);
    }

    let mut found = Vec::new();
    for steam in &roots {
        for &appid in SUPPORTED_APPIDS {
            if !is_supported(appid) {
                continue;
            }
            match steam.find_app(appid) {
                Ok(Some((app, library))) => {
                    let detected = DetectedGame {
                        appid,
                        name: app
                            .name
                            .clone()
                            .unwrap_or_else(|| app.install_dir.clone()),
                        library_path: library.path().to_path_buf(),
                    };
                    if !found.contains(&detected) {
                        found.push(detected);
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::debug!(appid, error = %e, "find_app failed for a root; skipping");
                }
            }
        }
    }

    Ok(found)
}

/// The explicit Flatpak Steam root: `~/.var/app/com.valvesoftware.Steam/.steam/steam`.
///
/// Honors `$HOME`. Returns `None` if `$HOME` is unset.
pub(crate) fn flatpak_steam_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join(".var/app/com.valvesoftware.Steam/.steam/steam"),
    )
}
