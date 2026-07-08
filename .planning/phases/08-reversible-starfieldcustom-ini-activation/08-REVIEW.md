---
phase: 08-reversible-starfieldcustom-ini-activation
reviewed: 2026-07-08T10:06:51Z
depth: deep
files_reviewed: 13
files_reviewed_list:
  - crates/deploy/src/gameconfig.rs
  - crates/deploy/src/journal.rs
  - crates/deploy/src/engine.rs
  - crates/deploy/src/verify.rs
  - crates/deploy/src/error.rs
  - crates/deploy/src/lib.rs
  - crates/deploy/tests/ini_activation.rs
  - crates/store/src/vanilla.rs
  - src-tauri/src/commands/gameconfig.rs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/lib.rs
  - frontend/src/lib/api.ts
  - frontend/src/routes/+page.svelte
findings:
  critical: 1
  warning: 2
  info: 2
  total: 5
status: issues_found
---

# Phase 8: Code Review Report

**Reviewed:** 2026-07-08T10:06:51Z
**Depth:** deep
**Files Reviewed:** 13
**Status:** issues_found

## Summary

Phase 8 adds the first sanctioned engine write outside the `Data/` deploy root
(`StarfieldCustom.ini` under the Proton prefix). Most of the safety substrate is sound and
the invariants the review focus called out largely hold — I verify them explicitly below.
The surgical INI editor preserves BOM/EOL/comment/key-order byte-for-byte; the `KIND_INI`
journal replay resolves via `my_games_path` only and never touches `resolve_target` /
`guard_within_root`; `restore_ini` no-ops when no provenance row exists; and verify/repair
treat a user-blocked conflict as not-drift.

**However, one path defeats the three-valued provenance model and causes user-data loss.**
`ensure_ini_active` records the `ABSENCE_MARKER` (CreatedByNexTwist) *unconditionally*
whenever the on-disk INI is absent, with no check for an existing provenance row. When a
PreExisting user INI is activated and then the on-disk file is later deleted (a routine
Bethesda-modding step) and re-activation runs (via the SFINI-05 repair button, or the next
deploy), the real-blake3 provenance is silently overwritten to `ABSENCE_MARKER`. A
subsequent purge then **deletes the user's original INI** and never restores it — the exact
"green-but-wrong reversibility" class this phase was warned about. This is a BLOCKER.

Two narrower correctness gaps (an interrupted-purge window where the INI restore is not
journal-recoverable, and UTF-16 body mishandling) and two info-level items round out the
report.

### Invariants that HOLD (verified)

- **Guard non-widening (T-08-03):** `replay_ini` (journal.rs:214-223) resolves via
  `steam::my_games_path(&game.prefix)`, never `resolve_target`; `verify`/`repair` gate the
  INI check on `game.appid == steam::STARFIELD` and delegate entirely to `gameconfig`
  (verify.rs:121-123, 216-223). The `Data/`-root `guard_within_root` path is byte-for-byte
  intact and the orphan walk is unchanged.
- **drive_c containment + symlink refusal:** `resolve_ini_target` re-verifies lexical
  `drive_c` containment at the write site (gameconfig.rs:543-557) and `refuse_symlink`
  blocks writing through a foreign symlink (gameconfig.rs:562-570); both are unit-tested.
- **restore-absence gate (partial):** `restore_ini` no-ops when no provenance row exists
  (gameconfig.rs:453-458) and `restore_ini_at` drops the row after restoring
  (gameconfig.rs:536), so a never-activated user INI is never deleted and a stale marker is
  not stranded on the happy path. (The BLOCKER below is on the *record* side, not restore.)
- **Byte-fidelity + idempotency:** the editor's raw-line/EOL/BOM handling and the
  `AlreadyActive` early-return converge deploy×2 / profile-switch / crash-replay to one
  `[Archive]` with identical bytes for CRLF/LF/BOM inputs (well covered by editor_tests and
  ini_activation.rs).
- **Copy-paste trap avoided:** `replay_ini` swaps only target resolution vs `replay_purge`;
  no `Data/`-root leakage (test `ini_kind_path_under_prefix` proves no stray `Data/` INI).

## Critical Issues

### CR-01: Re-activation of an absent PreExisting INI overwrites provenance → purge deletes the user's original file

**File:** `crates/deploy/src/gameconfig.rs:436-439`
**Issue:**
`ensure_ini_active` captures provenance as:

```rust
let pre_existing = backup::backup_vanilla_if_absent(store, game, &target, sentinel)?;
if !pre_existing {
    store.record_vanilla(game.appid, sentinel, ABSENCE_MARKER)?;
}
```

`backup_vanilla_if_absent` returns `false` whenever the target does not exist on disk
(backup.rs:57-59, early `path_exists` return). The `ABSENCE_MARKER` is then recorded
**unconditionally**, with no check for an existing `vanilla_backup` row. `record_vanilla`
is `INSERT OR REPLACE` (vanilla.rs:28), so it silently overwrites a prior real-blake3-hash
(PreExisting) row.

This flips the three-valued provenance from `PreExisting` to `CreatedByNexTwist` and
strands the original bytes: the blob still sits at `originals/<appid>/<hash>` but no row
points to it, so `restore_ini_at` (gameconfig.rs:528) takes the `ABSENCE_MARKER` branch and
**deletes the file + prunes dirs** instead of restoring the user's original bytes.

**Concrete failure scenario (inputs → wrong outcome):**
1. User has a pre-existing `StarfieldCustom.ini` with their own settings.
2. Deploy loose mod → `ensure_ini_active` backs up original (row = real hash `H`, blob
   stored), writes the merged INI. Provenance = PreExisting. ✅
3. User deletes `StarfieldCustom.ini` on disk (standard "reset my INI" troubleshooting;
   does not relaunch the game, so it is not regenerated).
4. In NexTwist the user clicks Verify/Repair. `ini_drift` returns `Some(Missing)` (loose
   files deployed, target absent — gameconfig.rs:499-501); `repair` calls
   `ensure_ini_active(Block)` (verify.rs:216-222).
5. Target is absent → `backup_vanilla_if_absent` returns `false` →
   `record_vanilla(ABSENCE_MARKER)` **overwrites row `H`**. Provenance now = CreatedByNexTwist.
6. Later `purge` → `restore_ini` → `hash == ABSENCE_MARKER` → the file is **deleted** and
   the user's original INI content is permanently lost (blob `H` orphaned, never restored).

This violates the core byte-for-byte reversibility guarantee and causes user data loss. The
existing test `ini_verify_detects_removed_ini` only exercises the CreatedByNexTwist case
(`MyGamesOpts::default()`, no pre-existing INI), so re-recording the same `ABSENCE_MARKER`
masks the bug — the PreExisting-then-deleted-then-repaired path is untested.

The same class also fires through the deploy tail: any `deploy` / `deploy_winners` whose
Starfield INI happens to be absent at activation time re-records `ABSENCE_MARKER` over an
existing real-hash row.

**Fix:** only record the absence marker when there is no existing provenance row (the
authoritative provenance is captured on first activation and must never be downgraded):

```rust
let pre_existing = backup::backup_vanilla_if_absent(store, game, &target, sentinel)?;
// Only stamp CreatedByNexTwist when NO provenance exists yet. An existing real-hash
// (PreExisting) row is authoritative and must never be downgraded to ABSENCE_MARKER,
// or a later purge would delete a user file whose original bytes we still hold.
if !pre_existing && store.vanilla_for(game.appid, sentinel)?.is_none() {
    store.record_vanilla(game.appid, sentinel, ABSENCE_MARKER)?;
}
```

Add a regression test: seed a PreExisting INI, activate, `fs::remove_file` the on-disk INI,
`repair` (or re-`deploy`), then `purge` and assert the original bytes are restored
byte-for-byte (not deleted).

## Warnings

### WR-01: Interrupted-purge window leaves the INI activated with no journal-recoverable intent

**File:** `crates/deploy/src/engine.rs:461-463`, `crates/deploy/src/gameconfig.rs:494-497`
**Issue:**
`purge` restores the INI as its final step *after* the `Data/` manifest loop has fully
completed (every file-purge row flipped to `done`, manifest emptied), and `restore_ini`
records its own `begin_ini` intent only *inside* that call. If the process is killed in the
window between the last file-purge `finish_purge` and `restore_ini`'s `begin_ini`, there is
no pending journal row describing the INI restore.

On the next launch, `recover_on_launch` → `journal::replay` finds nothing to replay, and
`verify` → `ini_drift` returns `None` because `list_deployed_files` is now empty
(gameconfig.rs:495-497 short-circuits on an empty manifest). The activated INI (merged
bytes) and its stale `vanilla_backup` row therefore remain, and no drift is reported — the
game is not byte-for-byte pristine after a purge that the journal claims is recoverable.

It self-heals if the user explicitly re-runs `purge` (the Starfield tail calls
`restore_ini` unconditionally and the row is still present), but crash recovery alone does
not restore it.

**Fix:** make the stranded state observable/recoverable. Simplest: in `ini_drift`, when the
manifest is empty but a `vanilla_backup` row for `INI_FILENAME` still exists, treat it as
drift (an INI that should have been restored). Or restore the INI from the same
manifest/journal-derived signal `recover_on_launch` already uses, rather than gating solely
on `list_deployed_files`. (Narrow window; low likelihood but a genuine reversibility gap in
a purge path that is supposed to be fully journal-recoverable.)

### WR-02: UTF-16 BOM is detected but the body is parsed as bytes — a UTF-16 INI with a real user value is silently clobbered

**File:** `crates/deploy/src/gameconfig.rs:145-153, 237-262, 311-335`
**Issue:**
`detect_bom` explicitly recognizes UTF-16 LE/BE BOMs and preserves them as a prefix
(gameconfig.rs:147-149), signalling intent to support UTF-16. But `merge_body` /
`current_resource_value` scan the post-BOM body as ASCII bytes (`is_archive_header`,
`split_key`, `=` search). In a UTF-16 file every ASCII char is interleaved with `0x00`, so
`[Archive]` and `sResourceDataDirsFinal=<value>` never match. Consequences:

- `detect_conflict` returns `None` for a UTF-16 file that *does* carry a non-empty user
  `sResourceDataDirsFinal` → the Block guard is bypassed, and
- `plan_merge` appends a fresh UTF-8 `[Archive]` block onto the UTF-16 body, producing a
  mixed-encoding, corrupt INI and silently overwriting the user's intent.

Realistically a Bethesda `StarfieldCustom.ini` is ANSI/UTF-8 (Assumption A2), so likelihood
is low — but the code advertises UTF-16 awareness it does not deliver, which is exactly the
kind of encoding edge the byte-fidelity requirement (SFINI-03) calls out.
**Fix:** either drop the UTF-16 BOM handling and treat a UTF-16 BOM as "not a byte-oriented
INI we own" (surface a conflict / refuse to edit), or genuinely decode UTF-16 before
parsing. Refusing (returning `Conflict`/`NotPristine`) is the safe, lazy choice given the
domain — never append a UTF-8 block into a UTF-16 file.

## Info

### IN-01: `DeployError::IniConflict` is defined but never constructed (dead variant)

**File:** `crates/deploy/src/error.rs:56-60`
**Issue:** The `IniConflict { current_value }` error variant is documented as how a
non-empty user value should surface (and the review focus references it), but the engine
actually surfaces conflicts as the `IniOutcome::Blocked { current_value }` *success* value
(gameconfig.rs:420-422) and never constructs `DeployError::IniConflict`. The variant is
dead code. Returning `Blocked` as an `Ok` outcome is a defensible design (the deploy tail
must not fail on a conflict), so the fix is to delete the unused variant rather than wire it
up.
**Fix:** remove `DeployError::IniConflict` (and its rustdoc) unless a future caller needs an
`Err`-typed conflict.

### IN-02: Frontend `VerifyReport` type omits `ini_drift` (and `orphan_dirs`)

**File:** `frontend/src/lib/api.ts:96-101`
**Issue:** The Rust `VerifyReport` now carries `ini_drift: Option<IniDrift>` and
`orphan_dirs` (verify.rs:35-57), but the TS interface only lists
`missing/changed/orphans/pristine`. Extra JSON fields are harmlessly ignored, and the
`pristine` flag is correct, so there is no runtime bug — but the UI cannot tell the user
*why* a Starfield deploy is non-pristine (a missing/changed INI vs a Data/ orphan). Since
this phase introduced `ini_drift`, mirror it in the TS type so the UI can distinguish the
INI-drift case.
**Fix:** add `ini_drift: "Missing" | "Changed" | null;` (and `orphan_dirs: string[];`) to
the `VerifyReport` interface.

---

_Reviewed: 2026-07-08T10:06:51Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
