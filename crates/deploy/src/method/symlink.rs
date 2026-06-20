//! Symlink deployment — the cross-device fallback.
//!
//! Used when staging and the game tree are on different filesystems (hardlink/reflink
//! impossible). PER-FILE ONLY — we never symlink a directory into `Data/` (Pitfall 2:
//! a Steam update could write *through* a directory symlink into staging, and Wine
//! path translation mishandles directory symlinks). The link target is the absolute
//! staged-file path so it resolves regardless of the game tree's location.

use std::io;
use std::path::Path;

use nextwist_core::DeployMethod;

use super::DeploymentMethod;

/// Deploy via `std::os::unix::fs::symlink` to the staged file (per-file only).
pub struct SymlinkMethod;

impl DeploymentMethod for SymlinkMethod {
    fn deploy_file(&self, src: &Path, dst: &Path) -> io::Result<()> {
        // Absolutize the target so the symlink resolves no matter where dst lives.
        let target = std::fs::canonicalize(src)?;
        std::os::unix::fs::symlink(&target, dst)
    }

    fn remove_file(&self, dst: &Path) -> io::Result<()> {
        super::remove_if_present(dst)
    }

    fn name(&self) -> DeployMethod {
        DeployMethod::Symlink
    }
}
