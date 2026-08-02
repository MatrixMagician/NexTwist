---
phase: 02-multi-mod-management
fixed_at: 2026-06-21T00:00:00Z
review_path: .planning/phases/02-multi-mod-management/02-REVIEW.md
iteration: 1
findings_in_scope: 10
fixed: 10
skipped: 0
status: all_fixed
---

# Phase 2: Code Review Fix Report

**Fixed at:** 2026-06-21
**Source review:** .planning/phases/02-multi-mod-management/02-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 10 (2 Critical/BLOCKER + 8 Warning; Info findings out of scope)
- Fixed: 10
- Skipped: 0

**Full gate (all green after fixes):**
- `cargo test --workspace` — all pass
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo deny check` — advisories/bans/licenses/sources ok (only a pre-existing
  unmatched-license-allowance warning in `deny.toml`, not introduced here)
- `cargo build -p nextwist` + `(cd frontend && npm run check)` — 0 errors

## Fixed Issues

### CR-01: `deploy_winner_set` redeploys without purging — orphans files and breaks reversibility

**Files modified:** `crates/deploy/src/engine.rs`, `src-tauri/src/commands/conflicts.rs`, `crates/deploy/tests/conflict_redeploy.rs`
**Commit:** ec19053
**Applied fix:** Added a new headless `deploy::redeploy_winners(store, game, winners)` entry
point that does `purge(...)` (back to byte-for-byte pristine, restoring vanilla originals
and clearing the manifest) THEN `deploy_winners(...)` in one journaled sequence — mirroring
`switch_profile`'s purge-then-deploy reconcile (Pitfall 4). Routed the `deploy_winner_set`
Tauri command through it so the live CONF-03 path can no longer orphan dropped files or
corrupt the `pre_existing` vanilla-backup flag on a second deploy. Added a regression test
(`redeploy_winners_reconciles_without_manual_purge`) that deploys winner set A (where a mod
overrides the vanilla `Skyrim.esm`), then redeploys a CHANGED set (a mod disabled so paths
leave the set) WITHOUT any manual purge between deploys, asserting the vanilla master is
restored, dropped files are not orphaned, and a final purge returns byte-for-byte pristine.

### CR-02: `delete_profile` can delete the active profile, orphaning its live deployment

**Files modified:** `crates/store/src/profiles.rs`, `src-tauri/src/commands/profiles.rs`
**Commit:** b3765c0
**Applied fix:** `Store::delete_profile` now refuses to delete the currently-active profile,
returning a clear `StoreError::Db("cannot delete the active profile; switch to another
profile first")` (the caller must switch away — which purges the outgoing deployment to
pristine — before deleting). The active check uses `OptionalExtension::optional()` on a
`SELECT active`. The command-layer doc now states the gate so the UI can disable the Delete
button on the active profile. Added a test (`delete_active_profile_is_refused`) proving the
active profile cannot be deleted (invariant: it stays present + active, exactly one active
remains), an inactive profile is still deletable, and a former-active profile is deletable
after switching away.

### WR-01: masterlist fetch pins the request URL but follows redirects to any host

**Files modified:** `crates/loadorder/src/masterlist.rs`
**Commit:** d58564c
**Applied fix:** `real_fetch` now builds a `reqwest::blocking::Client` with
`redirect(Policy::none())` so the body can only come from the pinned
`raw.githubusercontent.com` URL (the default client silently follows up to 10 redirects to
any host). Added defence-in-depth re-assertion that the final response host equals the
pinned host, returning an error otherwise.

### WR-02: `switch_profile` leaves the OLD profile marked active if deploy/plugins fail after purge

**Files modified:** `crates/deploy/src/profile.rs`, `crates/store/src/profiles.rs`
**Commit:** 75ba543
**Applied fix:** Added `Store::clear_active_profile(appid)` (clears the active flag on every
profile for a game). Refactored `switch_profile` to run the post-purge half (deploy →
plugins → mark active) in a `switch_after_purge` helper, and on ANY error from it clears the
stale active flag via `inspect_err` — so once the old deployment is purged off disk the store
never keeps an OLD profile flagged active while its files are gone. The happy-path
`profile_switch.rs` tests still pass unchanged.
**NOTE: requires human verification** — this is an error-path/logic change. The happy path is
test-covered and green; the new failure-path behavior (clearing the flag on a mid-switch
error) is reasoned-through but not directly exercised by an automated failure-injection test.

### WR-03: `set_plugin_enabled` read-modify-write is non-atomic across two separate lock acquisitions

**Files modified:** `src-tauri/src/commands/plugins.rs`
**Commit:** 51d6d1c
**Applied fix:** Added a synchronous `merged_plugins_locked(&guard, appid)` that performs the
active-profile read, the enabled-mods read, the on-disk plugin scan, and the stored-state
read all under a single already-held state lock. `set_plugin_enabled` now acquires the lock
ONCE and does the whole resolve-merge-toggle-persist under it (atomic against concurrent
plugin ops). `merged_plugins` (used by `list_plugins`/`sort_with_loot`) was likewise reduced
to a single lock acquisition delegating to the locked helper. Removed the now-unused
`enabled_staging_roots` helper. Verified clippy-clean.

### WR-04: `deploy_winners` silently drops a winner whose source file vanished, with no report

**Files modified:** `crates/deploy/src/engine.rs`, `frontend/src/lib/api.ts`
**Commit:** c96f37e
**Applied fix:** Added a `skipped: Vec<PathBuf>` field to `DeployReport`; `deploy_winners`
now records the target rel of any winner whose source file is missing at deploy time instead
of silently dropping it, so the report is honest about an incomplete deployment (`deployed`
counts only files actually placed). Updated both `DeployReport` constructors and the frontend
`DeployReport` TypeScript interface to mirror the new field. Frontend `npm run check` passes.

### WR-05: `save_plugin_order` persists state before writing plugins.txt, leaving them inconsistent on write failure

**Files modified:** `src-tauri/src/commands/plugins.rs`
**Commit:** 7145496
**Applied fix:** Reordered `save_plugin_order` to write `plugins.txt` (via
`loadorder::apply_load_order`) FIRST and persist the `plugin_state` rows only after the file
write succeeds — so a libloot/IO failure leaves the DB untouched, matching the user's
"nothing was saved" mental model. The DB persist runs under a single lock and returns the
written path.
**NOTE: requires human verification** — this is an error-path/ordering logic change. It
compiles and the happy path is covered by the build; the failure-mode reordering is
reasoned-through but not directly exercised by an automated write-failure-injection test.

### WR-06: `set_mod_rank` / `set_profile_mod` accept arbitrary mod_id/profile_id with no FK enforcement

**Files modified:** `crates/store/src/migrations/V3__profile_fks.sql` (new), `crates/store/src/profiles.rs`, `crates/store/src/plugins.rs`
**Commit:** 200c69c
**Applied fix:** Added a NEW V3 migration (V2 left untouched per T-02-01 additive) that
rebuilds `profile_mod` and `plugin_state` with foreign keys: `profile_mod.profile_id →
profile(id) ON DELETE CASCADE`, `profile_mod.mod_id → managed_mod(id) ON DELETE CASCADE`,
and `plugin_state.profile_id → profile(id) ON DELETE CASCADE`. SQLite cannot add FKs via
`ALTER TABLE`, so the migration uses the supported table-rebuild pattern (create-new, copy,
drop, rename, recreate indexes) and first deletes any pre-existing dangling rows so the new
constraints validate cleanly. Updated the store tests (which previously inserted membership /
plugin rows referencing fabricated profile/mod ids — exactly the gap this closes) to create
real `managed_mod` / `profile` rows. Added new tests proving dangling profile/mod ids are now
rejected by the FK and that removing a mod cascades its membership rows away. All 31 store
tests + deploy integration tests (`profile_switch`, `conflict_redeploy`) pass.

### WR-07: `deploy_root` / `add_game_by_folder` pick the FIRST case-insensitive "data" match nondeterministically

**Files modified:** `crates/deploy/src/lib.rs`, `crates/steam/src/resolve.rs`
**Commit:** ffbd138
**Applied fix:** `deploy_root` now collects ALL case-insensitive "data" matches and chooses
deterministically — exact `"Data"` first, then exact `"data"`, else the lexicographically
smallest variant — so a case-sensitive FS holding both `Data` and `data` resolves to a stable
deploy root across runs (a purge/deploy-root mismatch hazard for reversibility). Applied the
same determinism to `entry_ci` in resolve.rs (exact-case match wins, else lexicographically
smallest), covering the `Data/` marker and exe resolution. Clippy-clean; deploy + steam tests
pass.

### WR-08: `set_active_profile` and `delete_profile` use `unchecked_transaction` (no nesting guard)

**Files modified:** `crates/store/src/profiles.rs`
**Commit:** 95ab2dc
**Applied fix:** The `Store` facade exposes `&self` methods over an owned `Connection`, so the
checked `Connection::transaction()` (which requires `&mut self`) is not callable from these
methods — `unchecked_transaction()` is genuinely required, not an oversight. Per the review's
accepted alternative ("if there is [a reason], add a comment stating the invariant"),
documented at BOTH call sites the invariant that every `Store` call site is top-level (no
outer transaction is ever open, so the skipped runtime guard would never fire) and that a
future refactor introducing an outer transaction should switch the facade to `&mut self` +
`transaction()` rather than nesting. The `delete_profile` note also records that V3's
`ON DELETE CASCADE` now covers the child-row deletes.

## Skipped Issues

None — all 10 in-scope findings were fixed.

The 6 Info findings (IN-01 through IN-06) were out of scope for the `critical_warning` fix
scope and were not addressed.

---

_Fixed: 2026-06-21_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
