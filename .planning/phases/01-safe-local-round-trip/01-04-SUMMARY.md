---
phase: 01-safe-local-round-trip
plan: 04
subsystem: reversible-deployment-engine
status: complete
tags: [rust, reflink-copy, blake3, walkdir, exdev, btrfs, operation-journal, crash-recovery, proptest, vanilla-backup, crown-jewel]
dependency_graph:
  requires:
    - "crates/core domain types (Game, FileEntry, DeployMethod, StoreError) — Plan 01"
    - "crates/store op_journal/manifest/vanilla facades (begin_op/mark_done/pending_ops, record/list/remove_deployed_file, record_vanilla/backup_key_exists/vanilla_for) — Plan 01"
    - "crates/testkit byte-for-byte pristine assertion (snapshot_tree + assert_trees_identical) — Plan 01"
    - "extract::StagedMod { staging_root, files } shape (mirrored as deploy::StagedFiles) — Plan 03"
  provides:
    - "crates/deploy::probe(staging, game_data) -> FsCaps (st_dev, empirical reflink+hardlink, casefold)"
    - "crates/deploy::{choose_method, apply_idempotent} — reflink->hardlink->symlink->copy ladder with EXDEV downgrade"
    - "crates/deploy::{deploy, deploy_with_abort, purge, recover_on_launch} engine + DeployReport/PurgeReport/RecoveryReport"
    - "crates/deploy::StagedFiles { staging_root, files } — deploy worklist (Plan 06 maps StagedMod into this)"
    - "intent-before-act journal protocol + idempotent crash-recovery replay (journal::replay)"
    - "backup-before-overwrite into content-addressed vanilla store (backup::{backup_vanilla_if_absent, restore_vanilla})"
  affects:
    - "Plan 05 (casefold/verify): builds DEPLOY-07 verify/repair drift + DEPLOY-08 casefold on top of this engine"
    - "Plan 06 (tauri): thin commands wrap deploy/purge/recover; map extract::StagedMod -> deploy::StagedFiles"
tech_stack:
  added:
    - "reflink-copy 0.1.30 (reflink + check_reflink_support; Linux verdict is empirical, not check_reflink_support)"
    - "blake3 1.8 (content-addressed vanilla store keys + source hashes)"
    - "walkdir 2.5, tracing 0.1, thiserror 2 (deploy deps)"
    - "proptest 1.11 + tempfile 3.27 + testkit (deploy dev-deps)"
  patterns:
    - "Intent-before-act operation journal: durable pending row (synchronous=FULL) BEFORE syscall; manifest row + done-flip AFTER"
    - "Idempotent file ops (remove-if-present-then-create) so journal replay after a crash is always safe"
    - "Per-target empirical fs probe; method ladder downgrades on io::ErrorKind::CrossesDevices / errno 18"
    - "Backup-before-overwrite into content-addressed (blake3) vanilla store; is_ours decided from manifest, never guessed"
    - "Purge is manifest-driven ONLY (never a directory scan); orphans reported, not deleted (Pitfall 4)"
    - "Per-file deployment only; never a directory symlink (Pitfall 2)"
key_files:
  created:
    - crates/deploy/Cargo.toml
    - crates/deploy/src/lib.rs
    - crates/deploy/src/error.rs
    - crates/deploy/src/probe.rs
    - crates/deploy/src/method/mod.rs
    - crates/deploy/src/method/reflink.rs
    - crates/deploy/src/method/hardlink.rs
    - crates/deploy/src/method/symlink.rs
    - crates/deploy/src/method/copy.rs
    - crates/deploy/src/journal.rs
    - crates/deploy/src/backup.rs
    - crates/deploy/src/engine.rs
    - crates/deploy/tests/fs_probe.rs
    - crates/deploy/tests/method_ladder.rs
    - crates/deploy/tests/vanilla_restore.rs
    - crates/deploy/tests/round_trip_pristine.rs
    - crates/deploy/tests/crash_recovery.rs
  modified:
    - Cargo.lock
    - .gitignore
decisions:
  - "A1 RESOLVED: ReflinkSupport variants are Supported/NotSupported/Unknown (reflink-copy 0.1.30 source); on Linux check_reflink_support always returns Unknown, so the authoritative reflink verdict is an empirical throwaway reflink_copy::reflink, mirroring the throwaway hardlink probe"
  - "Recovery policy: a pending deploy rolls FORWARD when the staged source is locatable at staging_dir/<target_rel>, else rolls BACK to pristine — both reach a consistent state; the journal does not persist the staging root (Phase-1 single-mod), so forward completion relies on the staging_dir/<rel> contract"
  - "Vanilla store lives at <staging_dir>/../originals/<appid>/<blake3> — NexTwist app-managed area, never inside the game tree"
  - "guard_within_root enforces V4: all targets resolved under <install_dir>/Data via lexical containment (works for not-yet-created paths)"
  - "deploy::StagedFiles introduced (not a dependency on extract) so the engine stays consumable by Plan 06 which already holds a StagedMod"
metrics:
  duration_min: 15
  tasks_completed: 3
  files_created: 17
  tests_passing: 18
  completed: 2026-06-20
---

# Phase 1 Plan 04: Reversible Deployment Engine Summary

Built `crates/deploy` — the crown jewel: a per-target filesystem-capability probe, a reflink → hardlink → symlink → copy method ladder chosen per-target with EXDEV/`CrossesDevices` fallback, an intent-before-act operation-journal protocol with idempotent crash-recovery replay, backup-before-overwrite into a content-addressed blake3 vanilla store, and the `deploy()` / `purge()` / `recover_on_launch()` orchestration — proven by the crash-recovery centerpiece and the round-trip-pristine proptest, with the real cross-device EXDEV path exercised on the dev btrfs filesystem.

## What Was Built

- **Task 1 — Per-target probe + EXDEV-aware method ladder** (`588498a`): `probe.rs` (`FsCaps { same_device, reflink, hardlink_ok, casefold }`) compares `st_dev` and runs empirical throwaway `reflink` AND `hard_link` probes between the staging and game-data dirs (the authoritative btrfs-subvolume EXDEV backstop), plus a best-effort ext4 casefold read via `FS_IOC_GETFLAGS` degrading to `Unknown`. `method/` defines the `DeploymentMethod` trait and Reflink/Hardlink/Symlink/Copy (per-file only — never a directory symlink), with `choose_method` selecting the strongest applicable primitive and `apply_idempotent` (remove-if-present-then-create) downgrading on `CrossesDevices`/errno 18. `fs_probe` + `method_ladder` integration tests run a REAL cross-device pair (dev btrfs vs `/tmp` tmpfs), proving the EXDEV downgrade and per-file round-trip.
- **Task 2 — Journal, backup, deploy/purge/recover engine** (`8869ae3`): `backup.rs` copies a pre-existing non-ours file into `<originals>/<appid>/<blake3>` (content-addressed dedupe) and restores it byte-for-byte; `is_ours` is manifest-driven and stray symlinks are never copied through. `journal.rs` implements the intent-before-act protocol over the store primitives — `begin_*` writes a durable `pending` row before the syscall, `finish_*` writes the manifest row + flips to `done` after, and `replay` rolls every non-`done` row forward or back to a consistent state. `engine.rs` wires `deploy()` (probe → journal → backup → idempotent apply → finish, zero originals modified in place), `purge()` (manifest-driven only, restores vanilla, reports orphans), and `recover_on_launch()`. `vanilla_restore` test proves a replaced vanilla file is restored byte-for-byte and a pure-add takes no backup.
- **Task 3 — round_trip_pristine + crash_recovery centerpiece** (`6648c1e`): `round_trip_pristine.rs` is a 48-case proptest over randomized vanilla + mod trees (pure-adds AND overwrites) asserting deploy→purge is byte-for-byte pristine with no orphans and an empty manifest/journal, plus explicit empty-mod and all-overwrite edge cases. `crash_recovery.rs` (CENTERPIECE) drives `deploy_with_abort` to commit `pending` rows and place files but abort before the done-flip, then a FRESH store handle runs `recover_on_launch` (zero pending rows after) and a follow-up `purge` restores byte-for-byte pristine — sweeping the abort point `0..4` across the whole crash window, plus a forward-recovery test.

## Interfaces Provided (contract for Plans 05–06)

- `deploy::probe(staging, game_data) -> io::Result<FsCaps>`; `FsCaps { same_device, reflink, hardlink_ok, casefold: Casefold{On|Off|Unknown} }` (ENV-04).
- `deploy::choose_method(&FsCaps) -> DeployMethod`; `deploy::apply_idempotent(tag, src, dst) -> Result<DeployMethod, DeployError>` (DEPLOY-05).
- `deploy::deploy(store, game, &StagedFiles) -> Result<DeployReport>` and `deploy_with_abort(.., abort_after)` (test seam); `DeployReport { deployed, backed_up, methods }`.
- `deploy::purge(store, game) -> Result<PurgeReport>`; `PurgeReport { removed, restored, orphans }` (manifest-driven, DEPLOY-01/02/03/04).
- `deploy::recover_on_launch(store, game) -> Result<RecoveryReport>`; `RecoveryReport { replayed }` (DEPLOY-06).
- `deploy::StagedFiles { staging_root, files }` — Plan 06 maps `extract::StagedMod { staging_root, files }` into this directly.
- `deploy::DeployError` — `Store`/`Io{path,source}`/`PathEscape`/`NotPristine`/`Aborted`.

## Verification

- `cargo test -p nextwist-deploy` — **18 passed** (5 lib + 2 crash_recovery + 2 fs_probe + 4 method_ladder + 3 round_trip_pristine + 2 vanilla_restore), 0 failed.
- `cargo test --workspace` (incl. all doctests) — **ALL GREEN (exit 0)**: 2 core + 5 deploy lib + 13 deploy integration + 3 extract + 4 zip_slip + 13 steam + 3 resolve_game + 13 store + 6 testkit, plus every Doc-tests target.
- `cargo clippy --workspace --all-targets` — **0 warnings**.
- `cargo deny check bans` — **bans ok** (unrar ban active; new deploy deps respect it).
- **Phase gate exercised on the dev btrfs filesystem:** `fs_probe`/`method_ladder` ran a genuine cross-device pair (dev tree btrfs `st_dev=50` vs `/tmp` tmpfs `st_dev=49`), so the EXDEV downgrade and per-file round-trip are proven on the hardest fs case, not skipped. The crash_recovery + round_trip_pristine tests pass on the tmpfs/tempdir CI path; their comments flag the manual dev-btrfs re-run per VALIDATION.md.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Empty-mod deploy is now a clean no-op**
- **Found during:** Task 3 (`empty_mod_edge_case_is_pristine` and a proptest case with zero mod files)
- **Issue:** `deploy()` probed `staging_root` before the file loop; for a zero-file mod the staging root is never materialized, so `probe`'s `fs::metadata(staging)` returned `NotFound` and aborted the deploy.
- **Fix:** Return an empty `DeployReport` early when `staged.files.is_empty()` (a valid no-op deploy) before probing; tag probe errors with the staging path rather than the data dir.
- **Files modified:** crates/deploy/src/engine.rs
- **Commit:** 6648c1e

**2. [Rule 1 - Test-data bug] Disjoint dir/file alphabets in the round-trip generator**
- **Found during:** Task 3 (proptest shrank to `Data/b.nif` used as both a file and a directory)
- **Issue:** The initial path generator drew all segments from one alphabet, so it could declare the same name as both a leaf file and an intermediate directory within one tree — a tree no real, extract-validated mod can produce, and which `write_files` cannot materialize.
- **Fix:** Generate paths from DISJOINT directory and file alphabets so a name is never both a file and a directory. This is a test-data correctness fix, not a product-code change. The stale proptest-regressions seed (capturing the now-impossible case) was removed and `**/*.proptest-regressions` gitignored (`916664e`).
- **Files modified:** crates/deploy/tests/round_trip_pristine.rs, .gitignore
- **Commit:** 6648c1e, 916664e

### Plan-flagged TODOs resolved at code time

- **A1 (ReflinkSupport variant names):** Confirmed against reflink-copy 0.1.30 source — `Supported`/`NotSupported`/`Unknown`. Crucially, `check_reflink_support` is Windows-only and returns `Ok(ReflinkSupport::Unknown)` on Linux, so the probe does not depend on it for the verdict — it runs a real throwaway `reflink_copy::reflink` (the actual FICLONE path) for the authoritative Linux answer, alongside the throwaway hardlink probe. No TODO left in code.
- **7z/sevenz signatures (A3):** Out of this plan's scope — already settled in Plan 03 (`extract`); `deploy` consumes only `StagedMod`'s shape.

## Threat Mitigations Applied

- **T-01-11 (crash-induced vanilla loss):** backup-before-overwrite into the content-addressed store + intent-before-act journal + idempotent replay — proven by the `crash_recovery` centerpiece (abort 0..4 → recover → purge → pristine).
- **T-01-12 (non-atomic manifest → orphans → non-pristine purge):** journal intent-before-act; `purge` driven ONLY by `list_deployed_files` (never a directory scan); orphans reported in `PurgeReport.orphans`, not deleted.
- **T-01-13 (overwrite vanilla in place, unrecoverable):** every pre-existing non-ours file is backed up before overwrite; reflink (independent inode) preferred over hardlink; staged tree is read-only (from Plan 03).
- **T-01-14 (Steam update writes through a directory symlink):** deploy is strictly per-file; `SymlinkMethod` symlinks individual files only, with a test asserting no directory symlink is created.
- **T-01-15 (writes outside the resolved game dir):** `guard_within_root` resolves all targets under `<install_dir>/Data` via lexical containment and rejects escapes with `DeployError::PathEscape`; the vanilla store lives under app-managed dirs.

## Notes for Downstream Plans

- **Plan 05** adds DEPLOY-07 (verify/repair drift: hash-diff manifest vs disk) and DEPLOY-08 (casefold normalization vs the `steam::CasingMap`) on top of this engine. `FsCaps.casefold` already surfaces the ext4 `+F` warning input. `resolve_target`/`deploy_root` honor the on-disk `Data` casing case-insensitively today; per-segment casefold rewrite is Plan 05's call.
- **Plan 06** wraps `deploy`/`purge`/`recover_on_launch` in thin Tauri commands and must map `extract::StagedMod { staging_root, files }` into `deploy::StagedFiles { staging_root, files }` (identical shape). `recover_on_launch` should be called once at startup before serving UI.
- **Recovery + staging-root contract:** `journal::replay` reconstructs the staged source as `game.staging_dir.join(target_rel)`. For forward recovery to complete an interrupted deploy, the per-game staged tree must be rooted at `game.staging_dir` (so `staging_dir/Data/...` is the source). When the source is not locatable, recovery safely rolls the op BACK to pristine. Persisting the staging root per journal row (to always roll forward) is a Phase-2 enhancement requiring an additive `op_journal` column — intentionally out of scope here.

## Known Stubs

None. Every module is fully implemented and tested; the Task-1 placeholders for `journal.rs`/`backup.rs`/`engine.rs` were replaced with real implementations in Task 2. No `todo!`/`unimplemented!`/placeholder/empty-return code remains (scanned).

## Threat Flags

None — no security surface beyond the plan's `<threat_model>`. The two trust boundaries (staged tree → game Data/, and app-crash → on-disk+DB state) are exactly the ones in the plan, and T-01-11 through T-01-15 are all mitigated as specified.

## Self-Check: PASSED

- All 17 created files verified present on disk (`crates/deploy/{Cargo.toml, src/*, src/method/*, tests/*}`).
- All 4 task/infra commits verified in git log (`588498a`, `8869ae3`, `6648c1e`, `916664e`).
- `cargo test -p nextwist-deploy` 18/18 green; `cargo test --workspace` all green (exit 0, incl. doctests); clippy 0 warnings; `cargo deny check bans` ok.
