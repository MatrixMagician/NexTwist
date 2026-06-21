---
status: complete
phase: 02-multi-mod-management
source: [02-VERIFICATION.md]
started: 2026-06-21
updated: 2026-06-21
---

## Current Test

[testing complete]

## Tests

### 1. In-game plugins.txt under Proton
expected: Enabled set + load order written by NexTwist actually apply in-game (Skyrim SE / Fallout 4) launched via Steam Proton.
result: passed — after fixes, confirmed in-app on real Fallout 4 (2026-06-21): Plugin Manager **Save** succeeds with no `load order interaction failed` (the exact all-DLC/order-0 state that errored before), and **Sort with LOOT** no longer panics. Debug session `loadorder-active-write` (resolved). The earlier "blocker" below documents the original failure + root causes for history.
verified_fixes: |
  - RC1 (loadorder ordering): commit — `crates/loadorder/src/loot.rs` load_canonical_order + reconcile_order; regression test `fo4_multi_master_game_master_first_active_survives`.
  - LOOT-sort async panic (PLUGIN-03): commit — `src-tauri/src/commands/plugins.rs` runs blocking `propose_sort` via `tauri::async_runtime::spawn_blocking`.
  - Dev-launch config (unblocked the in-app test): `src-tauri/tauri.conf.json` beforeDev/BuildCommand `../frontend`→`frontend` (workspace-root cwd; matches CI).
  - Remaining (separate, Phase-4 gap): install archive root-detection — see `.planning/todos/pending/install-archive-root-detection.md`. A full visible content load was NOT exercised because the test mod deploys double-nested; RC1 confirmed via DLC reorder + Save instead.
original_severity: blocker
reported: "On real Fallout 4 (appid 377160), NexTwist's Save wrote a header-only, active-less Plugins.txt — no enabled plugins reach the game."
root_cause: |
  Reproduced against the live FO4 Data/ via the real `loadorder::apply_load_order` (throwaway
  diagnostic, temp output). TWO independent defects in crates/loadorder/src/loot.rs (libloot 0.29.5):

  RC1 — game-master not forced first (PLUGIN-02). `masters_first_order` sorts the master group
  by (order, name). The live `plugin_state` had `order_index = 0` for every row, so masters sort
  ALPHABETICALLY: `ccBGS…esl`, `DLC…esm`, then `Fallout4.esm` LAST. FO4 hard-requires
  `Fallout4.esm` to load first, so `game.set_load_order(...)` rejects the whole write with
  libloot `"load order interaction failed"` whenever ≥2 masters are present. Reproduced: any
  master+DLC pair errors; forcing `Fallout4.esm` first clears it. Skyrim SE has the identical
  hazard (`Skyrim.esm` must be first). The masters_first comparator must pin the game master
  (and other implicitly-first plugins) ahead of the name sort, not rely on user order_index.

  RC2 — MISDIAGNOSED (NOT a bug). Debug session `loadorder-active-write` disproved this against
  libloadorder 18.8.2 source + live FO4 data: the "empty / 0-asterisk Plugins.txt" I saw was
  because the repro only enabled DLC/CC plugins — those are EARLY-LOADERS (implicitly active),
  which libloot intentionally OMITS from Plugins.txt (an empty file for an all-early-loader set
  is CORRECT). Regular `.esp` / non-CCC `.esl` actives DO persist as `*Name` through the existing
  seed→load→set_load_order seam. No active-setter change was needed.

  FIX APPLIED (commit pending) — RC1 fixed at the root in crates/loadorder/src/loot.rs: stopped
  hand-rolling the master order; new sequence is open_game → seed asterisk file →
  `load_canonical_order` (libloot's resolved order, early-loaders pinned at their required fixed
  positions) → `reconcile_order` (keep that prefix verbatim, splice the user's order in only for
  the regular `.esp` mods they control) → `set_order_and_save`. Covers Skyrim SE via the same
  AsteriskBasedLoadOrder + hardcoded-list mechanism. New real-data regression test
  `fo4_multi_master_game_master_first_active_survives` reproduces the old error and passes on the
  fix. `cargo test --workspace` 150 passed; clippy clean; live prefix untouched.

  Contributing (SEPARATE bug, now logged): the test mod was deployed double-nested —
  `Data/Super Cheat Legendary Weapon Fountain/Data/LegendaryWeaponFountain.esp` (whole archive
  wrapper + Info.txt/Screenshot copied into Data/). Mod install has no archive-root detection;
  the Creation Engine only loads `Data/*.esp`, so it never loads. Logged as pending todo
  `install-archive-root-detection.md` (likely Phase-4 scope). This blocks a full visible in-game
  content test of the loadorder fix using this mod; RC1 is being confirmed in-app via DLC reorder.

  Why phase tests missed RC1: crates/loadorder tests used a synthetic single-plugin fixture with
  no game-master-first constraint and no multi-master interaction, so RC1 never fired. Closed by
  the new multi-master regression test.
artifacts: [crates/loadorder/src/loot.rs (load_canonical_order, reconcile_order, apply_load_order); crates/loadorder/tests/plugins.rs (new regression test)]
missing: [game-master-first pin in load order, explicit active-plugins persistence, real-data (multi-master FO4/SkyrimSE) loadorder test]

### 2. Real Proton-prefix AppData folder name
expected: The per-game AppData/Local folder constants (e.g. "Skyrim Special Edition", "Fallout4") match the live Proton prefix on real hardware; libloot with_local_path round-trips to the correct Plugins.txt.
result: passed — verified on disk against the live FO4 prefix. NexTwist's stored prefix `/mnt/.../compatdata/377160/pfx` is correct, and `appdata_folder_name(377160) → "Fallout4"` matches the real folder `…/AppData/Local/Fallout4` (no space) byte-for-byte; NexTwist writes Plugins.txt to exactly that path (confirmed by the 12:50 write at the correct location). The path/prefix seam is correct — the write CONTENT is what failed (see Test 1).

### 3. WR-02 mid-switch failure clears stale active flag
expected: If a profile switch fails after the purge step, no profile is left falsely marked active (state/disk consistent). Happy path is test-covered; this is the failure-injection path the fixer flagged as not automatically tested.
result: [passed] — automated via failure-injection test `failed_switch_after_purge_clears_stale_active_flag` in `crates/deploy/tests/profile_switch.rs`. Switching an unsupported-appid game forces `apply_profile_plugins` to fail AFTER purge+deploy; asserts no profile remains active (stale flag cleared) and a subsequent purge restores the install byte-for-byte pristine. (was manual/in-game; now machine-verified)

### 4. WR-05 plugins.txt write-failure leaves DB untouched
expected: If the plugins.txt write fails, the DB plugin_state is not persisted (write-before-persist ordering holds). Happy path is test-covered; this is the failure-injection path the fixer flagged as not automatically tested.
result: [passed] — automated via unit test `save_plugin_order_inner_leaves_db_untouched_on_write_failure` in `src-tauri/src/commands/plugins.rs`. The write-before-persist ordering was extracted into the sync `save_plugin_order_inner` helper (command is now a thin wrapper); the test forces `apply_load_order` to fail (unwritable prefix) and asserts `plugin_state` stays empty. (was manual; now machine-verified)

## Summary

total: 4
passed: 4
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps

- truth: "Enabled plugins + chosen load order written by NexTwist are honored by the Creation Engine in-game (asterisk Plugins.txt at the Proton-prefix AppData path)."
  status: RESOLVED
  reason: "RC1 fixed (defer early-loader order to libloot via load_canonical_order + reconcile_order); LOOT-sort async panic fixed (spawn_blocking); dev-launch config fixed. RC2 was a misdiagnosis (early-loaders are intentionally omitted from Plugins.txt). Confirmed in-app on real FO4 + workspace tests/clippy green + new multi-master regression test."
  severity: blocker
  test: 1
  resolved_by: "debug session loadorder-active-write"

- truth: "A mod archive with a wrapper folder installs so its plugin lands at Data/<Plugin>.esp and loads in-game."
  status: CARRIED FORWARD (separate from Phase-2 loadorder)
  reason: "No archive root-detection: the mod deployed double-nested (Data/<Wrapper>/Data/Plugin.esp) plus non-game files, so the Creation Engine never loads it. Reversibility intact (tracked in deployed_file)."
  severity: major
  scope: "likely Phase 4 (Guided Installers) or a Phase-1 staging follow-up"
  tracked_in: ".planning/todos/pending/install-archive-root-detection.md"
