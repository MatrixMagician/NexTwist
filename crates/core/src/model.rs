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

/// NexusMods provenance for a managed mod that was acquired in-app (NEXUS-03/06).
///
/// Recorded additively (V4 migration) against the mod's `managed_mod` row so a
/// Nexus-sourced mod is otherwise indistinguishable from a local-archive mod — it still
/// deploys/purges through the Phase-1/2 engine. `mod_id` is the local `managed_mod` row
/// id; `nexus_mod_id`/`file_id` identify the file on NexusMods; `version`/`display_name`
/// come from the GraphQL v2 metadata read. The FK CASCADEs, so deleting the mod sheds
/// this row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NexusSource {
    /// Local `managed_mod` row id this provenance belongs to.
    pub mod_id: i64,
    /// NexusMods mod id the file came from.
    pub nexus_mod_id: u64,
    /// NexusMods file id that was downloaded.
    pub file_id: u64,
    /// The downloaded file's version string (from GraphQL v2 metadata).
    pub version: String,
    /// The downloaded file's display name (shown in the downloads list).
    pub display_name: String,
}

/// A NexusMods Collection revision pinned for a game (COLL-01).
///
/// A Collection is parsed from its revision manifest (`collection.json`) and persisted
/// additively (V5 migration). `(appid, slug, revision)` identify the exact revision and
/// form the idempotent upsert key. `profile_id` is `None` until the Collection is
/// materialised into its dedicated Phase-2 profile (Plan 04); deploying a Collection is a
/// profile switch, never a new primitive. The `id` field is assigned by the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collection {
    /// Stable row id assigned by the store (0 before insert).
    pub id: i64,
    /// Steam AppID the Collection targets.
    pub appid: u32,
    /// The Collection's URL slug on NexusMods.
    pub slug: String,
    /// The pinned revision number.
    pub revision: u32,
    /// Human-readable Collection name (from the manifest `info.name`).
    pub name: String,
    /// The dedicated Phase-2 profile this Collection deploys into, once materialised.
    pub profile_id: Option<i64>,
}

/// One pinned mod inside a Collection (COLL-02).
///
/// Carries the Nexus source identity (`nexus_mod_id`/`file_id`/`md5`), the install
/// `phase` (0-based ordering), the conflict `rank` (lower = higher priority, derived
/// from the manifest `modRules`), and a link to the local `managed_mod` row it stages
/// into. Recorded additively against its `collection` (V5); both FKs CASCADE so the link
/// sheds when the collection or the managed_mod is deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionMod {
    /// The local `managed_mod` row id this pinned mod stages into.
    pub mod_id: i64,
    /// NexusMods mod id the pinned file belongs to.
    pub nexus_mod_id: u64,
    /// NexusMods file id pinned by the Collection revision.
    pub file_id: u64,
    /// The pinned file's md5 (when the manifest supplies one) for file-matching.
    pub md5: Option<String>,
    /// Install ordering phase (0-based) from the manifest.
    pub phase: u32,
    /// Conflict rank (lower = higher priority), derived from the manifest `modRules`.
    pub rank: u32,
    /// The replayed FOMOD `choices` JSON for this mod, when the manifest pins one
    /// (`{type:"fomod", options:[…]}`); `None` for a mod with no scripted installer.
    pub choices_json: Option<String>,
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
    fn nexus_source_serde_round_trips() {
        let s = NexusSource {
            mod_id: 7,
            nexus_mod_id: 12604,
            file_id: 120063,
            version: "1.6.3".into(),
            display_name: "SKSE64".into(),
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: NexusSource = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
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
    fn collection_serde_round_trips() {
        let c = Collection {
            id: 3,
            appid: 489830,
            slug: "skyrim-essentials".into(),
            revision: 7,
            name: "Skyrim Essentials".into(),
            profile_id: Some(11),
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: Collection = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);

        // A not-yet-materialised collection (no profile) round-trips too.
        let no_profile = Collection {
            profile_id: None,
            ..c
        };
        let json = serde_json::to_string(&no_profile).unwrap();
        let back: Collection = serde_json::from_str(&json).unwrap();
        assert_eq!(no_profile, back);
    }

    #[test]
    fn collection_mod_serde_round_trips() {
        let cm = CollectionMod {
            mod_id: 42,
            nexus_mod_id: 12604,
            file_id: 120063,
            md5: Some("d41d8cd98f00b204e9800998ecf8427e".into()),
            phase: 0,
            rank: 3,
            choices_json: Some(r#"{"type":"fomod","options":[]}"#.into()),
        };
        let json = serde_json::to_string(&cm).unwrap();
        let back: CollectionMod = serde_json::from_str(&json).unwrap();
        assert_eq!(cm, back);

        // A mod with no md5 and no FOMOD choices round-trips too.
        let bare = CollectionMod {
            md5: None,
            choices_json: None,
            ..cm
        };
        let json = serde_json::to_string(&bare).unwrap();
        let back: CollectionMod = serde_json::from_str(&json).unwrap();
        assert_eq!(bare, back);
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
