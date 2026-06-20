//! crash_recovery (DEPLOY-06) — THE CENTERPIECE.
//!
//! Simulate a kill mid-deploy: drive `deploy_with_abort` so the engine commits the
//! `pending` journal rows and places files on disk, but aborts BEFORE writing the
//! manifest rows / flipping the intents to `done` for the aborted remainder. Then
//! open a FRESH store handle against the same DB + dirs (as a relaunch would), call
//! `recover_on_launch`, and assert:
//!   (a) no non-`done` journal rows remain,
//!   (b) the tree is in a consistent state, and
//!   (c) a follow-up `purge` returns the game to byte-for-byte pristine.
//!
//! The phase gate re-runs this on the dev btrfs filesystem (the hardest fs case) per
//! VALIDATION.md; here it runs deterministically on a tempdir.

use std::fs;
use std::path::PathBuf;

use deploy::{deploy_with_abort, purge, recover_on_launch, DeployError, StagedFiles};
use nextwist_core::Game;
use store::Store;
use tempfile::TempDir;
use testkit::{assert_trees_identical, snapshot_tree};

struct Fixture {
    root: TempDir,
    install: PathBuf,
    staging: PathBuf,
    db: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        let install = root.path().join("game");
        let staging = root.path().join("app/staging/489830");
        let db = root.path().join("app/nextwist.db");
        fs::create_dir_all(install.join("Data")).unwrap();
        fs::create_dir_all(&staging).unwrap();
        Fixture { root, install, staging, db }
    }

    fn game(&self) -> Game {
        Game {
            appid: 489830,
            name: "Skyrim Special Edition".into(),
            install_dir: self.install.clone(),
            prefix: self.root.path().join("prefix"),
            staging_dir: self.staging.clone(),
        }
    }

    /// Open a fresh store handle against the same DB (simulates a relaunch).
    fn open_store(&self) -> Store {
        let store = Store::open(&self.db).unwrap();
        // Re-register the game (INSERT OR REPLACE) so the row survives across handles;
        // a real relaunch reads it from the DB. add_managed_game is idempotent.
        store.add_managed_game(&self.game()).unwrap();
        store
    }
}

/// Build vanilla + staged trees: some mod files overwrite vanilla, some are pure adds.
fn lay_trees(fx: &Fixture) -> (StagedFiles, std::collections::BTreeMap<PathBuf, String>) {
    // Vanilla: two originals, one of which the mod will overwrite.
    fs::create_dir_all(fx.install.join("Data/textures")).unwrap();
    fs::write(fx.install.join("Data/textures/rock.dds"), b"VANILLA-ROCK").unwrap();
    fs::write(fx.install.join("Data/Skyrim.esm"), b"VANILLA-MASTER").unwrap();
    let pristine = snapshot_tree(&fx.install).unwrap();

    // Staged mod rooted DIRECTLY at game.staging_dir (the contract `replay`
    // reconstructs, so forward recovery can locate each interrupted source):
    // overwrite rock.dds + add three new files (so an abort can land partway through
    // and leave a mix of done/pending/placed work).
    let staged_root = fx.staging.clone();
    fs::create_dir_all(staged_root.join("Data/textures")).unwrap();
    fs::create_dir_all(staged_root.join("Data/meshes")).unwrap();
    fs::create_dir_all(staged_root.join("Data/scripts")).unwrap();
    fs::write(staged_root.join("Data/textures/rock.dds"), b"MOD-ROCK").unwrap();
    fs::write(staged_root.join("Data/meshes/m1.nif"), b"MOD-M1").unwrap();
    fs::write(staged_root.join("Data/meshes/m2.nif"), b"MOD-M2").unwrap();
    fs::write(staged_root.join("Data/scripts/s.pex"), b"MOD-S").unwrap();

    let staged = StagedFiles {
        staging_root: staged_root,
        files: vec![
            PathBuf::from("Data/textures/rock.dds"),
            PathBuf::from("Data/meshes/m1.nif"),
            PathBuf::from("Data/meshes/m2.nif"),
            PathBuf::from("Data/scripts/s.pex"),
        ],
    };
    (staged, pristine)
}

/// After an abort at each possible point, recovery + purge must reach pristine.
#[test]
fn abort_mid_deploy_then_recover_then_purge_is_pristine() {
    // Abort after 0, 1, 2, and 3 fully-completed files (the 4th file's syscall runs
    // and its pending row is committed, but it is never finished) — covering the
    // whole crash window.
    for abort_after in 0..4usize {
        let fx = Fixture::new();
        let (staged, pristine) = lay_trees(&fx);
        let game = fx.game();

        // --- crash: deploy aborts mid-flight ---
        {
            let store = fx.open_store();
            let err = deploy_with_abort(&store, &game, &staged, abort_after)
                .expect_err("deploy_with_abort must abort");
            assert!(
                matches!(err, DeployError::Aborted(n) if n == abort_after),
                "expected Aborted({abort_after}), got {err:?}"
            );
            // At least one pending row remains (the aborted file's intent), unless
            // abort_after places exactly zero work — even then the first file's
            // pending row is committed before its abort.
            let pending = store.pending_ops().unwrap();
            assert!(
                !pending.is_empty(),
                "a crash mid-deploy must leave >=1 pending journal row (abort_after={abort_after})"
            );
            // store dropped here = the process "died".
        }

        // --- relaunch: fresh store handle, recover, then purge ---
        let store = fx.open_store();
        let recovery = recover_on_launch(&store, &game).unwrap();
        assert!(
            recovery.replayed >= 1,
            "recovery must replay the interrupted op(s) (abort_after={abort_after})"
        );
        // (a) no non-done journal rows remain.
        assert!(
            store.pending_ops().unwrap().is_empty(),
            "recovery must leave zero pending rows (abort_after={abort_after})"
        );

        // (c) a follow-up purge returns the game to byte-for-byte pristine.
        let report = purge(&store, &game).unwrap();
        assert!(
            report.orphans.is_empty(),
            "purge must leave no orphans (abort_after={abort_after}): {:?}",
            report.orphans
        );
        let after = snapshot_tree(&fx.install).unwrap();
        assert_trees_identical(&pristine, &after);
        assert!(store.list_deployed_files(game.appid).unwrap().is_empty());

        // NOTE: the phase gate re-runs this on the dev btrfs filesystem per
        // VALIDATION.md — the hardest fs case (cross-subvolume EXDEV downgrade).
        let _ = &fx; // keep the tempdir alive to end of iteration
    }
}

/// A deploy that aborts and is then recovered FORWARD (the staged source is still
/// present at the per-game staging root that `replay` reconstructs) must FINISH the
/// interrupted op: roll it forward to `done` and record its manifest row. Files that
/// were never journaled stay undeployed (recovery converges to consistency; it does
/// not re-run un-started work). A subsequent purge still restores pristine.
///
/// Here the staged tree is rooted directly at `game.staging_dir` (the contract
/// `replay` assumes: `staging_dir/<target_rel>` is the source), so recovery locates
/// the source and rolls forward rather than rolling back.
#[test]
fn recovery_rolls_forward_when_staging_intact() {
    let fx = Fixture::new();
    let game = fx.game();

    // Vanilla originals.
    fs::create_dir_all(fx.install.join("Data/textures")).unwrap();
    fs::write(fx.install.join("Data/textures/rock.dds"), b"VANILLA-ROCK").unwrap();
    let pristine = snapshot_tree(&fx.install).unwrap();

    // Stage the mod DIRECTLY under game.staging_dir (the recovery contract root).
    fs::create_dir_all(fx.staging.join("Data/textures")).unwrap();
    fs::create_dir_all(fx.staging.join("Data/meshes")).unwrap();
    fs::write(fx.staging.join("Data/textures/rock.dds"), b"MOD-ROCK").unwrap();
    fs::write(fx.staging.join("Data/meshes/m1.nif"), b"MOD-M1").unwrap();
    let staged = StagedFiles {
        staging_root: fx.staging.clone(),
        files: vec![
            PathBuf::from("Data/textures/rock.dds"),
            PathBuf::from("Data/meshes/m1.nif"),
        ],
    };

    // Abort after the first file is fully done: file[0] complete, file[1] placed +
    // pending (interrupted).
    {
        let store = fx.open_store();
        let _ = deploy_with_abort(&store, &game, &staged, 1).unwrap_err();
        assert_eq!(store.list_deployed_files(game.appid).unwrap().len(), 1);
        assert_eq!(store.pending_ops().unwrap().len(), 1);
    }

    let store = fx.open_store();
    recover_on_launch(&store, &game).unwrap();

    // Forward recovery FINISHES the one interrupted op: 2 files now recorded.
    assert_eq!(
        store.list_deployed_files(game.appid).unwrap().len(),
        2,
        "forward recovery must finish exactly the interrupted op (1 done + 1 recovered)"
    );
    assert!(
        store.pending_ops().unwrap().is_empty(),
        "no pending rows remain after recovery"
    );
    assert_eq!(
        fs::read(fx.install.join("Data/textures/rock.dds")).unwrap(),
        b"MOD-ROCK",
        "the completed overwrite must carry the moded bytes"
    );
    assert!(
        fx.install.join("Data/meshes/m1.nif").exists(),
        "the recovered (interrupted) file must be present on disk"
    );

    // And a purge still returns to pristine.
    purge(&store, &game).unwrap();
    let after = snapshot_tree(&fx.install).unwrap();
    assert_trees_identical(&pristine, &after);
}
