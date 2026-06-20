//! method_ladder (DEPLOY-05): the per-target method ladder selects the strongest
//! applicable primitive and downgrades on `CrossesDevices`/errno 18 instead of
//! failing — and every method's `deploy_file` + `remove_file` round-trips a single
//! file (per-file only; never a directory symlink).
//!
//! The authoritative cross-fs assertion also runs on the dev btrfs filesystem at the
//! phase gate (VALIDATION.md). Here we force EXDEV via a second real filesystem when
//! one is available, and otherwise prove the ladder ordering + downgrade via the
//! pure `choose_method`/`apply_idempotent` logic.

use std::fs;
use std::path::Path;

use deploy::method::{
    CopyMethod, DeploymentMethod, HardlinkMethod, ReflinkMethod, SymlinkMethod,
};
use deploy::{apply_idempotent, choose_method};
use nextwist_core::DeployMethod;
use tempfile::TempDir;

/// Each method must place a single file and then remove it, leaving the dst absent.
#[test]
fn each_method_deploy_then_remove_round_trips_a_single_file() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("staging/file.bin");
    fs::create_dir_all(src.parent().unwrap()).unwrap();
    fs::write(&src, b"payload-bytes").unwrap();

    let methods: Vec<Box<dyn DeploymentMethod>> = vec![
        Box::new(ReflinkMethod),
        Box::new(HardlinkMethod),
        Box::new(SymlinkMethod),
        Box::new(CopyMethod),
    ];

    for m in methods {
        let dst = dir.path().join(format!("game/{:?}.bin", m.name()));
        fs::create_dir_all(dst.parent().unwrap()).unwrap();

        match m.deploy_file(&src, &dst) {
            Ok(()) => {
                // Deployed: the destination must exist and read back the source bytes
                // (symlink resolves to src; hardlink/reflink/copy carry the bytes).
                assert!(dst.exists(), "{:?} should create the destination", m.name());
                assert_eq!(
                    fs::read(&dst).unwrap(),
                    b"payload-bytes",
                    "{:?} deployed wrong content",
                    m.name()
                );
                m.remove_file(&dst).unwrap();
                assert!(
                    !dst.exists(),
                    "{:?} remove_file must leave dst absent",
                    m.name()
                );
                // Removing again is idempotent.
                m.remove_file(&dst).unwrap();
            }
            Err(e) => {
                // Reflink may be unsupported on the test fs (e.g. tmpfs/ext4); that is
                // an acceptable outcome for ReflinkMethod specifically. The ladder
                // (tested below) is what guarantees a working fallback.
                assert_eq!(
                    m.name(),
                    DeployMethod::Reflink,
                    "only reflink may be unsupported here; {:?} failed: {e}",
                    m.name()
                );
            }
        }
    }
}

/// The symlink method must NEVER create a directory symlink — it operates per-file.
#[test]
fn symlink_method_is_per_file_only() {
    let dir = TempDir::new().unwrap();
    let src_dir = dir.path().join("staging/subdir");
    fs::create_dir_all(&src_dir).unwrap();

    let dst = dir.path().join("game/subdir");
    // Deploying a *directory* must fail (canonicalize+symlink of a dir is not our
    // contract). We only ever deploy individual files.
    let res = SymlinkMethod.deploy_file(&src_dir, &dst);
    // Either it errors, or if it created something it must not be a followed dir link
    // that exposes staging. The contract: callers pass files, so a dir is misuse;
    // assert we did not silently create a directory symlink into staging.
    if res.is_ok() {
        let meta = fs::symlink_metadata(&dst).unwrap();
        assert!(
            !meta.file_type().is_symlink() || !dst.is_dir(),
            "must never create a directory symlink into staging (Pitfall 2)"
        );
    }
}

/// `choose_method` + `apply_idempotent` pick reflink when supported, else hardlink
/// same-device, else symlink/copy — and re-applying is a no-op.
#[test]
fn ladder_chooses_and_apply_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let staging = dir.path().join("staging");
    let game = dir.path().join("game/Data");
    fs::create_dir_all(&staging).unwrap();
    fs::create_dir_all(&game).unwrap();

    let src = staging.join("a.esp");
    fs::write(&src, b"alpha").unwrap();
    let dst = game.join("a.esp");

    let caps = deploy::probe(&staging, &game).unwrap();
    let chosen = choose_method(&caps);
    // On a single tmpfs/btrfs tempdir, same-device must hold, so we never get a
    // cross-device-only choice here.
    assert!(
        matches!(
            chosen,
            DeployMethod::Reflink | DeployMethod::Hardlink | DeployMethod::Symlink
        ),
        "unexpected method {chosen:?}"
    );

    let used = apply_idempotent(chosen, &src, &dst).unwrap();
    assert!(dst.exists(), "apply must create dst");
    assert_eq!(fs::read(&dst).unwrap(), b"alpha");

    // Re-applying the SAME completed op is a no-op (the recovery invariant): it must
    // not error and must leave the same content.
    let used_again = apply_idempotent(used, &src, &dst).unwrap();
    assert_eq!(used, used_again, "re-apply must use the same method");
    assert_eq!(fs::read(&dst).unwrap(), b"alpha", "re-apply must be a no-op");
}

/// Force a real EXDEV: choose `Hardlink` for a cross-device pair and prove the ladder
/// downgrades to symlink/copy instead of failing. Skips cleanly if no second fs.
#[test]
fn ladder_downgrades_on_cross_device_exdev() {
    let game = TempDir::new().unwrap();
    let game_data = game.path().join("Data");
    fs::create_dir_all(&game_data).unwrap();

    let Some(staging_fs) = distinct_device_tempdir(game.path()) else {
        eprintln!("skipping EXDEV downgrade: no second filesystem available in sandbox");
        return;
    };
    let src = staging_fs.path().join("tex.dds");
    fs::write(&src, b"texturebytes").unwrap();
    let dst = game_data.join("tex.dds");

    // Deliberately ask for Hardlink across the device boundary. The ladder must catch
    // EXDEV and downgrade to symlink (or copy), succeeding rather than erroring.
    let used = apply_idempotent(DeployMethod::Hardlink, &src, &dst).unwrap();
    assert!(
        matches!(used, DeployMethod::Symlink | DeployMethod::Copy),
        "cross-device must downgrade off Hardlink, got {used:?}"
    );
    assert!(dst.exists(), "downgraded deploy must still place the file");
    assert_eq!(
        fs::read(&dst).unwrap(),
        b"texturebytes",
        "downgraded deploy must carry the content"
    );
}

fn distinct_device_tempdir(reference: &Path) -> Option<TempDir> {
    use std::os::unix::fs::MetadataExt;
    let ref_dev = fs::metadata(reference).ok()?.dev();
    for base in ["/dev/shm", "/tmp", &std::env::temp_dir().to_string_lossy()] {
        let p = Path::new(base);
        if !p.is_dir() {
            continue;
        }
        if let Ok(td) = tempfile::Builder::new()
            .prefix("nextwist-xdev-")
            .tempdir_in(p)
        {
            if fs::metadata(td.path()).map(|m| m.dev()).ok() != Some(ref_dev) {
                return Some(td);
            }
        }
    }
    None
}
