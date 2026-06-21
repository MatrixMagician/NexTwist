//! Install-dir resolution + Proton-prefix derivation (ENV-02, ENV-03).
//!
//! steamlocate returns the `(App, Library)` pair but exposes NO compatdata API, so
//! the Proton prefix `compatdata/<appid>/pfx` is derived MANUALLY here from the
//! library root (RESEARCH.md Pitfall 5). Resolution honors `$STEAM_COMPAT_DATA_PATH`
//! when set and re-resolves on every call (paths can move — never cached to disk).
//!
//! Only the two supported Bethesda AppIDs are accepted (allow-list, ENV-03).

use std::path::{Path, PathBuf};

use nextwist_core::Game;
use serde::Deserialize;

use crate::error::SteamError;

/// Skyrim Special Edition AppID.
pub const SKYRIM_SE: u32 = 489830;
/// Fallout 4 AppID.
pub const FALLOUT4: u32 = 377160;

/// The complete allow-list of supported games (ENV-03).
pub const SUPPORTED_APPIDS: &[u32] = &[SKYRIM_SE, FALLOUT4];

/// Display name for a supported AppID (used when the manifest omits one).
fn default_name(appid: u32) -> &'static str {
    match appid {
        SKYRIM_SE => "Skyrim Special Edition",
        FALLOUT4 => "Fallout 4",
        _ => "Unknown Game",
    }
}

/// True iff `appid` is one of the two supported Bethesda games.
pub fn is_supported(appid: u32) -> bool {
    SUPPORTED_APPIDS.contains(&appid)
}

/// A fully resolved game: the install directory and the derived Proton prefix.
///
/// `prefix_exists` records whether the derived `compatdata/<appid>/pfx` actually
/// exists on disk yet (Proton creates it on first launch). A resolved-but-missing
/// prefix is surfaced as a warning by the caller, not an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedGame {
    /// Steam AppID.
    pub appid: u32,
    /// Human-readable name.
    pub name: String,
    /// Resolved install dir (`.../steamapps/common/<install_dir>`).
    pub install_dir: PathBuf,
    /// Derived Proton prefix (`.../steamapps/compatdata/<appid>/pfx`), or the
    /// `$STEAM_COMPAT_DATA_PATH` override joined with `pfx`.
    pub prefix: PathBuf,
    /// Whether the derived prefix directory currently exists on disk.
    pub prefix_exists: bool,
}

impl ResolvedGame {
    /// Convert into a [`core::Game`]. `staging_dir` is chosen by the caller (Plan 06
    /// suggests a same-filesystem path); here we default it to a `.nextwist-staging`
    /// sibling of the install dir so the struct is complete and on the same FS.
    pub fn into_game(self) -> Game {
        let staging_dir = self
            .install_dir
            .parent()
            .map(|p| p.join(format!(".nextwist-staging/{}", self.appid)))
            .unwrap_or_else(|| PathBuf::from(format!(".nextwist-staging/{}", self.appid)));
        Game {
            appid: self.appid,
            name: self.name,
            install_dir: self.install_dir,
            prefix: self.prefix,
            staging_dir,
        }
    }
}

/// Resolve a supported game by AppID using the real Steam installation.
///
/// Rejects non-allow-listed AppIDs ([`SteamError::Unsupported`]) before touching the
/// filesystem. Calls `find_app` across all detected roots, builds the install dir, and
/// derives the Proton prefix manually. Honors `$STEAM_COMPAT_DATA_PATH`.
pub fn resolve_game(appid: u32) -> Result<ResolvedGame, SteamError> {
    if !is_supported(appid) {
        return Err(SteamError::Unsupported(appid));
    }

    // Re-resolve every call (paths can move; never cache to disk).
    let roots = steamlocate::locate_all().map_err(|e| SteamError::Locate(e.to_string()))?;
    let mut roots = roots;
    if let Some(flatpak) = crate::discover::flatpak_steam_root()
        && flatpak.is_dir()
        && !roots.iter().any(|d| d.path() == flatpak)
        && let Ok(extra) = steamlocate::SteamDir::from_dir(&flatpak)
    {
        roots.push(extra);
    }
    if roots.is_empty() {
        return Err(SteamError::NoSteam);
    }

    for steam in &roots {
        match steam.find_app(appid) {
            Ok(Some((app, library))) => {
                let name = app
                    .name
                    .clone()
                    .unwrap_or_else(|| default_name(appid).to_string());
                return Ok(build_resolved(
                    appid,
                    &name,
                    library.path(),
                    &app.install_dir,
                ));
            }
            Ok(None) => {}
            Err(e) => {
                tracing::debug!(appid, error = %e, "find_app failed for a root; trying next");
            }
        }
    }

    Err(SteamError::NotInstalled(appid))
}

/// Internal test seam: resolve a supported game from an explicit library root,
/// reading the `appmanifest_<appid>.acf` for `installdir`.
///
/// This is what the integration test injects a synthetic fixture root into, so CI
/// never depends on the host's real Steam install. The public [`resolve_game`]
/// delegates equivalent logic through steamlocate.
pub fn resolve_from_root(library_root: &Path, appid: u32) -> Result<ResolvedGame, SteamError> {
    if !is_supported(appid) {
        return Err(SteamError::Unsupported(appid));
    }

    let manifest = library_root
        .join("steamapps")
        .join(format!("appmanifest_{appid}.acf"));
    let raw = std::fs::read_to_string(&manifest).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            SteamError::NotInstalled(appid)
        } else {
            SteamError::Io {
                path: manifest.clone(),
                source,
            }
        }
    })?;

    let app: AppManifest =
        keyvalues_serde::from_str(&raw).map_err(|e| SteamError::Locate(e.to_string()))?;

    let name = app
        .name
        .clone()
        .unwrap_or_else(|| default_name(appid).to_string());

    Ok(build_resolved(appid, &name, library_root, &app.installdir))
}

/// The subset of `appmanifest_<id>.acf` fields we need. The ACF root key is
/// `AppState`; keyvalues-serde deserializes that inner object directly.
#[derive(Debug, Deserialize)]
struct AppManifest {
    installdir: String,
    name: Option<String>,
}

/// Build a [`ResolvedGame`] from a library root + the app's `installdir`.
///
/// install_dir = `<root>/steamapps/common/<installdir>`
/// prefix      = `<root>/steamapps/compatdata/<appid>/pfx`  (or `$STEAM_COMPAT_DATA_PATH/pfx`)
fn build_resolved(appid: u32, name: &str, library_root: &Path, installdir: &str) -> ResolvedGame {
    let install_dir = library_root
        .join("steamapps")
        .join("common")
        .join(installdir);

    let prefix = proton_prefix(library_root, appid);
    let prefix_exists = prefix.is_dir();

    ResolvedGame {
        appid,
        name: name.to_string(),
        install_dir,
        prefix,
        prefix_exists,
    }
}

/// Derive the Proton prefix path. Honors `$STEAM_COMPAT_DATA_PATH` (a per-game
/// compatdata root that Steam exports when launching), else derives it from the
/// Steam library layout.
fn proton_prefix(library_root: &Path, appid: u32) -> PathBuf {
    if let Some(override_root) = std::env::var_os("STEAM_COMPAT_DATA_PATH") {
        // STEAM_COMPAT_DATA_PATH points at `.../compatdata/<appid>`; the prefix is its
        // `pfx` child.
        return PathBuf::from(override_root).join("pfx");
    }
    library_root
        .join("steamapps")
        .join("compatdata")
        .join(appid.to_string())
        .join("pfx")
}

/// Manual "add game by folder" fallback (ENV-03) for non-standard / Snap installs.
///
/// Validates the supplied folder contains the expected Bethesda markers (a `Data/`
/// directory and the game executable) BEFORE accepting it (threat T-01-04: untrusted
/// path validation). Derives the prefix from a sibling `compatdata` when discoverable,
/// else records the install dir with `prefix_exists = false` (unresolved-prefix
/// warning surfaced by the caller).
///
/// `appid` must be one of the supported games; the folder must contain that game's
/// executable.
pub fn add_game_by_folder(path: &Path, appid: u32) -> Result<ResolvedGame, SteamError> {
    if !is_supported(appid) {
        return Err(SteamError::Unsupported(appid));
    }

    let meta = std::fs::metadata(path).map_err(|source| SteamError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if !meta.is_dir() {
        return Err(SteamError::InvalidGameFolder {
            path: path.to_path_buf(),
            missing: "an existing directory".to_string(),
        });
    }

    // Marker 1: a case-insensitive `Data/` subdirectory.
    if !has_subdir_ci(path, "data") {
        return Err(SteamError::InvalidGameFolder {
            path: path.to_path_buf(),
            missing: "Data/ directory".to_string(),
        });
    }

    // Marker 2: the game's executable (case-insensitive).
    let exe = expected_exe(appid);
    if !has_file_ci(path, exe) {
        return Err(SteamError::InvalidGameFolder {
            path: path.to_path_buf(),
            missing: format!("{exe} executable"),
        });
    }

    // Try to derive a prefix from a sibling compatdata (…/common/<game> → …/compatdata).
    let prefix = sibling_compatdata_prefix(path, appid)
        .unwrap_or_else(|| proton_prefix_from_install(path, appid));
    let prefix_exists = prefix.is_dir();

    Ok(ResolvedGame {
        appid,
        name: default_name(appid).to_string(),
        install_dir: path.to_path_buf(),
        prefix,
        prefix_exists,
    })
}

/// The expected executable filename for a supported game.
fn expected_exe(appid: u32) -> &'static str {
    match appid {
        SKYRIM_SE => "SkyrimSE.exe",
        FALLOUT4 => "Fallout4.exe",
        _ => "",
    }
}

/// `install_dir` is typically `<library>/steamapps/common/<game>`; the sibling
/// compatdata is `<library>/steamapps/compatdata/<appid>/pfx`. Derive it by walking
/// up from `common/<game>` to `steamapps`.
fn sibling_compatdata_prefix(install_dir: &Path, appid: u32) -> Option<PathBuf> {
    let common = install_dir.parent()?; // .../steamapps/common
    if common.file_name()?.eq_ignore_ascii_case("common") {
        let steamapps = common.parent()?; // .../steamapps
        let candidate = steamapps
            .join("compatdata")
            .join(appid.to_string())
            .join("pfx");
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

/// Best-effort prefix path when the folder is not in a standard Steam layout: place an
/// (likely-nonexistent) compatdata path next to the install dir's parent. The caller
/// surfaces `prefix_exists = false` as an unresolved-prefix warning.
fn proton_prefix_from_install(install_dir: &Path, appid: u32) -> PathBuf {
    install_dir
        .parent()
        .map(|p| p.join("compatdata").join(appid.to_string()).join("pfx"))
        .unwrap_or_else(|| {
            PathBuf::from("compatdata")
                .join(appid.to_string())
                .join("pfx")
        })
}

/// Case-insensitive check for a child *directory* named `name`.
fn has_subdir_ci(dir: &Path, name: &str) -> bool {
    entry_ci(dir, name).map(|p| p.is_dir()).unwrap_or(false)
}

/// Case-insensitive check for a child *file* named `name`.
fn has_file_ci(dir: &Path, name: &str) -> bool {
    entry_ci(dir, name).map(|p| p.is_file()).unwrap_or(false)
}

/// Return the child of `dir` whose name matches `name` case-insensitively, chosen
/// DETERMINISTICALLY (WR-07): an exact-case match wins, else the lexicographically
/// smallest case-variant. `read_dir` order is filesystem-dependent and unordered, so a
/// first-match-wins choice would be nondeterministic across runs if a case-sensitive FS
/// (NexTwist's Proton target) holds multiple case-variants of `name` (e.g. `Data`/`data`,
/// or two executables) — a hazard for the reversibility guarantee built on top of it.
fn entry_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut matches: Vec<String> = rd
        .flatten()
        .filter_map(|entry| {
            let n = entry.file_name().to_str()?.to_owned();
            n.eq_ignore_ascii_case(name).then_some(n)
        })
        .collect();
    if matches.is_empty() {
        return None;
    }
    matches.sort();
    let chosen = matches
        .iter()
        .find(|n| n.as_str() == name)
        .unwrap_or(&matches[0]);
    Some(dir.join(chosen))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// `$STEAM_COMPAT_DATA_PATH` is process-global; serialize every test that reads or
    /// writes it so the parallel test runner can't leak it across tests.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Build a minimal synthetic library root with an appmanifest + install tree.
    fn synthetic_library(appid: u32, installdir: &str, with_prefix: bool) -> TempDir {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let steamapps = root.join("steamapps");
        std::fs::create_dir_all(steamapps.join("common").join(installdir).join("Data")).unwrap();
        if with_prefix {
            std::fs::create_dir_all(
                steamapps
                    .join("compatdata")
                    .join(appid.to_string())
                    .join("pfx"),
            )
            .unwrap();
        }
        let acf = format!(
            "\"AppState\"\n{{\n\t\"appid\"\t\"{appid}\"\n\t\"name\"\t\"Test Game\"\n\t\"installdir\"\t\"{installdir}\"\n}}\n"
        );
        std::fs::write(
            steamapps.join(format!("appmanifest_{appid}.acf")),
            acf,
        )
        .unwrap();
        dir
    }

    #[test]
    fn resolve_from_root_builds_install_dir_and_prefix() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = synthetic_library(SKYRIM_SE, "Skyrim Special Edition", true);
        let root = dir.path();
        // Ensure no override leaks in from the environment.
        unsafe { std::env::remove_var("STEAM_COMPAT_DATA_PATH") };

        let resolved = resolve_from_root(root, SKYRIM_SE).unwrap();
        assert_eq!(resolved.appid, SKYRIM_SE);
        assert_eq!(
            resolved.install_dir,
            root.join("steamapps/common/Skyrim Special Edition")
        );
        assert_eq!(
            resolved.prefix,
            root.join("steamapps/compatdata/489830/pfx")
        );
        assert!(resolved.prefix_exists);
        assert_eq!(resolved.name, "Test Game");
    }

    #[test]
    fn resolve_rejects_unsupported_appid() {
        // 220 = Half-Life 2 (not supported). Must error via allow-list, not panic.
        let err = resolve_game(220).unwrap_err();
        assert!(matches!(err, SteamError::Unsupported(220)));
    }

    #[test]
    fn resolve_from_root_unsupported_appid_errors() {
        let dir = TempDir::new().unwrap();
        let err = resolve_from_root(dir.path(), 220).unwrap_err();
        assert!(matches!(err, SteamError::Unsupported(220)));
    }

    #[test]
    fn resolve_from_root_not_installed_errors() {
        // Supported AppID but no appmanifest present.
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("steamapps")).unwrap();
        let err = resolve_from_root(dir.path(), FALLOUT4).unwrap_err();
        assert!(matches!(err, SteamError::NotInstalled(377160)));
    }

    #[test]
    fn steam_compat_data_path_override_is_honored() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = synthetic_library(FALLOUT4, "Fallout 4", false);
        let override_dir = TempDir::new().unwrap();
        let compat = override_dir.path().join("compatdata").join("377160");
        std::fs::create_dir_all(compat.join("pfx")).unwrap();
        unsafe { std::env::set_var("STEAM_COMPAT_DATA_PATH", &compat) };

        let resolved = resolve_from_root(dir.path(), FALLOUT4).unwrap();
        assert_eq!(resolved.prefix, compat.join("pfx"));
        assert!(resolved.prefix_exists);

        unsafe { std::env::remove_var("STEAM_COMPAT_DATA_PATH") };
    }

    #[test]
    fn add_game_by_folder_validates_markers() {
        let dir = TempDir::new().unwrap();
        let game = dir.path().join("Skyrim Special Edition");
        // Missing everything → error.
        std::fs::create_dir_all(&game).unwrap();
        let err = add_game_by_folder(&game, SKYRIM_SE).unwrap_err();
        assert!(matches!(err, SteamError::InvalidGameFolder { .. }));

        // Add Data/ but no exe → still error.
        std::fs::create_dir_all(game.join("Data")).unwrap();
        let err = add_game_by_folder(&game, SKYRIM_SE).unwrap_err();
        assert!(matches!(err, SteamError::InvalidGameFolder { .. }));

        // Add the exe (mixed case to prove CI matching) → accepted.
        std::fs::write(game.join("skyrimse.exe"), b"MZ").unwrap();
        let resolved = add_game_by_folder(&game, SKYRIM_SE).unwrap();
        assert_eq!(resolved.appid, SKYRIM_SE);
        assert_eq!(resolved.install_dir, game);
    }

    #[test]
    fn add_game_by_folder_rejects_unsupported() {
        let dir = TempDir::new().unwrap();
        let err = add_game_by_folder(dir.path(), 220).unwrap_err();
        assert!(matches!(err, SteamError::Unsupported(220)));
    }

    #[test]
    fn into_game_places_staging_on_same_fs() {
        let resolved = ResolvedGame {
            appid: SKYRIM_SE,
            name: "Skyrim Special Edition".into(),
            install_dir: PathBuf::from("/games/steamapps/common/Skyrim Special Edition"),
            prefix: PathBuf::from("/games/steamapps/compatdata/489830/pfx"),
            prefix_exists: true,
        };
        let game = resolved.into_game();
        assert_eq!(
            game.staging_dir,
            PathBuf::from("/games/steamapps/common/.nextwist-staging/489830")
        );
    }
}
