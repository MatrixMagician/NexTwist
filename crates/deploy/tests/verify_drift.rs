//! verify_drift (DEPLOY-07): the verify/repair pass hash-diffs the per-game manifest
//! against the on-disk game tree and classifies drift as `missing` (recorded but absent
//! on disk), `changed` (recorded but on-disk bytes hash differently), or `orphan` (on
//! disk under Data/ but neither recorded by us nor a known vanilla original). `repair`
//! restores missing+changed managed files but NEVER deletes orphans — unmanaged
//! user/vanilla files are reported only (Pitfall 4 / threat T-01-16).

use std::fs;
use std::path::{Path, PathBuf};

use deploy::{deploy, repair, verify, StagedFiles};
use nextwist_core::Game;
use store::Store;

/// A deployed test harness: a game with a single 2-file mod deployed into its Data/.
struct Harness {
    _root: tempfile::TempDir,
    store: Store,
    game: Game,
    install: PathBuf,
}

/// The two `Data/`-rooted relpaths the test mod deploys.
const REL_A: &str = "Data/textures/a.dds";
const REL_B: &str = "Data/meshes/b.nif";

fn deploy_two_file_mod() -> Harness {
    let root = tempfile::TempDir::new().unwrap();
    let install = root.path().join("game");
    let staging = root.path().join("app/staging/489830");
    fs::create_dir_all(install.join("Data")).unwrap();
    fs::create_dir_all(&staging).unwrap();

    let store = Store::open(&root.path().join("app/nextwist.db")).unwrap();
    let game = Game {
        appid: 489830,
        name: "Skyrim Special Edition".into(),
        install_dir: install.clone(),
        prefix: root.path().join("prefix"),
        staging_dir: staging.clone(),
    };
    store.add_managed_game(&game).unwrap();

    // Stage the mod tree at the game's staging_dir itself, so the staged source for a
    // `Data/`-rooted relpath is `staging_dir/Data/...` — the recover/repair contract
    // documented in Plan 04 (journal::replay reconstructs the source the same way).
    for (rel, bytes) in [(REL_A, b"mod-a-bytes".as_slice()), (REL_B, b"mod-b-bytes")] {
        let p = staging.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, bytes).unwrap();
    }
    let staged = StagedFiles {
        staging_root: staging.clone(),
        files: vec![PathBuf::from(REL_A), PathBuf::from(REL_B)],
    };
    deploy(&store, &game, &staged).unwrap();

    Harness {
        _root: root,
        store,
        game,
        install,
    }
}

/// The absolute on-disk path for a `Data/`-rooted relpath under the install dir.
fn on_disk(install: &Path, rel: &str) -> PathBuf {
    install.join(rel)
}

#[test]
fn clean_deployment_is_pristine() {
    let h = deploy_two_file_mod();
    let report = verify(&h.store, &h.game).unwrap();
    assert!(report.pristine, "fresh deployment must verify pristine: {report:?}");
    assert!(report.missing.is_empty());
    assert!(report.changed.is_empty());
    assert!(report.orphans.is_empty());
}

#[test]
fn deleted_deployed_file_is_missing() {
    let h = deploy_two_file_mod();
    fs::remove_file(on_disk(&h.install, REL_A)).unwrap();

    let report = verify(&h.store, &h.game).unwrap();
    assert!(!report.pristine);
    assert!(
        report.missing.iter().any(|p| p.ends_with("textures/a.dds")),
        "deleted file must be reported missing: {report:?}"
    );
    assert!(report.changed.is_empty());
}

#[test]
fn mutated_deployed_file_is_changed() {
    let h = deploy_two_file_mod();
    // Replace the deployed placement with different bytes (a real on-disk mutation).
    let target = on_disk(&h.install, REL_B);
    fs::remove_file(&target).unwrap();
    fs::write(&target, b"TAMPERED-bytes").unwrap();

    let report = verify(&h.store, &h.game).unwrap();
    assert!(!report.pristine);
    assert!(
        report.changed.iter().any(|p| p.ends_with("meshes/b.nif")),
        "mutated file must be reported changed: {report:?}"
    );
    assert!(report.missing.is_empty());
}

#[test]
fn unrecorded_extra_file_is_orphan_and_repair_does_not_delete_it() {
    let h = deploy_two_file_mod();
    // Drop an extra, unrecorded file under Data/ (an unmanaged user/vanilla file).
    let orphan = on_disk(&h.install, "Data/extra/user.txt");
    fs::create_dir_all(orphan.parent().unwrap()).unwrap();
    fs::write(&orphan, b"user-added").unwrap();

    let report = verify(&h.store, &h.game).unwrap();
    assert!(!report.pristine);
    assert!(
        report.orphans.iter().any(|p| p.ends_with("extra/user.txt")),
        "extra unrecorded file must be reported as an orphan: {report:?}"
    );

    // repair MUST report-not-delete the orphan (threat T-01-16 / Pitfall 4).
    let rep = repair(&h.store, &h.game).unwrap();
    assert!(
        rep.orphans.iter().any(|p| p.ends_with("extra/user.txt")),
        "repair must surface the orphan: {rep:?}"
    );
    assert!(orphan.is_file(), "repair must NEVER delete an orphan");
    assert_eq!(fs::read(&orphan).unwrap(), b"user-added");
}

#[test]
fn repair_restores_missing_and_changed_managed_files() {
    let h = deploy_two_file_mod();
    // Damage both: delete A, mutate B.
    fs::remove_file(on_disk(&h.install, REL_A)).unwrap();
    let b = on_disk(&h.install, REL_B);
    fs::remove_file(&b).unwrap();
    fs::write(&b, b"TAMPERED").unwrap();

    let before = verify(&h.store, &h.game).unwrap();
    assert!(!before.pristine);

    let rep = repair(&h.store, &h.game).unwrap();
    assert_eq!(rep.restored_missing, 1, "one missing file re-deployed: {rep:?}");
    assert_eq!(rep.restored_changed, 1, "one changed file restored: {rep:?}");

    // After repair the deployment verifies pristine again.
    let after = verify(&h.store, &h.game).unwrap();
    assert!(after.pristine, "repair must restore pristine: {after:?}");
    assert_eq!(fs::read(on_disk(&h.install, REL_A)).unwrap(), b"mod-a-bytes");
    assert_eq!(fs::read(&b).unwrap(), b"mod-b-bytes");
}
