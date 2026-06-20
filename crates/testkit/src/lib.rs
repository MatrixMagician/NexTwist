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
pub fn fake_game_tree<P: AsRef<Path>>(
    root: P,
    files: &[(&str, &[u8])],
) -> io::Result<PathBuf> {
    write_tree(root.as_ref(), files)?;
    Ok(root.as_ref().to_path_buf())
}

/// Materialize a fake staged-mod tree under `root` from `(relpath, bytes)` pairs.
///
/// Identical mechanics to [`fake_game_tree`]; named separately so test intent reads
/// clearly (vanilla game vs. staged mod).
pub fn fake_staged_mod<P: AsRef<Path>>(
    root: P,
    files: &[(&str, &[u8])],
) -> io::Result<PathBuf> {
    write_tree(root.as_ref(), files)?;
    Ok(root.as_ref().to_path_buf())
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
        assert_eq!(snap.get(Path::new("Data")).map(String::as_str), Some(DIR_SENTINEL));
        // A file entry is never the dir sentinel.
        assert_ne!(snap.get(Path::new("Data/a.esp")).map(String::as_str), Some(DIR_SENTINEL));
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
