---
slug: loadorder-active-write
status: resolved
trigger: "crates/loadorder plugin write broken on real Fallout 4 (UAT-1, phase 2): enabled plugins + load order written by NexTwist do not reach the game; Plugins.txt ends up header-only/empty with no active (*) entries."
created: 2026-06-21
updated: 2026-06-21
resolved: 2026-06-21
phase: 02-multi-mod-management
component: crates/loadorder/src/loot.rs
verification: "Confirmed in-app on real Fallout 4 (user): Plugin Manager Save succeeds with no 'load order interaction failed'; Sort with LOOT no longer panics. cargo test --workspace green; clippy clean; new regression test fo4_multi_master_game_master_first_active_survives passes. Two adjacent bugs found+fixed during verification: LOOT-sort async panic (spawn_blocking) and tauri dev-launch config. Install archive root-detection carried forward as a separate Phase-4 gap."
---

## Symptoms

- **Expected:** After enabling plugins + setting order in NexTwist and Save, the asterisk-format
  `Plugins.txt` at `<prefix>/drive_c/users/steamuser/AppData/Local/Fallout4/Plugins.txt` lists the
  enabled plugins (masters-first, `*Name` for active) and the Creation Engine loads exactly those.
- **Actual:** The live file is the vanilla 109-byte header only (no `*` entries). Reproduced via the
  real `loadorder::apply_load_order` against the live FO4 `Data/` (throwaway harness, temp output):
  - `just-master` (Fallout4.esm only): OK but **0 active `*` lines** written.
  - `master+1dlc` (Fallout4.esm + DLCCoast.esm): **ERR `Loot("load order interaction failed")`**.
  - full real DLC master set: **ERR `Loot("load order interaction failed")`**.
  - `Fallout4.esm` forced first (order 0, DLCs order 1..): **OK but still 0 active `*` lines**;
    the returned `active_plugins_file_path` file was **empty**.
- **Error:** `Loot("load order interaction failed")` from `game.set_load_order(...)`.
- **Timeline:** First real-hardware test of Phase-2 plugin management (libloot 0.29.5). Phase tests
  were green — they used a synthetic single-plugin fixture that exercised neither failure mode.
- **Reproduction:** `loadorder::apply_load_order(377160, "/mnt/home/oliverh/SteamLibrary/steamapps/common/Fallout 4", <temp appdata>, &plugins)` with ≥2 real masters and/or all order_index=0.

## Root Causes (reproduced, pre-loaded)

### RC1 — game master not forced first (PLUGIN-02 ordering)
`masters_first_order` (crates/loadorder/src/loot.rs) sorts the master group by `(order, name)`.
Live `plugin_state` had `order_index = 0` for every row, so masters sort ALPHABETICALLY:
`ccBGS…esl`, `DLC…esm`, then `Fallout4.esm` LAST. FO4 hard-requires `Fallout4.esm` first, so
`set_load_order` rejects with `"load order interaction failed"` when ≥2 masters are present.
Forcing `Fallout4.esm` first clears the error. **Skyrim SE: identical hazard (`Skyrim.esm`).**
Fix direction: pin the game master (and any implicitly-first plugins) ahead of the name sort,
independent of user `order_index`.

### RC2 — PRIMARY: active (*) state never persisted (PLUGIN-01/02)
Even when ordering succeeds, the final `Plugins.txt` has **zero `*` lines** (empty file on the
full real set). The Plan-02 spike assumption — "seed the asterisk file, then `set_load_order`
preserves the loaded active flags" — is **false** for libloot 0.29.5 on real data: `set_load_order`
writes order only and drops the seeded actives. **Open question for this session:** the correct
libloot 0.29.5 API/sequence to persist ACTIVE plugins — e.g. an explicit `set_active_plugins`
call, or writing the canonical asterisk `Plugins.txt` as the FINAL step after `set_load_order`
(and confirming libloot/the engine then reads it). Must keep masters-first (D-08) and the
`with_local_path` Linux seam; current code path is `open_game → seed asterisk file →
load_current_load_order_state → set_load_order`.

### Contributing (separate, lower severity)
Test mod `LegendaryWeaponFountain.esp` was STAGED but never DEPLOYED into `Data/` (only under
`.nextwist-staging/377160/...`), so the on-disk filter in `apply_load_order` correctly drops it —
its plugin cannot load until the mod is deployed. Possibly a UI/flow gap; confirm during fix.

## Constraints
- Reproduce against live FO4 at `/mnt/home/oliverh/SteamLibrary/steamapps/common/Fallout 4`
  writing ONLY to a TEMP appdata — **never** touch the user's real prefix/game files.
- Keep masters-first (D-08); keep `Game::with_local_path` (Linux seam, Pitfall 1).
- libloot 0.29.5 (`Cargo.lock` confirmed).

## Current Focus

hypothesis: RC2 is NOT "set_load_order drops actives". Source proof (libloadorder 18.8.2): set_load_order→replace_plugins→map_to_plugins→to_plugin CLONES the already-loaded Plugin (preserving its active flag) for any name that exists in the in-memory `plugins` vec, else Plugin::new (inactive). So seed→load→set_load_order DOES preserve active state for plugins that load() actually parsed. The real RC2 mechanism must be either (a) the seed file is never read back (load() path), or (b) asterisk_plugins_txt writes the WRONG body for FO4 — specifically the existing test's belief "masters are implicitly active, never written with *" is TRUE only for the GAME master (Fallout4.esm/Skyrim.esm), but FALSE for DLC .esm and cc*.esl, which are NOT implicitly active on FO4 and MUST be written `*Name` to be active. asterisk_plugins_txt currently DOES write `*` for any enabled plugin incl. DLC/esl, so that part may be fine — must reproduce to see what actually lands.
test: Throwaway example crates/loadorder/examples/repro_fo4.rs exercising apply_load_order against live FO4 Data with TEMP appdata; dump (1) the seeded file, (2) the final Plugins.txt, (3) game.load_order()/is_plugin_active per plugin. Then test a corrected sequence (RC1 pin Fallout4.esm first via implicitly-active/early-loader awareness; confirm DLC/esl actives land).
expecting: Reproduce empty/0-asterisk final file on full real master set; identify exact divergence vs seed; prove a sequence that yields `*DLCCoast.esm` etc. masters-first with Fallout4.esm first.
next_action: Apply fix to crates/loadorder/src/loot.rs — replace the alphabetical `masters_first_order` argument to `set_load_order` with libloot's own canonical post-load order (early-loaders correctly sequenced) reconciled with the user's desired NON-early-loader tail order. Add a real-data-style regression test (multi-DLC, game-master-first, active-flag survives, disabled stays ordered). Delete repro_fo4.rs.

reasoning_checkpoint:
  hypothesis: "RC1 is the sole bug: NexTwist hand-rolls the master-group order alphabetically (masters_first_order, order_index all 0), which reorders FO4/SkyrimSE hardcoded EARLY-LOADER plugins (game master + DLC .esm + CCC .esl) out of libloadorder's required fixed sequence, so set_load_order rejects with 'load order interaction failed'. RC2 (no asterisks persisted) is NOT a bug: an all-early-loader set legitimately yields an empty Plugins.txt; non-early-loader actives persist correctly through the existing seed→load→set_load_order seam."
  confirming_evidence:
    - "EXP A: alphabetical real-DLC order → set_load_order ERR 'load order interaction failed' (direct repro of RC1)."
    - "EXP B: feeding libloot's own canonical load_order() → OK; all 16 early-loaders active; empty Plugins.txt is correct (save() skips loads_early plugins)."
    - "EXP C/E: non-early-loader .esp/.esl asterisk state persists (active and inactive both correct in the written file) — disproves RC2."
    - "EXP D: canonical early-loader prefix + user tail order on a mixed set → OK, masters-first, user order honored, correct asterisks."
    - "SOURCE: FALLOUT4_HARDCODED_PLUGINS fixed order + validate_early_loader_positions require early-loaders contiguous-first in that exact order; save() skips loads_early."
  falsification_test: "If the fix (defer to libloot's canonical order for early-loaders + user tail for the rest) still errors or drops user-mod actives on a multi-DLC + multi-mod set, the hypothesis is wrong. EXP D already runs this exact scenario and passes."
  fix_rationale: "Removing the alphabetical master sort and instead using libloot's resolved load_order() for the early-loader prefix addresses the ROOT cause (hand-rolled order fighting libloadorder's hardcoded sequence), not a symptom. libloot enforces masters-first internally (D-08), so the user only needs to control the non-early-loader tail order. The asterisk seam is unchanged because it already works."
  blind_spots: "Not yet tested: (1) ghosted (.esp.ghost) plugins; (2) a user mod whose master is a DLC (hoisting); (3) SkyrimSE live data (only FO4 hardware available) — mitigated by source: SkyrimSE uses the same AsteriskBasedLoadOrder + hardcoded-list mechanism. (4) The contributing issue (staged-but-not-deployed mod) is a separate UI/flow gap, out of scope for this fix."

## Source-Level Findings (libloot 0.29.5 / libloadorder 18.8.2)

- libloot `Game::set_load_order` (game.rs:543) = `load_order.set_load_order()` then `load_order.save()`. NO public `set_active_plugins` on `Game`. Active state enters ONLY via the Plugins.txt that `load()` reads.
- `AsteriskBasedLoadOrder::save` (asterisk_based.rs:156-218) writes `*` iff `plugin.is_active()`, and SKIPS plugins where `game_settings().loads_early(name)` (early-loading plugins are never written). For FO4, Fallout4.esm is an early-loader → it is NEVER written to Plugins.txt (matches "0 lines for Fallout4.esm" symptom; that is CORRECT, not the bug).
- `set_load_order`→`replace_plugins` (mutable.rs:151)→`map_to_plugins`→`to_plugin` (mutable.rs:439): clones the matching ALREADY-LOADED plugin (keeps active flag) or `Plugin::new` (inactive) if not previously loaded. So active state survives set_load_order for plugins present in the loaded set — CONFIRMS the seam can preserve actives.
- `load()` (asterisk_based.rs:137): clears plugins, reads `read_from_active_plugins_file()` (the seed), then `total_insertion_order` keeps a (name,active) tuple ONLY if the file is also found by `find_plugins()` (installed in Data/), then `load_unique_plugins` header-parses each via `Plugin::with_active`. Then `add_implicitly_active_plugins()` + `hoist_masters`.
- `read_from_active_plugins_file` is IGNORED (returns empty) when `ignore_active_plugins_file()` — true for FO4 only if `sTestFile` INI entries exist. Live FO4 INI has NONE (checked) → seed IS honored.
- Plugins.txt encoding: read via WINDOWS-1252 `decode_without_bom_handling_and_without_replacement`; ASCII names are safe. Seed is plain UTF-8 ASCII → fine.
- IMPLICATION for RC1: `replace_plugins`→`validate_load_order` enforces early-loading plugin positions and masters-before-non-masters. Passing a masters-first order where Fallout4.esm is NOT first violates the early-loader hardcoded-position check → `set_load_order` error "load order interaction failed". Fix: order must put Fallout4.esm (and any early loaders / implicitly-active masters) first, independent of user order_index.

## Evidence

- timestamp: 2026-06-21 — Reproduced both RC1 (set_load_order error on ≥2 masters not-master-first) and RC2 (0 active `*` lines even on success; empty file) via real `apply_load_order` against live FO4 Data with temp appdata output. Live prefix path + AppData folder name (`Fallout4`) confirmed correct (UAT-2 passed).

- timestamp: 2026-06-21 — SOURCE: libloadorder 18.8.2 `FALLOUT4_HARDCODED_PLUGINS` is a FIXED ORDER: Fallout4.esm, DLCRobot.esm, DLCworkshop01.esm, DLCCoast.esm, DLCworkshop02.esm, DLCworkshop03.esm, DLCNukaWorld.esm, DLCUltraHighResolution.esm. `early_loading_plugins()` = that list + the `Fallout4.ccc` lines (CCC order). `implicitly_active_plugins()` = early-loaders + test_files. `validate_early_loader_positions` requires every PRESENT early-loader to be at position `i - missing_before_it` — i.e. early-loaders MUST appear first, in this exact relative order, contiguous. `save()` SKIPS every `loads_early` plugin (never written to Plugins.txt). Live install HAS `Fallout4.ccc` (172 lines) and the installed cc*.esl are listed in it → they are early-loaders.

- timestamp: 2026-06-21 — EMPIRICAL (crates/loadorder/examples/repro_fo4.rs, temp appdata, read-only live Data):
  - EXP A (alphabetical DLC order, current `masters_first_order` with order_index=0): `set_load_order ERR: load order interaction failed`. → **RC1 PROVEN**: alphabetical DLC ordering (DLCCoast before DLCRobot) violates the hardcoded early-loader sequence.
  - EXP B (feed libloot's OWN canonical `load_order()` after an empty-seed load): `set_load_order OK`; all 16 plugins `is_plugin_active()==true`; final Plugins.txt = **0 bytes (empty) — and that is CORRECT**: all 16 are early-loaders (game master + 6 DLC + 9 CCC esl), which `save()` intentionally omits. They are active via the implicit mechanism, not Plugins.txt.
  - EXP C (synthetic NON-early-loader `MyMod.esp` + non-CCC `MyLight.esl`, asterisk seed, temp install): `set_load_order OK`; both `active==true`; final Plugins.txt correctly = `*MyLight.esl\n*MyMod.esp`. → **RC2 (as originally stated) DISPROVEN**: active/asterisk state DOES persist through seed→load→set_load_order for non-early-loaders. The original "0 active lines" symptom is the CORRECT empty file for an all-early-loader (all-DLC/CCC) set.

## Revised Diagnosis

- **RC1 (the real, only bug):** `masters_first_order` sorts the master group by `(order, name)`; with every `order_index = 0` (real DB state when the user has not manually reordered) this is ALPHABETICAL, which reorders FO4's hardcoded early-loaders (DLCs + CCC esls) out of their required fixed sequence → libloadorder rejects with `load order interaction failed`. Same hazard for SkyrimSE (Skyrim.esm/Update.esm/Dawnguard/HearthFires/Dragonborn). The masters-first hand-roll is actively HARMFUL for early-loaders.
- **RC2 was a misdiagnosis:** asterisk/active persistence already works. The empty Plugins.txt seen in UAT was for an all-DLC/CCC set and is correct libloot behavior. No active-plugin-setter change is needed; the existing seed→load→set_load_order seam is sound for the plugins that actually belong in Plugins.txt.
- **Fix direction:** stop hand-rolling the early-loader order. Either (a) for the early-loader/implicitly-active prefix, defer to libloot's canonical order (load with the seed, read `game.load_order()`, then pass THAT — masters-first is preserved and early-loaders are correctly sequenced), or (b) order only the NON-early-loader plugins explicitly and let libloot place the early-loaders. libloot enforces masters-first internally (D-08), so the safe, minimal fix is: seed the asterisk file, `load_current_load_order_state`, then `set_load_order` using libloot's own post-load `load_order()` reconciled with the user's desired NON-early-loader ordering — never an alphabetical master sort.

## Eliminated

- hypothesis: "libloot requires the COMPLETE Data/ plugin set in the order" — ELIMINATED: passing all 16 Data/ plugins still errored `load order interaction failed` (cause was game-master-not-first, RC1).
- hypothesis: "wrong prefix / AppData folder name (space vs no-space)" — ELIMINATED: stored prefix and `appdata_folder_name(377160)="Fallout4"` match the live folder byte-for-byte; write lands at the correct path.
- hypothesis: "RC2 — libloot drops seeded active/asterisk state on set_load_order; needs an explicit active-plugins setter" — ELIMINATED: EXP C/E proved non-early-loader `.esp`/`.esl` actives persist as `*Name` through seed→load→set_load_order; the empty Plugins.txt in UAT was an all-DLC/CCC set, which libloot correctly omits from the file (those plugins are implicitly active). No setter needed.

## Resolution

root_cause: |
  RC1 (sole bug). `apply_load_order` passed `masters_first_order(&on_disk)` — an
  alphabetical sort of the master group (every store `order_index == 0`) — to libloot's
  `set_load_order`. For Fallout 4 (and SkyrimSE), the game master + DLC `.esm` + Creation-Club
  `.esl` are EARLY-LOADING / implicitly-active plugins that libloadorder 18.8.2 requires at
  fixed positions in its hardcoded order (FALLOUT4_HARDCODED_PLUGINS = Fallout4.esm, DLCRobot,
  DLCworkshop01, DLCCoast, ... + the Fallout4.ccc lines). The alphabetical sort placed e.g.
  DLCCoast before DLCRobot, violating `validate_early_loader_positions`, so `set_load_order`
  rejected the whole write with `"load order interaction failed"` whenever ≥2 early-loaders
  were present. The "header-only/no-asterisk Plugins.txt" symptom (RC2) was NOT a bug: an
  all-DLC/CCC enabled set legitimately yields an empty Plugins.txt because libloot omits
  early-loaders from the file (they are implicitly active); only regular `.esp`/non-CCC `.esl`
  mods are written as `*Name`.

fix: |
  crates/loadorder/src/loot.rs — stopped hand-rolling the early-loader order. New sequence in
  `apply_load_order`: seed the asterisk Plugins.txt → `load_canonical_order` (new fn:
  load_current_load_order_state + read libloot's resolved `load_order()`, where early-loaders
  are already at their required fixed positions) → `reconcile_order` (new fn: keep every
  master-group plugin at its canonical slot, fill the regular-`.esp` slots in the user's
  desired order) → `set_order_and_save` (now a thin set_load_order+persist; the implicit load
  it used to do moved into `load_canonical_order`). Updated the module API doc to record the
  early-loader constraint and that an all-early-loader set correctly writes an empty
  Plugins.txt. Same mechanism covers SkyrimSE (shared AsteriskBasedLoadOrder + hardcoded list).

verification: |
  - New regression test `fo4_multi_master_game_master_first_active_survives`
    (crates/loadorder/tests/plugins.rs): multi-DLC FO4 set passed in ALPHABETICAL order with
    all order_index=0 (the exact RC1 trigger) + enabled and disabled regular mods. Asserts:
    apply succeeds (no "load order interaction failed"), Fallout4.esm first, DLCRobot before
    DLCCoast (hardcoded, not alphabetical), `*EnabledMod.esp` active in Plugins.txt,
    `DisabledMod.esp` present without asterisk, early-loading masters omitted. CONFIRMED this
    test FAILS with the old alphabetical path (reproduces the exact RC1 error) and PASSES with
    the fix.
  - Empirically proven against LIVE FO4 Data (read-only) + temp appdata via a throwaway
    harness (since removed): EXP A reproduced the error; EXP B/D/F proved the canonical-order
    sequence succeeds with correct masters-first ordering and active flags; EXP C/E proved
    asterisk persistence for regular/disabled mods.
  - Full workspace: `cargo test --workspace` 150 passed; `cargo clippy -p nextwist-loadorder
    --all-targets` clean. Live prefix Plugins.txt confirmed untouched (still vanilla 109 B).

files_changed:
  - crates/loadorder/src/loot.rs (RC1 fix: load_canonical_order + reconcile_order; apply_load_order rewired; set_order_and_save split; module doc)
  - crates/loadorder/tests/plugins.rs (new real-data-style FO4 regression test closing the fixture gap)
  - crates/loadorder/tests/libloot_spike.rs (updated to the load_canonical_order + set_order_and_save split)
