//! Conflict resolution (CONF-01/02/03) — the pure fold that turns many enabled mods
//! into a single deterministic winner-per-path deploy set.
//!
//! ## What this is
//!
//! When several enabled mods stage a file at the SAME deploy-root-relative path
//! (`target_rel`), exactly one of them must win — the game can only have one file
//! there, and the manifest enforces `deployed_file UNIQUE(appid, target_rel)`. This
//! module computes that winner from the user's priority order and reports every
//! contested path so the UI can show "who wins" (CONF-01).
//!
//! ## How it decides (D-01 / CONF-02)
//!
//! Each mod carries a `rank` — **lower rank = higher priority** (1-based, top of the
//! list). For each contested path the providers are sorted by rank ascending and the
//! winner is `providers[0]`. Changing a mod's rank above another flips the winner.
//!
//! ## The contract it produces (Pitfall 3 — no duplicate `target_rel`)
//!
//! [`resolve`] is a **pure in-memory fold** over the enabled mods' staged trees (its
//! only I/O is reading those staged directory listings). It emits:
//!
//! * a `Vec<WinnerFile>` — one entry per `target_rel`, each tagged with the WINNING
//!   mod's `staging_root` + relpath + `mod_id`. This is the multi-root deploy set the
//!   engine consumes via [`crate::deploy_winners`]. Because it is deduped to a single
//!   winner per path, it satisfies the manifest UNIQUE constraint BEFORE any syscall.
//! * a `Vec<FileConflict>` — one entry per CONTESTED path (providers > 1), naming all
//!   providers and the winner (drives the CONF-01 conflict table).
//!
//! ## Safety (T-02-06 / Security §V5/V12)
//!
//! Every winner path is asserted to lexically resolve INSIDE its own mod's staging
//! root (defence in depth — staged trees were already zip-slip/symlink-validated at
//! extract time). An escape yields [`DeployError::PathEscape`] and aborts resolution.
//!
//! The safe engine (`deploy`/`purge`) is NEVER bypassed: this module only chooses
//! WHICH (root, rel, mod) tuples to hand to the unchanged journaled deploy primitive.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nextwist_core::FileConflict;
use walkdir::WalkDir;

use crate::error::DeployError;
use crate::path_guard::{guard_within_root, lexical_normalize};

/// One enabled mod's contribution to a conflict resolution: its row id, the root of
/// its staged (read-only) tree, and its priority rank.
///
/// `rank` is **lower = higher priority** (1-based), matching `managed_mod.rank` and
/// the UI's top-of-list-wins ordering (CONF-02 / D-01).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModInput {
    /// `managed_mod` row id (recorded as the winning file's owner in the manifest).
    pub mod_id: i64,
    /// Root of this mod's staged, `Data/`-rooted, read-only tree.
    pub staging_root: PathBuf,
    /// Priority rank — lower wins. 1-based.
    pub rank: u32,
}

/// A single resolved winner: the deploy engine deploys this file from `staging_root`
/// to `<deploy_root>/<rel-without-Data>`, recording `mod_id` as its owner (D-03).
///
/// This is the per-file (root, rel) pair the **multi-root contract** (Plan 02-03
/// decision: Option A) introduces — `StagedFiles` carries ONE `staging_root`, but
/// multi-mod winners come from DIFFERENT roots, so the winner set is a `Vec` of these
/// instead. `engine::deploy`/`StagedFiles` are left UNCHANGED for Phase-1 callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinnerFile {
    /// The winning mod's row id — recorded as `FileEntry.source_mod` (D-03).
    pub mod_id: i64,
    /// The winning mod's staging root.
    pub staging_root: PathBuf,
    /// `Data/`-rooted relpath under `staging_root` (and the manifest key after casing
    /// normalization in the engine).
    pub rel: PathBuf,
}

/// Resolve many enabled mods into a single deterministic winner per `target_rel`.
///
/// Pure fold (the only I/O is walking each mod's staged tree). For each
/// deploy-root-relative path provided by 1+ mods, the winner is the lowest-rank (=
/// highest-priority) provider; the output `Vec<WinnerFile>` has exactly one entry per
/// path (Pitfall 3 — UNIQUE-safe), and a [`FileConflict`] is emitted only for paths
/// with more than one provider (CONF-01).
///
/// Iteration/output order is deterministic (a `BTreeMap` keyed by `target_rel`), so a
/// given mod set always produces the same winner set + conflict list.
///
/// # Errors
///
/// [`DeployError::PathEscape`] if any winner's relpath lexically escapes its mod's
/// staging root (T-02-06); [`DeployError::Io`] if a staged tree cannot be walked.
pub fn resolve(mods: &[ModInput]) -> Result<(Vec<WinnerFile>, Vec<FileConflict>), DeployError> {
    // target_rel -> providers, each (rank, mod_id, staging_root). The OUTER map is a
    // BTreeMap so winners/conflicts come out in a stable, path-sorted order.
    let mut by_path: BTreeMap<PathBuf, Vec<(u32, i64, PathBuf)>> = BTreeMap::new();

    for m in mods {
        for rel in staged_rels(&m.staging_root)? {
            by_path
                .entry(rel)
                .or_default()
                .push((m.rank, m.mod_id, m.staging_root.clone()));
        }
    }

    let mut winners = Vec::with_capacity(by_path.len());
    let mut conflicts = Vec::new();

    for (target_rel, mut providers) in by_path {
        // Lowest rank wins; ties broken by mod_id for determinism (a stable tie-break
        // means equal-rank mods still produce a single, repeatable winner).
        providers.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let (_rank, winner_mod, winner_root) = providers[0].clone();

        // T-02-06: the winning relpath must resolve inside its own staging root.
        let abs = winner_root.join(&target_rel);
        guard_within_root(&winner_root, &abs)?;

        if providers.len() > 1 {
            conflicts.push(FileConflict {
                target_rel: target_rel.clone(),
                providers: providers.iter().map(|(_, id, _)| *id).collect(),
                winner: winner_mod,
            });
        }

        winners.push(WinnerFile {
            mod_id: winner_mod,
            staging_root: winner_root,
            rel: target_rel,
        });
    }

    Ok((winners, conflicts))
}

/// Walk a staged mod tree and return its `Data/`-rooted regular-file relpaths.
///
/// Directories and non-regular entries are skipped (only files deploy). A staging root
/// that does not exist yet (e.g. an empty mod) yields an empty list rather than an
/// error, mirroring the engine's empty-mod no-op tolerance.
fn staged_rels(staging_root: &Path) -> Result<Vec<PathBuf>, DeployError> {
    if !staging_root.exists() {
        return Ok(Vec::new());
    }
    let root_norm = lexical_normalize(staging_root);
    let mut rels = Vec::new();
    for entry in WalkDir::new(staging_root).follow_links(false) {
        let entry = entry.map_err(|e| {
            DeployError::io(staging_root, std::io::Error::other(e))
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        // Defence in depth: confirm the walked file is inside the staging root before
        // deriving its relpath (a symlink could otherwise point outside — they were
        // rejected at extract time, but we never trust that blindly here).
        let abs_norm = lexical_normalize(entry.path());
        if !abs_norm.starts_with(&root_norm) {
            return Err(DeployError::PathEscape(entry.path().to_path_buf()));
        }
        let rel = abs_norm
            .strip_prefix(&root_norm)
            .map_err(|e| DeployError::io(staging_root, std::io::Error::new(std::io::ErrorKind::InvalidInput, e)))?
            .to_path_buf();
        rels.push(rel);
    }
    Ok(rels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use testkit::fake_staged_mod;

    /// Two mods both provide Data/shared.esp: the lower-rank mod wins, the output has
    /// exactly ONE entry for that path, and a FileConflict lists both providers with
    /// the lower-rank mod as winner (CONF-01/CONF-02).
    #[test]
    fn lower_rank_wins_shared_path() {
        let dir = TempDir::new().unwrap();
        let a = fake_staged_mod(
            dir.path().join("a"),
            &[("Data/shared.esp", b"A-BYTES"), ("Data/only_a.esp", b"A")],
        )
        .unwrap();
        let b = fake_staged_mod(
            dir.path().join("b"),
            &[("Data/shared.esp", b"B-BYTES"), ("Data/only_b.esp", b"B")],
        )
        .unwrap();

        let mods = vec![
            ModInput { mod_id: 10, staging_root: a.clone(), rank: 1 },
            ModInput { mod_id: 20, staging_root: b.clone(), rank: 2 },
        ];
        let (winners, conflicts) = resolve(&mods).unwrap();

        // Exactly one winner per path; shared.esp resolves to mod A (rank 1).
        let shared: Vec<&WinnerFile> = winners
            .iter()
            .filter(|w| w.rel == Path::new("Data/shared.esp"))
            .collect();
        assert_eq!(shared.len(), 1, "exactly one winner for the shared path");
        assert_eq!(shared[0].mod_id, 10);
        assert_eq!(shared[0].staging_root, a);

        // One conflict for the shared path naming both providers, winner = mod A.
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].target_rel, PathBuf::from("Data/shared.esp"));
        let mut provs = conflicts[0].providers.clone();
        provs.sort();
        assert_eq!(provs, vec![10, 20]);
        assert_eq!(conflicts[0].winner, 10);

        // The two unique files appear exactly once each.
        assert_eq!(winners.iter().filter(|w| w.rel == Path::new("Data/only_a.esp")).count(), 1);
        assert_eq!(winners.iter().filter(|w| w.rel == Path::new("Data/only_b.esp")).count(), 1);
    }

    /// No two mods share a path: zero conflicts, every staged file appears exactly once.
    #[test]
    fn no_shared_paths_no_conflicts() {
        let dir = TempDir::new().unwrap();
        let a = fake_staged_mod(dir.path().join("a"), &[("Data/a1.esp", b"1"), ("Data/a2.esp", b"2")]).unwrap();
        let b = fake_staged_mod(dir.path().join("b"), &[("Data/b1.esp", b"3")]).unwrap();

        let mods = vec![
            ModInput { mod_id: 1, staging_root: a, rank: 1 },
            ModInput { mod_id: 2, staging_root: b, rank: 2 },
        ];
        let (winners, conflicts) = resolve(&mods).unwrap();
        assert!(conflicts.is_empty(), "no shared paths => no conflicts");
        assert_eq!(winners.len(), 3);
    }

    /// Raising the winning mod's rank above the other flips the winner (CONF-02).
    #[test]
    fn rank_change_flips_winner() {
        let dir = TempDir::new().unwrap();
        let a = fake_staged_mod(dir.path().join("a"), &[("Data/shared.esp", b"A")]).unwrap();
        let b = fake_staged_mod(dir.path().join("b"), &[("Data/shared.esp", b"B")]).unwrap();

        // A=1, B=2 -> A wins.
        let (w1, _) = resolve(&[
            ModInput { mod_id: 10, staging_root: a.clone(), rank: 1 },
            ModInput { mod_id: 20, staging_root: b.clone(), rank: 2 },
        ])
        .unwrap();
        assert_eq!(w1.iter().find(|w| w.rel == Path::new("Data/shared.esp")).unwrap().mod_id, 10);

        // Flip: B=1, A=2 -> B wins.
        let (w2, _) = resolve(&[
            ModInput { mod_id: 10, staging_root: a, rank: 2 },
            ModInput { mod_id: 20, staging_root: b, rank: 1 },
        ])
        .unwrap();
        assert_eq!(w2.iter().find(|w| w.rel == Path::new("Data/shared.esp")).unwrap().mod_id, 20);
    }

    /// Pitfall 3 (mandatory): resolve NEVER emits two entries for the same target_rel,
    /// even with three mods all contending for the same path plus overlaps.
    #[test]
    fn never_emits_duplicate_target_rel() {
        let dir = TempDir::new().unwrap();
        let a = fake_staged_mod(dir.path().join("a"), &[("Data/x.esp", b"A"), ("Data/y.esp", b"A")]).unwrap();
        let b = fake_staged_mod(dir.path().join("b"), &[("Data/x.esp", b"B"), ("Data/y.esp", b"B")]).unwrap();
        let c = fake_staged_mod(dir.path().join("c"), &[("Data/x.esp", b"C")]).unwrap();

        let (winners, _) = resolve(&[
            ModInput { mod_id: 1, staging_root: a, rank: 1 },
            ModInput { mod_id: 2, staging_root: b, rank: 2 },
            ModInput { mod_id: 3, staging_root: c, rank: 3 },
        ])
        .unwrap();

        let mut seen = std::collections::BTreeSet::new();
        for w in &winners {
            assert!(seen.insert(w.rel.clone()), "duplicate target_rel emitted: {:?}", w.rel);
        }
        // x.esp + y.esp = two unique winners only.
        assert_eq!(winners.len(), 2);
    }

    /// A winner whose relpath lexically escapes its staging root is rejected. We build
    /// a real escaping file (a sibling outside the staging root) and reference it via a
    /// `..` relpath so the lexical guard fires.
    #[test]
    fn path_escape_winner_rejected() {
        // Construct providers by hand to inject an escaping relpath: walkdir would not
        // surface a `..` rel from a normal tree, so we exercise the guard directly via
        // a crafted staging root whose only file is reachable through `..`.
        let dir = TempDir::new().unwrap();
        let staging = dir.path().join("mod/staging");
        std::fs::create_dir_all(&staging).unwrap();
        // A file OUTSIDE the staging root.
        std::fs::write(dir.path().join("mod/outside.esp"), b"X").unwrap();

        // Directly assert the guard rejects an escaping winner path (the same guard
        // resolve() applies per winner).
        let escaping = staging.join("../outside.esp");
        let err = guard_within_root(&staging, &escaping).unwrap_err();
        assert!(matches!(err, DeployError::PathEscape(_)), "got {err:?}");
    }

    /// An empty / non-existent staging root contributes no files (no error).
    #[test]
    fn empty_mod_is_a_noop() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("does-not-exist");
        let (winners, conflicts) = resolve(&[ModInput { mod_id: 1, staging_root: missing, rank: 1 }]).unwrap();
        assert!(winners.is_empty());
        assert!(conflicts.is_empty());
    }
}
