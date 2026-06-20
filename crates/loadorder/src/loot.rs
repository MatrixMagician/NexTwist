//! The libloot wrapper — the Linux seam for plugin / load-order management.
//!
//! This is the verified minimal surface over libloot 0.29.5 (de-risked by the
//! `libloot_spike` integration test, RESEARCH A1/A3). The non-negotiable invariant:
//! on Linux libloot CANNOT derive the AppData/Local plugins.txt location (it calls
//! `dirs::data_local_dir()`, meaningless inside a Proton prefix, and returns
//! `NoLocalAppData`). NexTwist therefore ALWAYS constructs the game with
//! [`libloot::Game::with_local_path`], supplying the Proton-prefix AppData path built
//! by [`appdata_local_path`] — NEVER `Game::new` (Pitfall 1).
//!
//! ## Verified libloot 0.29.5 API used here (Plan 04 builds on this)
//!
//! * `Game::with_local_path(GameType, game_path: &Path, game_local_path: &Path)
//!   -> Result<Game, GameHandleCreationError>` — `game_path` MUST be an existing
//!   directory; `game_local_path` is the AppData/Local/<GameName> folder itself
//!   (libloot does NOT append the game-folder name again when given a local path).
//! * `Game::load_current_load_order_state(&mut self) -> Result<(), LoadOrderStateError>`
//!   — reads the existing Plugins.txt / load-order state (tolerates an absent file).
//! * `Game::set_load_order(&mut self, &[&str]) -> Result<(), LoadOrderError>` — sets
//!   AND persists the order (it calls `save()` internally; there is NO separate
//!   `Game::save`). Masters-first is enforced INTERNALLY by libloot (D-08).
//! * `Game::active_plugins_file_path(&self) -> &PathBuf` — the exact Plugins.txt path
//!   libloot reads/writes; for SkyrimSE it is `<local_path>/Plugins.txt`, asterisk
//!   format (`*Active.esp`).
//! * `Game::is_plugin_active(&self, &str) -> bool`.
//! * `GameType::{SkyrimSE, Fallout4}`.
//!
//! Spike limitation (recorded for Plan 04): the public `Game` API exposes load-order
//! and an active-state *query*, but no active-state *setter*. A plugin's active flag
//! enters through the Plugins.txt that libloot loads (in NexTwist, generated from the
//! DB `plugin_state`); `set_load_order` preserves the active state of already-loaded
//! plugins. libloot/libloadorder also open and header-parse every named plugin
//! (esplugin `header_only`), so every plugin in a load order must physically exist in
//! the game `Data/` dir with at least a valid 24-byte TES4 header.

use std::path::{Path, PathBuf};

use libloot::{Game, GameType};

use crate::error::LoadOrderError;

/// Skyrim Special Edition AppID (mirrors `nextwist_steam::resolve::SKYRIM_SE`).
const SKYRIM_SE: u32 = 489830;
/// Fallout 4 AppID (mirrors `nextwist_steam::resolve::FALLOUT4`).
const FALLOUT4: u32 = 377160;

/// Build the Proton-prefix AppData/Local path libloot's `with_local_path` targets on
/// Linux: `<prefix>/drive_c/users/steamuser/AppData/Local/<game_name>` (Pitfall 1/2).
///
/// `prefix` is the resolved Proton prefix root (the `steam` crate supplies it; the
/// spike supplies a fixture via `testkit::fake_proton_prefix`). `game_name` is the
/// Steam AppData folder name — `"Skyrim Special Edition"` / `"Fallout4"` (A3), matching
/// libloadorder's `skyrim_se_appdata_folder_name` / `fallout4_appdata_folder_name`.
/// This whole path is passed straight to `with_local_path` as the local path.
pub fn appdata_local_path(prefix: &Path, game_name: &str) -> PathBuf {
    prefix
        .join("drive_c")
        .join("users")
        .join("steamuser")
        .join("AppData")
        .join("Local")
        .join(game_name)
}

/// Map a supported Steam AppID to its [`libloot::GameType`]; `None` for any other game.
///
/// Only the two supported Bethesda titles are accepted (allow-list, mirrors the
/// `steam` crate's `SUPPORTED_APPIDS`).
pub fn game_type_for(appid: u32) -> Option<GameType> {
    match appid {
        SKYRIM_SE => Some(GameType::SkyrimSE),
        FALLOUT4 => Some(GameType::Fallout4),
        _ => None,
    }
}

/// Open a libloot [`Game`] for a supported AppID using the Proton-prefix AppData path.
///
/// ALWAYS uses `Game::with_local_path` (never `Game::new`) so the Linux seam works
/// (Pitfall 1). The `appdata_local` parent dirs are created first, because a game that
/// has never been launched has no `AppData/Local/<Game>` folder yet (Pitfall 2) and
/// libloot will write Plugins.txt there on save.
///
/// # Errors
/// * [`LoadOrderError::NoLocalAppData`] if `appdata_local` is empty (an unresolved
///   prefix — the seam invariant cannot be satisfied).
/// * [`LoadOrderError::Io`] if the AppData parent dirs cannot be created.
/// * [`LoadOrderError::Loot`] if libloot rejects the game construction (e.g. the
///   install dir is not a directory).
pub fn open_game(
    appid: u32,
    install_dir: &Path,
    appdata_local: &Path,
) -> Result<Game, LoadOrderError> {
    let game_type =
        game_type_for(appid).ok_or(LoadOrderError::NoLocalAppData(appdata_local.to_path_buf()))?;

    if appdata_local.as_os_str().is_empty() {
        return Err(LoadOrderError::NoLocalAppData(appdata_local.to_path_buf()));
    }

    // Pitfall 2: a never-launched game has no AppData/Local/<Game> yet; create it so
    // libloot can write Plugins.txt there.
    std::fs::create_dir_all(appdata_local)
        .map_err(|source| LoadOrderError::io(appdata_local, source))?;

    Game::with_local_path(game_type, install_dir, appdata_local)
        .map_err(|e| LoadOrderError::Loot(e.to_string()))
}

/// Load the current load-order state, set the given order, and persist it.
///
/// `set_load_order` writes the asterisk-format Plugins.txt at
/// [`Game::active_plugins_file_path`] (it saves internally — there is no separate
/// `save`). Masters-first is enforced INSIDE libloot; do not hand-roll it (D-08).
///
/// # Errors
/// [`LoadOrderError::Loot`] if libloot fails to read the existing state or to
/// set/persist the new order.
pub fn set_order_and_save(game: &mut Game, order: &[&str]) -> Result<(), LoadOrderError> {
    game.load_current_load_order_state()
        .map_err(|e| LoadOrderError::Loot(e.to_string()))?;
    game.set_load_order(order)
        .map_err(|e| LoadOrderError::Loot(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appdata_local_path_builds_the_proton_appdata_subpath() {
        let p = appdata_local_path(Path::new("/pfx"), "Fallout4");
        assert_eq!(
            p,
            Path::new("/pfx/drive_c/users/steamuser/AppData/Local/Fallout4")
        );
    }

    #[test]
    fn game_type_for_allow_lists_only_the_two_supported_games() {
        assert!(matches!(game_type_for(SKYRIM_SE), Some(GameType::SkyrimSE)));
        assert!(matches!(game_type_for(FALLOUT4), Some(GameType::Fallout4)));
        assert!(game_type_for(0).is_none());
        assert!(game_type_for(220).is_none());
    }

    #[test]
    fn open_game_rejects_unsupported_appid_as_no_local_appdata() {
        let err = open_game(220, Path::new("/nonexistent"), Path::new("/tmp/x")).unwrap_err();
        assert!(matches!(err, LoadOrderError::NoLocalAppData(_)));
    }

    #[test]
    fn open_game_rejects_empty_appdata_as_no_local_appdata() {
        let err = open_game(SKYRIM_SE, Path::new("/nonexistent"), Path::new("")).unwrap_err();
        assert!(matches!(err, LoadOrderError::NoLocalAppData(_)));
    }
}
