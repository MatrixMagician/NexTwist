//! Locate and deserialize `fomod/ModuleConfig.xml` into the [`crate::model`] AST.
//!
//! The `fomod` folder and the `ModuleConfig.xml` filename are matched
//! case-insensitively (Pitfall 3 — the spec documents the fomod folder as
//! case-insensitive, and Wine/authoring produce inconsistent casing). A leading UTF-8
//! BOM is stripped before deserialization (Pitfall 5). Deserialization is
//! namespace-ignorant: quick-xml's serde matches LOCAL element names, so the
//! `xsi:noNamespaceSchemaLocation` attribute on real-world files is ignored.

use std::path::{Path, PathBuf};

use quick_xml::de::from_str;
use walkdir::WalkDir;

use crate::error::FomodError;
use crate::model::FomodModule;

/// Locate `fomod/ModuleConfig.xml` case-insensitively under `tree_root`, strip a leading
/// UTF-8 BOM, and deserialize it into a [`FomodModule`].
pub fn parse_module_config(tree_root: &Path) -> Result<FomodModule, FomodError> {
    let config_path = locate_module_config(tree_root)
        .ok_or_else(|| FomodError::ConfigNotFound(tree_root.to_path_buf()))?;

    let raw = std::fs::read_to_string(&config_path)
        .map_err(|e| FomodError::io(&config_path, e))?;
    let xml = raw.strip_prefix('\u{feff}').unwrap_or(&raw);

    let module: FomodModule = from_str(xml).map_err(|e| FomodError::Xml(e.to_string()))?;
    tracing::debug!(module = %module.module_name, "parsed FOMOD ModuleConfig.xml");
    Ok(module)
}

/// Find a `fomod` directory (any case) under `tree_root` and return the path to its
/// `ModuleConfig.xml` (any case). Returns `None` if no such file exists.
fn locate_module_config(tree_root: &Path) -> Option<PathBuf> {
    for entry in WalkDir::new(tree_root).follow_links(false).into_iter().flatten() {
        if !entry.file_type().is_dir() {
            continue;
        }
        if !entry
            .file_name()
            .to_str()
            .is_some_and(|n| n.eq_ignore_ascii_case("fomod"))
        {
            continue;
        }
        // Found a fomod dir — scan its direct children for ModuleConfig.xml (any case).
        if let Ok(rd) = std::fs::read_dir(entry.path()) {
            for child in rd.flatten() {
                if child
                    .file_name()
                    .to_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case("moduleconfig.xml"))
                {
                    return Some(child.path());
                }
            }
        }
    }
    None
}

/// Resolve a FOMOD `source` string (e.g. `Textures/X.DDS`) onto the actual staged-tree
/// path, matching every path component case-insensitively. Returns the real on-disk path
/// or [`FomodError::MissingSource`] if no case-insensitive match exists.
pub fn resolve_source_path(tree_root: &Path, source: &str) -> Result<PathBuf, FomodError> {
    // Normalize separators: FOMOD authors use `\` (Windows) or `/`.
    let normalized = source.replace('\\', "/");
    let mut current = tree_root.to_path_buf();

    for component in normalized.split('/').filter(|c| !c.is_empty() && *c != ".") {
        let matched = std::fs::read_dir(&current)
            .map_err(|e| FomodError::io(&current, e))?
            .flatten()
            .find_map(|e| {
                let name = e.file_name();
                name.to_str()
                    .filter(|n| n.eq_ignore_ascii_case(component))
                    .map(|_| e.path())
            });
        match matched {
            Some(p) => current = p,
            None => {
                return Err(FomodError::MissingSource(source.to_string()));
            }
        }
    }
    Ok(current)
}
