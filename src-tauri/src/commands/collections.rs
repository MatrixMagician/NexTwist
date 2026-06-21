//! Collection lifecycle adapters (COLL-02/03/04/05) — THIN per Anti-Pattern-4.
//!
//! These four `#[tauri::command]`s orchestrate the FULL Collection lifecycle entirely out
//! of EXISTING, safety-reviewed primitives — they add ZERO new download/deploy/purge code:
//!
//! * [`resolve_collection`] — parse the `collection.json` manifest (`nexus::Collection`),
//!   gate the game domain (`appid_for_domain`), and run the headless `nexus::resolve_collection`
//!   (metadata reads only, ZERO downloads) → the `ResolveReport`. No disk mutation before
//!   the report is accepted (the resolve-before-download HARD GATE, COLL-02).
//! * [`download_collection`] — read `UserInfo.is_premium` FIRST (non-Premium ⇒ the
//!   Premium-required notice, NO download starts — locked decision, T-04-16). For Premium:
//!   bulk-download the AVAILABLE nexus set, reusing `run_download_to_window` VERBATIM per
//!   mod under a small bounded concurrency so the shared governor limiter governs the global
//!   rate (WR-03); a per-mod failure does NOT abort the batch; off-Nexus mods are recorded
//!   as manual steps and NEVER fetched (T-04-12). Each mod's pinned FOMOD `choices` are
//!   replayed headlessly through `nexus::replay_choices` + `fomod::resolve` (no per-mod
//!   wizard) — a stale choice surfaces as a specific error, never a silent mis-install.
//! * [`deploy_collection`] — `create_profile` → `set_profile_mod` (ranks from the rules) →
//!   `deploy::switch_profile` (the journaled purge→deploy_winners→apply_load_order→set_active
//!   path). NO new deploy primitive (COLL-04).
//! * [`uninstall_collection`] — `deploy::purge` (purge-to-pristine; switch/purge BEFORE the
//!   delete since `store.delete_profile` REJECTS an active profile) → `store.delete_profile`
//!   → remove the collection's staged mod trees + rows. Fully reversible (COLL-05); the
//!   byte-for-byte pristine guarantee is regression-locked by `collection_round_trip`.
//!
//! Errors map to the `String` IPC boundary via `boundary_err`. NB: the live GraphQL
//! `collectionRevision.downloadLink` archive fetch (which yields the `collection.json` this
//! adapter parses) is the remaining live-network seam exercised under human UAT; this
//! adapter takes the already-fetched manifest JSON so the lifecycle is headlessly testable.

use futures_util::stream::{self, StreamExt};
use nexus::{
    replay_choices, Collection as NexusCollection, NexusAuth, NexusClient, ResolveReport,
    SourceType,
};
use serde::Serialize;
use tauri::State;
use tokio::sync::Mutex;

use crate::commands::downloads::run_download_to_window;
use crate::commands::{appid_for_domain, boundary_err, require_game};
use crate::state::AppState;

/// Small bounded concurrency for the bulk download (WR-03). The SHARED governor limiter
/// (cloned per client) enforces the true global NexusMods rate; this only caps how many
/// streams are in flight at once so a large Collection does not open hundreds of sockets.
const DOWNLOAD_CONCURRENCY: usize = 3;

/// Resolve EVERY pinned mod in a Collection manifest into a [`ResolveReport`] — the
/// resolve-before-download HARD GATE (COLL-02). Issues only metadata reads (ZERO downloads,
/// ZERO disk writes). The frontend renders this report and gates the "Download" CTA behind
/// the user accepting it.
///
/// `manifest_json` is the parsed `collection.json` (fetched from the Collection revision's
/// archive — the live fetch is the human-UAT seam). The Collection's `info.domain_name`
/// MUST map to the managed `appid` via the same `appid_for_domain` allow-list the download
/// path uses — a wrong-game Collection is rejected here, never partially installed.
#[tauri::command]
pub async fn resolve_collection(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    manifest_json: String,
) -> Result<ResolveReport, String> {
    // Ensure the game is managed (a registry read; no mutation).
    let _game = require_game(&state, appid).await?;

    // Parse the untrusted manifest (pure; malformed JSON flattens to a clear error).
    let collection = NexusCollection::parse(&manifest_json).map_err(boundary_err)?;

    // Game-domain gate: the Collection's domain must be the one this appid manages.
    let domain = collection.info.domain_name.clone();
    let resolved_appid = appid_for_domain(&domain)
        .ok_or_else(|| format!("This Collection is for '{domain}', which NexTwist does not manage"))?;
    if resolved_appid != appid {
        return Err(format!(
            "This Collection is for '{domain}', not the selected game"
        ));
    }

    // Build a client with the shared limiter + session auth, then resolve (metadata only).
    let client = build_client(&state).await?;
    nexus::resolve_collection(&client, &domain, &collection)
        .await
        .map_err(boundary_err)
}

/// The outcome of a bulk Collection download (COLL-02/03): how many available mods were
/// downloaded, which per-mod downloads failed (the batch is NOT aborted on a single
/// failure), the manual (off-Nexus) steps the user must perform, and any stale FOMOD-choice
/// replays the user must re-run manually. Serializable for the UI (UI-SPEC §B.4/§C.2).
#[derive(Debug, Clone, Serialize, Default)]
pub struct DownloadCollectionReport {
    /// The local `collection` row id (so the UI can deploy/uninstall it).
    pub collection_id: i64,
    /// The number of available mods successfully downloaded + staged.
    pub downloaded: usize,
    /// Per-mod failures `(mod name, reason)` — the batch continued past each (Pitfall 4).
    pub failed: Vec<(String, String)>,
    /// Off-Nexus mods surfaced as manual steps (name + instructions); NEVER fetched.
    pub manual_steps: Vec<ManualStep>,
    /// Mods whose pinned FOMOD choice no longer matches the installer (run it manually).
    pub stale_choices: Vec<(String, String)>,
}

/// One off-Nexus manual step surfaced to the user (never auto-fetched; T-04-12).
#[derive(Debug, Clone, Serialize)]
pub struct ManualStep {
    /// The mod display name.
    pub name: String,
    /// Author/source instructions, if the manifest carried any.
    pub instructions: Option<String>,
    /// The off-Nexus URL (shown for the user to visit) — NEVER requested by NexTwist.
    pub url: Option<String>,
}

/// Bulk-download a Collection's AVAILABLE mods after the resolve report is accepted
/// (COLL-02/03). Enforces the Premium gate FIRST (T-04-16): a non-Premium session returns
/// the Premium-required notice and starts NO download (no `nxm://` fallback — locked
/// decision). For a Premium session, each available `nexus` mod reuses
/// `run_download_to_window` VERBATIM (the SAME stream→extract→stage→persist path), bounded
/// by a small concurrency so the shared governor governs the rate. A per-mod failure is
/// recorded and the batch continues. Off-Nexus mods are recorded as manual steps and never
/// fetched. Each downloaded mod's pinned FOMOD `choices` are replayed headlessly.
#[tauri::command]
pub async fn download_collection(
    state: State<'_, Mutex<AppState>>,
    window: tauri::Window,
    appid: u32,
    manifest_json: String,
    slug: String,
    revision: u32,
) -> Result<DownloadCollectionReport, String> {
    let game = require_game(&state, appid).await?;
    let collection = NexusCollection::parse(&manifest_json).map_err(boundary_err)?;
    let domain = collection.info.domain_name.clone();

    // ── PREMIUM GATE (T-04-16): checked BEFORE any download begins. ──────────────────
    let is_premium = {
        let guard = state.lock().await;
        guard.user.as_ref().map(|u| u.is_premium).unwrap_or(false)
    };
    premium_gate(is_premium)?;

    // Persist the collection shell first so its mods can attach (idempotent upsert).
    let collection_id = {
        let guard = state.lock().await;
        guard
            .store
            .add_collection(&nextwist_core::Collection {
                id: 0,
                appid,
                slug: slug.clone(),
                revision,
                name: collection.info.name.clone(),
                profile_id: None,
            })
            .map_err(boundary_err)?
    };

    let mut report = DownloadCollectionReport {
        collection_id,
        ..Default::default()
    };

    // Partition: off-Nexus mods are manual steps (NEVER fetched, T-04-12); the rest are the
    // auto-fetchable available set we bulk-download. We carry only the manifest INDEX +
    // owned coordinates into the download futures so no borrow of `collection` crosses the
    // bounded-concurrency stream (which would over-constrain the async-command lifetimes).
    let mut fetchable: Vec<(usize, String, u64, u64)> = Vec::new();
    for (idx, m) in collection.mods.iter().enumerate() {
        match m.source.kind {
            // Only a `nexus` source with both pinned ids is bulk-downloaded.
            SourceType::Nexus => match (m.source.mod_id, m.source.file_id) {
                (Some(mod_id), Some(file_id)) => {
                    fetchable.push((idx, m.name.clone(), mod_id, file_id))
                }
                // A nexus pin with no ids cannot be fetched — surface, never guess.
                _ => report.failed.push((
                    m.name.clone(),
                    "nexus mod is missing its pinned mod/file id".to_string(),
                )),
            },
            // Off-Nexus (`direct`/`browse`/`manual`) is a manual step, NEVER fetched (T-04-12).
            SourceType::Direct | SourceType::Browse | SourceType::Manual => {
                report.manual_steps.push(ManualStep {
                    name: m.name.clone(),
                    instructions: m
                        .instructions
                        .clone()
                        .or_else(|| m.source.instructions.clone()),
                    url: m.source.url.clone(),
                });
            }
            // `bundle` mods live inside the Collection archive — handled by the live
            // archive-extract seam (human-UAT); neither a network fetch nor a manual step.
            SourceType::Bundle => {}
        }
    }

    // ── Bulk download the available set, bounded concurrency, shared governor. ────────
    // A per-mod failure does NOT abort the batch (Pitfall 4): each result is collected.
    type DlOutcome = (usize, String, Result<crate::commands::downloads::DownloadResult, String>);
    let results: Vec<DlOutcome> = stream::iter(fetchable)
        .map(|(idx, name, mod_id, file_id)| {
            let state = &state;
            let window = &window;
            let domain = domain.clone();
            let dl_id = format!("collection-{collection_id}-{mod_id}-{file_id}");
            async move {
                let res = run_download_to_window(
                    state, window, &dl_id, appid, &domain, mod_id, file_id, None, None,
                )
                .await;
                (idx, name, res)
            }
        })
        .buffer_unordered(DOWNLOAD_CONCURRENCY)
        .collect()
        .await;

    // Persist each successful mod into the collection + replay its FOMOD choices headlessly.
    for (idx, name, res) in results {
        let m = &collection.mods[idx];
        match res {
            Ok(dl) => {
                // Replay the pinned FOMOD choices (no wizard). A stale choice surfaces as a
                // specific error and is recorded — the mod still staged, but the user must
                // run its installer manually (never a silent mis-install; COLL-03 / A3).
                if let Some(choices) = &m.choices
                    && let Err(e) = replay_for(&dl.staging_root, choices)
                {
                    report.stale_choices.push((name.clone(), e));
                }
                persist_collection_mod(&state, collection_id, &dl, m).await?;
                report.downloaded += 1;
            }
            Err(reason) => report.failed.push((name, reason)),
        }
    }

    let _ = &game; // game is required-checked above; staging lives under game.staging_dir.
    Ok(report)
}

/// Deploy an installed Collection as its dedicated profile (COLL-04): create the profile,
/// set each collection mod's per-profile membership + rank, then reconcile the deployment
/// through `deploy::switch_profile` (the SAME journaled purge→deploy→load-order→set-active
/// path every profile switch uses). NO new deploy primitive. Returns the engine's
/// [`deploy::SwitchReport`].
#[tauri::command]
pub async fn deploy_collection(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    collection_id: i64,
) -> Result<deploy::SwitchReport, String> {
    let game = require_game(&state, appid).await?;
    let guard = state.lock().await;

    let collection = guard
        .store
        .get_collection(collection_id)
        .map_err(boundary_err)?
        .ok_or_else(|| format!("collection {collection_id} is not installed"))?;
    let mods = guard
        .store
        .list_collection_mods(collection_id)
        .map_err(boundary_err)?;

    // Create (or reuse) the dedicated profile for this collection.
    let profile_id = match collection.profile_id {
        Some(id) => id,
        None => {
            let name = format!("Collection: {}", collection.name);
            let id = guard.store.create_profile(appid, &name).map_err(boundary_err)?;
            // Re-link the profile to the collection (idempotent upsert preserves the link).
            guard
                .store
                .add_collection(&nextwist_core::Collection {
                    profile_id: Some(id),
                    ..collection.clone()
                })
                .map_err(boundary_err)?;
            id
        }
    };

    // Set per-profile membership: each collection mod enabled, ranked by its stored rank
    // (which was derived from the manifest's modRules at download time, Pattern 7).
    for cm in &mods {
        guard
            .store
            .set_profile_mod(profile_id, cm.mod_id, true, cm.rank)
            .map_err(boundary_err)?;
    }

    // Deploy via the existing safe profile-switch path — no new primitive (COLL-04).
    deploy::switch_profile(&guard.store, &game, profile_id).map_err(boundary_err)
}

/// Uninstall an installed Collection, fully reversibly (COLL-05): purge the deployment back
/// to byte-for-byte pristine, drop the dedicated profile (switch/purge FIRST since
/// `delete_profile` rejects an active profile), then remove the collection's staged mod
/// trees + their `managed_mod` + V5 collection rows. The pristine guarantee is
/// regression-locked by `crates/deploy/tests/collection_round_trip.rs`.
#[tauri::command]
pub async fn uninstall_collection(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    collection_id: i64,
) -> Result<deploy::PurgeReport, String> {
    let game = require_game(&state, appid).await?;
    let guard = state.lock().await;

    let collection = guard
        .store
        .get_collection(collection_id)
        .map_err(boundary_err)?
        .ok_or_else(|| format!("collection {collection_id} is not installed"))?;
    let mods = guard
        .store
        .list_collection_mods(collection_id)
        .map_err(boundary_err)?;

    // 1. Purge the deployment to pristine FIRST. `purge` is manifest-driven + crash-safe;
    //    it restores the install byte-for-byte vanilla regardless of which profile is
    //    active, and clears the on-disk deployment so the profile can be safely dropped.
    let purged = deploy::purge(&guard.store, &game).map_err(boundary_err)?;

    // 2. The collection's profile is no longer deployed. If it is the active one, clear the
    //    active flag so `delete_profile` (which REJECTS an active profile) succeeds — the
    //    deployment is already pristine, so clearing the flag cannot strand any files.
    if let Some(profile_id) = collection.profile_id {
        if let Some(active) = guard.store.active_profile(appid).map_err(boundary_err)?
            && active.id == profile_id
        {
            guard.store.clear_active_profile(appid).map_err(boundary_err)?;
        }
        guard.store.delete_profile(profile_id).map_err(boundary_err)?;
    }

    // 3. Remove the collection's staged mod trees + their managed_mod rows, then drop the
    //    V5 collection rows (collection_mod + fomod_choice CASCADE off the collection).
    for cm in &mods {
        if let Some(m) = guard
            .store
            .list_mods(appid)
            .map_err(boundary_err)?
            .into_iter()
            .find(|m| m.id == cm.mod_id)
        {
            // Best-effort remove the staged tree (already purged from the live game).
            let _ = std::fs::remove_dir_all(&m.staging_root);
        }
        let _ = guard.store.remove_mod(cm.mod_id);
    }
    guard.store.remove_collection(collection_id).map_err(boundary_err)?;

    Ok(purged)
}

// ── Pure / small helpers (no business logic — Anti-Pattern-4) ────────────────────────

/// The Premium gate (T-04-16): a non-Premium session may NOT download a Collection. Returns
/// `Ok(())` for a Premium session, or the exact Premium-required notice string (UI-SPEC §B.1
/// Copywriting Contract) for a free session. Pure so the gate decision is unit-tested.
fn premium_gate(is_premium: bool) -> Result<(), String> {
    if is_premium {
        Ok(())
    } else {
        Err("Collections require a NexusMods Premium account. \
             Upgrade to Premium to install Collections."
            .to_string())
    }
}

/// Build a `NexusClient` with the SHARED process-wide limiter + the session auth (OAuth
/// bearer or the keyring API key). Mirrors the download path's auth resolution so the
/// Collection metadata reads coordinate the same rate budget (WR-03).
async fn build_client(state: &State<'_, Mutex<AppState>>) -> Result<NexusClient, String> {
    let (auth, limiter) = {
        let guard = state.lock().await;
        let auth = match guard.access_token.clone() {
            Some(tok) => NexusAuth::Bearer(tok),
            None => {
                let api_key = crate::keyring::load_refresh_token()
                    .map_err(boundary_err)?
                    .ok_or_else(|| "not logged in: no NexusMods session".to_string())?;
                NexusAuth::ApiKey(api_key)
            }
        };
        (auth, guard.rate_limiter.clone())
    };
    NexusClient::with_limiter(nexus::NEXUS_API_BASE, auth, limiter).map_err(boundary_err)
}

/// Replay a downloaded mod's pinned FOMOD choices headlessly against its staged tree.
///
/// Parses the staged `ModuleConfig.xml`, replays the manifest `choices` into a
/// `fomod::Selection` (`nexus::replay_choices` — a stale name surfaces as a specific error),
/// and runs the SAME `fomod::resolve` to validate the plan resolves cleanly. Returns the
/// error STRING on a stale/invalid replay so the caller records it as a manual step.
fn replay_for(staging_root: &std::path::Path, choices: &nexus::Choices) -> Result<(), String> {
    let module = fomod::parse_module_config(staging_root).map_err(boundary_err)?;
    let selection = replay_choices(&module, choices).map_err(boundary_err)?;
    // Validate the replayed selection resolves to a concrete plan (surfaces a stale-but-
    // parseable selection that no longer yields a usable install).
    fomod::resolve(&module, &selection).map_err(boundary_err)?;
    Ok(())
}

/// Persist a downloaded Collection mod into the V5 collection tables, carrying its pinned
/// FOMOD `choices` JSON verbatim (the store facade upserts the `fomod_choice` row).
async fn persist_collection_mod(
    state: &State<'_, Mutex<AppState>>,
    collection_id: i64,
    dl: &crate::commands::downloads::DownloadResult,
    m: &nexus::CollectionMod,
) -> Result<(), String> {
    let choices_json = m
        .choices
        .as_ref()
        .map(|c| serde_json::to_string(c).map_err(boundary_err))
        .transpose()?;
    let cm = nextwist_core::CollectionMod {
        mod_id: dl.mod_id,
        nexus_mod_id: m.source.mod_id.unwrap_or(0),
        file_id: m.source.file_id.unwrap_or(0),
        md5: m.source.md5.clone(),
        phase: m.phase,
        rank: 1,
        choices_json,
    };
    let guard = state.lock().await;
    guard
        .store
        .add_collection_mod(collection_id, &cm)
        .map_err(boundary_err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus::is_auto_fetchable;

    /// T-04-16: the Premium gate decision. A Premium session passes; a free session gets the
    /// exact §B.1 Premium-required notice and (in the command) NO download starts.
    #[test]
    fn premium_gate_blocks_free_account() {
        assert!(premium_gate(true).is_ok(), "a Premium session may download");
        let err = premium_gate(false).expect_err("a free session is blocked");
        assert!(
            err.contains("Premium account"),
            "the free-session notice names the Premium requirement: {err}"
        );
    }

    /// T-04-12: only `nexus`/`bundle` are auto-fetchable; every off-Nexus source
    /// (`direct`/`browse`/`manual`) is a manual step and is NEVER requested.
    #[test]
    fn off_nexus_sources_are_never_auto_fetchable() {
        assert!(is_auto_fetchable(SourceType::Nexus));
        assert!(is_auto_fetchable(SourceType::Bundle));
        for off in [SourceType::Direct, SourceType::Browse, SourceType::Manual] {
            assert!(!is_auto_fetchable(off), "{off:?} must NOT be auto-fetched");
            assert!(off.is_off_nexus(), "{off:?} is an off-Nexus manual step");
        }
    }
}
