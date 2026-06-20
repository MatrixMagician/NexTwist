//! fs_probe (ENV-04): the per-target capability probe reports same_device /
//! reflink / hardlink_ok / casefold for a `(staging, game_data)` directory pair.
//!
//! The authoritative cross-fs assertion also runs on the dev btrfs filesystem at the
//! phase gate (VALIDATION.md). Here we cover the same-device happy path on a tempdir
//! plus, where a second real filesystem is available, the cross-device verdict.

use std::fs;

use deploy::probe;
use deploy::probe::Casefold;
use tempfile::TempDir;

#[test]
fn probe_same_dir_reports_same_device_and_hardlink_ok() {
    let dir = TempDir::new().unwrap();
    let staging = dir.path().join("staging");
    let game = dir.path().join("game/Data");
    fs::create_dir_all(&staging).unwrap();
    fs::create_dir_all(&game).unwrap();

    let caps = probe(&staging, &game).unwrap();

    assert!(
        caps.same_device,
        "two dirs under one tempdir must share an st_dev"
    );
    assert!(
        caps.hardlink_ok,
        "same-device hardlink probe must succeed (no EXDEV within one fs)"
    );
    // casefold is best-effort (A6): any verdict is acceptable, it must not error.
    assert!(matches!(
        caps.casefold,
        Casefold::On | Casefold::Off | Casefold::Unknown
    ));
    // The probe must leave no probe files behind in either directory.
    assert_eq!(count_entries(&staging), 0, "staging must be clean post-probe");
    assert_eq!(count_entries(&game), 0, "game data must be clean post-probe");
}

/// When a second, distinct filesystem is available (e.g. `/dev/shm` or `/tmp` tmpfs
/// vs the test's own tempdir), the probe must report `same_device = false` and the
/// hardlink probe must report `false` (EXDEV), proving cross-device detection.
#[test]
fn probe_cross_device_reports_not_same_device() {
    let here = TempDir::new().unwrap();
    let here_data = here.path().join("Data");
    fs::create_dir_all(&here_data).unwrap();

    // Find a directory on a different device than `here`.
    let Some(other) = distinct_device_tempdir(here.path()) else {
        eprintln!("skipping cross-device probe: no second filesystem available in sandbox");
        return;
    };
    let other_staging = other.path().join("staging");
    fs::create_dir_all(&other_staging).unwrap();

    let caps = probe(&other_staging, &here_data).unwrap();
    assert!(
        !caps.same_device,
        "staging on a different fs than game data must report cross-device"
    );
    assert!(
        !caps.hardlink_ok,
        "a cross-device hardlink probe must fail (EXDEV)"
    );
}

fn count_entries(dir: &std::path::Path) -> usize {
    fs::read_dir(dir).map(|rd| rd.count()).unwrap_or(0)
}

/// Try a few well-known second-filesystem locations and return a TempDir on a device
/// distinct from `reference`'s device, or `None` if none is available.
fn distinct_device_tempdir(reference: &std::path::Path) -> Option<TempDir> {
    use std::os::unix::fs::MetadataExt;
    let ref_dev = fs::metadata(reference).ok()?.dev();
    for base in ["/dev/shm", "/tmp", &std::env::temp_dir().to_string_lossy()] {
        let p = std::path::Path::new(base);
        if !p.is_dir() {
            continue;
        }
        if fs::metadata(p).map(|m| m.dev()).ok() == Some(ref_dev) {
            continue; // same device as the reference; not useful
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
