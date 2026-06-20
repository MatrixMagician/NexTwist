//! Shared domain types — the vocabulary every NexTwist crate speaks.
//!
//! These types are pure data with NO I/O-framework dependencies (no rusqlite, no
//! tauri, no reqwest). Downstream crates (`store`, `steam`, `extract`, `deploy`)
//! and the Tauri shell all link against these shapes. The naming is fixed as a
//! contract for Plans 02–06; field tweaks are allowed but the shapes are stable.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A Steam/Proton game NexTwist manages.
///
/// `install_dir` is the resolved game install (e.g. `.../steamapps/common/Skyrim Special Edition`).
/// `prefix` is the resolved Proton/Wine prefix (`.../compatdata/<appid>/pfx`).
/// `staging_dir` is where mod archives are extracted before deployment — chosen on
/// the same filesystem as the install where possible so hardlink/reflink stays viable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    /// Steam AppID (e.g. Skyrim SE = 489830, Fallout 4 = 377160).
    pub appid: u32,
    /// Human-readable name.
    pub name: String,
    /// Resolved game install directory.
    pub install_dir: PathBuf,
    /// Resolved Proton/Wine prefix directory.
    pub prefix: PathBuf,
    /// Where this game's mods are staged.
    pub staging_dir: PathBuf,
}

/// A single mod managed for a game (Phase 1 supports one enabled mod at a time;
/// the `id`/`enabled` scaffolding pre-positions Phase 2 multi-mod/load-order work).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedMod {
    /// Stable row id assigned by the store.
    pub id: i64,
    /// Display name of the mod.
    pub name: String,
    /// Root of this mod's staged file tree.
    pub staging_root: PathBuf,
    /// Whether the mod is currently enabled (deployed).
    pub enabled: bool,
}

/// One deployed file recorded in the per-game manifest.
///
/// This is the unit of reversibility: every file NexTwist places into a game's
/// `Data/` tree has a manifest row so purge can remove exactly what was deployed
/// and restore any pre-existing (vanilla) file it overwrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Path of the deployed file relative to the game's deploy root (`Data/`).
    pub target_rel: PathBuf,
    /// Row id of the mod that owns this file.
    pub source_mod: i64,
    /// Method used to place the file on disk.
    pub method: DeployMethod,
    /// blake3 hex content hash of the deployed file.
    pub hash: String,
    /// True if a vanilla file already existed at this target and was backed up first.
    pub pre_existing: bool,
}

/// The per-target filesystem primitive used to place a file.
///
/// Chosen per (staging, target) pair at deploy time via an empirical capability
/// probe (Plan 04), never globally — btrfs returns EXDEV across subvolumes even on
/// the same disk, so the ladder degrades reflink → hardlink → symlink → copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeployMethod {
    /// Copy-on-write clone (independent inode; safest, space-efficient).
    Reflink,
    /// Hard link (same inode; same-device only).
    Hardlink,
    /// Symbolic link (cross-device fallback).
    Symlink,
    /// Plain byte copy (last-resort fallback).
    Copy,
}

impl DeployMethod {
    /// Stable lowercase token used for DB persistence.
    pub fn as_str(&self) -> &'static str {
        match self {
            DeployMethod::Reflink => "reflink",
            DeployMethod::Hardlink => "hardlink",
            DeployMethod::Symlink => "symlink",
            DeployMethod::Copy => "copy",
        }
    }

    /// Parse the persisted token back into a [`DeployMethod`].
    pub fn from_token(s: &str) -> Option<Self> {
        match s {
            "reflink" => Some(DeployMethod::Reflink),
            "hardlink" => Some(DeployMethod::Hardlink),
            "symlink" => Some(DeployMethod::Symlink),
            "copy" => Some(DeployMethod::Copy),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deploy_method_token_round_trips() {
        for m in [
            DeployMethod::Reflink,
            DeployMethod::Hardlink,
            DeployMethod::Symlink,
            DeployMethod::Copy,
        ] {
            assert_eq!(DeployMethod::from_token(m.as_str()), Some(m));
        }
        assert_eq!(DeployMethod::from_token("nonsense"), None);
    }

    #[test]
    fn game_serde_round_trips() {
        let g = Game {
            appid: 489830,
            name: "Skyrim Special Edition".into(),
            install_dir: PathBuf::from("/games/SkyrimSE"),
            prefix: PathBuf::from("/games/compatdata/489830/pfx"),
            staging_dir: PathBuf::from("/games/staging/489830"),
        };
        let json = serde_json::to_string(&g).unwrap();
        let back: Game = serde_json::from_str(&json).unwrap();
        assert_eq!(g, back);
    }
}
