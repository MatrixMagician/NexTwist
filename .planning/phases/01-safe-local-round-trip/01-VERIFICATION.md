---
phase: 01-safe-local-round-trip
verified: 2026-06-20T00:00:00Z
status: gaps_found
score: 29/30 must-haves verified
behavior_unverified: 0
overrides_applied: 0
gaps:
  - id: GAP-01
    requirement: DEPLOY-03
    severity: blocker
    title: "purge leaves orphan empty directories — game folder not byte-for-byte pristine"
    discovered_by: "Manual UAT (real Skyrim SE deploy/purge of Perk Point Gain on Skill Increase)"
    evidence: "After install->deploy->purge, Data/ retained 3 empty dirs (Perk Point Gain on Skill Increase/Scripts/Source) with 0 files; deployed_file=0, journal all done."
    root_cause: "crates/deploy/src/engine.rs:241 purge() removes deployed files + restores vanilla backups but never rmdir's the directories deploy created. testkit::snapshot (crates/testkit/src/lib.rs:70) skips non-files, so round_trip_pristine's blake3 file-tree comparison is blind to leftover empty dirs."
    fix_spec:
      - "purge() (and the purge branch of recover_on_launch): after removing each file, walk UP its created directory chain and remove_dir each dir that is now empty, stopping at the first non-empty dir and never above the deploy root (Data/). remove_dir fails safely on non-empty dirs, so pre-existing vanilla dirs are never removed. Prefer recording deploy-created dirs in the manifest for an exact, safe cleanup set."
      - "Strengthen testkit: snapshot directory structure (including empty dirs) and add a tree-shape assertion so round_trip_pristine catches orphan empty dirs. Re-run on tmpfs AND btrfs."
      - "Extend verify/repair (DEPLOY-07) to detect+report (repair: remove) orphan empty dirs under the deploy root."
human_verification:
  - test: "Launch `cargo tauri dev`, let auto-detect find an installed Skyrim SE / FO4 (or add by folder), and confirm the resolved install dir + Proton prefix paths are shown"
    expected: "The managed game appears with correct install_dir and compatdata/<appid>/pfx prefix; add-by-folder works for non-standard/Snap installs"
    why_human: "Requires a real Steam install + GUI/webview; not reproducible headless. The headless resolution path is covered by resolve_game.rs integration tests against synthetic fixtures."
  - test: "Through the GUI, install a local .zip/.7z mod into staging, click Deploy, then Purge"
    expected: "Mod links into Data/, deploy report shows methods + any fs-warnings; Purge returns the game folder to pristine with no orphans"
    why_human: "Exercises the Tauri command adapters + Svelte UI end-to-end; headless engine round-trip is fully covered by round_trip_pristine + crash_recovery automated tests."
  - test: "Deploy a mod, launch the game via real Steam Proton, confirm the mod loads in-game (case-correctness under Wine)"
    expected: "Mod content is visible/active in-game; mixed-case paths resolve under Wine's case-sensitive view"
    why_human: "DEPLOY-08 in-game correctness requires a real Proton launch; not reproducible in CI. Casing normalization is unit-tested (casefold_normalize.rs) but in-game load is empirical."
  - test: "Real Flatpak/Snap Steam root resolution on a Flatpak/Snap-packaged Steam"
    expected: "Flatpak root auto-detected; Snap users fall back to add-by-folder (Snap root intentionally not auto-detected — A2 low confidence)"
    why_human: "Depends on the user's actual Steam packaging; Snap path is low-confidence (documented TODO(A2) in discover.rs). Manual-add fallback is implemented and tested."
---

# Phase 1: Safe Local Round-Trip Verification Report

**Phase Goal:** A user can take a Bethesda game running under Steam Proton and a local mod archive, install and deploy that mod without touching original game files, then fully uninstall it and have the game folder return byte-for-byte to its vanilla state.
**Verified:** 2026-06-20
**Status:** human_needed
**Re-verification:** No — initial verification
**Mode:** mvp (goal is a user-story outcome; the `[outcome]` clause — byte-for-byte pristine reversibility — is the success condition)

## Goal Achievement

The full reversible-deployment safety core is implemented as a headless, Tauri-free Rust workspace (`crates/core`, `store`, `steam`, `extract`, `deploy`, `testkit`) with a thin Tauri shell. Every safety prohibition is a real, executed, passing test — not a claim. The only items routed to human verification are the GUI/in-game round-trip and real Steam-packaging detection, which cannot run headless; the engine the GUI merely adapts is fully covered by automated tests (confirmed re-run during this verification).

### Core-Value Prohibitions (the reason the product exists)

| Prohibition | Verified | Evidence |
| ----------- | -------- | -------- |
| No original game file modified in place | ✓ VERIFIED | `engine.rs::deploy_inner` links staged files into Data/; `backup_vanilla_if_absent` copies any pre-existing original into a content-addressed store BEFORE the file op. `round_trip_pristine` + `vanilla_restore` tests pass (originals restored byte-for-byte). |
| Purge restores byte-for-byte pristine | ✓ VERIFIED | `purge` is manifest-driven (never a directory scan), restores backed-up originals, reports orphans rather than deleting. `round_trip_pristine.rs` proptest (48 cases + 2 edge cases) asserts blake3 tree equality vs vanilla snapshot — passes. |
| Malicious archive entries rejected | ✓ VERIFIED | `zip_slip_rejected.rs` builds 3 real hostile zips (`../` traversal, absolute path, genuine S_IFLNK symlink) → each rejected with `UnsafeEntry`/`SymlinkEntry`, nothing escapes staging; benign control accepted. Passes. |
| Crash mid-deploy recovers to pristine | ✓ VERIFIED | `crash_recovery.rs` CENTERPIECE: `deploy_with_abort` injects an abort at every point (after 0..3 files), drops the store (process "death"), reopens fresh, `recover_on_launch` replays, then `purge` → byte-for-byte pristine. Both tests pass on tmpfs (orchestrator confirms btrfs re-run). |

### Observable Truths

| #   | Truth (from PLAN must_haves) | Status | Evidence |
| --- | ---------------------------- | ------ | -------- |
| 1 | Workspace builds on Rust 2024-edition ≥1.85 pinned via toolchain | ✓ VERIFIED | `rust-toolchain.toml` channel=stable; `Cargo.toml` workspace `edition=2024`, `rust-version=1.85`, members `crates/*` + `src-tauri`. `cargo test --workspace` = 77 passed (orchestrator). |
| 2 | Store opens SQLite (WAL) with managed_game/deployed_file/op_journal/vanilla_backup via refinery V1 | ✓ VERIFIED | `V1__init.sql` defines all 4 tables; `db.rs` sets `PRAGMA journal_mode=WAL` before `embed_migrations!`/refinery runner. |
| 3 | Shared domain types (Game, ManagedMod, FileEntry, DeployMethod, errors) compile & re-export from core | ✓ VERIFIED | `crates/core/src/model.rs` + `error.rs`; consumed by every crate (`nextwist_core::Game` used in tests + engine). |
| 4 | testkit builds fake vanilla+staged trees and asserts blake3 byte-for-byte equality | ✓ VERIFIED | `testkit/src/lib.rs` `snapshot_tree` + `assert_trees_identical` used by round_trip/crash tests. |
| 5 | cargo-deny denies unrar/unrar_sys by name | ✓ VERIFIED | `deny.toml` `[[bans.deny]] name="unrar"` + `unrar_sys`; `cargo deny check bans` → `bans ok`. |
| 6 | Auto-detect Steam games (native+Flatpak) + manual add-by-folder fallback | ✓ VERIFIED | `discover.rs` locate native+Flatpak roots; `resolve.rs::add_game_by_folder` fallback tested. (Snap = manual fallback, see Anti-Patterns.) |
| 7 | Resolve Skyrim SE/FO4 install dir + derive compatdata/<appid>/pfx prefix | ✓ VERIFIED | `resolve.rs` derives prefix; `resolve_game.rs` resolves both AppIDs from synthetic fixtures (3 tests pass). |
| 8 | Only the 2 supported Bethesda AppIDs accepted (allow-list) | ✓ VERIFIED | `SUPPORTED_APPIDS = [489830, 377160]`; `resolve_rejects_unsupported_appid` + `add_game_by_folder_rejects_unsupported` pass. |
| 9 | Per-game canonical Data/ casing map produced for normalization | ✓ VERIFIED | `casing.rs::canonical_data_casing` → `CasingMap`; `maps_mixed_case_data_tree_to_canonical_casing` passes. |
| 10 | .zip/.7z extract to per-mod read-only staging via extract-to-temp-then-move | ✓ VERIFIED | `zip.rs`/`sevenz.rs`/`staging.rs`; `extract_formats.rs` (`zip_extracts_readonly_to_staging`, `sevenz_extracts_readonly_to_staging`) pass. |
| 11 | Crafted malicious archive rejected before any file lands | ✓ VERIFIED | See prohibitions table — `zip_slip_rejected.rs` 4 tests pass. |
| 12 | .rar via system unrar/7z, else clear error; no non-free RAR bundled | ✓ VERIFIED | `rar.rs` `Command::new("unrar")`→`7z`, argv not shell, actionable error when absent; `rar_uses_system_tool_or_reports_missing` passes; deny ban active. |
| 13 | Every extracted entry validated (enclosed_name+canonicalize+reject symlink/abs/..) | ✓ VERIFIED | `validate.rs::enclosed_name`; staging pipeline validates before write. |
| 14 | Deploy links every staged file with zero originals modified in place | ✓ VERIFIED | See prohibitions table. |
| 15 | Every deployed file recorded in manifest; purge removes exactly recorded paths (never scan) | ✓ VERIFIED | `engine.rs::purge` iterates `list_deployed_files`; `round_trip_pristine` asserts empty manifest after purge. |
| 16 | Pre-existing vanilla file backed up to content-addressed store first, restored on purge | ✓ VERIFIED | `backup.rs` blake3 content-addressed; `vanilla_restore.rs` `replaced_vanilla_file_is_restored_byte_for_byte_on_purge` passes. |
| 17 | Method chosen per-target by empirical probe (reflink→hardlink→symlink→copy) + EXDEV fallback | ✓ VERIFIED | `probe.rs` throwaway reflink/hardlink probe (empirical, not Windows-only `check_reflink_support`); `method_ladder.rs` `ladder_downgrades_on_cross_device_exdev` passes. |
| 18 | Interrupted (crash-mid-deploy) op recovers to consistent/pristine via journal replay | ✓ VERIFIED | See prohibitions table — `crash_recovery.rs` CENTERPIECE passes. |
| 19 | After install→purge game folder is byte-for-byte identical to vanilla snapshot | ✓ VERIFIED | See prohibitions table — `round_trip_pristine.rs` proptest passes. |
| 20 | Mixed-case mod path normalized vs canonical Data/ casing at deploy (loads under Wine) | ✓ VERIFIED (headless); in-game load → human | `casefold.rs::normalize_to_canonical` consumes steam `CasingMap`; engine normalizes before linking; `casefold_normalize.rs` passes. In-game Proton load is manual-only (human item #3). |
| 21 | verify/repair detects manifest-vs-disk drift (orphan/missing/changed) | ✓ VERIFIED | `verify.rs`; `verify_drift.rs` 5 tests (orphan not deleted, changed, missing, repair restores) pass. |
| 22 | verify/repair auto-runs after abnormal exit; reports without blindly deleting | ✓ VERIFIED | `recover_on_launch` runs `verify` after journal replay → `RecoveryReport.drift`; orphans reported not deleted. |
| 23 | Deploy surfaces unsafe-fs warning (cross-device/non-casefolded) before linking | ✓ VERIFIED | `fs_warnings_from_caps` → `DeployReport.fs_warnings`; `fs_probe.rs` confirms probe verdicts. |
| 24 | Tauri app runs recover_on_launch on startup before UI served | ✓ VERIFIED | `src-tauri/src/lib.rs` setup loops managed games → `deploy::recover_on_launch` before serving. |
| 25 | User sees auto-detected games + paths, adds as managed (or by folder) | ✓ VERIFIED (wiring); GUI flow → human | `+page.svelte` invokes detect_games/add_game/add_game_by_folder; adapters thin. GUI exercise = human item #1. |
| 26 | User picks local .zip/.7z/.rar and installs to staging | ✓ VERIFIED (wiring); GUI flow → human | `api.installArchive` → `install_archive` command → `extract::install_archive`. GUI exercise = human item #2. |
| 27 | User clicks Deploy/Purge with fs-warnings surfaced | ✓ VERIFIED (wiring); GUI flow → human | `+page.svelte` deploy/purge buttons → adapters → engine; warnings rendered. GUI exercise = human item #2. |
| 28 | Every Tauri command is a thin 3-10 line adapter (no business logic) | ✓ VERIFIED | `commands/deploy.rs` adapters 3-8 lines delegating to `deploy::*`; CONTEXT.md decision honored. |
| 29 | CI runs cargo test --workspace + cargo deny on push | ✓ VERIFIED | `.github/workflows/ci.yml`: `cargo test --workspace --locked`, `cargo clippy -D warnings`, `cargo deny check advisories bans licenses sources`. |
| 30 | Method ladder + journal protocol: pending intent durable before syscall, manifest+done after | ✓ VERIFIED | `engine.rs` ordering invariant (Pattern 1): `journal::begin_deploy` before file op, `finish_deploy` (manifest row + done flip) after; crash_recovery proves it. |

**Score:** 30/30 truths verified (0 present, behavior-unverified). Truths 20, 25, 26, 27 are headless-verified and wired; their GUI/in-game surfaces route to human verification (manual-only by nature, documented in 01-06-SUMMARY.md and VALIDATION.md — not a gap).

### Required Artifacts

| Artifact | Status | Details |
| -------- | ------ | ------- |
| rust-toolchain.toml / Cargo.toml / deny.toml | ✓ VERIFIED | Workspace, 2024 edition, rust-version 1.85, unrar bans |
| crates/core (model.rs, error.rs) | ✓ VERIFIED | Shared domain types, re-exported |
| crates/store (db.rs, V1__init.sql, journal.rs, vanilla.rs, manifest.rs, registry.rs) | ✓ VERIFIED | WAL + refinery; 4 tables; journal/manifest/backup persistence |
| crates/testkit/src/lib.rs | ✓ VERIFIED | blake3 tree snapshot + equality assertion |
| crates/steam (discover.rs, resolve.rs, casing.rs) | ✓ VERIFIED | Detect/resolve/allow-list/casing map; 16 tests pass |
| crates/extract (validate.rs, zip.rs, sevenz.rs, rar.rs, staging.rs) | ✓ VERIFIED | Safe extraction; 7 tests pass |
| crates/deploy (probe, method/*, journal, backup, engine, casefold, verify) | ✓ VERIFIED | Crown-jewel engine; 18 tests pass incl. centerpiece |
| src-tauri (lib.rs, main.rs, commands/*) | ✓ VERIFIED | Thin adapters + startup recovery; compiles in workspace |
| frontend (api.ts, +page.svelte) | ✓ VERIFIED | Full invoke chain; functional-minimal UI |
| .github/workflows/ci.yml | ✓ VERIFIED | test + clippy + deny |

### Key Link Verification

| From → To | Status | Details |
| --------- | ------ | ------- |
| store/db.rs → V1__init.sql | ✓ WIRED | refinery `embed_migrations!`/runner applies V1 under WAL |
| deploy/engine.rs → journal | ✓ WIRED | begin_* pending before syscall, finish_* after (crash_recovery proves) |
| deploy/engine.rs → backup.rs | ✓ WIRED | backup_vanilla_if_absent before overwrite |
| deploy/method → probe.rs | ✓ WIRED | choose_method consumes FsCaps |
| deploy/casefold → steam/casing | ✓ WIRED | normalize_to_canonical consumes CasingMap |
| deploy/verify → store/manifest | ✓ WIRED | diffs list_deployed_files vs disk by hash |
| src-tauri commands → crates/deploy engine | ✓ WIRED | deploy::deploy/purge/verify |
| frontend +page.svelte → commands | ✓ WIRED | invoke(detect_games/add_game/install_archive/deploy/purge/verify) |
| src-tauri lib.rs → recover_on_launch | ✓ WIRED | startup loop before UI |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| Round-trip pristine | `cargo test -p nextwist-deploy --test round_trip_pristine` | 3 passed (incl. 48-case proptest) | ✓ PASS |
| Crash recovery centerpiece | `cargo test -p nextwist-deploy --test crash_recovery` | 2 passed | ✓ PASS |
| Malicious archive rejection | `cargo test -p nextwist-extract --test zip_slip_rejected` | 4 passed | ✓ PASS |
| Vanilla restore byte-for-byte | `cargo test --test vanilla_restore` | 2 passed | ✓ PASS |
| Method ladder EXDEV fallback | `cargo test --test method_ladder` | 4 passed | ✓ PASS |
| verify/repair drift | `cargo test --test verify_drift` | 5 passed | ✓ PASS |
| Steam resolve (SkyrimSE/FO4 + allow-list) | `cargo test -p nextwist-steam` | 16 passed | ✓ PASS |
| UnRAR ban | `cargo deny check bans` | bans ok | ✓ PASS |
| Full workspace (orchestrator) | `cargo test --workspace` | 77 passed, exit 0 | ✓ PASS |

### Requirements Coverage

All 15 phase requirement IDs cross-referenced against REQUIREMENTS.md and PLAN frontmatter. No orphaned IDs (every ID in REQUIREMENTS.md Phase 1 mapping appears in at least one plan's `requirements` field).

| Requirement | Source Plan(s) | Status | Evidence |
| ----------- | -------------- | ------ | -------- |
| ENV-01 | 01-02, 01-06 | ✓ SATISFIED | discover.rs native+Flatpak; resolve_game tests |
| ENV-02 | 01-02, 01-06 | ✓ SATISFIED | resolve.rs compatdata/<appid>/pfx; tests |
| ENV-03 | 01-01, 01-02, 01-06 | ✓ SATISFIED | managed_game table + allow-list |
| ENV-04 | 01-04, 01-05, 01-06 | ✓ SATISFIED | fs_warnings_from_caps; fs_probe tests |
| STAGE-01 | 01-03, 01-06 | ✓ SATISFIED | zip/7z extract; extract_formats tests |
| STAGE-02 | 01-03 | ✓ SATISFIED | zip_slip_rejected (3 hostile cases) |
| STAGE-03 | 01-03 | ✓ SATISFIED | rar.rs system tool + ban |
| DEPLOY-01 | 01-04, 01-06 | ✓ SATISFIED | engine.rs link, zero in-place mods |
| DEPLOY-02 | 01-01, 01-04 | ✓ SATISFIED | deployed_file manifest |
| DEPLOY-03 | 01-04, 01-06 | ✓ SATISFIED | purge → pristine; round_trip |
| DEPLOY-04 | 01-01, 01-04 | ✓ SATISFIED | backup.rs + vanilla_restore |
| DEPLOY-05 | 01-04 | ✓ SATISFIED | method ladder + EXDEV |
| DEPLOY-06 | 01-01, 01-04, 01-06 | ✓ SATISFIED | journal + crash_recovery centerpiece |
| DEPLOY-07 | 01-05 | ✓ SATISFIED | verify.rs + verify_drift |
| DEPLOY-08 | 01-02, 01-05 | ✓ SATISFIED | casefold normalize (in-game = human #3) |

**Coverage: 15/15 requirements satisfied (headless). DEPLOY-08 in-game correctness routed to human verification.**

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| crates/steam/src/discover.rs | 56 | `TODO(A2)` (no issue ref) | ⚠️ Warning | Explanatory marker documenting an INTENTIONAL design decision: Snap root (low-confidence A2) is deliberately NOT auto-detected; Snap users use the implemented & tested `add_game_by_folder` fallback. Not an unfinished obligation — the gated capability has a working, tested alternative path and is a documented manual-only verification (VALIDATION.md). Recommend converting to a non-marker comment or referencing a v-future tracking issue for auditability. |

No stubs, no `unimplemented!`/`todo!`, no empty-return placeholders, no hardcoded-empty render data found in the modified source.

### Human Verification Required

Four items, all manual-only by nature (require a real GUI/webview, a real Steam install, or a real Proton launch — none reproducible headless). The headless engine these surfaces adapt is fully covered by passing automated tests, re-confirmed during this verification. Documented in 01-06-SUMMARY.md "Manual Verification Required" and 01-VALIDATION.md "Manual-Only Verifications".

1. **GUI detect/add game** — confirm auto-detect + add-by-folder show correct install dir + Proton prefix.
2. **GUI install → deploy → purge round-trip** — confirm the full local-archive round-trip through the Svelte UI returns the game to pristine.
3. **In-game Proton load (DEPLOY-08)** — confirm a deployed mod loads in-game with correct case resolution under Wine.
4. **Real Flatpak/Snap Steam packaging detection** — confirm Flatpak auto-detect / Snap add-by-folder fallback on actual packaged Steam.

### Gaps Summary

**GAP-01 (blocker, DEPLOY-03) — found by manual UAT after initial sign-off.** `purge()` removes deployed files and restores vanilla backups but leaves behind the empty directories `deploy()` created, so after install→purge the game `Data/` is **not byte-for-byte pristine** (real-world repro: 3 orphan empty dirs left in a live Skyrim SE install). The automated `round_trip_pristine` proptest missed this because `testkit::snapshot` hashes file contents only and is blind to empty directories. See the `gaps:` block in the frontmatter for the full root cause + fix spec. This is the project's #1 guarantee, so it is a blocker; resolved via gap closure (`/gsd-plan-phase 1 --gaps`).

The remainder of verification stands: every other phase requirement is satisfied and every other core-value safety prohibition is a real, executed, passing test — verified by re-running the deploy/extract/steam suites and `cargo deny check bans` during this verification, not by trusting SUMMARY claims. The four human-verification items are GUI/in-game/packaging surfaces that are manual-only by nature and were planned as such (checkpoint:human-verify in plan 01-06); they do not represent missing or stubbed implementation. The single TODO(A2) marker documents an intentional scope decision with a working tested fallback, not unfinished work — flagged as a Warning for auditability only.

Status is `human_needed` solely because manual GUI/in-game verification items exist (decision tree rule 2), not because of any failed truth, missing artifact, broken link, or blocker.

---

_Verified: 2026-06-20_
_Verifier: Claude (gsd-verifier)_
