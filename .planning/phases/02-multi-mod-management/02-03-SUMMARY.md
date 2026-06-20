---
phase: 02-multi-mod-management
plan: 03
subsystem: conflict resolution + winner-set deploy + conflict UI
tags: [conflict, resolver, pure-fold, multi-root-deploy, pristine, tauri, svelte, CONF-01, CONF-02, CONF-03, D-01, D-03, D-04]
status: complete
requires:
  - "Plan 02-01 core types (ManagedMod.rank, FileConflict) + store.list_mods/set_mod_rank"
  - "Phase-1 deploy engine (deploy/purge/recover, StagedFiles, journal, backup, testkit pristine harness)"
provides:
  - "deploy::conflict::resolve — pure fold over enabled mods by rank -> Vec<WinnerFile> (one per target_rel) + Vec<FileConflict>"
  - "WinnerFile { mod_id, staging_root, rel } — the multi-root deploy contract (Option A)"
  - "deploy::deploy_winners(store, game, &[WinnerFile]) — multi-root winner-set deploy through the unchanged safe primitive; records winning mod_id (D-03)"
  - "shared deploy::path_guard (guard_within_root / lexical_normalize)"
  - "Tauri commands: list_mods, list_conflicts, set_mod_rank, deploy_winner_set"
  - "Conflict view (UI-SPEC §A): priority list + file-level conflict table + pending banner"
affects:
  - "Plan 02-05 (profile switch) — reuses conflict::resolve + deploy_winners for the deploy half of a switch (RESEARCH Pattern 4)"
tech-stack:
  added: []
  patterns:
    - "Pure fold conflict resolver (BTreeMap<target_rel, Vec<(rank, mod_id, root)>> -> single winner per path; deterministic order)"
    - "Multi-root deploy via per-file (staging_root, rel) WinnerFile, leaving Phase-1 single-root StagedFiles/deploy UNCHANGED"
    - "Shared per-file deploy primitive (deploy_one_file) called by both deploy_inner (single-root) and deploy_winners (multi-root)"
    - "Pending-vs-deployed signature compare in the UI (enabled-mod-ids-in-rank-order) for the D-04 pending banner"
key-files:
  created:
    - crates/deploy/src/conflict.rs
    - crates/deploy/src/path_guard.rs
    - crates/deploy/tests/conflict_redeploy.rs
    - src-tauri/src/commands/conflicts.rs
  modified:
    - crates/deploy/src/engine.rs
    - crates/deploy/src/lib.rs
    - src-tauri/src/commands/mod.rs
    - src-tauri/src/lib.rs
    - frontend/src/lib/api.ts
    - frontend/src/routes/+page.svelte
decisions:
  - "Multi-root contract = Option A: NEW WinnerFile { mod_id, staging_root, rel } + NEW deploy_winners path; StagedFiles + deploy/deploy_inner left byte-identical so all Phase-1 callers/tests are unaffected."
  - "Per-file deploy body extracted to a shared deploy_one_file helper (incl. the test-only abort-injection seam) so single-root and multi-root deploy share ONE journaled/backup/method-laddered primitive — the safe engine is never duplicated or bypassed."
  - "guard_within_root/lexical_normalize promoted from engine.rs into a shared path_guard module and reused by the resolver for per-winner path-escape rejection (T-02-06), rather than copy-pasting the guard."
  - "Resolver tie-break: equal-rank providers are broken by mod_id ascending for a stable, repeatable winner."
  - "Added list_mods Tauri command (Rule 2) because UI-SPEC §A.1 priority list + §A.2 winner-name mapping cannot render without a mod-list source; kept in conflicts.rs (a single store.list_mods read, no business logic)."
metrics:
  duration_min: 27
  tasks: 3
  files: 10
  tests_added: 8
  completed: 2026-06-21
---

# Phase 2 Plan 03: Conflict Vertical Slice Summary

A user with multiple enabled mods can SEE file-level conflicts, SET priority so a chosen mod deterministically wins, and DEPLOY the winner set — end-to-end (store -> pure-fold resolver -> multi-root deploy through the UNCHANGED safe engine -> thin Tauri commands -> Svelte Conflict view), with the round-trip-pristine guarantee preserved (including across a priority-change redeploy).

## What Was Built

- **`deploy::conflict::resolve`** (`crates/deploy/src/conflict.rs`) — a pure in-memory fold over enabled `ModInput { mod_id, staging_root, rank }`. Its only I/O is walking each mod's staged tree. Builds `BTreeMap<target_rel, Vec<(rank, mod_id, staging_root)>>`, sorts each path's providers by rank ascending (tie-break by mod_id), and emits the winner. Output: `Vec<WinnerFile>` (exactly one per `target_rel` — UNIQUE-safe, Pitfall 3) + `Vec<FileConflict>` (only for contested paths, providers > 1). Deterministic order via the BTreeMap. Each winner path is asserted lexically inside its own staging root (T-02-06 -> `DeployError::PathEscape`).
- **`deploy::deploy_winners`** (`crates/deploy/src/engine.rs`) — deploys the multi-root winner set. Probes each winner against its OWN staging root, then calls the same per-file primitive as Phase-1 deploy; records the winning `mod_id` as `FileEntry.source_mod` (D-03). The safe engine (journal/backup/method-ladder) is never bypassed.
- **Shared `deploy_one_file` helper** — the per-file deploy body (casing-normalize, guard, hash, journal pending, backup-before-overwrite, idempotent op, manifest + done flip) extracted so `deploy_inner` (single-root) and `deploy_winners` (multi-root) call ONE primitive. The test-only abort-injection seam lives inside it, preserving the crash-recovery centerpiece's exact window.
- **Shared `path_guard` module** — `guard_within_root`/`lexical_normalize` promoted out of engine.rs; reused by the resolver.
- **Tauri commands** (`src-tauri/src/commands/conflicts.rs`) — `list_mods`, `list_conflicts`, `set_mod_rank` (persist-only, pending until Deploy — D-04), `deploy_winner_set`. Each is a thin adapter: `require_game` + one headless call + `.map_err(boundary_err)`.
- **Conflict view** (`frontend/src/routes/+page.svelte`, UI-SPEC §A) — mod priority list with `▲▼` keyboard/click reorder (rank swap, no DnD-only), file-level conflict table with the `●` accent winner dot + winner name (600), and a Warning-styled "Changes pending" banner whose Deploy button takes the Accent color when pending / neutral "Up to date" when in sync (D-04). Empty-state copy per the UI-SPEC.

## The StagedFiles Multi-Root Contract Decision (flagged per plan)

**Chosen: Option A.** `StagedFiles { staging_root, files }` carries ONE staging root, but multi-mod winners come from DIFFERENT roots. Rather than extend `StagedFiles` (Option B), I added a sibling per-file type and a sibling deploy path:

```rust
// crates/deploy/src/conflict.rs
pub struct WinnerFile { pub mod_id: i64, pub staging_root: PathBuf, pub rel: PathBuf }
pub fn resolve(mods: &[ModInput]) -> Result<(Vec<WinnerFile>, Vec<FileConflict>), DeployError>;

// crates/deploy/src/engine.rs
pub fn deploy_winners(store: &Store, game: &Game, winners: &[WinnerFile]) -> Result<DeployReport, DeployError>;
```

`StagedFiles`, `deploy`, and `deploy_inner` are byte-for-byte unchanged in public behavior, so every Phase-1 caller and test is unaffected (verified: all Phase-1 deploy tests still green). **Plan 02-05's profile switch reuses `conflict::resolve` + `deploy_winners` for the deploy half of a switch.**

## Safety Invariant Proven (BLOCKING-PRISTINE — Task 2)

`crates/deploy/tests/conflict_redeploy.rs` (testkit DIR_SENTINEL harness):
- `conflict_winner_set_deploys_unique_and_pristine` — two mods contest `Data/shared.esp`; resolve+`deploy_winners` places exactly one owner per path (no `deployed_file` UNIQUE violation), the winning bytes land, the manifest records the winning mod id (D-03), and a fresh-handle `purge` returns the install **byte-for-byte pristine**.
- `rank_change_redeploy_stays_pristine` — deploy (A wins) -> purge (pristine) -> flip ranks (B wins) -> redeploy -> purge -> still pristine. Proves redeploy after a priority change is fully reversible (Pitfall 4 across a switch).

Multi-mod winners are staged at per-mod roots OUTSIDE `game.staging_dir`, exercising the real multi-root path.

## Deviations from Plan

### Auto-fixed / scope additions

**1. [Rule 2 - Missing critical functionality] Added `list_mods` Tauri command**
- **Found during:** Task 3 (Conflict view).
- **Why:** UI-SPEC §A.1 (priority list rows: name/enabled/rank) and §A.2 (mapping winner/provider mod ids to names) cannot render without a mod-list source, and no `list_mods` command existed (Phase-1 mods.rs only has `install_archive`). A priority list with no data source would be a non-functional stub.
- **Fix:** Added a thin `list_mods` adapter (single `store.list_mods` read) to `conflicts.rs` and registered it. Within the plan's backend file set (conflicts.rs / mod.rs / lib.rs); zero business logic.
- **Commit:** `1b8a68d`.

No other deviations — resolver, deploy path, tests, commands, and view followed the plan.

## Known Limitations (documented, not stubs)

- **Enabled toggle is read-only in the priority list.** UI-SPEC §A.1 shows an "enabled toggle", but no `set_mod_enabled` command is in this plan's scope (per-profile enable/membership is Plan 02-05's `profile_mod`). The view renders enabled status as a read-only indicator and the resolver folds over the enabled set; wiring an enable/disable mutation is deferred to the profile slice. This is a deliberate scope boundary, not an unwired stub — the conflict slice (see/set-priority/deploy) is fully functional.
- **Crash mid-`deploy_winners` recovery rolls BACK to pristine, not forward.** Phase-1 `replay_deploy` reconstructs a pending op's source from `game.staging_dir/<target_rel>`; multi-root winners live under per-mod roots, so a crash mid-winner-deploy that the journal replays will (not finding the source at `game.staging_dir`) roll the interrupted file BACK and restore vanilla — always converging to pristine (the safety invariant holds), then the user re-deploys. This is safe and reversible; forward-recovery of a multi-root winner is a future enhancement (would require the journal to record the per-file source root). Phase-1 single-root crash-recovery behavior is unchanged.

## Verification Results

- `cargo test -p nextwist-deploy --lib conflict` — 6 passed (resolver: lower-rank wins, no-conflict case, rank-flip, mandatory no-duplicate-target_rel, path-escape rejection, empty-mod no-op).
- `cargo test -p nextwist-deploy --test conflict_redeploy` — 2 passed (CONF-03 unique+pristine, rank-change redeploy pristine).
- `cargo test --workspace` — 119 passed, 0 failed (Phase-1 + 02-01/02-02 baseline intact; additive, no regressions).
- `cargo build -p nextwist` — compiles.
- `cd frontend && npm run check` — 141 files, 0 errors, 0 warnings.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.

## Threat Mitigations (from plan threat_model)

- **T-02-06** (winner path EoP) — mitigated: each winner asserted inside its staging root via shared `guard_within_root`; unit-tested (`path_escape_winner_rejected`).
- **T-02-07** (two owners for one path) — mitigated: resolver dedups to one winner per target_rel before deploy; `never_emits_duplicate_target_rel` unit test + conflict_redeploy asserts no UNIQUE violation.
- **T-02-08** (multi-mod deploy+purge integrity) — mitigated: conflict_redeploy asserts byte-for-byte pristine after deploy+purge AND after a rank-change redeploy.
- **T-02-09** (IPC args) — accept: single-user desktop; adapters validate appid via `require_game`; mod_id/rank are bounded ints treated as data.

No new security surface introduced beyond the threat_model.

## Self-Check: PASSED

- FOUND: crates/deploy/src/conflict.rs
- FOUND: crates/deploy/src/path_guard.rs
- FOUND: crates/deploy/tests/conflict_redeploy.rs
- FOUND: src-tauri/src/commands/conflicts.rs
- FOUND commit 8d5db51 (Task 1), 936d78a (Task 2), 1b8a68d (Task 3)
