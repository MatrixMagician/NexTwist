//! Profile-switch reconcile (PROF-02/PROF-03) — the capstone of multi-mod management.
//!
//! Switching the active profile changes WHICH mods, plugins, and order are deployed for
//! a game. NexTwist does this WITHOUT ever bypassing the safe engine and WITHOUT a
//! diff-deploy shortcut (Pitfall 4 / RESEARCH Pattern 4): a switch is always a full
//! **purge-to-pristine** of the current deployment followed by a **fresh deploy** of the
//! target profile's winner set, then the target profile's `plugins.txt` is written.
//!
//! ## The reconcile sequence ([`switch_profile`], D-15)
//!
//! 1. **`purge(old)`** — manifest-driven, crash-safe restore to byte-for-byte pristine
//!    (the existing Phase-1 primitive). Because purge is total, profile A's unique files
//!    can never survive into profile B (T-02-15).
//! 2. **resolve + `deploy_winners(new)`** — read the target profile's enabled membership
//!    (mod set + per-profile ranks via the Plan-01 store), build [`ModInput`]s, run the
//!    Plan-03 conflict resolver, and deploy the deduped winner set through the SAME
//!    journaled per-file primitive as Phase-1 deploy (the safe engine is never bypassed).
//! 3. **`apply_load_order(new)`** — write the target profile's asterisk `plugins.txt` at
//!    the Proton-prefix AppData location via libloot (Plan-04 primitive; PROF-02 carries
//!    the plugin order, D-13).
//! 4. **`set_active_profile(new)`** — only AFTER a successful deploy, so exactly one
//!    profile is active and the active flag never points at a half-applied state (T-02-16).
//!
//! Order matters: purge BEFORE deploy (Pitfall 4); `set_active` only after deploy succeeds.
//!
//! ## Why this is crash-safe across switches (T-02-14)
//!
//! Both `purge` and `deploy_winners` are the existing journaled, intent-before-act,
//! idempotent primitives; a crash mid-switch is replayed by `recover_on_launch` to a
//! consistent state, always converging to pristine. The cross-switch round-trip-pristine
//! invariant is regression-locked by `crates/deploy/tests/profile_switch.rs`.

use nextwist_core::{Game, Plugin};
use store::Store;

use crate::conflict::{resolve, ModInput};
use crate::engine::{deploy_winners, purge, DeployReport, PurgeReport};
use crate::error::DeployError;

use std::path::PathBuf;

/// What [`switch_profile`] did: the purge of the previous deployment, the deploy of the
/// target profile's winner set, and the path of the `plugins.txt` written for the new
/// profile. Serializable so it crosses the Tauri IPC boundary to the UI (UI-SPEC §D).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SwitchReport {
    /// The purge of the OLD deployment (restored to pristine before the new deploy).
    pub purged: PurgeReport,
    /// The deploy of the TARGET profile's resolved winner set.
    pub deployed: DeployReport,
    /// The `plugins.txt` written for the target profile at the Proton-prefix AppData
    /// location (the result of `apply_load_order`).
    pub plugins_txt: PathBuf,
}

/// Switch the active profile for `game` to `target_profile_id`, reconciling the on-disk
/// deployment through the safe engine (PROF-02): purge the current deployment to
/// pristine, deploy the target profile's enabled-mod winner set, write the target
/// profile's `plugins.txt`, and mark the target active.
///
/// This NEVER bypasses the engine and NEVER does a diff-deploy: it is a full
/// purge-to-pristine then a fresh deploy of the target set (Pitfall 4). Each profile
/// preserves its own enabled set + per-profile ranks + plugin order (PROF-03), read from
/// the Plan-01 `profile_mod` / `plugin_state` store tables.
///
/// # Errors
///
/// * [`DeployError`] from the purge or deploy halves (IO, path-escape, journal).
/// * [`DeployError::Profile`] if the target profile's `plugins.txt` cannot be written
///   (the libloot reason is wrapped) or the store cannot be read.
///
/// On any error after the purge, the deployment is already pristine (purge succeeded) or
/// the journal will recover it on next launch — the game is never left unreversible.
/// Additionally (WR-02), on any failure AFTER the purge but BEFORE the target is marked
/// active, the active flag is CLEARED, so the store never reports an OLD profile as active
/// while its deployment has already been purged off disk (a stale-active-flag drift the
/// UI would otherwise act on, e.g. a subsequent conflict deploy against a phantom set).
pub fn switch_profile(
    store: &Store,
    game: &Game,
    target_profile_id: i64,
) -> Result<SwitchReport, DeployError> {
    // 1. Purge the CURRENT deployment back to byte-for-byte pristine (Pitfall 4: always a
    //    full purge between profiles, never a diff-deploy). Manifest-driven + crash-safe.
    //    (If this fails, nothing has changed and the OLD active flag is still correct.)
    let purged = purge(store, game)?;

    // From here on the on-disk deployment is pristine — the OLD active profile no longer
    // describes any deployed files. Any failure before we mark the TARGET active must
    // clear the stale active flag so the persisted state stays honest (WR-02).
    switch_after_purge(store, game, target_profile_id, purged).inspect_err(|_| {
        // Best-effort: drop the stale active flag. If this cleanup itself fails we keep
        // the original error (the deployment is still pristine / journal-recoverable).
        let _ = store.clear_active_profile(game.appid);
    })
}

/// The post-purge half of [`switch_profile`] (deploy → plugins → mark active), factored
/// out so [`switch_profile`] can run a single stale-active-flag cleanup on any error from
/// these steps (WR-02). `purged` is threaded through to build the final report.
fn switch_after_purge(
    store: &Store,
    game: &Game,
    target_profile_id: i64,
    purged: PurgeReport,
) -> Result<SwitchReport, DeployError> {
    // 2. Build the target profile's enabled-mod winner set and deploy it through the
    //    UNCHANGED safe engine. The per-profile membership (enabled flag + per-profile
    //    rank) is the source of truth — PROF-03 (each profile keeps its own set/order).
    let inputs = enabled_inputs_for_profile(store, game.appid, target_profile_id)?;
    let (winners, _conflicts) = resolve(&inputs)?;
    let deployed = deploy_winners(store, game, &winners)?;

    // 3. Write the target profile's plugins.txt at the Proton-prefix AppData location via
    //    libloot (Plan-04 apply_load_order; masters-first enforced internally). PROF-02
    //    carries plugin order across the switch (D-13). deploy -> loadorder is acyclic.
    let plugins_txt = apply_profile_plugins(store, game, target_profile_id)?;

    // 4. Mark the target active ONLY after a successful deploy (exactly one active; the
    //    active flag never points at a half-applied state — T-02-16).
    store
        .set_active_profile(game.appid, target_profile_id)
        .map_err(|e| DeployError::Profile(e.to_string()))?;

    Ok(SwitchReport {
        purged,
        deployed,
        plugins_txt,
    })
}

/// Materialize the target profile's ENABLED mods as resolver [`ModInput`]s.
///
/// Joins the per-profile membership (`list_profile_mods` → `(mod_id, enabled, rank)`)
/// against the game's managed mods (`list_mods` → staging roots), keeping only enabled
/// members and tagging each with its PER-PROFILE rank (so the same shared mod can win in
/// one profile and lose in another — PROF-03). Membership rows for mods that no longer
/// exist are skipped defensively.
fn enabled_inputs_for_profile(
    store: &Store,
    appid: u32,
    profile_id: i64,
) -> Result<Vec<ModInput>, DeployError> {
    let members = store
        .list_profile_mods(profile_id)
        .map_err(|e| DeployError::Profile(e.to_string()))?;
    let mods = store
        .list_mods(appid)
        .map_err(|e| DeployError::Profile(e.to_string()))?;

    let mut inputs = Vec::new();
    for (mod_id, enabled, rank) in members {
        if !enabled {
            continue;
        }
        // Look up the mod's staging root (skip a membership row whose mod was removed).
        if let Some(m) = mods.iter().find(|m| m.id == mod_id) {
            inputs.push(ModInput {
                mod_id,
                staging_root: m.staging_root.clone(),
                rank,
            });
        }
    }
    Ok(inputs)
}

/// Write the target profile's `plugins.txt` at the Proton-prefix AppData location via the
/// Plan-04 `loadorder::apply_load_order` primitive, returning the written path.
///
/// Reads the profile's persisted plugin enable/order (`list_plugin_state`) and hands it to
/// libloot, which writes the canonical asterisk-format masters-first active-plugins file.
/// An unsupported game (no AppData folder name) is a profile error surfaced to the caller.
fn apply_profile_plugins(
    store: &Store,
    game: &Game,
    profile_id: i64,
) -> Result<PathBuf, DeployError> {
    let plugins: Vec<Plugin> = store
        .list_plugin_state(profile_id)
        .map_err(|e| DeployError::Profile(e.to_string()))?;

    let folder = loadorder::appdata_folder_name(game.appid)
        .ok_or_else(|| DeployError::Profile(format!("game {} is not supported", game.appid)))?;
    let appdata_local = loadorder::appdata_local_path(&game.prefix, folder);

    loadorder::apply_load_order(game.appid, &game.install_dir, &appdata_local, &plugins)
        .map_err(|e| DeployError::Profile(e.to_string()))
}
