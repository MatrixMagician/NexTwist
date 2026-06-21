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
//! * `Game::load_order(&self) -> Vec<&str>` — libloot's resolved order after a load. Its
//!   leading run of *early-loading* / *implicitly-active* plugins (game master + the game's
//!   hardcoded DLC list + Creation-Club `*.ccc` plugins) is placed at REQUIRED fixed
//!   positions; NexTwist must defer to it, not hand-roll that prefix (debug
//!   `loadorder-active-write`, RC1).
//! * `Game::set_load_order(&mut self, &[&str]) -> Result<(), LoadOrderError>` — sets
//!   AND persists the order (it calls `save()` internally; there is NO separate
//!   `Game::save`). Masters-first is enforced INTERNALLY by libloot (D-08), BUT the order
//!   passed MUST keep libloot's fixed early-loader prefix or it rejects with
//!   `"load order interaction failed"`.
//! * `Game::active_plugins_file_path(&self) -> &PathBuf` — the exact Plugins.txt path
//!   libloot reads/writes; for SkyrimSE it is `<local_path>/Plugins.txt`, asterisk
//!   format (`*Active.esp`).
//! * `Game::is_plugin_active(&self, &str) -> bool`.
//! * `GameType::{SkyrimSE, Fallout4}`.
//!
//! Active-state seam: the public `Game` API exposes load-order and an active-state *query*,
//! but no active-state *setter*. A plugin's active flag enters through the Plugins.txt that
//! libloot loads (in NexTwist, generated from the DB `plugin_state`); `set_load_order`
//! preserves the active state of already-loaded plugins (libloadorder clones the loaded
//! `Plugin`, keeping its active flag — verified against live FO4 data). NOTE: *early-loading*
//! plugins (game master + hardcoded DLC + CCC) are NEVER written to Plugins.txt by libloot's
//! `save()` — they are implicitly active — so a load order made ENTIRELY of such plugins
//! correctly produces an EMPTY Plugins.txt; only regular `.esp` / non-CCC `.esl` mods appear
//! as `*Name`. libloot/libloadorder also open and header-parse every named plugin (esplugin
//! `header_only`), so every plugin in a load order must physically exist in the game `Data/`
//! dir with at least a valid 24-byte TES4 header.

use std::path::{Path, PathBuf};

use libloot::{Game, GameType};
use nextwist_core::{Plugin, PluginKind};

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

/// The Steam AppData/Local folder name for a supported AppID — the `<game_name>` segment
/// [`appdata_local_path`] joins (A3). `"Skyrim Special Edition"` / `"Fallout4"`, matching
/// libloadorder's `skyrim_se_appdata_folder_name` / `fallout4_appdata_folder_name`. `None`
/// for an unsupported game (the command layer maps `None` to a clear boundary error).
pub fn appdata_folder_name(appid: u32) -> Option<&'static str> {
    match appid {
        SKYRIM_SE => Some("Skyrim Special Edition"),
        FALLOUT4 => Some("Fallout4"),
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

/// Load the current load-order state and return libloot's canonical resolved order.
///
/// After reading the seeded Plugins.txt, libloot/libloadorder place every
/// *early-loading* / *implicitly-active* plugin (the game master plus the game's
/// hardcoded DLC list and the Creation-Club `*.ccc` plugins) at its REQUIRED fixed
/// position, ahead of everything else, and append the remaining installed plugins.
/// This returned order is therefore the only sequence libloot will accept for the
/// early-loader prefix; hand-rolling it (e.g. an alphabetical master sort) reorders
/// those fixed plugins and makes `set_load_order` reject with
/// `"load order interaction failed"` (debug `loadorder-active-write`, RC1).
///
/// # Errors
/// [`LoadOrderError::Loot`] if libloot fails to read the existing state.
pub fn load_canonical_order(game: &mut Game) -> Result<Vec<String>, LoadOrderError> {
    game.load_current_load_order_state()
        .map_err(|e| LoadOrderError::Loot(e.to_string()))?;
    Ok(game.load_order().iter().map(|s| (*s).to_string()).collect())
}

/// Set the given order and persist it (libloot saves internally — no separate `save`).
///
/// Masters-first is enforced INSIDE libloot (D-08). The `order` MUST keep libloot's
/// own fixed early-loader prefix (see [`load_canonical_order`]); only the trailing
/// non-early-loader plugins may be reordered, or libloot rejects the order.
///
/// # Errors
/// [`LoadOrderError::Loot`] if libloot fails to set/persist the new order.
pub fn set_order_and_save(game: &mut Game, order: &[&str]) -> Result<(), LoadOrderError> {
    game.set_load_order(order)
        .map_err(|e| LoadOrderError::Loot(e.to_string()))?;
    Ok(())
}

/// Reconcile libloot's canonical order with the user's desired order for movable plugins.
///
/// `canonical` is libloot's post-load order: its *early-loader* plugins (game master +
/// hardcoded DLC + Creation-Club `*.ccc` plugins) sit at REQUIRED fixed positions that
/// libloadorder validates; reordering them is rejected (`"load order interaction failed"`,
/// RC1). Those plugins are all in the master group (`.esm` / `.esl`) and NexTwist must
/// defer their relative order to libloot.
///
/// `user_movable` is the user's desired order for the plugins they actually control —
/// regular `.esp` plugins (and any other non-master-group plugin). These are NOT
/// early-loaders, so libloot lets them be reordered freely (subject to master deps it also
/// validates). We keep every master-group plugin at its canonical position and emit the
/// `user_movable` plugins, in the user's order, in the canonical slots those plugins
/// occupied. Plugins not present in `canonical` (libloot did not load them) are ignored.
///
/// This guarantees the early-loader prefix is byte-for-byte libloot's own order (always
/// accepted) while honouring the user's relative order for the regular mods.
fn reconcile_order(canonical: &[String], user_movable: &[String]) -> Vec<String> {
    let canonical_set: std::collections::HashSet<&str> =
        canonical.iter().map(String::as_str).collect();
    let movable_set: std::collections::HashSet<&str> =
        user_movable.iter().map(String::as_str).collect();

    // The movable plugins in the USER's order, restricted to plugins libloot actually loaded.
    let mut movable_iter = user_movable
        .iter()
        .filter(|m| canonical_set.contains(m.as_str()))
        .cloned();

    // Walk canonical: master-group / non-movable plugins keep their slot; each movable slot
    // is filled from the user-ordered movable sequence (slot count == movable count).
    let mut out: Vec<String> = Vec::with_capacity(canonical.len());
    for name in canonical {
        if movable_set.contains(name.as_str()) {
            if let Some(next) = movable_iter.next() {
                out.push(next);
            }
        } else {
            out.push(name.clone());
        }
    }
    out
}

/// True if a plugin kind is part of the master group (sorts ahead of regular `.esp`).
fn is_master_group(kind: PluginKind) -> bool {
    matches!(kind, PluginKind::Esm | PluginKind::Esl)
}

/// Order a desired plugin set masters-first (`.esm`/ESL before `.esp`), preserving each
/// plugin's relative `order` within its group. libloot ALSO enforces masters-first
/// internally on `set_load_order` (D-08), so this is belt-and-suspenders that also gives a
/// deterministic, masters-first order argument; we never rely on this as the sole guard.
///
/// Returns the plugin NAMES in the masters-first order.
pub fn masters_first_order(plugins: &[Plugin]) -> Vec<String> {
    let mut sorted: Vec<&Plugin> = plugins.iter().collect();
    sorted.sort_by(|a, b| {
        let a_master = is_master_group(a.kind);
        let b_master = is_master_group(b.kind);
        // Masters group first, then by the plugin's own order, then by name for stability.
        b_master
            .cmp(&a_master)
            .then(a.order.cmp(&b.order))
            .then_with(|| a.name.cmp(&b.name))
    });
    sorted.into_iter().map(|p| p.name.clone()).collect()
}

/// Render the asterisk-format active-plugins file body for a desired plugin set.
///
/// SkyrimSE/Fallout4 use the asterisk method: an ENABLED plugin is written with a leading
/// `*`; a disabled plugin is written WITHOUT the asterisk (libloot's format keeps the line
/// so the relative order is recorded but the plugin is inactive). The lines are in
/// masters-first order to match what libloot persists.
///
/// This is the ONLY place NexTwist materializes active flags, and only as a SEED that
/// libloot then re-reads/re-writes — we are NOT hand-rolling the canonical format, we are
/// feeding libloot its own input (the Plan-02 spike proved this round-trips).
fn asterisk_plugins_txt(plugins: &[Plugin]) -> String {
    // Preserve a name -> enabled lookup, then walk the masters-first order.
    let order = masters_first_order(plugins);
    let mut body = String::new();
    for name in &order {
        let enabled = plugins
            .iter()
            .find(|p| &p.name == name)
            .map(|p| p.enabled)
            .unwrap_or(false);
        if enabled {
            body.push('*');
        }
        body.push_str(name);
        body.push('\n');
    }
    body
}

/// Apply a desired plugin enable/order set: write the asterisk-format Plugins.txt at
/// libloot's `active_plugins_file_path` inside the Proton prefix and persist the load
/// order via libloot. Returns the written Plugins.txt path.
///
/// Sequence (the verified seam — there is no active-plugin setter in libloot 0.29.5, so
/// active state enters via the Plugins.txt libloot loads, and the order must respect
/// libloot's own fixed early-loader sequence — debug `loadorder-active-write`):
///   1. `open_game` (with_local_path; creates the AppData dir — Pitfall 2),
///   2. SEED the asterisk Plugins.txt from the desired `plugins` (enabled → `*Name`),
///   3. `load_canonical_order` (`load_current_load_order_state` + read libloot's resolved
///      order — early-loaders / implicitly-active plugins are placed at their REQUIRED
///      fixed positions: game master, then the game's hardcoded DLC list, then CCC),
///   4. [`reconcile_order`]: keep that fixed early-loader prefix verbatim and splice the
///      user's desired order in for the plugins the user actually controls,
///   5. `set_order_and_save` (libloot also enforces masters-first internally, D-08, and
///      persists — there is NO separate `Game::save`).
///
/// Why NOT a hand-rolled masters-first sort: with every store `order_index == 0` the old
/// alphabetical master sort reordered Fallout 4's hardcoded DLC list (e.g. `DLCCoast`
/// before `DLCRobot`), which libloadorder rejects (`"load order interaction failed"`).
/// Deferring the early-loader order to libloot fixes that at the root (RC1). The asterisk
/// seam itself was never broken: an all-DLC/CCC set legitimately writes an EMPTY Plugins.txt
/// (those plugins are implicitly active and intentionally omitted from the file), while a
/// regular `.esp` / non-CCC `.esl` mod's active flag persists as `*Name` (RC2 was a
/// misdiagnosis — verified against live FO4 data).
///
/// `plugins` whose files do not physically exist under the game `Data/` are dropped before
/// calling libloot, because libloot/libloadorder header-parse every named plugin and error
/// on a missing file. Only on-disk plugins are ordered; this keeps a stale store entry from
/// aborting the whole write.
///
/// # Errors
/// * [`LoadOrderError::NoLocalAppData`] for an unresolved prefix / unsupported appid.
/// * [`LoadOrderError::Io`] if the seed Plugins.txt cannot be written.
/// * [`LoadOrderError::Loot`] if libloot fails to load or persist the order.
pub fn apply_load_order(
    appid: u32,
    install_dir: &Path,
    appdata_local: &Path,
    plugins: &[Plugin],
) -> Result<PathBuf, LoadOrderError> {
    let mut game = open_game(appid, install_dir, appdata_local)?;

    // libloot/libloadorder header-parse every named plugin and error on a missing file,
    // so only order plugins that actually exist on disk (in Data/ or as the active file's
    // resolved data path). A stale store row for a removed mod must not abort the write.
    let data_dir = install_dir.join("Data");
    let on_disk: Vec<Plugin> = plugins
        .iter()
        .filter(|p| data_dir.join(&p.name).is_file())
        .cloned()
        .collect();

    // Seed the asterisk active-plugins file libloot will read (active state seam).
    let active_file = game.active_plugins_file_path().clone();
    if let Some(parent) = active_file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| LoadOrderError::io(parent, e))?;
    }
    std::fs::write(&active_file, asterisk_plugins_txt(&on_disk))
        .map_err(|e| LoadOrderError::io(&active_file, e))?;

    // Load the seeded active state and read libloot's canonical order (early-loaders at
    // their required fixed positions). The user only controls the relative order of the
    // NON-master-group plugins (regular `.esp` mods); master-group plugins (`.esm`/`.esl`,
    // which include the game master + hardcoded DLC + CCC early-loaders) defer to libloot.
    // NEVER hand-roll the early-loader order (RC1).
    let canonical = load_canonical_order(&mut game)?;
    let user_movable: Vec<String> = on_disk
        .iter()
        .filter(|p| !is_master_group(p.kind))
        .map(|p| p.name.clone())
        .collect();
    let order = reconcile_order(&canonical, &user_movable);
    let order_refs: Vec<&str> = order.iter().map(String::as_str).collect();
    set_order_and_save(&mut game, &order_refs)?;

    Ok(game.active_plugins_file_path().clone())
}

/// A LOOT sort proposal: the suggested order plus any critical warnings (PLUGIN-03, D-12).
///
/// `proposed` is the order libloot's `sort_plugins` returns — it is a SUGGESTION the UI
/// shows for review; NOTHING is written until the user confirms (then [`apply_load_order`]
/// is called separately). `warnings` are the masterlist's Warn/Error-level general
/// messages (dirty plugins / missing masters etc., A2) surfaced for the review; an empty
/// list means libloot reported no critical messages for the loaded set.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SortProposal {
    /// The proposed plugin order (libloot `sort_plugins` output). Writes nothing.
    pub proposed: Vec<String>,
    /// Critical (Warn/Error) masterlist messages to surface above the proposal.
    pub warnings: Vec<String>,
}

/// Propose a LOOT-sorted order WITHOUT writing anything (D-12: propose-then-apply).
///
/// Ensures the masterlist is available (fetch/cache/bundled fallback), loads it into
/// libloot's `Database`, header-loads the on-disk plugins, and runs `sort_plugins`. The
/// returned [`SortProposal`] also carries the masterlist's critical (Warn/Error) general
/// messages (A2). Applying the proposal is a SEPARATE, user-confirmed call to
/// [`apply_load_order`] — this function never persists.
///
/// Only plugins whose files exist under the game `Data/` are sorted (libloot header-parses
/// each named plugin; a stale entry must not abort the sort).
///
/// # Errors
/// * [`LoadOrderError::NoLocalAppData`] / unsupported appid via [`open_game`].
/// * [`LoadOrderError::Loot`] if masterlist load, plugin load, or sort fails.
pub fn propose_sort(
    appid: u32,
    install_dir: &Path,
    appdata_local: &Path,
    app_data: &Path,
    plugins: &[Plugin],
) -> Result<SortProposal, LoadOrderError> {
    let mut game = open_game(appid, install_dir, appdata_local)?;
    game.load_current_load_order_state()
        .map_err(|e| LoadOrderError::Loot(e.to_string()))?;

    // Resolve + load the masterlist into the game's Database (fetch/cache/bundled).
    let masterlist = crate::masterlist::ensure_masterlist(app_data, appid, false)?;
    {
        let db = game.database();
        let mut db = db
            .write()
            .map_err(|e| LoadOrderError::Loot(format!("database lock poisoned: {e}")))?;
        db.load_masterlist(&masterlist)
            .map_err(|e| LoadOrderError::Loot(e.to_string()))?;
    }

    // Header-load only the plugins that physically exist in Data/ (libloot parses each).
    let data_dir = install_dir.join("Data");
    let on_disk: Vec<&Plugin> = plugins
        .iter()
        .filter(|p| data_dir.join(&p.name).is_file())
        .collect();
    let paths: Vec<PathBuf> = on_disk.iter().map(|p| data_dir.join(&p.name)).collect();
    let path_refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
    game.load_plugins(&path_refs)
        .map_err(|e| LoadOrderError::Loot(e.to_string()))?;

    let names: Vec<&str> = on_disk.iter().map(|p| p.name.as_str()).collect();
    let proposed = game
        .sort_plugins(&names)
        .map_err(|e| LoadOrderError::Loot(e.to_string()))?;

    let warnings = critical_warnings(&game);
    Ok(SortProposal { proposed, warnings })
}

/// Extract the masterlist's critical (Warn/Error) general messages for the review (A2).
///
/// Reads `Database::general_messages` (masterlist only, conditions evaluated) and keeps
/// only Warn/Error severities, rendered to plain text. If the database lock is poisoned or
/// condition evaluation fails, returns an empty list rather than failing the sort — the
/// proposed order is the load-bearing output; warnings are advisory (A2 fallback).
fn critical_warnings(game: &Game) -> Vec<String> {
    use libloot::metadata::MessageType;
    use libloot::{EvalMode, MergeMode};

    let db_arc = game.database();
    let db = match db_arc.read() {
        Ok(db) => db,
        Err(_) => return Vec::new(),
    };
    let messages = match db.general_messages(MergeMode::WithoutUserMetadata, EvalMode::Evaluate) {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };
    messages
        .into_iter()
        .filter(|m| matches!(m.message_type(), MessageType::Warn | MessageType::Error))
        .map(|m| {
            let text = m
                .content()
                .first()
                .map(|c| c.text().to_string())
                .unwrap_or_default();
            format!("{}: {}", m.message_type(), text)
        })
        .collect()
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
