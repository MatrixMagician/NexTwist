//! Intent-before-act operation-journal protocol + idempotent replay.
//!
//! SQLite WAL gives ACID *inside* the DB, but a `link()`/`reflink()`/`copy()` syscall
//! and the row recording it are two operations that cannot be made atomic together.
//! So we record the *intent* of each file op as a `pending` `op_journal` row, COMMIT
//! it (the store opens with `synchronous=FULL` so the intent is durable before the
//! syscall runs), perform the IDEMPOTENT file op, then flip the row to `done` and
//! write the manifest row. A crash leaves a `pending` row whose on-disk effect is
//! either absent or already complete — both recoverable because the op is idempotent.
//!
//! This module owns the *protocol* (the store provides only durable row primitives):
//!
//! * [`begin_deploy`] / [`begin_ini`] / [`begin_purge`] — record a `pending` intent
//!   (durable BEFORE the syscall).
//! * [`finish_deploy`] / [`finish_purge`] — flip to `done` + write the manifest row
//!   (AFTER the syscall).
//! * [`replay`] — on launch, roll every non-`done` row forward (finish a deploy) or
//!   back (undo a purge + restore vanilla), idempotently, to a consistent state.

use std::path::{Path, PathBuf};

use nextwist_core::{DeployMethod, FileEntry, Game};
use store::{JournalId, JournalRow, OpIntent, Store};

use crate::backup;
use crate::error::DeployError;
use crate::method::apply_idempotent;

/// Operation kind tokens recorded in the journal `kind` column.
pub const KIND_DEPLOY: &str = "deploy";
pub const KIND_PURGE: &str = "purge";
/// The StarfieldCustom.ini activation op. Rides a bare `StarfieldCustom.ini` sentinel
/// OUTSIDE the `Data/` deploy root; its replay resolves via `steam::my_games_path`, never
/// `resolve_target`/`guard_within_root` — the `Data/`-root guard stays untouched.
pub const KIND_INI: &str = "ini";

/// Record a `pending` deploy intent for `target_rel` and return its id. The store
/// commits this row under `synchronous=FULL` so it is on stable storage before the
/// caller performs the filesystem syscall.
pub fn begin_deploy(
    store: &Store,
    appid: u32,
    target_rel: &Path,
    method: DeployMethod,
    source_hash: &str,
    staging_root: &Path,
) -> Result<JournalId, DeployError> {
    let intent = OpIntent {
        appid,
        target_rel: target_rel.to_path_buf(),
        method: Some(method),
        source_hash: Some(source_hash.to_string()),
        kind: KIND_DEPLOY.to_string(),
        staging_root: Some(staging_root.to_path_buf()),
    };
    Ok(store.begin_op(&intent)?)
}

/// Record a `pending` StarfieldCustom.ini-activation intent and return its id. The
/// `sentinel_rel` is the bare `StarfieldCustom.ini` (NO `Data/` prefix) so it
/// can never collide with a `Data/`-rooted manifest relpath. Mirrors [`begin_purge`]
/// (`method: None`, `source_hash: None`).
pub fn begin_ini(store: &Store, appid: u32, sentinel_rel: &Path) -> Result<JournalId, DeployError> {
    let intent = OpIntent {
        appid,
        target_rel: sentinel_rel.to_path_buf(),
        method: None,
        source_hash: None,
        kind: KIND_INI.to_string(),
        staging_root: None,
    };
    Ok(store.begin_op(&intent)?)
}

/// Record a `pending` purge intent for `target_rel` and return its id.
pub fn begin_purge(store: &Store, appid: u32, target_rel: &Path) -> Result<JournalId, DeployError> {
    let intent = OpIntent {
        appid,
        target_rel: target_rel.to_path_buf(),
        method: None,
        source_hash: None,
        kind: KIND_PURGE.to_string(),
        staging_root: None,
    };
    Ok(store.begin_op(&intent)?)
}

/// Flip a deploy intent to `done` and write its manifest row. Called only after the
/// idempotent file op has succeeded. (`record_deployed_file` upserts, so a replay
/// that re-finishes a row is harmless.)
pub fn finish_deploy(
    store: &Store,
    id: JournalId,
    appid: u32,
    entry: &FileEntry,
) -> Result<(), DeployError> {
    store.record_deployed_file(appid, entry)?;
    store.mark_done(id)?;
    Ok(())
}

/// Flip a purge intent to `done` after the file has been removed and its manifest +
/// vanilla rows dropped.
pub fn finish_purge(store: &Store, id: JournalId) -> Result<(), DeployError> {
    store.mark_done(id)?;
    Ok(())
}

/// The outcome of a [`replay`] sweep: how many rows were rolled forward/back, and the
/// `Data/`-rooted relpaths of every **purge** row that was replayed.
///
/// The purged relpaths are returned so `recover_on_launch` can run the SAME
/// manifest/journal-derived empty-directory cleanup that `purge()` runs — keeping a
/// crash-then-recover purge path directory-pristine WITHOUT a blind disk scan.
#[derive(Debug, Clone, Default)]
pub struct ReplayOutcome {
    /// Number of journal rows replayed (rolled forward or back).
    pub replayed: usize,
    /// `Data/`-rooted relpaths of replayed purge rows (the emptied-dir cleanup set).
    pub purged_rels: Vec<PathBuf>,
}

/// Replay every non-`done` journal row to reach a consistent state (crash recovery).
///
/// Policy (idempotent, so always safe to repeat):
/// * a `pending` **deploy** row → roll FORWARD: re-apply the idempotent file op from
///   staging, write the manifest row, mark done. (If the staged source is gone we
///   cannot complete the deploy, so we roll it back instead — remove any partial
///   placement and restore vanilla.)
/// * a `pending` **purge** row → roll FORWARD: remove the (idempotent) target,
///   restore any recorded vanilla original, drop the rows, mark done.
///
/// Returns the replay outcome (count + replayed purge relpaths).
pub fn replay(store: &Store, game: &Game) -> Result<ReplayOutcome, DeployError> {
    let rows = store.pending_ops()?;
    let mut outcome = ReplayOutcome::default();
    for row in &rows {
        if row.appid != game.appid {
            // Not this game's op; leave it for that game's recovery pass.
            continue;
        }
        match row.kind.as_str() {
            KIND_DEPLOY => replay_deploy(store, game, row)?,
            KIND_PURGE => {
                replay_purge(store, game, row)?;
                // Record the relpath so the recovery purge path can clean up the dirs
                // the original deploy created (the same set purge() would clean up).
                outcome.purged_rels.push(row.target_rel.clone());
            }
            KIND_INI => replay_ini(store, game, row)?,
            other => {
                tracing::warn!(
                    kind = other,
                    "unknown journal kind; marking done to avoid a stuck row"
                );
                store.mark_done(row.id)?;
            }
        }
        outcome.replayed += 1;
    }
    Ok(outcome)
}

/// Roll a pending deploy row forward (finish it) or back (undo it) idempotently.
///
/// Rolling back is only safe when the target can be put back the way it was. If the row
/// says a pre-existing file was about to be overwritten and no vanilla backup exists for
/// it, the original is left ALONE: a crash between the durable intent and the backup is
/// exactly that state, and deleting a file we cannot restore is the one thing this engine
/// promises never to do.
fn replay_deploy(store: &Store, game: &Game, row: &JournalRow) -> Result<(), DeployError> {
    let target = crate::resolve_target(&game.install_dir, &row.target_rel);
    // Locate the staged source under the root the deploy was actually given. Production
    // stages each mod in a per-mod subdir of the staging dir, so the root has to come
    // from the row; `staging_dir` alone only works for a single mod staged at its root
    // (which is what the older rows without the column assumed).
    let staged_src = row
        .staging_root
        .clone()
        .unwrap_or_else(|| game.staging_dir.clone())
        .join(&row.target_rel);

    let method = row.method.unwrap_or(DeployMethod::Copy);
    let source_hash = row.source_hash.clone().unwrap_or_default();

    if staged_src.is_file() {
        // Roll FORWARD. Back up first, exactly as `deploy` does: a crash between the
        // durable intent and the original backup leaves the vanilla file sitting at the
        // target un-backed-up, and overwriting it here would destroy it just as surely as
        // deleting it.
        //
        // But the crash may equally have landed AFTER the file op, in which case what is
        // at the target is our own placement, and backing that up would record the mod's
        // bytes as the "vanilla original" — poisoning the very ledger purge restores from.
        // The recorded `source_hash` tells the two apart exactly: it is the hash of the
        // staged source, so a target matching it is our placement (every rung of the
        // ladder — reflink, hardlink, symlink, copy — yields the source's content), and a
        // target that differs is the user's file. `backup_vanilla_if_absent` is idempotent
        // and content-addressed, so re-running it for an already-backed-up original is a
        // no-op.
        let target_is_our_placement = target_matches_hash(&target, &source_hash);
        let backed = if target_is_our_placement {
            false
        } else {
            backup::backup_vanilla_if_absent(store, game, &target, &row.target_rel)?
        };
        let used = apply_idempotent(method, &staged_src, &target)?;
        let entry = FileEntry {
            target_rel: row.target_rel.clone(),
            source_mod: 0,
            method: used,
            hash: source_hash,
            pre_existing: backed || store.vanilla_for(game.appid, &row.target_rel)?.is_some(),
        };
        finish_deploy(store, row.id, game.appid, &entry)?;
    } else if !target_matches_hash(&target, &source_hash)
        && store.vanilla_for(game.appid, &row.target_rel)?.is_none()
        && !crate::backup::is_ours(store, game.appid, &row.target_rel)?
        && target.exists()
    {
        // The staged source is gone AND there is nothing on disk we may safely remove:
        // no backup to restore from, and the manifest does not claim this file as ours,
        // so what is sitting there is the user's own (vanilla or hand-placed) file. Leave
        // it untouched and mark the row done so recovery still converges — a row we can
        // neither complete nor undo must not be retried forever.
        tracing::warn!(
            target_rel = %row.target_rel.display(),
            "recovery left an un-backed-up file in place: the staged source is gone and \
             no vanilla backup exists, so removing it would be unrecoverable"
        );
        store.mark_done(row.id)?;
    } else {
        // The staged source is gone — we cannot complete this deploy. Roll BACK to a
        // pristine state: remove any partial placement and restore vanilla if backed
        // up, then mark the row done so recovery converges.
        crate::method::remove_if_present(&target).map_err(|e| DeployError::io(&target, e))?;
        backup::restore_vanilla(store, game, &target, &row.target_rel)?;
        store.remove_deployed_file(game.appid, &row.target_rel)?;
        store.mark_done(row.id)?;
    }
    Ok(())
}

/// Whether the file at `target` is byte-identical to the content `expected_hash` names.
///
/// Used to tell "our own placement" from "the user's file" during replay. Every rung of
/// the method ladder (reflink, hardlink, symlink, copy) makes the target read back as the
/// staged source's bytes, so a hash match means the interrupted op's file step had already
/// happened. Any read failure, or an empty recorded hash (a row from a path that never
/// recorded one), answers `false`: the safe default is to treat the file as the user's.
fn target_matches_hash(target: &Path, expected_hash: &str) -> bool {
    if expected_hash.is_empty() {
        return false;
    }
    backup::blake3_file(target).is_ok_and(|actual| actual == expected_hash)
}

/// Roll a pending purge row forward idempotently: remove the target, restore vanilla,
/// drop the manifest row, mark done.
fn replay_purge(store: &Store, game: &Game, row: &JournalRow) -> Result<(), DeployError> {
    let target = crate::resolve_target(&game.install_dir, &row.target_rel);
    crate::method::remove_if_present(&target).map_err(|e| DeployError::io(&target, e))?;
    backup::restore_vanilla(store, game, &target, &row.target_rel)?;
    store.remove_deployed_file(game.appid, &row.target_rel)?;
    finish_purge(store, row.id)?;
    Ok(())
}

/// Roll a pending `KIND_INI` row back to its recorded provenance idempotently.
///
/// This is the copy of [`replay_purge`]'s body that swaps ONLY the target resolution: the
/// INI target resolves via `steam::my_games_path(&game.prefix)`, NEVER `resolve_target` /
/// `guard_within_root` (the copy-paste trap; the `Data/`-root guard stays
/// byte-for-byte untouched). Rolling a crashed *ensure* back to provenance is a
/// valid recovery: the activation simply didn't take, and re-runs on the next deploy.
fn replay_ini(store: &Store, game: &Game, row: &JournalRow) -> Result<(), DeployError> {
    let target = steam::my_games_path(&game.prefix).join(crate::gameconfig::INI_FILENAME);
    crate::gameconfig::restore_ini_at(store, game, &target)?;
    // The sentinel is never in the deploy manifest, so this is a harmless no-op mirror of
    // replay_purge; kept for symmetry.
    store.remove_deployed_file(game.appid, &row.target_rel)?;
    store.mark_done(row.id)?;
    Ok(())
}

#[cfg(test)]
mod ini_replay_tests {
    use super::*;
    use crate::gameconfig::{self, IniConflictResolution, ensure_ini_active};
    use std::path::Path;
    use store::Store;
    use tempfile::TempDir;

    const STARFIELD: u32 = 1716740;

    /// A crashed INI op (a lingering `pending` KIND_INI row) is replayed to provenance via
    /// `my_games_path` — NEVER `resolve_target` — so recovery touches the prefix INI and
    /// never leaves a stray `Data/StarfieldCustom.ini`.
    #[test]
    fn replay_ini_resolves_under_prefix_not_data_and_rolls_back() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let store = Store::open(&root.join("d.db")).unwrap();
        let game = Game {
            appid: STARFIELD,
            name: "Starfield".into(),
            install_dir: root.join("install"),
            prefix: root.join("prefix"),
            staging_dir: root.join("staging"),
        };
        // Activate: creates the INI under the PREFIX (CreatedByNexTwist).
        ensure_ini_active(&store, &game, IniConflictResolution::Block).unwrap();
        let prefix_ini = steam::my_games_path(&game.prefix).join(gameconfig::INI_FILENAME);
        assert!(prefix_ini.exists());

        // Simulate a crash mid-op: a lingering pending KIND_INI row.
        begin_ini(&store, game.appid, Path::new(gameconfig::INI_FILENAME)).unwrap();
        assert!(!store.pending_ops().unwrap().is_empty());

        // Recovery replays it to provenance (absence) idempotently.
        let outcome = replay(&store, &game).unwrap();
        assert_eq!(outcome.replayed, 1);
        assert!(
            !prefix_ini.exists(),
            "rolled back to CreatedByNexTwist absence"
        );
        // The replay resolved via my_games_path, never Data/ — no stray Data/ INI exists.
        let data_ini = crate::deploy_root(&game.install_dir).join(gameconfig::INI_FILENAME);
        assert!(
            !data_ini.exists(),
            "replay_ini must NEVER touch the Data/ root"
        );
        assert!(store.pending_ops().unwrap().is_empty());
    }
}
