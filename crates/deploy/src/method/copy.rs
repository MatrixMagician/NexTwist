//! Plain byte-copy deployment — the last-resort fallback.
//!
//! Used only when no link-based primitive applies (cross-device with no symlink, or
//! a filesystem that rejects everything else). Doubles disk usage, so it is the
//! bottom rung — but it always works and never returns `EXDEV`, which is what makes
//! it the guaranteed terminator of the method ladder.

use std::io;
use std::path::Path;

use nextwist_core::DeployMethod;

use super::DeploymentMethod;

/// Deploy via `std::fs::copy`.
pub struct CopyMethod;

impl DeploymentMethod for CopyMethod {
    fn deploy_file(&self, src: &Path, dst: &Path) -> io::Result<()> {
        std::fs::copy(src, dst).map(|_| ())
    }

    fn remove_file(&self, dst: &Path) -> io::Result<()> {
        super::remove_if_present(dst)
    }

    fn name(&self) -> DeployMethod {
        DeployMethod::Copy
    }
}
