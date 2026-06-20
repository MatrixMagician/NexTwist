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

#[test]
fn empty_orphan_dir_is_reported_and_repair_removes_it() {
    let h = deploy_two_file_mod();
    // Plant an EMPTY mod-introduced subdir under Data/ (the GAP-01 orphan shape).
    let orphan_dir = on_disk(&h.install, "Data/leftover/empty");
    fs::create_dir_all(&orphan_dir).unwrap();

    let report = verify(&h.store, &h.game).unwrap();
    assert!(!report.pristine, "an orphan empty dir must make verify non-pristine");
    assert!(
        report.orphan_dirs.iter().any(|p| p.ends_with("leftover/empty")),
        "empty mod-introduced subdir must be reported in orphan_dirs: {report:?}"
    );
    // It is a DIR orphan, not a FILE orphan.
    assert!(report.orphans.is_empty(), "no file orphans expected: {report:?}");

    // repair removes exactly the orphan empty dir(s).
    let rep = repair(&h.store, &h.game).unwrap();
    assert_eq!(
        rep.removed_orphan_dirs, 2,
        "both the empty leaf and its now-empty parent are removed bottom-up: {rep:?}"
    );
    assert!(!orphan_dir.exists(), "repair must remove the orphan empty dir");
    assert!(
        !on_disk(&h.install, "Data/leftover").exists(),
        "the parent, now empty, is also removed bottom-up"
    );

    // The managed deployment is untouched and the tree verifies pristine again.
    let after = verify(&h.store, &h.game).unwrap();
    assert!(after.pristine, "after removing orphan dirs the tree is pristine: {after:?}");
    assert!(on_disk(&h.install, REL_A).is_file());
    assert!(on_disk(&h.install, REL_B).is_file());
}

#[test]
fn dir_with_unmanaged_file_is_not_orphan_dir_and_file_is_not_deleted() {
    let h = deploy_two_file_mod();
    // A subdir that still HOLDS an unmanaged file is NOT an orphan dir; the file is a
    // (report-only) file orphan that repair must never delete.
    let unmanaged = on_disk(&h.install, "Data/usermod/keep.txt");
    fs::create_dir_all(unmanaged.parent().unwrap()).unwrap();
    fs::write(&unmanaged, b"user-content").unwrap();

    let report = verify(&h.store, &h.game).unwrap();
    assert!(!report.pristine);
    // The non-empty dir is NOT an orphan dir.
    assert!(
        !report.orphan_dirs.iter().any(|p| p.ends_with("usermod")),
        "a directory holding an unmanaged file must NOT be an orphan dir: {report:?}"
    );
    // Its file IS a (report-only) file orphan.
    assert!(
        report.orphans.iter().any(|p| p.ends_with("usermod/keep.txt")),
        "the unmanaged file must be a report-only file orphan: {report:?}"
    );

    // repair removes no orphan dir (the dir is non-empty) and never deletes the file.
    let rep = repair(&h.store, &h.game).unwrap();
    assert_eq!(rep.removed_orphan_dirs, 0, "no empty orphan dir to remove: {rep:?}");
    assert!(unmanaged.is_file(), "repair must NEVER delete an unmanaged file (T-01-16)");
    assert_eq!(fs::read(&unmanaged).unwrap(), b"user-content");
    assert!(
        unmanaged.parent().unwrap().is_dir(),
        "the dir holding an unmanaged file must remain"
    );
}

#[test]
fn clean_deployment_has_no_orphan_dirs() {
    // A clean deployment must produce zero false orphan_dirs (its mod-introduced dirs
    // are explained ancestors of managed targets).
    let h = deploy_two_file_mod();
    let report = verify(&h.store, &h.game).unwrap();
    assert!(report.pristine, "clean deployment must be pristine: {report:?}");
    assert!(
        report.orphan_dirs.is_empty(),
        "clean deployment must have no orphan dirs (managed-target ancestors are explained): {report:?}"
    );
}
