//! Lexical path-containment guards shared by the engine and the conflict resolver.
//!
//! These are the V4/V5/V12 access-control primitives: assert a constructed path stays
//! inside a resolved root WITHOUT touching disk (canonicalize would fail on a
//! not-yet-created target). Promoted here from `engine.rs` so the conflict resolver
//! (`conflict.rs`) reuses the EXACT same containment semantics for winner paths
//! instead of re-implementing them (PATTERNS "Path-confinement guard").

use std::path::{Component, Path, PathBuf};

use crate::error::DeployError;

/// Assert `target` is within `root` (never write/deploy outside the resolved root).
///
/// Uses lexical containment so it works for not-yet-created paths.
pub(crate) fn guard_within_root(root: &Path, target: &Path) -> Result<(), DeployError> {
    let root_norm = lexical_normalize(root);
    let target_norm = lexical_normalize(target);
    if target_norm.starts_with(&root_norm) {
        Ok(())
    } else {
        Err(DeployError::PathEscape(target.to_path_buf()))
    }
}

/// Lexically normalize a path (resolve `.`/`..` components) without touching disk.
pub(crate) fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}
