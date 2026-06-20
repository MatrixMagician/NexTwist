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

/// A single mod managed for a game. Phase 2 makes the store multi-mod: many
/// `ManagedMod` rows coexist per game, ordered by `rank` for conflict resolution.
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
    /// Deployment rank — LOWER rank = HIGHER priority = wins a file conflict; 1-based. D-01.
    pub rank: u32,
}

/// A profile: a lightweight reference set over the shared staging store (D-13/D-14).
///
/// A profile does NOT own mod files — it records which mods are enabled and at what
/// rank for a given game, plus its own plugin enable/order state. Many profiles for
/// one game share the same underlying staged mods; switching profiles re-deploys
/// the selected reference set, never re-extracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// Stable row id assigned by the store.
    pub id: i64,
    /// Steam AppID the profile belongs to.
    pub appid: u32,
    /// Human-readable profile name (unique per game).
    pub name: String,
    /// Whether this is the active profile for its game (exactly one active per game).
    pub active: bool,
}

/// The kind of a Bethesda plugin master/light/regular file, used to group masters
/// ahead of regular plugins when sorting load order (D-08).
///
/// `Esm` (`.esm`) and `Esl` (ESL-flagged / `.esl`) form the *master group* that sorts
/// ahead of `Esp` regular plugins. The lowercase token is the stable DB persistence form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginKind {
    /// A master file (`.esm`) — part of the master group, sorts ahead of `.esp`.
    Esm,
    /// A light/ESL-flagged plugin (`.esl`) — part of the master group.
    Esl,
    /// A regular plugin (`.esp`).
    Esp,
}

impl PluginKind {
    /// Stable lowercase token used for DB persistence.
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginKind::Esm => "esm",
            PluginKind::Esl => "esl",
            PluginKind::Esp => "esp",
        }
    }

    /// Parse the persisted token back into a [`PluginKind`].
    pub fn from_token(s: &str) -> Option<Self> {
        match s {
            "esm" => Some(PluginKind::Esm),
            "esl" => Some(PluginKind::Esl),
            "esp" => Some(PluginKind::Esp),
            _ => None,
        }
    }
}

/// Per-profile plugin enable + load-order state (D-07/D-13).
///
/// One row per plugin known to a profile: whether it is enabled and its position in
/// the load order. The actual ordering is computed by LOOT (Plan 04); this is the
/// persisted, per-profile result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plugin {
    /// Plugin filename (e.g. `Skyrim.esm`).
    pub name: String,
    /// Master/light/regular classification.
    pub kind: PluginKind,
    /// Whether the plugin is enabled in this profile's load order.
    pub enabled: bool,
    /// Zero-based position in the load order.
    pub order: u32,
}

/// A file-level conflict: many mods provide the same deploy-relative path (CONF-01).
///
/// `providers` and `winner` are `ManagedMod` row ids. `winner` is the provider whose
/// file is actually deployed (lowest rank wins, D-01); it is recorded in the deploy
/// manifest so purge stays pristine (D-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileConflict {
    /// Deploy-root-relative path multiple mods contend for.
    pub target_rel: PathBuf,
    /// Mod ids that provide a file at `target_rel`.
    pub providers: Vec<i64>,
    /// The mod id that wins the conflict (its file is deployed).
    pub winner: i64,
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

    #[test]
    fn plugin_kind_token_round_trips() {
        for k in [PluginKind::Esm, PluginKind::Esl, PluginKind::Esp] {
            assert_eq!(PluginKind::from_token(k.as_str()), Some(k));
        }
        assert_eq!(PluginKind::Esm.as_str(), "esm");
        assert_eq!(PluginKind::Esl.as_str(), "esl");
        assert_eq!(PluginKind::Esp.as_str(), "esp");
        assert_eq!(PluginKind::from_token("nonsense"), None);
    }

    #[test]
    fn managed_mod_serde_round_trips() {
        let m = ManagedMod {
            id: 7,
            name: "SkyUI".into(),
            staging_root: PathBuf::from("/games/staging/489830/SkyUI"),
            enabled: true,
            rank: 3,
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: ManagedMod = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn profile_serde_round_trips() {
        let p = Profile {
            id: 1,
            appid: 489830,
            name: "Default".into(),
            active: true,
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: Profile = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn plugin_serde_round_trips() {
        let p = Plugin {
            name: "Skyrim.esm".into(),
            kind: PluginKind::Esm,
            enabled: true,
            order: 0,
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: Plugin = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn file_conflict_serde_round_trips() {
        let c = FileConflict {
            target_rel: PathBuf::from("Data/meshes/x.nif"),
            providers: vec![3, 7, 11],
            winner: 3,
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: FileConflict = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }
}
