//! Hardlink deployment — same-device, instant, space-efficient.
//!
//! A hardlink shares the inode with the staged file. The staged tree is marked
//! read-only (by `extract`), preserving the safety invariant that the deployed file
//! cannot be edited through to corrupt staging. Fails with `EXDEV` across devices /
//! btrfs subvolumes — the ladder catches that and downgrades to symlink/copy.

use std::io;
use std::path::Path;

use nextwist_core::DeployMethod;

use super::DeploymentMethod;

/// Deploy via `std::fs::hard_link`.
pub struct HardlinkMethod;

impl DeploymentMethod for HardlinkMethod {
    fn deploy_file(&self, src: &Path, dst: &Path) -> io::Result<()> {
        // Returns Err(CrossesDevices) across filesystems; the ladder downgrades.
        std::fs::hard_link(src, dst)
    }

    fn remove_file(&self, dst: &Path) -> io::Result<()> {
        super::remove_if_present(dst)
    }

    fn name(&self) -> DeployMethod {
        DeployMethod::Hardlink
    }
}
