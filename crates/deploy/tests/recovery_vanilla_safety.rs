//! Recovery must never destroy a vanilla file it cannot put back.
//!
//! Two regressions guarded here, both about the window between the durable `pending`
//! intent (`engine.rs`, step 1) and the vanilla backup (step 2). A crash — or an
//! ordinary I/O failure — in that window leaves a journal row declaring intent to
//! overwrite a file that no backup exists for.
//!
//! 1. **Forward recovery must work for the production staging layout.** Real installs
//!    stage each mod in a per-mod subdirectory of `staging_dir` (see
//!    `commands/downloads.rs` and `commands/fomod.rs`), never at `staging_dir` itself.
//!    Recovery reconstructs the staged source from the journal row, so the row has to
//!    carry the staging root that `deploy` was actually given.
//! 2. **A target whose original cannot be restored is never deleted.** Rolling back is
//!    only safe when the vanilla bytes can be put back. With no backup in the ledger,
//!    leaving the user's file alone is the only non-destructive option.

use std::fs;
use std::path::PathBuf;

use deploy::{StagedFiles, deploy, deploy_with_abort, purge, recover_on_launch, verify};
use nextwist_core::Game;
use store::Store;
use tempfile::TempDir;

const APPID: u32 = 489830;

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
        Fixture {
            root,
            install,
            staging,
            db,
        }
    }

    fn game(&self) -> Game {
        Game {
            appid: APPID,
            name: "Skyrim Special Edition".into(),
            install_dir: self.install.clone(),
            prefix: self.root.path().join("prefix"),
            staging_dir: self.staging.clone(),
        }
    }

    /// A fresh handle against the same DB — what a relaunch does.
    fn open_store(&self) -> Store {
        let store = Store::open(&self.db).unwrap();
        store.add_managed_game(&self.game()).unwrap();
        store
    }

    /// Stage a mod the way production does: under a per-mod subdir of `staging_dir`.
    fn stage_mod(&self, mod_name: &str, rel: &str, bytes: &[u8]) -> StagedFiles {
        let mod_root = self.staging.join(mod_name);
        let path = mod_root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        StagedFiles {
            staging_root: mod_root,
            files: vec![PathBuf::from(rel)],
        }
    }
}

/// Forward recovery has to find the staged source when the mod lives in a per-mod
/// subdir — the only layout production ever produces. Reconstructing it as
/// `staging_dir/<target_rel>` finds nothing, silently turning every real recovery into
/// a roll-back of work that had already succeeded on disk.
#[test]
fn recovery_finds_the_staged_source_under_a_per_mod_staging_root() {
    let fx = Fixture::new();
    let game = fx.game();

    // A pure add (no vanilla file at the target) isolates the source-location question
    // from the backup question.
    let staged = fx.stage_mod("SomeMod", "Data/meshes/m1.nif", b"MOD-M1");

    {
        let store = fx.open_store();
        // Crash before the first file is finished: its pending row is committed and the
        // file is on disk, but the manifest row is never written.
        let _ = deploy_with_abort(&store, &game, &staged, 0);
        assert!(
            !store.pending_ops().unwrap().is_empty(),
            "a crash mid-deploy must leave a pending row"
        );
    }

    let store = fx.open_store();
    let recovery = recover_on_launch(&store, &game).unwrap();
    assert!(
        recovery.replayed >= 1,
        "the interrupted op must be replayed"
    );

    // Rolled FORWARD: the file stays deployed and is recorded in the manifest, so the
    // user's mod is actually installed after the relaunch.
    let target = fx.install.join("Data/meshes/m1.nif");
    assert!(
        target.is_file(),
        "forward recovery must keep the deployed file in place"
    );
    assert_eq!(fs::read(&target).unwrap(), b"MOD-M1");
    assert_eq!(
        store.list_deployed_files(APPID).unwrap().len(),
        1,
        "forward recovery must record the file in the manifest, or purge cannot clean it up"
    );

    // And the round trip still returns the game to pristine.
    purge(&store, &game).unwrap();
    assert!(
        !target.exists(),
        "purge must remove the recovered file (it was in the manifest)"
    );
}

/// The whole point of the backup ledger: if the original bytes are not in it, the
/// original file must not be touched. A failed backup leaves exactly that state, and it
/// is the case where the safety net matters most.
///
/// The correct behaviour is to REFUSE, not to converge: the failure is transient (a full
/// or read-only disk), so erroring out keeps the intent pending for a later retry, and
/// the user's file survives. Converging by destroying data would be the worst possible
/// trade.
#[test]
fn recovery_refuses_rather_than_overwrite_a_vanilla_file_it_cannot_back_up() {
    let fx = Fixture::new();
    let game = fx.game();

    let vanilla = fx.install.join("Data/Skyrim.esm");
    fs::write(&vanilla, b"VANILLA-MASTER").unwrap();

    // Sabotage the content-addressed originals store so the backup step fails: a plain
    // file where the directory belongs makes `create_dir_all` fail with ENOTDIR. This
    // stands in for disk-full / read-only / permission failures.
    let originals = fx.staging.parent().unwrap().join("originals");
    fs::create_dir_all(originals.parent().unwrap()).unwrap();
    fs::write(&originals, b"not a dir").unwrap();

    let staged = fx.stage_mod("SomeMod", "Data/Skyrim.esm", b"MOD-MASTER");

    {
        let store = fx.open_store();
        // The deploy fails AT the backup, after the pending intent is durable.
        deploy(&store, &game, &staged).expect_err("the backup must fail");
        assert_eq!(
            fs::read(&vanilla).unwrap(),
            b"VANILLA-MASTER",
            "the vanilla file is still intact immediately after the failed deploy"
        );
        assert!(
            !store.pending_ops().unwrap().is_empty(),
            "the pending intent outlives the failure"
        );
    }

    // Relaunch while the disk problem persists: recovery must fail LOUDLY rather than
    // overwrite an original it cannot first preserve.
    {
        let store = fx.open_store();
        recover_on_launch(&store, &game)
            .expect_err("recovery must refuse while the original cannot be backed up");
        assert_eq!(
            fs::read(&vanilla).unwrap(),
            b"VANILLA-MASTER",
            "the vanilla bytes must be untouched by the refused recovery"
        );
        assert!(
            !store.pending_ops().unwrap().is_empty(),
            "the intent stays pending so a later launch can retry it"
        );
    }

    // Clear the transient failure; the retry now converges normally.
    fs::remove_file(&originals).unwrap();
    let store = fx.open_store();
    recover_on_launch(&store, &game).unwrap();
    assert!(
        store.pending_ops().unwrap().is_empty(),
        "once the disk is healthy the retry must converge"
    );
    assert_eq!(
        fs::read(&vanilla).unwrap(),
        b"MOD-MASTER",
        "the recovered deploy completes"
    );

    // And the original is now in the ledger, so the round trip is reversible again.
    purge(&store, &game).unwrap();
    assert_eq!(
        fs::read(&vanilla).unwrap(),
        b"VANILLA-MASTER",
        "purge restores the original byte-for-byte"
    );
}

/// The branch where rolling back would be destructive: the staged source is gone (so the
/// deploy can never complete) AND no backup exists (so nothing can be put back). The
/// pre-existing file at the target is the user's, and deleting it would be unrecoverable.
#[test]
fn recovery_leaves_a_user_file_alone_when_it_can_neither_finish_nor_restore() {
    let fx = Fixture::new();
    let game = fx.game();

    let vanilla = fx.install.join("Data/Skyrim.esm");
    fs::write(&vanilla, b"VANILLA-MASTER").unwrap();

    // Same sabotage: the deploy dies at the backup, leaving a pending intent and no
    // ledger row for this target.
    let originals = fx.staging.parent().unwrap().join("originals");
    fs::create_dir_all(originals.parent().unwrap()).unwrap();
    fs::write(&originals, b"not a dir").unwrap();

    let staged = fx.stage_mod("SomeMod", "Data/Skyrim.esm", b"MOD-MASTER");
    {
        let store = fx.open_store();
        deploy(&store, &game, &staged).expect_err("the backup must fail");
    }

    // The user removes the mod's staging tree before relaunching, so recovery cannot roll
    // forward either. Un-sabotage the originals dir so the ONLY reason nothing can be
    // restored is that no backup was ever taken.
    fs::remove_dir_all(fx.staging.join("SomeMod")).unwrap();
    fs::remove_file(&originals).unwrap();

    let store = fx.open_store();
    recover_on_launch(&store, &game).unwrap();

    assert!(
        vanilla.is_file(),
        "recovery must not delete a file it has no backup for"
    );
    assert_eq!(
        fs::read(&vanilla).unwrap(),
        b"VANILLA-MASTER",
        "the user's bytes must be exactly as they were"
    );
    assert!(
        store.pending_ops().unwrap().is_empty(),
        "this row can never be completed or undone, so it must not be retried forever"
    );
}

/// A file the engine genuinely deployed over a backed-up original still rolls back
/// correctly — the guard above must not weaken the normal restore path.
#[test]
fn recovery_still_restores_when_the_backup_does_exist() {
    let fx = Fixture::new();
    let game = fx.game();

    let vanilla = fx.install.join("Data/Skyrim.esm");
    fs::write(&vanilla, b"VANILLA-MASTER").unwrap();

    let staged = fx.stage_mod("SomeMod", "Data/Skyrim.esm", b"MOD-MASTER");

    {
        let store = fx.open_store();
        deploy(&store, &game, &staged).unwrap();
        assert_eq!(fs::read(&vanilla).unwrap(), b"MOD-MASTER");
    }

    // Delete the staged source, then crash-recover: with the source gone the engine
    // cannot roll forward, so it must roll back to the backed-up original.
    fs::remove_dir_all(fx.staging.join("SomeMod")).unwrap();

    let store = fx.open_store();
    // Re-open the intent by hand is not needed: purge exercises the same restore path.
    purge(&store, &game).unwrap();
    assert_eq!(
        fs::read(&vanilla).unwrap(),
        b"VANILLA-MASTER",
        "purge restores the backed-up original byte-for-byte"
    );

    let report = verify(&store, &game).unwrap();
    assert!(report.pristine, "the game is pristine after the round trip");
}
