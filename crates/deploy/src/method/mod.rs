//! The `DeploymentMethod` trait + per-target method ladder (DEPLOY-05).
//!
//! A method places one staged file into the game tree and can remove it again. The
//! ladder is reflink → hardlink → symlink → copy, chosen per-target from the
//! [`FsCaps`](crate::probe::FsCaps) probe, and downgrades on `EXDEV` /
//! `io::ErrorKind::CrossesDevices` at apply time (the st_dev probe can miss a btrfs
//! subvolume boundary, so the apply path catches it as a backstop).
//!
//! ## Idempotency is the recovery invariant
//!
//! [`apply_idempotent`] is remove-if-present-then-create, so re-applying an already
//! completed op is a no-op — which is exactly what makes journal replay after a crash
//! safe. Every method deploys / removes a SINGLE FILE; we never symlink a directory
//! (Pitfall 2: a Steam update could write *through* a directory symlink into staging).

mod copy;
mod hardlink;
mod reflink;
mod symlink;

pub use copy::CopyMethod;
pub use hardlink::HardlinkMethod;
pub use reflink::ReflinkMethod;
pub use symlink::SymlinkMethod;

use std::io;
use std::path::Path;

use nextwist_core::DeployMethod;

use crate::error::DeployError;
use crate::probe::FsCaps;

/// A per-file deployment primitive: place a staged file, or remove a placed one.
pub trait DeploymentMethod {
    /// Place `src` (a staged regular file) at `dst`. The parent of `dst` is created
    /// by the caller. Must NOT operate on directories.
    fn deploy_file(&self, src: &Path, dst: &Path) -> io::Result<()>;

    /// Remove `dst` if it is a file we placed. Idempotent: a missing `dst` is `Ok`.
    fn remove_file(&self, dst: &Path) -> io::Result<()>;

    /// The [`DeployMethod`] tag this primitive records in the manifest.
    fn name(&self) -> DeployMethod;
}

/// Return the strongest applicable method for a probed `(staging, game_data)` pair.
///
/// reflink (independent inode, safest) → hardlink (same-device) → symlink
/// (cross-device fallback) → copy (last resort). Cross-device pairs skip the
/// link-based rungs entirely; the apply path still catches a late `EXDEV`.
pub fn choose_method(caps: &FsCaps) -> DeployMethod {
    if caps.reflink {
        DeployMethod::Reflink
    } else if caps.same_device && caps.hardlink_ok {
        DeployMethod::Hardlink
    } else {
        // Cross-device (or hardlink-incapable): prefer symlink over a full copy.
        DeployMethod::Symlink
    }
}

/// Construct the [`DeploymentMethod`] for a [`DeployMethod`] tag.
pub fn method_for(tag: DeployMethod) -> Box<dyn DeploymentMethod> {
    match tag {
        DeployMethod::Reflink => Box::new(ReflinkMethod),
        DeployMethod::Hardlink => Box::new(HardlinkMethod),
        DeployMethod::Symlink => Box::new(SymlinkMethod),
        DeployMethod::Copy => Box::new(CopyMethod),
    }
}

/// Idempotently apply `tag` to place `src` at `dst`, downgrading on cross-device
/// errors, and return the method that actually succeeded.
///
/// Idempotency: if `dst` already exists it is removed first, so re-applying a
/// completed op is a no-op (the recovery invariant). Downgrade ladder on `EXDEV`:
/// reflink → hardlink → symlink → copy. The parent directory of `dst` is created.
///
/// Symlink and copy never fail with `EXDEV`, so the ladder always terminates.
pub fn apply_idempotent(
    tag: DeployMethod,
    src: &Path,
    dst: &Path,
) -> Result<DeployMethod, DeployError> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| DeployError::io(parent, e))?;
    }

    // Try the chosen rung and each weaker rung until one succeeds. Remove any
    // existing dst before each attempt so the op is a no-op on re-apply.
    for candidate in ladder_from(tag) {
        // Remove a prior placement (ours) so create-style ops don't hit AlreadyExists.
        remove_if_present(dst).map_err(|e| DeployError::io(dst, e))?;
        let method = method_for(candidate);
        match method.deploy_file(src, dst) {
            Ok(()) => return Ok(candidate),
            Err(e) if is_cross_device(&e) => {
                tracing::warn!(
                    method = ?candidate,
                    src = %src.display(),
                    dst = %dst.display(),
                    "cross-device; downgrading method"
                );
                continue;
            }
            Err(e) => return Err(DeployError::io(dst, e)),
        }
    }
    // The ladder always ends in Copy, which cannot return EXDEV — unreachable in
    // practice, but return a precise error rather than panicking.
    Err(DeployError::io(
        dst,
        io::Error::other("method ladder exhausted"),
    ))
}

/// The weakening sequence starting at `tag` and ending at `Copy`.
fn ladder_from(tag: DeployMethod) -> Vec<DeployMethod> {
    let all = [
        DeployMethod::Reflink,
        DeployMethod::Hardlink,
        DeployMethod::Symlink,
        DeployMethod::Copy,
    ];
    let start = all.iter().position(|m| *m == tag).unwrap_or(0);
    all[start..].to_vec()
}

/// Whether an I/O error is a cross-device link error (EXDEV / errno 18).
pub(crate) fn is_cross_device(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::CrossesDevices || e.raw_os_error() == Some(18)
}

/// Remove `path` if it exists (file or symlink), treating absence as success.
pub(crate) fn remove_if_present(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::Casefold;

    fn caps(same: bool, reflink: bool, hardlink: bool) -> FsCaps {
        FsCaps {
            same_device: same,
            reflink,
            hardlink_ok: hardlink,
            casefold: Casefold::Unknown,
        }
    }

    #[test]
    fn choose_reflink_when_supported() {
        assert_eq!(choose_method(&caps(true, true, true)), DeployMethod::Reflink);
    }

    #[test]
    fn choose_hardlink_when_same_device_no_reflink() {
        assert_eq!(
            choose_method(&caps(true, false, true)),
            DeployMethod::Hardlink
        );
    }

    #[test]
    fn choose_symlink_when_cross_device() {
        assert_eq!(
            choose_method(&caps(false, false, false)),
            DeployMethod::Symlink
        );
    }

    #[test]
    fn ladder_starts_at_tag_and_ends_at_copy() {
        assert_eq!(
            ladder_from(DeployMethod::Hardlink),
            vec![DeployMethod::Hardlink, DeployMethod::Symlink, DeployMethod::Copy]
        );
        assert_eq!(ladder_from(DeployMethod::Copy), vec![DeployMethod::Copy]);
    }
}
