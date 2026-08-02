//! `nextwist-testkit` — shared test helpers for the safety-critical engine.
//!
//! This crate exists to make the **byte-for-byte pristine assertion** a single,
//! well-tested primitive. The DEPLOY-01/02/03 `round_trip_pristine` test and the
//! DEPLOY-06 `crash_recovery` centerpiece (Plan 04) both build on the
//! [`snapshot_tree`] and [`assert_trees_identical`] pair: deploy a mod, purge it,
//! then assert the game tree's snapshot equals the pre-deploy vanilla snapshot. The
//! diff output is intentionally explicit (which paths differ / are orphaned / are
//! missing) so a failing round-trip test points straight at the offending file.
//!
//! Used as a `dev-dependency` by the `steam`, `extract`, and `deploy` test suites.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// A structural snapshot of a directory tree: relative path -> entry marker.
///
/// For a regular file the marker is the blake3 hex of its bytes. For a directory the
/// marker is the reserved [`DIR_SENTINEL`] value — a non-hex string a 64-char blake3
/// digest can never collide with — so the snapshot captures the directory *shape*
/// (including EMPTY directories), not just file contents. This is load-bearing for the
/// GAP-01 byte-for-byte-pristine guarantee: an orphan empty directory left behind by a
/// purge is a real difference from vanilla and must be detected.
///
/// `BTreeMap` so iteration/diffing is deterministic and ordered.
pub type TreeSnapshot = BTreeMap<PathBuf, String>;

/// Reserved snapshot marker for a directory entry.
///
/// A blake3 hex digest is exactly 64 lowercase-hex characters, so this `<dir>`-prefixed
/// value (containing characters outside `[0-9a-f]`) can never equal a file's hash. That
/// guarantees a file vs. a directory at the same path always compares as a difference,
/// and that directory entries are unambiguously distinguishable from file entries.
pub const DIR_SENTINEL: &str = "<dir>";

/// Materialize a fake vanilla game tree under `root` from `(relpath, bytes)` pairs.
///
/// Creates parent directories as needed. `root` is typically a `Data/`-rooted game
/// directory on a temp dir. Returns `root` for convenient chaining.
pub fn fake_game_tree<P: AsRef<Path>>(root: P, files: &[(&str, &[u8])]) -> io::Result<PathBuf> {
    write_tree(root.as_ref(), files)?;
    Ok(root.as_ref().to_path_buf())
}

/// Materialize a fake staged-mod tree under `root` from `(relpath, bytes)` pairs.
///
/// Identical mechanics to [`fake_game_tree`]; named separately so test intent reads
/// clearly (vanilla game vs. staged mod).
pub fn fake_staged_mod<P: AsRef<Path>>(root: P, files: &[(&str, &[u8])]) -> io::Result<PathBuf> {
    write_tree(root.as_ref(), files)?;
    Ok(root.as_ref().to_path_buf())
}

/// Build a fake Proton-prefix tree under `root` with the AppData/Local/<game_name>
/// folder libloot's `with_local_path` targets on Linux, returning `root` (the prefix
/// root, suitable to pass where the `steam` crate would supply a real prefix).
///
/// This mimics the exact location a real Proton prefix exposes:
/// `<root>/drive_c/users/steamuser/AppData/Local/<game_name>/` (Pitfall 1/2). On Linux
/// libloot cannot derive this path itself (it returns `NoLocalAppData`), so NexTwist
/// must always supply it; this fixture lets `plugins.txt` round-trips be asserted
/// **headlessly** in CI without a real Proton install. When `plugins_txt` is `Some`,
/// a `Plugins.txt` seeded with that content is written inside the AppData/Local folder
/// (the asterisk-format active-plugins file libloot reads/writes).
///
/// The `<game_name>` is the Steam AppData folder name —
/// `"Skyrim Special Edition"` / `"Fallout4"` (mirrors libloadorder's
/// `*_appdata_folder_name`). Parent directories are created as needed.
pub fn fake_proton_prefix(
    root: &Path,
    game_name: &str,
    plugins_txt: Option<&str>,
) -> io::Result<PathBuf> {
    let appdata_local = root
        .join("drive_c")
        .join("users")
        .join("steamuser")
        .join("AppData")
        .join("Local")
        .join(game_name);
    fs::create_dir_all(&appdata_local)?;
    if let Some(contents) = plugins_txt {
        fs::write(appdata_local.join("Plugins.txt"), contents)?;
    }
    Ok(root.to_path_buf())
}

/// Options for [`fake_my_games_prefix`] — the CE2 `Documents/My Games/<folder>` fixture.
///
/// All fields default to the simplest shape (canonical casing, no marker, no `user.reg`),
/// so a bare `MyGamesOpts::default()` builds an empty first-launch-pending prefix.
#[derive(Default)]
pub struct MyGamesOpts<'a> {
    /// Build the `Documents/My Games/<folder>` tail MIS-CASED
    /// (`documents/my games/<folder lowercased>`) to exercise the case-fold resolver.
    /// Default builds the canonical Bethesda casing.
    pub case_variant: bool,
    /// Seed a file with this name inside the `My Games/<folder>` dir. A non-empty dir
    /// flips the resolver from `FirstLaunchPending` to `Ready`.
    pub marker: Option<&'a str>,
    /// Write `<root>/user.reg` with a `"Personal"` value under the Wine
    /// `User Shell Folders` section. The value is a logical Windows path
    /// (single backslashes, e.g. `C:\users\steamuser\Documents`); the fixture escapes
    /// the backslashes exactly as Wine does on disk.
    pub personal: Option<&'a str>,
}

/// Build a fake Proton-prefix tree seeded with the CE2 `Documents/My Games/<folder>`
/// subtree the `steam::ce2` resolver targets, returning `root` (the prefix root).
///
/// This is the `Documents/My Games` sibling of [`fake_proton_prefix`] (which seeds
/// `AppData/Local`). Starfield's `StarfieldCustom.ini` + first-launch marker live under
/// `<root>/drive_c/users/steamuser/Documents/My Games/<folder>/` — this fixture lets the
/// case-fold / first-launch / redirection resolver tests run headlessly in CI.
///
/// **Read-only invariant:** Phase 6 performs NO writes to a real prefix; this builder only
/// materializes a *fake* prefix on a temp dir so the read-only resolver has something to
/// probe.
///
/// Shapes (via [`MyGamesOpts`]):
/// * canonical `Documents/My Games/<folder>` (default),
/// * `case_variant` → mis-cased `documents/my games/<folder lowercased>` (the MANDATORY
///   SFDET-02 case-mismatch fixture),
/// * `marker` → a seeded file inside the folder (flips `FirstLaunchPending` → `Ready`),
/// * `personal` → a `user.reg` carrying a `"Personal"` shell-folder redirect.
pub fn fake_my_games_prefix(
    root: &Path,
    folder: &str,
    opts: MyGamesOpts<'_>,
) -> io::Result<PathBuf> {
    let base = root.join("drive_c").join("users").join("steamuser");
    let my_games = if opts.case_variant {
        base.join("documents")
            .join("my games")
            .join(folder.to_lowercase())
    } else {
        base.join("Documents").join("My Games").join(folder)
    };
    fs::create_dir_all(&my_games)?;
    if let Some(name) = opts.marker {
        fs::write(my_games.join(name), b"")?;
    }
    if let Some(personal) = opts.personal {
        // Wine stores backslashes doubled in user.reg; mirror that on disk.
        let escaped = personal.replace('\\', "\\\\");
        let reg = format!(
            "[Software\\\\Microsoft\\\\Windows\\\\CurrentVersion\\\\Explorer\\\\User Shell Folders]\n\"Personal\"=\"{escaped}\"\n"
        );
        fs::write(root.join("user.reg"), reg)?;
    }
    Ok(root.to_path_buf())
}

/// Write a minimal esplugin-parseable plugin file (a bare 24-byte TES4 header record) into
/// `dir` under `name`, with the given raw TES4 header flags.
///
/// The header layout mirrors the Plan-02 spike fixture: `b"TES4"`, a zero subrecord-size,
/// the 32-bit little-endian `flags`, then a zero form-id and 8 padding bytes. Callers pass
/// the flag bits their game classifier reads — `0x1` = master, and for Starfield `0x400` =
/// medium (see [`write_medium_plugin`]). This is the ONE shared plugin-fixture builder the
/// loadorder integration tests reuse, so the header shape lives in exactly one place.
pub fn write_plugin_header(dir: &Path, name: &str, flags: u32) -> io::Result<()> {
    let mut bytes = Vec::with_capacity(24);
    bytes.extend_from_slice(b"TES4");
    bytes.extend_from_slice(&0u32.to_le_bytes()); // size_of_subrecords = 0
    bytes.extend_from_slice(&flags.to_le_bytes()); // TES4 record header flags
    bytes.extend_from_slice(&0u32.to_le_bytes()); // form_id
    bytes.extend_from_slice(&[0u8; 8]); // version control + unknown (ignored)
    fs::create_dir_all(dir)?;
    fs::write(dir.join(name), &bytes)
}

/// Write a minimal master (`0x1`) or regular (`0x0`) plugin fixture — the non-medium
/// convenience over [`write_plugin_header`].
pub fn write_min_plugin(dir: &Path, name: &str, master: bool) -> io::Result<()> {
    write_plugin_header(dir, name, if master { 0x1 } else { 0x0 })
}

/// Write a Starfield MEDIUM master fixture: a TES4 header with the master (`0x1`) AND the
/// Starfield medium (`0x400`) flag set, so `esplugin::Plugin::is_medium_plugin()` returns
/// true when parsed under `GameId::Starfield` (and false under SkyrimSE/Fallout4, which do
/// not support the medium flag — verified `esplugin 6.1.4 plugin.rs:503-510`). A medium
/// master is still a master (`is_master_file()` true → `PluginKind::Esm`), so `medium` is a
/// separate boolean, never a fourth `PluginKind`.
pub fn write_medium_plugin(dir: &Path, name: &str) -> io::Result<()> {
    write_plugin_header(dir, name, 0x1 | 0x400)
}

fn write_tree(root: &Path, files: &[(&str, &[u8])]) -> io::Result<()> {
    for (rel, bytes) in files {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, bytes)?;
    }
    Ok(())
}

/// Walk `root` and record every descendant entry keyed by its path relative to `root`:
/// each regular file by its blake3 content hash, and each DIRECTORY by the reserved
/// [`DIR_SENTINEL`] marker. The `root` directory itself is NOT recorded (only its
/// descendants), so a snapshot of an empty tree is empty and self-equality holds.
///
/// Tracking directories (including empty ones) is load-bearing: "byte-for-byte pristine"
/// means the directory *shape* too, not just file contents. An orphan empty directory a
/// purge fails to clean up is a real difference [`assert_trees_identical`] must flag.
/// Symlinks are not followed (and a placed symlink hashes as the bytes it resolves to via
/// `fs::read`, matching how the game would read it).
///
/// Returns an error if the tree cannot be walked or a file cannot be read.
pub fn snapshot_tree<P: AsRef<Path>>(root: P) -> io::Result<TreeSnapshot> {
    let root = root.as_ref();
    let mut snap = TreeSnapshot::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(io::Error::other)?;
        let abs = entry.path();
        // Never record the root itself — only its descendants — so an empty tree
        // snapshots to an empty map and self-equality holds.
        if abs == root {
            continue;
        }
        let rel = abs
            .strip_prefix(root)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
            .to_path_buf();
        let ft = entry.file_type();
        if ft.is_dir() {
            // Record the directory shape with the reserved sentinel (never a file hash).
            snap.insert(rel, DIR_SENTINEL.to_string());
        } else {
            // Regular file or symlink: hash the bytes it resolves to.
            let bytes = fs::read(abs)?;
            let hash = blake3::hash(&bytes).to_hex().to_string();
            snap.insert(rel, hash);
        }
    }
    Ok(snap)
}

/// Assert two snapshots are byte-for-byte identical.
///
/// Panics with a readable, actionable diff if they differ, classifying every
/// offending path as one of:
///
/// * **mutated** — present in both but with a different content hash,
/// * **orphan** — present in `actual` but missing from `expected` (a leftover),
/// * **missing** — present in `expected` but absent from `actual`.
///
/// This is the pristine-assertion primitive the round-trip and crash-recovery
/// integration tests rely on, so the diff is deliberately verbose.
pub fn assert_trees_identical(expected: &TreeSnapshot, actual: &TreeSnapshot) {
    let mut mutated = Vec::new();
    let mut missing = Vec::new();
    let mut orphan = Vec::new();

    for (rel, exp_hash) in expected {
        match actual.get(rel) {
            Some(act_hash) if act_hash == exp_hash => {}
            Some(act_hash) => mutated.push((rel.clone(), exp_hash.clone(), act_hash.clone())),
            None => missing.push(rel.clone()),
        }
    }
    for rel in actual.keys() {
        if !expected.contains_key(rel) {
            orphan.push(rel.clone());
        }
    }

    if mutated.is_empty() && missing.is_empty() && orphan.is_empty() {
        return;
    }

    let mut msg = String::from("trees are NOT byte-for-byte identical:\n");
    for (rel, exp, act) in &mutated {
        msg.push_str(&format!(
            "  MUTATED  {}\n    expected blake3 {}\n    actual   blake3 {}\n",
            rel.display(),
            exp,
            act
        ));
    }
    for rel in &missing {
        msg.push_str(&format!(
            "  MISSING  {} (in expected, absent from actual)\n",
            rel.display()
        ));
    }
    for rel in &orphan {
        msg.push_str(&format!(
            "  ORPHAN   {} (in actual, not in expected)\n",
            rel.display()
        ));
    }
    msg.push_str(&format!(
        "  summary: {} mutated, {} missing, {} orphan\n",
        mutated.len(),
        missing.len(),
        orphan.len()
    ));
    panic!("{msg}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn build(files: &[(&str, &[u8])]) -> (TempDir, TreeSnapshot) {
        let dir = TempDir::new().unwrap();
        write_tree(dir.path(), files).unwrap();
        let snap = snapshot_tree(dir.path()).unwrap();
        (dir, snap)
    }

    #[test]
    fn snapshot_covers_every_file_with_stable_hashes() {
        let (_d, snap) = build(&[
            ("Data/a.esp", b"alpha"),
            ("Data/textures/rock.dds", b"rockbytes"),
            ("readme.txt", b"hi"),
        ]);
        // 3 files + the intermediate Data/ and Data/textures/ directories.
        assert!(snap.contains_key(Path::new("Data/a.esp")));
        assert!(snap.contains_key(Path::new("Data/textures/rock.dds")));
        // The intermediate directory is tracked, carrying the dir sentinel (not a hash).
        assert_eq!(
            snap.get(Path::new("Data/textures")).map(String::as_str),
            Some(DIR_SENTINEL),
            "intermediate directory must be recorded with the dir sentinel"
        );
        assert_eq!(
            snap.get(Path::new("Data")).map(String::as_str),
            Some(DIR_SENTINEL)
        );
        // A file entry is never the dir sentinel.
        assert_ne!(
            snap.get(Path::new("Data/a.esp")).map(String::as_str),
            Some(DIR_SENTINEL)
        );
        // Same bytes hash identically regardless of where they live.
        let dir2 = TempDir::new().unwrap();
        write_tree(dir2.path(), &[("elsewhere/a.esp", b"alpha")]).unwrap();
        let snap2 = snapshot_tree(dir2.path()).unwrap();
        assert_eq!(
            snap.get(Path::new("Data/a.esp")),
            snap2.get(Path::new("elsewhere/a.esp"))
        );
    }

    #[test]
    #[should_panic(expected = "ORPHAN")]
    fn extra_empty_dir_orphan_fails() {
        // `actual` has an extra EMPTY directory absent from `expected` — an orphan.
        let dir_a = TempDir::new().unwrap();
        write_tree(dir_a.path(), &[("Data/a.esp", b"x")]).unwrap();
        let a = snapshot_tree(dir_a.path()).unwrap();

        let dir_b = TempDir::new().unwrap();
        write_tree(dir_b.path(), &[("Data/a.esp", b"x")]).unwrap();
        // Leftover empty directory the purge failed to clean up (the GAP-01 repro shape).
        fs::create_dir_all(dir_b.path().join("Data/leftover/empty")).unwrap();
        let b = snapshot_tree(dir_b.path()).unwrap();

        assert!(
            b.get(Path::new("Data/leftover/empty")).map(String::as_str) == Some(DIR_SENTINEL),
            "the leftover empty dir must be snapshotted"
        );
        assert_trees_identical(&a, &b);
    }

    #[test]
    fn identical_empty_dirs_pass() {
        // Two trees with the SAME empty directory structure are pristine-equal.
        let dir_a = TempDir::new().unwrap();
        write_tree(dir_a.path(), &[("Data/a.esp", b"x")]).unwrap();
        fs::create_dir_all(dir_a.path().join("Data/textures/empty")).unwrap();
        let a = snapshot_tree(dir_a.path()).unwrap();

        let dir_b = TempDir::new().unwrap();
        write_tree(dir_b.path(), &[("Data/a.esp", b"x")]).unwrap();
        fs::create_dir_all(dir_b.path().join("Data/textures/empty")).unwrap();
        let b = snapshot_tree(dir_b.path()).unwrap();

        assert_trees_identical(&a, &b);
    }

    #[test]
    fn identical_trees_pass() {
        let (_d1, a) = build(&[("Data/a.esp", b"x"), ("Data/b.esp", b"y")]);
        let (_d2, b) = build(&[("Data/a.esp", b"x"), ("Data/b.esp", b"y")]);
        assert_trees_identical(&a, &b);
    }

    #[test]
    #[should_panic(expected = "MUTATED")]
    fn mutated_byte_fails() {
        let (_d1, a) = build(&[("Data/a.esp", b"original")]);
        let (_d2, b) = build(&[("Data/a.esp", b"tampered!")]);
        assert_trees_identical(&a, &b);
    }

    #[test]
    #[should_panic(expected = "ORPHAN")]
    fn extra_orphan_file_fails() {
        let (_d1, a) = build(&[("Data/a.esp", b"x")]);
        let (_d2, b) = build(&[("Data/a.esp", b"x"), ("Data/leftover.esp", b"z")]);
        assert_trees_identical(&a, &b);
    }

    #[test]
    #[should_panic(expected = "MISSING")]
    fn missing_file_fails() {
        let (_d1, a) = build(&[("Data/a.esp", b"x"), ("Data/b.esp", b"y")]);
        let (_d2, b) = build(&[("Data/a.esp", b"x")]);
        assert_trees_identical(&a, &b);
    }

    #[test]
    fn fake_proton_prefix_builds_appdata_local_and_seeds_plugins_txt() {
        let dir = TempDir::new().unwrap();
        let root = fake_proton_prefix(
            dir.path(),
            "Skyrim Special Edition",
            Some("*Skyrim.esm\n*Update.esm\nUnmanaged.esp\n"),
        )
        .unwrap();
        // Returns the prefix root unchanged.
        assert_eq!(root, dir.path());
        // The full AppData/Local/<game_name> tree exists (the with_local_path target).
        let appdata_local =
            root.join("drive_c/users/steamuser/AppData/Local/Skyrim Special Edition");
        assert!(appdata_local.is_dir(), "AppData/Local/<game> must exist");
        // The seeded Plugins.txt round-trips (asterisk-format active-plugins file).
        let written = fs::read_to_string(appdata_local.join("Plugins.txt")).unwrap();
        assert_eq!(written, "*Skyrim.esm\n*Update.esm\nUnmanaged.esp\n");
    }

    #[test]
    fn fake_proton_prefix_without_plugins_txt_omits_the_file() {
        let dir = TempDir::new().unwrap();
        fake_proton_prefix(dir.path(), "Fallout4", None).unwrap();
        let appdata_local = dir
            .path()
            .join("drive_c/users/steamuser/AppData/Local/Fallout4");
        assert!(appdata_local.is_dir());
        assert!(
            !appdata_local.join("Plugins.txt").exists(),
            "no Plugins.txt should be seeded when plugins_txt is None"
        );
    }

    #[test]
    fn fake_my_games_prefix_builds_shapes_on_request() {
        // (a) canonical, no marker, no user.reg → dir exists, folder empty, no user.reg.
        let d = TempDir::new().unwrap();
        let root = fake_my_games_prefix(d.path(), "Starfield", MyGamesOpts::default()).unwrap();
        assert_eq!(root, d.path());
        let canonical = root.join("drive_c/users/steamuser/Documents/My Games/Starfield");
        assert!(canonical.is_dir(), "canonical My Games/<folder> must exist");
        assert_eq!(fs::read_dir(&canonical).unwrap().count(), 0, "folder empty");
        assert!(
            !root.join("user.reg").exists(),
            "no user.reg unless requested"
        );

        // (b) case-variant tail exists under the mis-cased path.
        let d2 = TempDir::new().unwrap();
        fake_my_games_prefix(
            d2.path(),
            "Starfield",
            MyGamesOpts {
                case_variant: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            d2.path()
                .join("drive_c/users/steamuser/documents/my games/starfield")
                .is_dir(),
            "case-variant mis-cased tail must exist"
        );

        // (c) marker + user.reg written only when requested.
        let d3 = TempDir::new().unwrap();
        fake_my_games_prefix(
            d3.path(),
            "Starfield",
            MyGamesOpts {
                marker: Some("StarfieldCustom.ini"),
                personal: Some(r"C:\users\steamuser\Documents"),
                ..Default::default()
            },
        )
        .unwrap();
        let folder = d3
            .path()
            .join("drive_c/users/steamuser/Documents/My Games/Starfield");
        assert!(
            folder.join("StarfieldCustom.ini").is_file(),
            "marker seeded"
        );
        let reg = fs::read_to_string(d3.path().join("user.reg")).unwrap();
        assert!(reg.contains("\"Personal\"=\"C:\\\\users\\\\steamuser\\\\Documents\""));
        assert!(reg.contains("User Shell Folders"));
    }

    #[test]
    fn plugin_header_builders_set_the_expected_flags() {
        let dir = TempDir::new().unwrap();
        write_min_plugin(dir.path(), "Regular.esp", false).unwrap();
        write_min_plugin(dir.path(), "Master.esm", true).unwrap();
        write_medium_plugin(dir.path(), "Medium.esm").unwrap();

        // The 24-byte TES4 header carries the flags at bytes [8..12) (little-endian).
        let flags = |name: &str| -> u32 {
            let b = fs::read(dir.path().join(name)).unwrap();
            assert_eq!(&b[0..4], b"TES4", "valid TES4 magic");
            assert_eq!(b.len(), 24, "minimal 24-byte header");
            u32::from_le_bytes(b[8..12].try_into().unwrap())
        };
        assert_eq!(flags("Regular.esp"), 0x0, "regular plugin: no flags");
        assert_eq!(flags("Master.esm") & 0x1, 0x1, "master flag set");
        assert_eq!(
            flags("Medium.esm") & 0x1,
            0x1,
            "medium master is still a master"
        );
        assert_eq!(
            flags("Medium.esm") & 0x400,
            0x400,
            "Starfield medium flag (0x400) set"
        );
    }

    #[test]
    fn fake_builders_materialize_and_roundtrip() {
        let game = TempDir::new().unwrap();
        let staged = TempDir::new().unwrap();
        fake_game_tree(game.path(), &[("Data/Skyrim.esm", b"vanilla")]).unwrap();
        fake_staged_mod(staged.path(), &[("Data/Mod.esp", b"modbytes")]).unwrap();
        assert!(game.path().join("Data/Skyrim.esm").is_file());
        assert!(staged.path().join("Data/Mod.esp").is_file());
        // A snapshot of the game tree round-trips against itself.
        let s = snapshot_tree(game.path()).unwrap();
        assert_trees_identical(&s, &snapshot_tree(game.path()).unwrap());
    }
}
