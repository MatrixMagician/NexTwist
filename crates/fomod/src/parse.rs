//! Locate and deserialize `fomod/ModuleConfig.xml` into the [`crate::model`] AST.
//!
//! STUB (Task 1 RED): signatures only. Implemented in Task 2 (GREEN).

use std::path::{Path, PathBuf};

use crate::error::FomodError;
use crate::model::FomodModule;

/// Locate `fomod/ModuleConfig.xml` case-insensitively under `tree_root`, strip a leading
/// UTF-8 BOM, and deserialize it into a [`FomodModule`].
///
/// `tree_root` is the detected archive root (see `extract::detect_archive_root`). The
/// `fomod` folder and `ModuleConfig.xml` filename are matched case-insensitively
/// (Pitfall 3 — the spec documents the fomod folder as case-insensitive).
pub fn parse_module_config(tree_root: &Path) -> Result<FomodModule, FomodError> {
    let _ = tree_root;
    unimplemented!("parse_module_config implemented in Task 2 (GREEN)")
}

/// Resolve a FOMOD `source` string (e.g. `Textures/X.DDS`) onto the actual staged-tree
/// path, matching every path component case-insensitively. Returns the real on-disk path
/// or [`FomodError::MissingSource`] if no case-insensitive match exists.
pub fn resolve_source_path(tree_root: &Path, source: &str) -> Result<PathBuf, FomodError> {
    let _ = (tree_root, source);
    unimplemented!("resolve_source_path implemented in Task 2 (GREEN)")
}
