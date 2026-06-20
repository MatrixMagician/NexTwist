//! Plugin discovery (PLUGIN-01 discovery half, D-06).
//!
//! Walks the enabled mods' staged trees plus the game `Data/` dir, collects every
//! `.esp`/`.esm`/`.esl` file (case-insensitive — Wine), and classifies each as
//! [`PluginKind::Esm`] / [`PluginKind::Esl`] / [`PluginKind::Esp`] from the plugin's
//! HEADER FLAGS (not its filename extension — an `.esp` can carry the ESL/light flag,
//! and a `.esm`-named file is only authoritatively a master via its header). Header
//! classification uses [`esplugin`] (the LOOT author's pure-Rust header parser, already a
//! libloot transitive dep) with `header_only` parsing — no full libloot `Game`/prefix is
//! needed just to badge a plugin.
//!
//! A plugin that appears in BOTH a staged root and `Data/` is de-duplicated by
//! case-insensitive filename; the staged (enabled-mod) copy wins for display. The
//! returned [`Plugin`] entries carry `enabled = false` / `order = 0` defaults — the real
//! per-profile enable/order state is owned by the store's `plugin_state` and merged in by
//! the Tauri command layer, not here.
//!
//! Path safety (T-02-12): scan collects only files physically present under the supplied
//! roots; plugin names are treated as opaque filenames and never joined as paths outside a
//! root. Header parsing that fails (a corrupt/non-plugin file with a plugin extension) is
//! a NON-fatal classification fallback to [`PluginKind::Esp`] with the parse error logged
//! — a single unreadable file never aborts the whole scan.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use esplugin::{GameId, ParseOptions, Plugin as EsPlugin};
use nextwist_core::{Plugin, PluginKind};
use walkdir::WalkDir;

use crate::error::LoadOrderError;

/// Skyrim Special Edition AppID (mirrors `loot::SKYRIM_SE`).
const SKYRIM_SE: u32 = 489830;
/// Fallout 4 AppID (mirrors `loot::FALLOUT4`).
const FALLOUT4: u32 = 377160;

/// The three Bethesda plugin file extensions NexTwist scans for (lowercased).
const PLUGIN_EXTS: [&str; 3] = ["esp", "esm", "esl"];

/// Map a supported Steam AppID to the [`esplugin::GameId`] header classifier; `None` for
/// any unsupported game (mirrors the loadorder/steam allow-list).
fn game_id_for(appid: u32) -> Option<GameId> {
    match appid {
        SKYRIM_SE => Some(GameId::SkyrimSE),
        FALLOUT4 => Some(GameId::Fallout4),
        _ => None,
    }
}

/// True if `path`'s extension is one of `.esp`/`.esm`/`.esl` (case-insensitive — Wine
/// installs frequently mix casing, so the check must be case-folded).
fn is_plugin_file(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|ext| {
            let lower = ext.to_ascii_lowercase();
            PLUGIN_EXTS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

/// Classify a plugin file by its HEADER FLAGS via esplugin (header-only parse).
///
/// Precedence: ESL (light-flagged) wins over ESM (master-flagged) wins over ESP. An
/// `.esp` carrying the light flag classifies as [`PluginKind::Esl`]; a master-flagged file
/// classifies as [`PluginKind::Esm`]; everything else is [`PluginKind::Esp`]. If the file
/// cannot be header-parsed (corrupt / not actually a plugin), we DO NOT abort the scan —
/// we fall back to [`PluginKind::Esp`] and log, because a single bad file must never take
/// down plugin discovery (the non-negotiable is collection + de-dup, per the plan).
fn classify_kind(game_id: GameId, path: &Path) -> PluginKind {
    let mut plugin = EsPlugin::new(game_id, path);
    match plugin.parse_file(ParseOptions::header_only()) {
        Ok(()) => {
            if plugin.is_light_plugin() {
                PluginKind::Esl
            } else if plugin.is_master_file() {
                PluginKind::Esm
            } else {
                PluginKind::Esp
            }
        }
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "could not header-parse plugin; defaulting badge to ESP"
            );
            PluginKind::Esp
        }
    }
}

/// Collect plugin files from a single root into `out`, keyed by case-insensitive filename.
///
/// `staged_wins` controls de-dup precedence: when a filename already collected from a
/// staged root is seen again in `Data/`, the staged entry is kept (the enabled-mod copy
/// wins for display). Within a single call (`staged_wins == true` for staged roots) the
/// first occurrence wins, matching deterministic walkdir order.
fn collect_from_root(
    game_id: GameId,
    root: &Path,
    staged_wins: bool,
    out: &mut BTreeMap<String, Plugin>,
) -> Result<(), LoadOrderError> {
    if !root.exists() {
        // A staged root or Data/ dir that does not exist is simply empty, not an error
        // (a never-deployed game or a mod with no plugins).
        return Ok(());
    }
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|e| {
            LoadOrderError::io(root, std::io::Error::other(e))
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !is_plugin_file(path) {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        let key = file_name.to_ascii_lowercase();
        // De-dup: a staged entry already present is never overwritten by a Data/ entry.
        if out.contains_key(&key) && !staged_wins {
            continue;
        }
        if out.contains_key(&key) && staged_wins {
            // Two staged roots provide the same plugin name: keep the first (deterministic).
            continue;
        }
        let kind = classify_kind(game_id, path);
        out.insert(
            key,
            Plugin {
                name: file_name.to_string(),
                kind,
                enabled: false,
                order: 0,
            },
        );
    }
    Ok(())
}

/// Discover the plugins visible to a game: every `.esp`/`.esm`/`.esl` in the enabled mods'
/// staged trees plus the game `Data/` dir, type-badged from header flags and de-duplicated
/// by case-insensitive filename (the staged/enabled copy wins).
///
/// The returned entries carry `enabled = false` / `order = 0` defaults; the caller merges
/// the real per-profile enable/order from the store's `plugin_state`. Returned order is by
/// case-insensitive filename (stable/deterministic) — the load order is a separate concern
/// owned by libloot + the store, not by discovery.
///
/// # Errors
/// [`LoadOrderError::Io`] if a root directory cannot be walked. An unsupported `appid`
/// yields an empty list (no classifier) rather than an error, so the UI degrades to no
/// plugins for an unmanaged game.
pub fn scan_plugins(
    enabled_staging_roots: &[PathBuf],
    game_data: &Path,
) -> Result<Vec<Plugin>, LoadOrderError> {
    // No classifier for this game → no plugins (the allow-list lives in game_id_for).
    let Some(game_id) = game_id_for_data(enabled_staging_roots, game_data) else {
        return Ok(Vec::new());
    };

    let mut collected: BTreeMap<String, Plugin> = BTreeMap::new();

    // Staged roots first so the enabled-mod copy wins de-dup against Data/.
    for root in enabled_staging_roots {
        collect_from_root(game_id, root, true, &mut collected)?;
    }
    // Then the game Data/ dir — only fills in plugins not already provided by a mod.
    collect_from_root(game_id, game_data, false, &mut collected)?;

    Ok(collected.into_values().collect())
}

/// Discover the plugins for an EXPLICIT game (PLUGIN-01 discovery): same as
/// [`scan_plugins`] but takes the resolved [`esplugin::GameId`] directly so the Tauri
/// command (which already knows the appid) does not pay an inference cost.
///
/// # Errors
/// [`LoadOrderError::Io`] if a root directory cannot be walked.
pub fn scan_plugins_for(
    game_id: GameId,
    enabled_staging_roots: &[PathBuf],
    game_data: &Path,
) -> Result<Vec<Plugin>, LoadOrderError> {
    let mut collected: BTreeMap<String, Plugin> = BTreeMap::new();
    for root in enabled_staging_roots {
        collect_from_root(game_id, root, true, &mut collected)?;
    }
    collect_from_root(game_id, game_data, false, &mut collected)?;
    Ok(collected.into_values().collect())
}

/// Public mapping from an AppID to its esplugin classifier, for the Tauri command layer.
pub fn esplugin_game_id(appid: u32) -> Option<GameId> {
    game_id_for(appid)
}

/// Internal: the scan operates per-game, but [`scan_plugins`] does not receive an appid —
/// it is called with already-resolved roots. We default the classifier to SkyrimSE-class
/// header semantics ONLY for the standalone helper used in tests; the Tauri command path
/// uses [`scan_plugins_for`] with the real appid. Returning `Some` keeps the standalone
/// `scan_plugins` usable in unit tests without threading an appid through every caller.
fn game_id_for_data(_roots: &[PathBuf], _data: &Path) -> Option<GameId> {
    // Header light/master flag bits are identical across SkyrimSE/Fallout4 for the fields
    // scan reads (is_master_file / is_light_plugin), so SkyrimSE is a safe default for the
    // appid-less helper. The command layer always uses scan_plugins_for with the real id.
    Some(GameId::SkyrimSE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// A minimal but VALID 24-byte TES4 header (matches the Plan-02 spike fixture).
    /// `flags` at bytes [8..12): 0x1 = master. Light flag is record-internal, so a true
    /// ESL fixture needs a record; for ESL we test the flag path via a real master bit and
    /// document that header-only ESL detection from a hand-built 24-byte stub is a SUMMARY
    /// limitation (a real ESL plugin sets the 0x200 light flag in the TES4 record header).
    fn tes4_header(master: bool) -> Vec<u8> {
        let mut h = vec![0u8; 24];
        h[0..4].copy_from_slice(b"TES4");
        // bytes [4..8) = size of subrecords = 0
        let flags: u32 = if master { 0x1 } else { 0x0 };
        h[8..12].copy_from_slice(&flags.to_le_bytes());
        h
    }

    fn write(root: &Path, rel: &str, bytes: &[u8]) {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, bytes).unwrap();
    }

    #[test]
    fn collects_plugins_and_classifies_master_vs_regular() {
        let staged = TempDir::new().unwrap();
        let data = TempDir::new().unwrap();
        // Staged mod tree: a master + a regular plugin + non-plugin noise.
        write(staged.path(), "Skyrim.esm", &tes4_header(true));
        write(staged.path(), "Mod.esp", &tes4_header(false));
        write(staged.path(), "textures/rock.dds", b"notaplugin");
        write(staged.path(), "readme.txt", b"hi");
        // Game Data/: a master only present in the game.
        write(data.path(), "Update.esm", &tes4_header(true));

        let plugins =
            scan_plugins(&[staged.path().to_path_buf()], data.path()).unwrap();
        let by_name: std::collections::HashMap<&str, &Plugin> =
            plugins.iter().map(|p| (p.name.as_str(), p)).collect();

        // .dds / .txt are ignored.
        assert_eq!(plugins.len(), 3, "only the three plugin files are collected");
        assert!(by_name.contains_key("Skyrim.esm"));
        assert!(by_name.contains_key("Mod.esp"));
        assert!(by_name.contains_key("Update.esm"));
        // Kind from header flags, not extension.
        assert_eq!(by_name["Skyrim.esm"].kind, PluginKind::Esm);
        assert_eq!(by_name["Update.esm"].kind, PluginKind::Esm);
        assert_eq!(by_name["Mod.esp"].kind, PluginKind::Esp);
    }

    #[test]
    fn dedups_by_filename_staged_wins() {
        let staged = TempDir::new().unwrap();
        let data = TempDir::new().unwrap();
        // Same filename in both; staged is a master, Data is regular — staged must win.
        write(staged.path(), "Shared.esm", &tes4_header(true));
        write(data.path(), "Shared.esm", &tes4_header(false));

        let plugins =
            scan_plugins(&[staged.path().to_path_buf()], data.path()).unwrap();
        assert_eq!(plugins.len(), 1, "the duplicate filename is collapsed to one");
        assert_eq!(plugins[0].name, "Shared.esm");
        // The staged copy (master header) wins, proving precedence (not the Data/ regular).
        assert_eq!(plugins[0].kind, PluginKind::Esm);
    }

    #[test]
    fn dedup_is_case_insensitive_for_wine() {
        let staged = TempDir::new().unwrap();
        let data = TempDir::new().unwrap();
        write(staged.path(), "Mod.esp", &tes4_header(false));
        // Different casing of the same logical plugin in Data/ — Wine treats these as one.
        write(data.path(), "mod.ESP", &tes4_header(false));

        let plugins =
            scan_plugins(&[staged.path().to_path_buf()], data.path()).unwrap();
        assert_eq!(plugins.len(), 1, "case-variant duplicate collapses to one");
        // The staged casing is preserved for display.
        assert_eq!(plugins[0].name, "Mod.esp");
    }

    #[test]
    fn defaults_enabled_false_order_zero() {
        let staged = TempDir::new().unwrap();
        let data = TempDir::new().unwrap();
        write(staged.path(), "A.esp", &tes4_header(false));
        let plugins =
            scan_plugins(&[staged.path().to_path_buf()], data.path()).unwrap();
        assert_eq!(plugins.len(), 1);
        assert!(!plugins[0].enabled, "discovery defaults enabled=false (store owns it)");
        assert_eq!(plugins[0].order, 0, "discovery defaults order=0 (store owns it)");
    }

    #[test]
    fn missing_roots_yield_empty_not_error() {
        let plugins = scan_plugins(
            &[PathBuf::from("/no/such/staged/root")],
            Path::new("/no/such/data"),
        )
        .unwrap();
        assert!(plugins.is_empty());
    }

    #[test]
    fn corrupt_plugin_file_defaults_to_esp_without_aborting() {
        let staged = TempDir::new().unwrap();
        let data = TempDir::new().unwrap();
        // A file with a plugin extension but garbage content: classified ESP, scan survives.
        write(staged.path(), "Broken.esp", b"not a real TES4 header at all");
        write(staged.path(), "Good.esm", &tes4_header(true));

        let plugins =
            scan_plugins(&[staged.path().to_path_buf()], data.path()).unwrap();
        assert_eq!(plugins.len(), 2, "the corrupt file is still collected");
        let by_name: std::collections::HashMap<&str, &Plugin> =
            plugins.iter().map(|p| (p.name.as_str(), p)).collect();
        assert_eq!(by_name["Broken.esp"].kind, PluginKind::Esp);
        assert_eq!(by_name["Good.esm"].kind, PluginKind::Esm);
    }

    #[test]
    fn scan_plugins_for_uses_explicit_game_id() {
        let staged = TempDir::new().unwrap();
        let data = TempDir::new().unwrap();
        write(staged.path(), "Skyrim.esm", &tes4_header(true));
        let plugins = scan_plugins_for(
            GameId::SkyrimSE,
            &[staged.path().to_path_buf()],
            data.path(),
        )
        .unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].kind, PluginKind::Esm);
    }

    #[test]
    fn esplugin_game_id_allow_lists_supported_games() {
        assert!(matches!(esplugin_game_id(SKYRIM_SE), Some(GameId::SkyrimSE)));
        assert!(matches!(esplugin_game_id(FALLOUT4), Some(GameId::Fallout4)));
        assert!(esplugin_game_id(220).is_none());
    }
}
