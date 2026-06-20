//! Per-game canonical `Data/` casing map (DEPLOY-08 input).
//!
//! Wine/Proton does NOT abstract the filesystem: a Windows `open("Data\\Textures\\x")`
//! becomes a case-sensitive Linux `open()`, so mixed-case mod paths (authored on
//! case-insensitive NTFS) silently fail to load (RESEARCH.md Pitfall 4). The deploy
//! engine's `casefold.rs` (Plan 05) rewrites incoming mod paths to the game's REAL
//! casing — and the knowledge of that real casing lives HERE.
//!
//! This module ONLY produces the canonical-casing knowledge; it performs NO rewriting
//! (that is deploy's job per the Responsibility Map). Full implementation lands in
//! Task 2; this is the placeholder so `lib.rs` resolves during Task 1.

use std::path::Path;

use crate::error::SteamError;

/// A lowercase-key → canonical-cased-component map for a game's `Data/` tree.
#[derive(Debug, Clone, Default)]
pub struct CasingMap;

/// Walk the game's `Data/` tree and produce its canonical casing map.
///
/// Stub for Task 1 — fully implemented in Task 2.
pub fn canonical_data_casing(_install_dir: &Path) -> Result<CasingMap, SteamError> {
    Ok(CasingMap)
}
