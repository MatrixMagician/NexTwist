//! Collection availability resolver — the resolve-before-download HARD GATE (COLL-02).
//!
//! Given a parsed [`crate::collection::Collection`], classify EVERY pinned mod's
//! availability into a [`ResolveReport`] **before any download or disk write** (success
//! criterion #2; the STATE Phase-4 blocker mitigation). The gate is structural: this module
//! only ever calls [`NexusClient::file_availability`] (a single metadata read per `nexus`
//! mod, gated through the shared `governor` limiter via `until_ready()` first) — it has NO
//! download path, so it cannot issue a download (T-04-10).
//!
//! Classification follows the Source-type table (RESEARCH Collection Manifest Reference):
//! * `nexus`  → the file-info read decides Available / Archived / Unavailable;
//! * `bundle` → [`ModStatus::Available`] (the file is inside the collection archive — no fetch);
//! * `direct` / `browse` / `manual` → [`ModStatus::Manual`] — off-Nexus, NEVER auto-fetched
//!   (locked decision; T-04-08 SSRF mitigation). No request is issued for these.
//!
//! Off-Nexus sources are classified purely from `source.type`; the resolver issues no
//! network request for them at all, so a malicious/​off-Nexus `url` is never contacted.

use crate::client::{FileAvailability, NexusClient};
use crate::collection::{Collection, SourceType};
use crate::error::NexusError;

/// The resolved availability of one Collection mod (COLL-02; UI-SPEC §B.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModStatus {
    /// The mod's file is available to download (nexus) or bundled (bundle).
    Available,
    /// The pinned nexus file exists but is archived.
    Archived,
    /// The pinned nexus file no longer exists (removed).
    Unavailable,
    /// The mod is off-Nexus (`direct`/`browse`/`manual`) — a required MANUAL step, never fetched.
    Manual,
}

/// One mod's entry in the resolve report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMod {
    /// Mod display name (from the manifest).
    pub name: String,
    /// Pinned version string.
    pub version: String,
    /// The source kind that produced this classification.
    pub source: SourceType,
    /// The resolved availability status.
    pub status: ModStatus,
}

/// The full resolve report for a Collection: one [`ResolvedMod`] per pinned mod, computed
/// with ZERO downloads. The "Download Collection" CTA is gated behind the user accepting this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveReport {
    /// One entry per mod in the manifest, in manifest order.
    pub mods: Vec<ResolvedMod>,
}

impl ResolveReport {
    /// Whether every mod is actionable in-app (Available or bundled). When false, the report
    /// has archived / unavailable / manual entries the user must acknowledge.
    pub fn all_available(&self) -> bool {
        self.mods.iter().all(|m| m.status == ModStatus::Available)
    }

    /// The mods that require a manual (off-Nexus) step.
    pub fn manual_steps(&self) -> impl Iterator<Item = &ResolvedMod> {
        self.mods.iter().filter(|m| m.status == ModStatus::Manual)
    }
}

/// Resolve every pinned mod's availability into a [`ResolveReport`] — the hard
/// resolve-before-download gate (COLL-02). Issues only metadata reads (zero downloads).
///
/// For each mod:
/// * `nexus` with a `(mod_id, file_id)` pair → [`NexusClient::file_availability`] (a single
///   rate-limited metadata read) maps to Available / Archived / Unavailable;
/// * `nexus` MISSING its `(mod_id, file_id)` → [`ModStatus::Unavailable`] (a malformed pin
///   we cannot resolve — surfaced, never silently fetched);
/// * `bundle` → [`ModStatus::Available`] (in-archive, no request);
/// * `direct` / `browse` / `manual` → [`ModStatus::Manual`] (off-Nexus, NO request issued).
///
/// `game_domain` is the Collection's `info.domain_name`. A per-mod metadata error (e.g. a
/// 429 or transport failure) aborts the resolve with that [`NexusError`] — the caller may
/// retry; no partial download has occurred because resolve never downloads.
pub async fn resolve_collection(
    client: &NexusClient,
    game_domain: &str,
    collection: &Collection,
) -> Result<ResolveReport, NexusError> {
    let mut mods = Vec::with_capacity(collection.mods.len());

    for m in &collection.mods {
        let status = match m.source.kind {
            SourceType::Bundle => ModStatus::Available,
            SourceType::Direct | SourceType::Browse | SourceType::Manual => {
                // Off-Nexus: classified from the type ALONE. No request is issued, so the
                // off-Nexus `url` is never contacted (T-04-08 SSRF mitigation).
                ModStatus::Manual
            }
            SourceType::Nexus => match (m.source.mod_id, m.source.file_id) {
                (Some(mod_id), Some(file_id)) => {
                    match client.file_availability(game_domain, mod_id, file_id).await? {
                        FileAvailability::Available => ModStatus::Available,
                        FileAvailability::Archived => ModStatus::Archived,
                        FileAvailability::Unavailable => ModStatus::Unavailable,
                    }
                }
                // A nexus source with no pinned ids cannot be resolved or fetched.
                _ => ModStatus::Unavailable,
            },
        };

        mods.push(ResolvedMod {
            name: m.name.clone(),
            version: m.version.clone(),
            source: m.source.kind,
            status,
        });
    }

    Ok(ResolveReport { mods })
}
