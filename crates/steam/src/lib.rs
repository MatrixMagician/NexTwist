//! `nextwist-steam` — Steam/Proton discovery, resolution, and casing.
//!
//! This crate quarantines all Steam/Proton-layout knowledge (per the Architectural
//! Responsibility Map in RESEARCH.md). It auto-detects installed Steam games across
//! native + Flatpak roots, offers a manual "add game by folder" fallback, accepts
//! ONLY the two supported Bethesda AppIDs (Skyrim SE 489830, Fallout 4 377160),
//! resolves each game's install directory, derives the Proton prefix
//! (`compatdata/<appid>/pfx`) that steamlocate does NOT expose, and produces a
//! per-game canonical `Data/` casing map the deploy engine uses to normalize
//! mixed-case mod paths under Wine (DEPLOY-08).
//!
//! It depends only on `core` types and `store` (from Plan 01); it returns resolved
//! [`core::Game`] structs. Persisting them via `store::add_managed_game` is the
//! caller's (Tauri command, Plan 06) job — this crate does pure resolution.

pub mod casing;
pub mod discover;
pub mod error;
pub mod resolve;

pub use casing::{CasingMap, canonical_data_casing};
pub use discover::{DetectedGame, detect_games};
pub use error::SteamError;
pub use resolve::{
    FALLOUT4, ResolvedGame, SKYRIM_SE, SUPPORTED_APPIDS, add_game_by_folder, is_supported,
    resolve_from_root, resolve_game,
};
