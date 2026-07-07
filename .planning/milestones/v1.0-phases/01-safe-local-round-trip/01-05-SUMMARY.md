---
phase: 01-safe-local-round-trip
plan: 05
subsystem: deployment-integrity-layer
status: complete
tags: [rust, casefold, wine-case-sensitivity, casing-map, verify-repair, drift-detection, blake3, walkdir, env-04, deploy-07, deploy-08]
dependency_graph:
  requires:
    - "crates/deploy engine (deploy/purge/recover_on_launch, DeployReport/RecoveryReport, probe FsCaps, resolve_target/deploy_root) — Plan 04"
    - "crates/steam canonical_data_casing(install_dir) -> CasingMap + CasingMap::canonical_dir — Plan 02"
    - "crates/store list_deployed_files(appid) + vanilla_for(appid, target_rel) — Plan 01"
    - "crates/core FileEntry { target_rel, method, hash, pre_existing } + Game — Plan 01"
  provides:
    - "crates/deploy::normalize_to_canonical(target_rel, &CasingMap) -> PathBuf — rewrites mod-path dir casing to the game's canonical Data/ casing (DEPLOY-08)"
    - "crates/deploy::{verify, repair} + VerifyReport { missing, changed, orphans, pristine } / RepairReport { restored_missing, restored_changed, orphans } (DEPLOY-07)"
    - "crates/deploy::FsWarning { CrossDevice, NotCasefolded } + DeployReport.fs_warnings (ENV-04 warning half)"
    - "recover_on_launch now returns RecoveryReport { replayed, drift } — auto verify after journal replay"
  affects:
    - "Plan 06 (tauri): commands surface DeployReport.fs_warnings + VerifyReport drift to the UI; call recover_on_launch at startup"
tech_stack:
  added:
    - "steam path-dep added to deploy (CasingMap consumer; no cycle — steam does not depend on deploy)"
  patterns:
    - "Casing normalization ALWAYS runs regardless of the best-effort Casefold probe (A6) for portability"
    - "Normalized relpath is recorded in the manifest so purge keys off the same path (round-trip pristine preserved)"
    - "verify is strictly read-only; repair touches ONLY manifest-recorded paths — orphans reported, never deleted (Pitfall 4 / T-01-16)"
    - "full-pristine-or-report: recover_on_launch auto-runs verify after replay so an abnormal exit always yields a drift status"
key_files:
  created:
    - crates/deploy/src/casefold.rs
    - crates/deploy/src/verify.rs
    - crates/deploy/tests/casefold_normalize.rs
    - crates/deploy/tests/verify_drift.rs
  modified:
    - crates/deploy/src/engine.rs
    - crates/deploy/src/lib.rs
    - crates/deploy/Cargo.toml
    - Cargo.lock
decisions:
  - "normalize_to_canonical maps DIRECTORY components only (CasingMap records dirs); leaf filenames are preserved verbatim, and a game-absent (mod-introduced) dir keeps the mod's casing — there is no canonical answer to defer to"
  - "A leading Data segment is matched case-insensitively and rewritten to CasingMap.data_dir_name; the remaining lookup keys are taken relative to Data/ (the map is Data/-rooted)"
  - "verify/repair reconstruct a re-deploy source as game.staging_dir.join(target_rel) — the same contract journal::replay uses (Plan 04); a missing staged source is skipped, never fabricated"
  - "orphan = on-disk file under the deploy root that is neither a manifest-recorded target nor a vanilla-backed original; reported only (never deleted)"
  - "NotCasefolded warning fires when the probe casefold verdict is Off OR Unknown (A6: absence of a confirmed On means we cannot rely on the kernel for case-insensitivity)"
metrics:
  duration_min: 12
  tasks_completed: 2
  files_created: 4
  files_modified: 4
  tests_passing: 33
  completed: 2026-06-20
---

# Phase 1 Plan 05: Deployment Integrity Layer Summary

Completed the deployment engine's integrity layer on top of Plan 04's reversible round-trip spine: case-sensitivity normalization (DEPLOY-08) so mixed-case Bethesda mod paths load under Wine, a read-only verify/repair drift pass (DEPLOY-07) that hash-diffs the per-game manifest against the on-disk `Data/` tree and auto-runs after an abnormal exit, and the unsafe-filesystem warning half of ENV-04 surfaced through `DeployReport.fs_warnings`.

## What Was Built

- **Task 1 — Casefold normalization + deploy hook + fs warnings** (`cb75150`): `casefold.rs` implements `normalize_to_canonical(target_rel, &CasingMap)` — it splits the mod relpath, rewrites each DIRECTORY component to the game's real on-disk casing via `CasingMap::canonical_dir` (per-segment, using the canonical leaf of the matched path), preserves the leaf filename verbatim, keeps a game-absent mod-introduced dir's casing, and rewrites a leading `Data` segment to `CasingMap.data_dir_name`. `deploy()` now derives the per-game `CasingMap` once (`steam::canonical_data_casing`, defaulting to an empty map if the tree can't be walked) and normalizes each staged relpath BEFORE target resolution, recording the normalized path in the manifest so purge stays byte-for-byte pristine. `FsWarning { CrossDevice, NotCasefolded }` + `DeployReport.fs_warnings` are derived from the existing `probe()` `FsCaps` (ENV-04 warning half). `casefold_normalize` integration test proves `TEXTURES->Textures`, nested `TEXTURES/ACTORS->Textures/Actors`, leaf-casing preservation, game-absent passthrough, and the `Data/`-rooted case.
- **Task 2 — verify/repair drift + auto-run after abnormal exit** (`40589e9`): `verify.rs` implements `verify(store, game) -> VerifyReport { missing, changed, orphans, pristine }` — strictly read-only: for each recorded `FileEntry` it checks on-disk existence (else `missing`) and blake3-hashes it (else `changed` when the digest differs from the recorded hash), then walks the deploy root and reports any file that is neither a managed target nor a vanilla-backed original as an `orphan`. `repair(store, game) -> RepairReport` re-deploys `missing` and restores `changed` from staging idempotently (`apply_idempotent`, source `game.staging_dir.join(target_rel)`) and surfaces orphans WITHOUT deleting them. `recover_on_launch` now auto-runs `verify` after journal replay and returns `RecoveryReport { replayed, drift }`. `verify_drift` integration test proves pristine on a clean deployment, and that a deleted file is `missing`, a mutated file is `changed`, an extra unrecorded file is an `orphan` that repair surfaces-but-never-deletes, and that repair restores both missing+changed back to pristine.

## Interfaces Provided (contract for Plan 06)

- `deploy::normalize_to_canonical(target_rel: &Path, casing: &steam::CasingMap) -> PathBuf` (DEPLOY-08).
- `deploy::verify(store, game) -> Result<VerifyReport>`; `VerifyReport { missing: Vec<PathBuf>, changed: Vec<PathBuf>, orphans: Vec<PathBuf>, pristine: bool }`.
- `deploy::repair(store, game) -> Result<RepairReport>`; `RepairReport { restored_missing: usize, restored_changed: usize, orphans: Vec<PathBuf> }`.
- `deploy::FsWarning { CrossDevice, NotCasefolded }`; `DeployReport.fs_warnings: Vec<FsWarning>` (ENV-04).
- `deploy::recover_on_launch(store, game) -> Result<RecoveryReport>`; `RecoveryReport { replayed: usize, drift: VerifyReport }`.

## Verification

- `cargo test -p nextwist-deploy` — **33 passed** (10 lib + 5 casefold_normalize + 5 verify_drift + 2 crash_recovery + 2 fs_probe + 4 method_ladder + 3 round_trip_pristine + 2 vanilla_restore), 0 failed.
- `cargo test --workspace` (incl. all doctests) — **77 passed, 0 failed** (core 2 + deploy 33 + extract 3 + zip_slip 4 + steam 13 + resolve_game 3 + store 13 + testkit 6).
- `cargo clippy --workspace --all-targets` — **0 warnings** (exit 0).
- `cargo deny check bans` — **bans ok** (unrar ban active; the new steam path-dep introduces no banned crates).
- Acceptance checks confirmed: mixed-case dir components rewritten to canonical casing while leaf + mod-new dirs preserved; deploy normalizes before linking and surfaces `CrossDevice`/`NotCasefolded`; verify classifies orphan/missing/changed and reports pristine on a clean deployment; repair restores managed files and only reports orphans; recover_on_launch runs verify after replay.

## Deviations from Plan

None — plan executed as written. Two integration-shape choices worth noting (both within the plan's stated contracts):

- The `verify_drift` test stages the mod at `game.staging_dir` itself (so a `Data/`-rooted relpath's source is `staging_dir/Data/...`), matching the recover/repair staging-root contract documented in the Plan 04 SUMMARY (`journal::replay` reconstructs the source identically). This is the intended integration shape, not a deviation.
- The plan's `<verify>` blocks use `cargo test -p deploy`; the crate's package name is `nextwist-deploy` (lib name `deploy`), so the equivalent `-p nextwist-deploy` was used. No behavioral difference.

## Threat Mitigations Applied

- **T-01-16 (repair auto-deletes an unmanaged file mistaken for an orphan):** `repair` touches ONLY manifest-recorded paths (re-deploy missing / restore changed); orphans are collected and returned but never removed. The `verify_drift` test asserts the planted orphan file still exists and is byte-identical after repair.
- **T-01-17 (silent case mismatch → mods load with no effect):** every mod-path directory component is normalized to the game's canonical `Data/` casing at deploy time (always, regardless of the best-effort casefold probe); the `NotCasefolded` warning is surfaced so the user knows case-sensitivity is being handled by path normalization rather than the kernel.
- **T-01-18 (undetected external drift after a crash):** `verify` hash-diffs the manifest against disk and `recover_on_launch` auto-runs it after journal replay; provenance (manifest vs vanilla ledger vs unmanaged) distinguishes ours / vanilla / orphan so unmanaged files are reported, not clobbered.

## Notes for Downstream Plans

- **Plan 06 (Tauri commands)** should: call `recover_on_launch` once at startup and surface `RecoveryReport.drift` if not pristine; surface `DeployReport.fs_warnings` (CrossDevice / NotCasefolded) to the user before/at deploy; expose `verify`/`repair` as commands and present `VerifyReport`/`RepairReport.orphans` as a review list (orphans are never auto-deleted — the UI decides).
- `normalize_to_canonical` is idempotent and pure; Plan 06 does not need to call it directly (deploy applies it internally), but it is `pub` for any UI preview of the resolved target path.
- The orphan definition is provenance-based (not-manifest AND not-vanilla-backed). On a real, fully-populated vanilla `Data/` tree the untouched vanilla files would be reported as orphans-to-review until a vanilla baseline is recorded — this is the SAFE direction (report, never delete). A first-class vanilla-baseline snapshot to suppress those benign orphans is a Phase-2 enhancement.

## Known Stubs

None. `casefold.rs` and `verify.rs` are fully implemented and covered by unit + integration tests; no `todo!`/`unimplemented!`/placeholder/TODO/empty-return code remains in the new or modified source (scanned).

## Threat Flags

None — no new security surface beyond the plan's `<threat_model>`. The two trust boundaries (external changes → deployed game tree, and mixed-case mod paths → Wine `open()`) are exactly those in the plan, and T-01-16 through T-01-18 are all mitigated as specified.

## Self-Check: PASSED

- All 4 created files verified present on disk (`crates/deploy/src/{casefold.rs, verify.rs}`, `crates/deploy/tests/{casefold_normalize.rs, verify_drift.rs}`).
- Both task commits verified in git log (`cb75150`, `40589e9`).
- `cargo test -p nextwist-deploy` 33/33 green; `cargo test --workspace` 77/77 green (incl. doctests); clippy 0 warnings; `cargo deny check bans` ok.
