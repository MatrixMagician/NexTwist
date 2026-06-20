//! Per-target filesystem-capability probe (ENV-04).
//!
//! The deploy engine NEVER decides a method globally — btrfs returns `EXDEV`
//! (`io::ErrorKind::CrossesDevices`, errno 18) across subvolumes even on the same
//! physical disk, and the dev machine here is btrfs, so cross-device is the COMMON
//! case. For each `(staging_dir, game_data_dir)` pair we empirically probe:
//!
//! * `same_device` — compare `st_dev` via `MetadataExt::dev()`.
//! * `reflink`     — whether a real throwaway reflink between the two dirs succeeds.
//! * `hardlink_ok` — whether a real throwaway `hard_link` between the two dirs
//!   succeeds (the authoritative btrfs-subvolume EXDEV backstop — A5: st_dev alone
//!   is insufficient).
//! * `casefold`    — best-effort ext4 `+F` casefold flag (A6: a warning, not a gate).
//!
//! ## Why empirical reflink, not `check_reflink_support`
//!
//! `reflink_copy::check_reflink_support` is implemented only on Windows; on Linux it
//! unconditionally returns `Ok(ReflinkSupport::Unknown)` (confirmed against
//! reflink-copy 0.1.30 source — Assumption A1 resolved). So the authoritative Linux
//! verdict is a real throwaway `reflink_copy::reflink` of a temp file from the
//! staging dir into the game-data dir, which exercises the actual FICLONE path.

use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use reflink_copy::{check_reflink_support, ReflinkSupport};

/// Result of [`probe`] for a `(staging, game_data)` directory pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsCaps {
    /// Staging and game-data dirs share an `st_dev` (necessary, not sufficient, for
    /// hardlink/reflink — btrfs subvolumes can differ on the same disk).
    pub same_device: bool,
    /// A real throwaway reflink between the two dirs succeeded.
    pub reflink: bool,
    /// A real throwaway `hard_link` between the two dirs succeeded.
    pub hardlink_ok: bool,
    /// Best-effort casefold verdict for the game-data dir.
    pub casefold: Casefold,
}

/// Best-effort case-folding (ext4 `+F`) verdict. A nice-to-have warning in Phase 1;
/// casing normalization is the locked primary approach, so this never gates deploy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Casefold {
    /// The directory has the casefold flag set.
    On,
    /// The directory does not have the casefold flag set.
    Off,
    /// Could not determine (ioctl unsupported / errored) — treated as a warning (A6).
    Unknown,
}

/// Probe filesystem capabilities for deploying from `staging` into `game_data`.
///
/// Both paths must be existing directories. Returns `same_device`, the empirical
/// `reflink`/`hardlink_ok` verdicts, and a best-effort `casefold` flag. Probe
/// failures degrade gracefully (a failed reflink/hardlink probe is reported as
/// `false`, not an error) so a hostile filesystem never aborts game-add.
pub fn probe(staging: &Path, game_data: &Path) -> io::Result<FsCaps> {
    let same_device = fs::metadata(staging)?.dev() == fs::metadata(game_data)?.dev();

    // `check_reflink_support` is a no-op (Unknown) on Linux — call it for portability
    // but rely on the empirical throwaway probe for the real verdict.
    let _portable_hint: ReflinkSupport =
        check_reflink_support(staging, game_data).unwrap_or(ReflinkSupport::Unknown);

    let reflink = throwaway_reflink_ok(staging, game_data);
    let hardlink_ok = throwaway_hardlink_ok(staging, game_data);
    let casefold = read_casefold(game_data);

    Ok(FsCaps {
        same_device,
        reflink,
        hardlink_ok,
        casefold,
    })
}

/// Attempt a real reflink of a freshly-written temp file in `staging` to a temp path
/// in `game_data`. Cleans up both paths. Returns whether the reflink syscall worked.
fn throwaway_reflink_ok(staging: &Path, game_data: &Path) -> bool {
    let src = match write_probe_file(staging, b"nextwist-reflink-probe") {
        Ok(p) => p,
        Err(_) => return false,
    };
    let dst = game_data.join(probe_name("reflink-dst"));
    let _ = fs::remove_file(&dst);
    let ok = reflink_copy::reflink(&src, &dst).is_ok();
    let _ = fs::remove_file(&src);
    let _ = fs::remove_file(&dst);
    ok
}

/// Attempt a real `hard_link` from a temp file in `staging` to a temp path in
/// `game_data`. This is the authoritative btrfs-subvolume EXDEV backstop. Cleans up.
fn throwaway_hardlink_ok(staging: &Path, game_data: &Path) -> bool {
    let src = match write_probe_file(staging, b"nextwist-hardlink-probe") {
        Ok(p) => p,
        Err(_) => return false,
    };
    let dst = game_data.join(probe_name("hardlink-dst"));
    let _ = fs::remove_file(&dst);
    let ok = match fs::hard_link(&src, &dst) {
        Ok(()) => true,
        Err(e)
            if e.kind() == io::ErrorKind::CrossesDevices || e.raw_os_error() == Some(18) =>
        {
            false
        }
        Err(_) => false,
    };
    let _ = fs::remove_file(&src);
    let _ = fs::remove_file(&dst);
    ok
}

/// Write a uniquely-named probe file into `dir`, returning its path.
fn write_probe_file(dir: &Path, contents: &[u8]) -> io::Result<std::path::PathBuf> {
    let path = dir.join(probe_name("src"));
    fs::write(&path, contents)?;
    Ok(path)
}

/// A unique-enough probe filename (pid + nanos + tag) to avoid concurrent collisions.
fn probe_name(tag: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(".nextwist-probe-{tag}-{}-{nanos}", std::process::id())
}

/// Best-effort read of the ext4 casefold flag via `FS_IOC_GETFLAGS`.
///
/// `FS_CASEFOLD_FL = 0x40000000` (A6 — confirmed value used widely; the ioctl path
/// is wrapped so any failure degrades to [`Casefold::Unknown`] rather than erroring).
fn read_casefold(dir: &Path) -> Casefold {
    const FS_CASEFOLD_FL: i64 = 0x4000_0000;
    match get_inode_flags(dir) {
        Ok(flags) if flags & FS_CASEFOLD_FL != 0 => Casefold::On,
        Ok(_) => Casefold::Off,
        Err(_) => Casefold::Unknown,
    }
}

/// Issue `FS_IOC_GETFLAGS` on `dir` and return the raw flags. Errors on any failure.
fn get_inode_flags(dir: &Path) -> io::Result<i64> {
    use std::os::unix::io::AsRawFd;
    // _IOR('f', 1, long) == FS_IOC_GETFLAGS. On Linux this is 0x80086601.
    const FS_IOC_GETFLAGS: libc_request_t = 0x8008_6601;
    let file = fs::File::open(dir)?;
    let fd = file.as_raw_fd();
    let mut flags: i64 = 0;
    // SAFETY: fd is a valid open directory fd; &mut flags points at a writable long.
    let rc = unsafe { raw_ioctl(fd, FS_IOC_GETFLAGS, &mut flags as *mut i64) };
    if rc == 0 {
        Ok(flags)
    } else {
        Err(io::Error::last_os_error())
    }
}

// We avoid pulling in the `libc` crate (not in the pinned stack) for a single ioctl;
// declare the syscall directly. The request type is `unsigned long` on Linux.
#[allow(non_camel_case_types)]
type libc_request_t = std::os::raw::c_ulong;

unsafe extern "C" {
    /// `int ioctl(int fd, unsigned long request, ...)`
    #[link_name = "ioctl"]
    fn raw_ioctl(fd: std::os::raw::c_int, request: libc_request_t, arg: *mut i64)
        -> std::os::raw::c_int;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn same_dir_reports_same_device() {
        let dir = TempDir::new().unwrap();
        let a = dir.path().join("staging");
        let b = dir.path().join("game");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let caps = probe(&a, &b).unwrap();
        assert!(caps.same_device, "two dirs under one tempdir share a device");
        // hardlink within the same tmpfs device must work.
        assert!(caps.hardlink_ok, "same-device hardlink probe should succeed");
        // casefold is best-effort; any of the three verdicts is acceptable here.
        assert!(matches!(
            caps.casefold,
            Casefold::On | Casefold::Off | Casefold::Unknown
        ));
    }
}
