//! Reflink (copy-on-write) deployment — the safest primitive.
//!
//! A reflink is an independent inode that shares physical blocks copy-on-write, so
//! editing the deployed file can never corrupt the read-only staging copy (unlike a
//! hardlink, which shares the inode). Preferred wherever the filesystem supports it
//! (btrfs/XFS/bcachefs). Per-file only — `reflink_copy::reflink` rejects directories.

use std::io;
use std::path::Path;

use nextwist_core::DeployMethod;

use super::DeploymentMethod;

/// Deploy via `reflink_copy::reflink` (NOT `reflink_or_copy` — we control fallback
/// explicitly in the ladder so the method recorded in the manifest is always true).
pub struct ReflinkMethod;

impl DeploymentMethod for ReflinkMethod {
    fn deploy_file(&self, src: &Path, dst: &Path) -> io::Result<()> {
        // `reflink` errors with AlreadyExists if dst exists; the ladder removes it
        // first. Cross-device / non-CoW filesystems surface an error the ladder
        // treats as a downgrade signal.
        reflink_copy::reflink(src, dst)
    }

    fn remove_file(&self, dst: &Path) -> io::Result<()> {
        super::remove_if_present(dst)
    }

    fn name(&self) -> DeployMethod {
        DeployMethod::Reflink
    }
}
