//! Proves the reflink rung of the ladder is genuinely exercised on a CoW filesystem,
//! rather than silently downgrading. Run with TMPDIR on btrfs/XFS.

use std::fs;

use deploy::method::deploy_one;
use deploy::{apply_idempotent, choose_method};
use nextwist_core::DeployMethod;
use tempfile::TempDir;

#[test]
fn reflink_rung_is_really_taken_on_a_cow_filesystem() {
    let dir = TempDir::new().unwrap();
    let staging = dir.path().join("staging");
    let game = dir.path().join("game/Data");
    fs::create_dir_all(&staging).unwrap();
    fs::create_dir_all(&game).unwrap();

    let src = staging.join("probe.bin");
    fs::write(&src, b"cow-bytes").unwrap();

    // Probe first: on tmpfs/ext4 there is nothing to prove, so skip rather than fail.
    let caps = deploy::probe(&staging, &game).unwrap();
    if !caps.reflink {
        eprintln!("skipping: test filesystem does not support reflink");
        return;
    }

    assert_eq!(
        choose_method(&caps),
        DeployMethod::Reflink,
        "a reflink-capable target must choose the strongest rung"
    );

    // The primitive itself must succeed, not fall back internally.
    let dst = game.join("direct.bin");
    deploy_one(DeployMethod::Reflink, &src, &dst).expect("reflink must succeed on a CoW fs");
    assert_eq!(fs::read(&dst).unwrap(), b"cow-bytes");

    // A reflink is an INDEPENDENT inode sharing blocks copy-on-write - not a hardlink
    // (shared inode) and not a symlink. That distinction is the safety property: editing
    // the deployed file can never write through into read-only staging.
    let src_meta = fs::symlink_metadata(&src).unwrap();
    let dst_meta = fs::symlink_metadata(&dst).unwrap();
    assert!(
        !dst_meta.file_type().is_symlink(),
        "a reflink must not be a symlink"
    );
    {
        use std::os::unix::fs::MetadataExt;
        assert_ne!(
            src_meta.ino(),
            dst_meta.ino(),
            "a reflink must be an independent inode, not a hardlink"
        );
    }

    // And the ladder records the rung that actually ran, rather than a weaker one.
    let laddered = game.join("laddered.bin");
    let used = apply_idempotent(DeployMethod::Reflink, &src, &laddered).unwrap();
    assert_eq!(
        used,
        DeployMethod::Reflink,
        "the recorded method must be the one that truly succeeded"
    );

    // Writing through the deployed copy must NOT corrupt the staged source (CoW).
    fs::write(&laddered, b"mutated!!").unwrap();
    assert_eq!(
        fs::read(&src).unwrap(),
        b"cow-bytes",
        "editing a reflinked deploy must never write through into staging"
    );
}
