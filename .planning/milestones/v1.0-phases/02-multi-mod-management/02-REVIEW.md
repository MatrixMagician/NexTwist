---
phase: 02-multi-mod-management
reviewed: 2026-06-21T00:00:00Z
depth: deep
files_reviewed: 38
files_reviewed_list:
  - crates/core/src/model.rs
  - crates/core/src/lib.rs
  - crates/deploy/src/conflict.rs
  - crates/deploy/src/engine.rs
  - crates/deploy/src/profile.rs
  - crates/deploy/src/path_guard.rs
  - crates/deploy/src/lib.rs
  - crates/deploy/src/error.rs
  - crates/deploy/src/verify.rs
  - crates/deploy/src/casefold.rs
  - crates/deploy/tests/conflict_redeploy.rs
  - crates/deploy/tests/profile_switch.rs
  - crates/loadorder/src/lib.rs
  - crates/loadorder/src/loot.rs
  - crates/loadorder/src/masterlist.rs
  - crates/loadorder/src/scan.rs
  - crates/loadorder/src/error.rs
  - crates/loadorder/tests/libloot_spike.rs
  - crates/loadorder/tests/plugins.rs
  - crates/store/src/mods.rs
  - crates/store/src/plugins.rs
  - crates/store/src/profiles.rs
  - crates/store/src/db.rs
  - crates/store/src/lib.rs
  - crates/store/src/migrations/V1__init.sql
  - crates/store/src/migrations/V2__multi_mod.sql
  - crates/steam/src/discover.rs
  - crates/steam/src/resolve.rs
  - crates/testkit/src/lib.rs
  - src-tauri/src/commands/conflicts.rs
  - src-tauri/src/commands/plugins.rs
  - src-tauri/src/commands/profiles.rs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/lib.rs
  - frontend/src/lib/api.ts
  - Cargo.toml
  - deny.toml
  - rust-toolchain.toml
findings:
  critical: 2
  warning: 8
  info: 6
  total: 16
status: issues_found
---

# Phase 2: Code Review Report

**Reviewed:** 2026-06-21
**Depth:** deep (cross-file: import graph + call chains across `deploy` / `store` / `loadorder` / Tauri command boundary)
**Files Reviewed:** 38
**Status:** issues_found

## Summary

Phase 2 builds the multi-mod / profile / plugin substrate on top of the Phase-1 reversible
engine. The headless safety core is, in the main, well-structured: the conflict resolver is a
pure fold that dedups to one winner per path before any syscall, `switch_profile` correctly
routes through the unchanged journaled `purge → deploy_winners` primitives, the path-containment
guard is shared between engine and resolver, and the V2 migration is strictly additive. The
test suite for the headless crates is genuinely adversarial (pristine snapshots include empty-dir
shape; rank-flip redeploy is regression-locked).

However, the **review surfaced two BLOCKER-class safety gaps at seams that the headless tests do
NOT exercise**, both of which can leave the game folder non-pristine — the one invariant the
project says must hold above all else:

1. **`deploy_winner_set` (the CONF-03 Tauri command) never purges the prior deployment before
   redeploying.** Unlike `switch_profile`, which always does a full purge-to-pristine first, the
   conflict-deploy command resolves and deploys directly. After a rank change that drops a file
   from the winner set, or after disabling a mod, the previously-deployed file is orphaned on
   disk and (because the manifest uses `INSERT OR REPLACE`, not a reconcile) is silently dropped
   from the manifest — defeating reversibility. The headless `conflict_redeploy.rs` test masks
   this by manually purging between deploys; the live command does not.

2. **`delete_profile` can delete the currently-active profile without purging its live
   deployment.** This orphans every deployed file of that profile on disk with no manifest path
   back to pristine through the normal profile flow, and leaves the game with zero active
   profiles, violating the "exactly one active profile" invariant.

The remaining warnings concern a redirect-host-pinning gap in the masterlist fetch, a stale
active-flag / non-atomic state window in `switch_profile` and `set_plugin_enabled`, and several
robustness issues. Info items are minor.

## Critical Issues

### CR-01: `deploy_winner_set` redeploys without purging — orphans files and breaks reversibility

**File:** `src-tauri/src/commands/conflicts.rs:74-83` (and the missing-purge contract vs. `crates/deploy/src/engine.rs:212` / `crates/store/src/manifest.rs:17-20`)

**Issue:** The CONF-03 deploy command resolves the enabled-mod winner set and calls
`deploy::deploy_winners` directly, with NO preceding `purge`:

```rust
pub async fn deploy_winner_set(state, appid) -> Result<DeployReport, String> {
    let game = require_game(&state, appid).await?;
    let inputs = enabled_mod_inputs(&state, appid).await?;
    let (winners, _conflicts) = conflict::resolve(&inputs).map_err(boundary_err)?;
    deploy::deploy_winners(&state.lock().await.store, &game, &winners).map_err(boundary_err)
}
```

`deploy_winners` → `deploy_one_file` → `journal::finish_deploy` → `store.record_deployed_file`,
which is `INSERT OR REPLACE INTO deployed_file` (manifest.rs:20). Consequences when the command
is invoked a SECOND time after the winner set has changed (a rank flip that changes which mod
provides a path, or a mod being disabled/removed so a path leaves the set):

- A path that was deployed before but is **not** in the new winner set is never removed from
  disk and its manifest row is never deleted. The file is orphaned and, worse, still "owned" in
  the manifest under whatever its last row said — purge will still find it, but any winner whose
  source mod was removed leaves a file the new manifest no longer attributes correctly.
- For a path whose winner changed mods, `INSERT OR REPLACE` silently overwrites the manifest row
  but the on-disk file from the previous method/inode is replaced by `apply_idempotent`
  (remove-then-create), so that specific path is fine — but the vanilla-backup `pre_existing`
  bookkeeping recorded on the FIRST deploy is now lost because `record_deployed_file` replaces the
  whole row (see CR-01 interaction with backup: the second deploy calls
  `backup_vanilla_if_absent` which is a no-op since a backup already exists, so `backed` is
  `false` and the replaced row records `pre_existing = false` even though a vanilla original IS
  backed up — purge then will NOT restore the vanilla file for that path). This breaks
  byte-for-byte pristine restore.

`switch_profile` (profile.rs:79-88) gets this right by always purging first. The conflict-deploy
command must do the same, or `deploy_winners` must itself reconcile against the existing manifest.

**Fix:** Make the redeploy a full purge-to-pristine then fresh deploy, mirroring `switch_profile`,
preferably by adding a headless `deploy::redeploy_winner_set` that does `purge` then
`deploy_winners` in one journaled sequence so the command stays a thin adapter:

```rust
// crates/deploy/src/engine.rs (new headless entry point)
pub fn redeploy_winners(store: &Store, game: &Game, winners: &[WinnerFile])
    -> Result<(PurgeReport, DeployReport), DeployError> {
    let purged = purge(store, game)?;          // back to pristine first (Pitfall 4)
    let deployed = deploy_winners(store, game, winners)?;
    Ok((purged, deployed))
}
```
```rust
// conflicts.rs
let (_purged, report) = deploy::redeploy_winners(&store, &game, &winners).map_err(boundary_err)?;
Ok(report)
```
Add a headless regression test that deploys winner set A, flips a rank, deploys again WITHOUT a
manual purge, then purges and asserts byte-for-byte pristine (the existing test cheats by purging
between deploys).

### CR-02: `delete_profile` can delete the active profile, orphaning its live deployment

**File:** `crates/store/src/profiles.rs:134-154`, exposed at `src-tauri/src/commands/profiles.rs:64-72`

**Issue:** `delete_profile` removes the `profile`, `profile_mod`, and `plugin_state` rows in a
transaction but does NOT check whether the target is the active profile, and never purges its
on-disk deployment. If the user deletes the active profile:

- Its deployed files remain on disk (the live deployment is still there).
- The game now has **zero** active profiles, violating the "exactly one active per game"
  invariant the codebase repeatedly asserts (profiles.rs:64 doc, model.rs:62).
- There is no longer a profile whose membership explains the deployed set, so the conflict /
  plugin views and any future `deploy_winner_set` operate against an empty enabled set while real
  files sit deployed — drift the user cannot reconcile through the normal UI.

The manifest still permits a `purge` (it is manifest-driven, not profile-driven), so the data is
recoverable via the Phase-1 purge path — but nothing in the profile flow triggers it, and the
"exactly one active" invariant is broken immediately. This is a data-consistency / safety-invariant
violation at the command boundary.

**Fix:** Refuse to delete the active profile, OR make deletion of the active profile first purge
the deployment and clear the active flag. Minimal safe guard at the store layer:

```rust
pub fn delete_profile(&self, profile_id: i64) -> Result<bool, StoreError> {
    // Reject deleting an active profile — the caller must switch away (which purges) first.
    let is_active: bool = self.conn.query_row(
        "SELECT active FROM profile WHERE id = ?1", params![profile_id],
        |r| r.get::<_, i64>(0)).optional()
        .map_err(|e| StoreError::Db(e.to_string()))?
        .map(|a| a != 0).unwrap_or(false);
    if is_active {
        return Err(StoreError::Db(
            "cannot delete the active profile; switch to another profile first".into()));
    }
    // ... existing transaction ...
}
```
The command layer should surface this as a clear error the UI can gate the Delete button on.

## Warnings

### WR-01: masterlist fetch pins the request URL but follows redirects to any host

**File:** `crates/loadorder/src/masterlist.rs:159-163`

**Issue:** The trust-boundary doc (masterlist.rs:6-13, T-02-10) states the fetch is "pinned:
HTTPS only, host `raw.githubusercontent.com`". But `reqwest::blocking::get(url)` uses the default
client, which **follows up to 10 redirects with no host restriction**. A redirect (or a MITM able
to inject a 30x at the TLS-terminating CDN edge) could send the client to an arbitrary host, and
the body it returns is then written to the on-disk cache and fed to libloot's masterlist parser.
The pinning is therefore only enforced on the first hop, not on where the bytes actually come from.

**Fix:** Build a client with redirects disabled (or a custom policy that rejects any non-pinned
host) and assert the final URL host:

```rust
fn real_fetch(url: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build().map_err(|e| e.to_string())?;
    let resp = client.get(url).send().map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?;
    // Optionally re-assert resp.url().host_str() == Some("raw.githubusercontent.com").
    resp.text().map_err(|e| e.to_string())
}
```

### WR-02: `switch_profile` leaves the OLD profile marked active if deploy/plugins fail after purge

**File:** `crates/deploy/src/profile.rs:79-99`

**Issue:** The sequence is purge → resolve → `deploy_winners` → `apply_profile_plugins` →
`set_active_profile`. If any step between the purge and `set_active_profile` returns `Err`, the
function aborts with the OLD profile still flagged active in the store, but the on-disk deployment
has already been purged to pristine. The store's active profile now describes a deployment that no
longer exists on disk. The game is pristine (so not data loss), but the persisted state is
inconsistent: the UI shows profile X active while nothing of X is deployed, and a subsequent
`deploy_winner_set` (see CR-01) would operate on X's enabled set with no purge. The doc
(profile.rs:73) acknowledges "pristine or journal-recoverable" but not the stale-active-flag drift.

**Fix:** On any error after the purge, clear the active flag (or set it only after a successful
deploy AND record an explicit "no active profile / pristine" state) so the active flag never
points at a deployment that isn't on disk. At minimum, document and surface this so the UI can
prompt a re-switch.

### WR-03: `set_plugin_enabled` read-modify-write is non-atomic across two separate lock acquisitions

**File:** `src-tauri/src/commands/plugins.rs:106-126` (and `merged_plugins` 55-90)

**Issue:** `set_plugin_enabled` calls `active_profile_id` (locks state), then `merged_plugins`
(locks state again, does a scan + a `list_plugin_state` read), then locks a THIRD time to
`set_plugin_state`. Between the read and the write the lock is released. `merged_plugins` itself
locks twice internally (once for `enabled_staging_roots`, once for `list_plugin_state`) with the
plugin scan in between. Each individual store op is consistent, but the merged view used to
construct the row to persist can be stale relative to the write. For a single-user desktop app the
window is small, but a concurrent `save_plugin_order` or `switch_profile` invocation can interleave
and produce a lost update or a row whose kind/order disagree with the just-persisted order.

**Fix:** Acquire the state lock once for the whole read-modify-write of a plugin toggle, or move
the merge+toggle into a single headless function the command calls under one lock. The same
single-lock discipline should apply to `merged_plugins`.

### WR-04: `deploy_winners` silently drops a winner whose source file vanished, with no report

**File:** `crates/deploy/src/engine.rs:237-243`

**Issue:** When a winner's `src` is no longer a file at deploy time, it is `continue`d past with no
counter and no entry in `DeployReport`. The comment calls this "one fewer file, never corruption,"
which is true for atomicity, but the user gets a `DeployReport` whose `deployed` count silently
omits the dropped file and no signal that the deployment is incomplete relative to the resolved
winner set. For a mod manager whose value proposition is "the user always knows what is
deployed," silent omission is a correctness/UX defect.

**Fix:** Track skipped winners (e.g. `report.skipped: Vec<PathBuf>`) and surface them, or return
an error if a resolved winner's source is missing (the resolver walked it moments earlier, so a
miss is unexpected and worth reporting rather than swallowing).

### WR-05: `save_plugin_order` persists state before writing plugins.txt, leaving them inconsistent on write failure

**File:** `src-tauri/src/commands/plugins.rs:135-164`

**Issue:** The command writes every `plugin_state` row first (lines 144-156) and only then calls
`loadorder::apply_load_order` (162). If `apply_load_order` fails (libloot error, IO on the prefix
AppData), the DB now records the new order/enabled state but the on-disk `plugins.txt` was never
written (or partially written) — the persisted state and the prefix disagree, and the error is
returned so the UI may believe nothing was saved. There is no transaction spanning the DB writes
and the file write (and there cannot be fully), but the ordering makes the inconsistent outcome the
default failure mode.

**Fix:** Either write `plugins.txt` first and persist `plugin_state` only after it succeeds (so a
failure leaves the DB untouched, matching the user's "nothing saved" mental model), or wrap the DB
writes in a transaction that is rolled back if the file write fails.

### WR-06: `set_mod_rank` / `set_profile_mod` accept arbitrary mod_id/profile_id with no FK enforcement

**File:** `crates/store/src/mods.rs:69-78`, `crates/store/src/profiles.rs:86-103`; schema `crates/store/src/migrations/V2__multi_mod.sql:51-58`

**Issue:** `profile_mod` and `plugin_state` declare no `FOREIGN KEY` to `profile(id)` /
`managed_mod(id)`, and `db.rs` sets `PRAGMA foreign_keys=ON` (which only matters where FKs are
declared). `set_profile_mod(profile_id, mod_id, ...)` and `set_mod_rank(id, ...)` will happily
insert/update membership rows for non-existent profiles or mods (the latter is documented as a
no-op only because `UPDATE ... WHERE id` matches nothing, but `set_profile_mod` INSERTs a dangling
row). `enabled_inputs_for_profile` (profile.rs:132) defends against dangling mod_ids by skipping
them, but dangling membership rows accumulate and a dangling profile_id is never caught. This is a
data-integrity gap the schema should enforce.

**Fix:** Add `FOREIGN KEY (profile_id) REFERENCES profile(id) ON DELETE CASCADE` to `profile_mod`
and `plugin_state`, and `FOREIGN KEY (mod_id) REFERENCES managed_mod(id) ON DELETE CASCADE` to
`profile_mod`. The `ON DELETE CASCADE` would also let `delete_profile`/`remove_mod` shed their
manual child-row deletes. (Note: adding FKs to existing tables requires a table rebuild in SQLite;
do it in V3, not by altering V2 retroactively.)

### WR-07: `deploy_root` / `add_game_by_folder` pick the FIRST case-insensitive "data" match nondeterministically

**File:** `crates/deploy/src/lib.rs:59-69`; `crates/steam/src/resolve.rs:318-330`

**Issue:** `deploy_root` iterates `read_dir` and returns the first entry equal-ignore-ascii-case to
"data". `read_dir` order is filesystem-dependent and unordered. If a game dir somehow contains both
`Data` and `data` (possible on a case-sensitive Linux FS, which is exactly the environment
NexTwist targets under Proton), the chosen deploy root is nondeterministic across runs, and a
purge computed against one casing could leave files under the other. `entry_ci` in resolve.rs has
the same first-match-wins property for the `Data/` marker and the exe.

**Fix:** When multiple case-variant matches exist, prefer an exact `"Data"` match, then fall back
deterministically (e.g. lexicographically smallest), and consider warning on the ambiguous case
since two `Data`-like dirs on a case-sensitive FS is a real hazard for a reversibility guarantee.

### WR-08: `set_active_profile` and `delete_profile` use `unchecked_transaction` (no nesting guard)

**File:** `crates/store/src/profiles.rs:65, 137`

**Issue:** `unchecked_transaction()` bypasses rusqlite's runtime guard against an already-open
transaction. If any caller ever wraps these in an outer transaction (or a future refactor does),
the inner `BEGIN`/`COMMIT` will silently corrupt transaction nesting (SQLite does not support
nested transactions without savepoints). Today the call sites are top-level so it works, but it is
a latent footgun on a safety-critical store. The choice is undocumented (unlike most of this
codebase, which justifies every non-obvious decision).

**Fix:** Use `self.conn.transaction()` (the checked variant) unless there is a documented reason
the unchecked form is required; if there is, add a comment stating the invariant that no outer
transaction is ever open.

## Info

### IN-01: `game_id_for_data` ignores its arguments and always returns SkyrimSE

**File:** `crates/loadorder/src/scan.rs:212-217`

**Issue:** `scan_plugins` infers the classifier via `game_id_for_data`, which unconditionally
returns `Some(GameId::SkyrimSE)` regardless of the roots/data passed. The doc justifies this (the
header flag bits scan reads are identical for SkyrimSE/Fallout4 and the command path uses
`scan_plugins_for` with the real id), so it is correct in practice — but a function named
`game_id_for_data` taking two args it never reads is a maintenance trap. Rename to
`default_scan_game_id()` and drop the unused params, or take the real appid.

### IN-02: ESL detection from a 24-byte TES4 stub is untested (acknowledged limitation)

**File:** `crates/loadorder/src/scan.rs:226-237` (test fixture comment)

**Issue:** The light/ESL flag lives in the TES4 record header, not the 24-byte file header the test
fixtures build, so `PluginKind::Esl` classification has no real coverage — only ESM vs ESP is
exercised. The behavior may be correct (it defers to `esplugin::is_light_plugin`), but the
ESL branch in `classify_kind` is effectively untested. Add a fixture with a real light-flagged
record, or note it as a known coverage gap in the phase verification.

### IN-03: `merged_plugins` is O(n*m) over scanned x stored plugins

**File:** `src-tauri/src/commands/plugins.rs:78-88`

**Issue:** The merge does a linear `stored.iter().find` per scanned plugin. For large load orders
(Bethesda modlists routinely exceed 1000 plugins) this is quadratic. Out of v1 perf scope, noted
only because it sits on the hot path of every `list_plugins`/`set_plugin_enabled` call. A
`HashMap<&str, &Plugin>` keyed by name would make it linear.

### IN-04: `RecoveryReport.drift` carries a `VerifyReport` but `repair` is never auto-invoked

**File:** `crates/deploy/src/engine.rs:465-482`, `crates/deploy/src/verify.rs`

**Issue:** `recover_on_launch` runs `verify` and reports drift but never `repair`s — by design
(the UI decides). Worth confirming the UI actually consumes `RecoveryReport.drift` and surfaces it;
`src-tauri/src/lib.rs:39-47` only logs `pristine` and discards the detailed report. If nothing
shows the drift to the user, the "full-pristine-or-report" guarantee is satisfied only in the log,
not the UI.

### IN-05: `resolve_data_dir` falls back to a relative `.nextwist` dir

**File:** `src-tauri/src/lib.rs:19-23`

**Issue:** If `app_data_dir()` fails, the fallback is the relative path `PathBuf::from(".nextwist")`,
which resolves against the process CWD — nondeterministic and potentially unwritable/shared. For a
persistence root that holds the deploy ledger and vanilla backups (the reversibility substrate),
an unstable location is risky. Prefer an absolute fallback under `$HOME`.

### IN-06: `boundary_err` flattens all typed errors to opaque strings at the IPC boundary

**File:** `src-tauri/src/commands/mod.rs:24-26`

**Issue:** Every headless error becomes a bare `String`, so the frontend cannot distinguish (e.g.)
a `PathEscape` safety abort from a transient IO error or a "not managed" lookup miss — it only gets
display text. This is an intentional thin-adapter choice, but for a safety-critical app the UI may
want to treat a `PathEscape`/`NotPristine` differently (block + alarm) from a recoverable IO error
(retry). Consider a small typed error DTO with a machine-readable `kind` field.

---

_Reviewed: 2026-06-21_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
