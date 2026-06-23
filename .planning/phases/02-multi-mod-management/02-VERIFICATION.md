---
phase: 02-multi-mod-management
verified: 2026-06-21T00:00:00Z
status: human_needed
score: 21/21 must-haves verified
behavior_unverified: 0
overrides_applied: 0
mode: mvp
security: 02-SECURITY.md (18/18 threats closed, 2026-06-23)
wr_failure_paths_automated: "WR-02 and WR-05 are now automated (dce7238 + profile_switch.rs strengthening 2026-06-23): failed_switch_after_purge_clears_stale_active_flag (no stale-active, journal-recoverable) + save_plugin_order_inner_leaves_db_untouched_on_write_failure (no partial DB save). Both PASS — no bug. The remaining human_verification items below are the in-game plugin-load-order launch + AppData-folder-name match, which require real Steam Proton hardware (/gsd-verify-work 2)."
re_verification: # none — initial verification
human_verification:
  - test: "Launch Skyrim SE (or Fallout 4) via Steam Proton AFTER saving a plugin order in NexTwist, and confirm the modded plugins actually load in-game (correct masters-first order, enabled set applied)."
    expected: "The game boots with exactly the enabled plugins from NexTwist's list, in the saved order; disabled plugins do not load."
    why_human: "plugins.txt is written headlessly to a fixture/real prefix and round-trips through libloot in tests, but whether Proton/Wine's game engine actually READS that file at the resolved AppData location and honors the order is only observable by launching the real game."
  - test: "Confirm the real Proton-prefix AppData folder name used by the live game matches loadorder::appdata_folder_name (Skyrim SE -> 'Skyrim Special Edition', Fallout 4 -> 'Fallout4') by inspecting <prefix>/drive_c/users/steamuser/AppData/Local/<name>/Plugins.txt after a real launch."
    expected: "The game's own Plugins.txt lives under the same AppData/Local/<name> folder NexTwist writes to; the names match exactly."
    why_human: "The folder name is a hardcoded per-game constant; libloot's with_local_path is fed this path in tests against a FIXTURE prefix, but the real folder name a launched game uses can only be confirmed against an actual Proton prefix."
  - test: "Failure-path WR-02: simulate or observe a profile switch where the post-purge deploy/plugins step fails (e.g. a missing staging file or unwritable prefix) and confirm no profile is left flagged active while its deployment has been purged off disk."
    expected: "After the failed switch, the store reports zero active profiles for that game (stale flag cleared); the on-disk deployment is pristine/journal-recoverable. The UI does not show an OLD profile as active over a phantom deployment."
    why_human: "The fixer explicitly flagged WR-02 as reasoned-through but NOT exercised by an automated failure-injection test; the happy path is green, the error path is not test-covered."
  - test: "Failure-path WR-05: simulate a plugins.txt write failure during save_plugin_order and confirm the DB plugin_state rows were NOT persisted (the user's 'nothing was saved' mental model holds)."
    expected: "On a libloot/IO write failure, the plugin_state DB rows are unchanged — no partial save where the order persisted in the DB but the file write failed."
    why_human: "The fixer explicitly flagged WR-05 as reasoned-through but NOT exercised by an automated write-failure-injection test; compile + happy path green, the failure reordering is not test-covered."
---

# Phase 2: Multi-Mod Management Verification Report

**Phase Goal:** A user managing many mods can see and resolve file conflicts, control which mod wins via priority/load order, order game plugins correctly in the right Proton prefix, and maintain multiple independent profiles per game that switch the deployed mod set on demand.
**Verified:** 2026-06-21
**Status:** human_needed
**Mode:** mvp
**Re-verification:** No — initial verification

## User Flow Coverage

User story / phase goal mapped to the four MVP success criteria. Each user-visible capability is traced to wired code + a passing test.

| Flow | Expected (user-visible) | Evidence | Status |
|------|-------------------------|----------|--------|
| See file conflicts | User opens Conflict view, sees a priority list of mods and a per-file "who provides / who wins" table | `frontend/src/routes/+page.svelte:45,163` (conflicts state + loadConflicts) → `invoke('list_conflicts')` (api.ts:132) → `commands/conflicts.rs:48` → `deploy::conflict::resolve` emits `Vec<FileConflict>` | ✓ |
| Set priority to pick winner | User drags/sets a mod's rank; the chosen mod deterministically wins; change is pending until Deploy | `+page.svelte:181` (set priority) → `invoke('set_mod_rank')` → `conflicts.rs:61` (persist only, D-04) → resolver `rank_change_flips_winner` test passes | ✓ |
| Deploy winner set safely | User clicks Deploy; only the winning files are placed; a later purge restores pristine | `invoke('deploy_winner_set')` → `conflicts.rs:84` → `deploy::redeploy_winners` (purge→deploy) → `conflict_redeploy.rs` 3 BLOCKING-PRISTINE tests pass | ✓ |
| Enable/disable + order plugins | User sees .esp/.esm/.esl with type badges, toggles enabled, reorders; plugins.txt is written masters-first at the prefix | `+page.svelte:51,201` → `invoke('list_plugins'/'set_plugin_enabled'/'save_plugin_order')` → `commands/plugins.rs` → `loadorder::apply_load_order` (asterisk, masters-first) → `plugins.rs` tests pass | ✓ (in-game load: human) |
| Auto-sort via LOOT | User clicks Sort, sees a proposed order + warnings, applies only on confirm (no silent apply) | `invoke('sort_with_loot')` → `plugins.rs:197` → `loadorder::propose_sort` returns `SortProposal` (writes nothing); apply is separate `apply_load_order` | ✓ |
| Create/switch/preserve profiles | User creates profiles, switches active (purge old → deploy new → plugins.txt → mark active), each keeps its own set/order | `+page.svelte:59,319` confirm-gated modal → `invoke('switch_profile')` → `deploy::switch_profile` → `profile_switch.rs` 2 tests (cross-switch pristine) pass | ✓ |
| Delete profile safely | User cannot delete the active profile (must switch away first) | `invoke('delete_profile')` → `store::delete_profile` refuses active (`profiles.rs:181`); `delete_active_profile_is_refused` test passes | ✓ |
| Outcome | Many mods managed: conflicts seen+resolved, winners controlled, plugins ordered in the right prefix, independent profiles switchable on demand | All seven flows above wired end-to-end; safety invariant locked by BLOCKING-PRISTINE tests | ✓ (Proton in-game: human) |

## Goal Achievement

### Observable Truths

| #  | Truth | Status | Evidence |
|----|-------|--------|----------|
| 1  | Fresh DB applies V1 then V2 cleanly with all new tables | ✓ VERIFIED | `V2__multi_mod.sql` creates managed_mod/profile/profile_mod/plugin_state; `store` 31 tests pass |
| 2  | Existing Phase-1 V1 DB upgrades to V2 and auto-creates one 'Default' profile per game | ✓ VERIFIED | `V2__multi_mod.sql:78` `INSERT INTO profile ... SELECT appid,'Default',1 FROM managed_game`; `db.rs:159 v2_migrates_phase1_state` reaches genuine V1-only state then verifies upgrade |
| 3  | managed_mod carries priority/rank ordering conflict winners | ✓ VERIFIED | `V2__multi_mod.sql:33` `rank ... DEFAULT 1`; `conflict::resolve` sorts by rank; `lower_rank_wins_shared_path` test |
| 4  | Each profile preserves its own enabled set + per-mod rank + plugin state | ✓ VERIFIED | `profile_mod(enabled,rank)` + `plugin_state(enabled,order_index)` tables; `enabled_inputs_for_profile` reads per-profile rank; `profile_switch.rs` asserts B's set replaces A's |
| 5  | cargo deny passes with libloot GPL-3.0-or-later allowed | ✓ VERIFIED | `cargo deny check` → "advisories ok, bans ok, licenses ok, sources ok" |
| 6  | Fixture Proton-prefix AppData/Local/<Game> can be built headless | ✓ VERIFIED | `testkit::fake_proton_prefix`; `fake_proton_prefix_builds_appdata_local_and_seeds_plugins_txt` test passes |
| 7  | libloot Game::with_local_path constructs against fixture (Linux seam) | ✓ VERIFIED | `loot.rs:103 open_game` always uses with_local_path; `libloot_spike.rs` 4 tests pass |
| 8  | Enable + set_load_order + save writes asterisk plugins.txt at libloot path | ✓ VERIFIED | `apply_load_order` seeds asterisk file + set_order_and_save; `writes_asterisk_masters_first` test passes |
| 9  | crates/loadorder is Tauri-free and compiles headless | ✓ VERIFIED | loadorder crate builds + 19 unit + 10 integration tests pass with no tauri dep |
| 10 | User can see contested target_rel: providers + winner (CONF-01) | ✓ VERIFIED | `resolve` emits `FileConflict{providers,winner}`; `list_conflicts` command + Conflict table UI |
| 11 | User can set rank so a chosen mod deterministically wins (CONF-02) | ✓ VERIFIED | `set_mod_rank` command; `rank_change_flips_winner` test |
| 12 | Resolver emits exactly one winner per target_rel (UNIQUE-safe) | ✓ VERIFIED | BTreeMap fold, deduped; `never_emits_duplicate_target_rel` test |
| 13 | Deploying winners + purge returns game byte-for-byte pristine (CONF-03) | ✓ VERIFIED | `conflict_redeploy.rs` BLOCKING-PRISTINE: `conflict_winner_set_deploys_unique_and_pristine`, `rank_change_redeploy_stays_pristine`, `redeploy_winners_reconciles_without_manual_purge` all pass (snapshot_tree byte-compare) |
| 14 | User sees .esp/.esm/.esl from staged trees + Data/ with type badges (PLUGIN-01) | ✓ VERIFIED | `scan::scan_plugins` classifies kind; `collects_plugins_and_classifies_master_vs_regular` test; Plugin manager UI |
| 15 | User can enable/disable plugins, state reflected | ✓ VERIFIED | `set_plugin_enabled` (atomic single-lock, WR-03); `toggle_active_reflected` test |
| 16 | Plugin order written asterisk-format at Proton AppData, masters-first (PLUGIN-02) | ✓ VERIFIED (headless); in-game load → human | `apply_load_order` + `masters_first_order`; `writes_asterisk_masters_first` test. Real Proton in-game load is a human item. |
| 17 | LOOT auto-sort proposes for review, applies only on confirm (PLUGIN-03) | ✓ VERIFIED | `propose_sort` returns `SortProposal` (writes nothing); apply is separate confirmed `apply_load_order`; UI gates apply on confirm (D-12) |
| 18 | User can create multiple independent profiles per game (PROF-01) | ✓ VERIFIED | `create_profile` command + `profile` table; store profile tests |
| 19 | Switching reconciles through safe engine: purge old → deploy new → plugins.txt (PROF-02) | ✓ VERIFIED | `profile.rs:78 switch_profile` (purge→resolve+deploy_winners→apply plugins→set_active); `profile_switch.rs` cross-switch pristine test |
| 20 | Each profile preserves its own set/priority/plugin order (PROF-03) | ✓ VERIFIED | per-profile `profile_mod`/`plugin_state`; `profile_switch_round_trips_pristine_across_switches` asserts A's snapshot restored after switch back |
| 21 | Switching is confirmation-gated and byte-for-byte reversible across switches | ✓ VERIFIED | confirm modal (`+page.svelte:319,322`); `profile_switch.rs` final purge → pristine assertion |

**Score:** 21/21 truths verified (0 present, behavior-unverified)

> Note on behavior-dependent truths: truths 13, 19, 20, 21 assert state transitions and the byte-for-byte pristine cancellation/cleanup invariant. They are marked VERIFIED (not merely present) because the BLOCKING-PRISTINE integration tests (`conflict_redeploy.rs`, `profile_switch.rs`) exercise the transitions and assert full-tree byte equality via `snapshot_tree`/`assert_trees_identical` — behavioral evidence, not presence alone.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/store/src/migrations/V2__multi_mod.sql` | 4 tables + Default-profile data migration | ✓ VERIFIED | All tables + D-16 INSERT present; V3 FK migration added by WR-06 (additive, V2 untouched) |
| `crates/store/src/mods.rs` | managed_mod CRUD + rank | ✓ VERIFIED | set_mod_rank used by conflicts cmd |
| `crates/store/src/profiles.rs` | profile CRUD + active flag + delete gate | ✓ VERIFIED | delete_profile refuses active (CR-02); clear_active_profile (WR-02) |
| `crates/store/src/plugins.rs` | per-profile plugin state | ✓ VERIFIED | list/set plugin_state, FK-guarded (WR-06) |
| `crates/core/src/model.rs` | ManagedMod.rank, Profile, Plugin, PluginKind, FileConflict | ✓ VERIFIED | types present + used across crates |
| `crates/loadorder/src/loot.rs` | libloot wrapper + apply + propose_sort | ✓ VERIFIED | with_local_path, apply_load_order, propose_sort all present + wired |
| `crates/loadorder/src/scan.rs` | scan_plugins with kind badges | ✓ VERIFIED | scan_plugins + classification, 6 tests |
| `crates/loadorder/src/masterlist.rs` | fetch + cache + bundled fallback | ✓ VERIFIED | ensure_masterlist; redirect Policy::none + host re-assert (WR-01) |
| `crates/loadorder/tests/libloot_spike.rs` | A1/A3 de-risk round-trip | ✓ VERIFIED | 4 tests pass |
| `crates/testkit/src/lib.rs` | fake_proton_prefix | ✓ VERIFIED | present + 10 tests pass |
| `crates/deploy/src/conflict.rs` | resolve() pure fold single-winner | ✓ VERIFIED | resolve present, path-escape guard, 6 tests |
| `crates/deploy/src/profile.rs` | switch_profile reconcile | ✓ VERIFIED | purge→deploy→plugins→active + WR-02 cleanup |
| `crates/deploy/src/engine.rs` (redeploy_winners) | purge-then-deploy reconcile (CR-01) | ✓ VERIFIED | redeploy_winners present, wired to deploy_winner_set cmd |
| `crates/deploy/tests/conflict_redeploy.rs` | CONF-03 winner deploy + pristine | ✓ VERIFIED | 3 BLOCKING-PRISTINE tests pass |
| `crates/deploy/tests/profile_switch.rs` | PROF-02 cross-switch pristine | ✓ VERIFIED | 2 tests pass |
| `src-tauri/src/commands/conflicts.rs` | list_conflicts/set_mod_rank/deploy_winner_set | ✓ VERIFIED | registered in invoke_handler; routes through redeploy_winners |
| `src-tauri/src/commands/plugins.rs` | list/set/save/sort | ✓ VERIFIED | registered; atomic locking (WR-03), write-before-persist (WR-05) |
| `src-tauri/src/commands/profiles.rs` | list/create/switch/delete | ✓ VERIFIED | registered; delete gated |
| `frontend/src/routes/+page.svelte` | Conflict + Plugin + Profile views | ✓ VERIFIED | all 3 views with state + handlers; confirm-gated switch/delete |
| `frontend/src/lib/api.ts` | invoke bindings | ✓ VERIFIED | all 13 phase commands bound |

### Key Link Verification

| From | To | Via | Status |
|------|----|----|--------|
| db.rs | V2__multi_mod.sql | embed_migrations auto-discovery | ✓ WIRED (v2 migration test passes) |
| conflicts.rs | conflict::resolve / redeploy_winners | deploy_winner_set delegates | ✓ WIRED |
| profile.rs | engine purge + deploy_winners | switch reconcile | ✓ WIRED |
| profile.rs | loadorder apply_load_order | apply_profile_plugins | ✓ WIRED |
| plugins.rs | loadorder propose_sort/apply_load_order | sort/save commands | ✓ WIRED |
| masterlist.rs | raw.githubusercontent.com (pinned) | reqwest rustls, redirect none | ✓ WIRED |
| api.ts | all Tauri commands | invoke() | ✓ WIRED (registered in lib.rs invoke_handler) |
| +page.svelte | api.ts | listConflicts/setModRank/deployWinnerSet/list+sort plugins/profiles | ✓ WIRED |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full workspace tests | `cargo test --workspace` | ~150 tests, 0 failed | ✓ PASS |
| License/GPL compliance | `cargo deny check` | advisories/bans/licenses/sources ok | ✓ PASS |
| Conflict pristine invariant | `conflict_redeploy.rs` (3 tests) | all pass (byte-for-byte) | ✓ PASS |
| Profile cross-switch pristine | `profile_switch.rs` (2 tests) | all pass (byte-for-byte) | ✓ PASS |
| V1→V2 migration + Default profile | `db.rs v2_migrates_phase1_state` | pass | ✓ PASS |
| delete active profile refused | `delete_active_profile_is_refused` | pass | ✓ PASS |
| plugins.txt actually loaded in-game under Proton | (requires real game launch) | n/a | ? SKIP → human |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| CONF-01 | 02-03 | See file-level conflicts | ✓ SATISFIED | list_conflicts + FileConflict + Conflict table |
| CONF-02 | 02-03 | Set priority to control winner | ✓ SATISFIED | set_mod_rank + rank-flip test |
| CONF-03 | 02-03 | Deployment applies winner choices deterministically | ✓ SATISFIED | redeploy_winners + BLOCKING-PRISTINE tests |
| PLUGIN-01 | 02-04 | Enable/disable plugins | ✓ SATISFIED | scan_plugins + set_plugin_enabled |
| PLUGIN-02 | 02-04 | View/adjust order, plugins.txt in correct prefix | ✓ SATISFIED (headless); in-game load → human | apply_load_order asterisk masters-first |
| PLUGIN-03 | 02-04 | Auto-sort via LOOT | ✓ SATISFIED | propose_sort propose-then-apply |
| PROF-01 | 02-05 | Multiple independent profiles per game | ✓ SATISFIED | create_profile + profile table |
| PROF-02 | 02-05 | Switch active profile, change deployed set | ✓ SATISFIED | switch_profile reconcile + cross-switch pristine test |
| PROF-03 | 02-05 | Each profile preserves its set/order | ✓ SATISFIED | per-profile profile_mod/plugin_state |

All 9 declared requirement IDs accounted for and SATISFIED. No orphaned requirements (REQUIREMENTS.md maps exactly CONF-01..03, PLUGIN-01..03, PROF-01..03 to Phase 2; all claimed in plans).

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| frontend/src/routes/+page.svelte | 440,482,750 | HTML `placeholder=` attributes | ℹ️ Info | Legitimate input placeholders, not stubs |
| frontend/src/routes/+page.svelte | 774,998 | `placeholder` CSS class on active-dot | ℹ️ Info | Styling class name, not a stub |

No TBD/FIXME/XXX debt markers in any phase-modified crate. No empty-return stubs, no console.log-only handlers. clippy clean per REVIEW-FIX (re-confirmed by green test build).

### Code Review Fix Verification

Both safety BLOCKERs from 02-REVIEW are confirmed FIXED in the live codebase:
- **CR-01** (deploy_winner_set redeploy orphans files): `deploy::redeploy_winners` (engine.rs:307) does purge-then-deploy; `deploy_winner_set` (conflicts.rs:92) routes through it; regression test `redeploy_winners_reconciles_without_manual_purge` passes.
- **CR-02** (delete_profile can delete active): `store::delete_profile` (profiles.rs:181) refuses the active profile with a clear error; `delete_active_profile_is_refused` test passes.

8 Warning fixes (WR-01..08) also confirmed in code. Two (WR-02, WR-05) are error-path logic the fixer explicitly flagged as not exercised by failure-injection tests → carried to Human Verification.

### Human Verification Required

1. **In-game plugins.txt under Proton** — launch the real game after saving a plugin order; confirm the enabled set + masters-first order actually take effect in-game.
2. **Real Proton AppData folder name** — confirm `appdata_folder_name` constants match the live game's actual `AppData/Local/<name>` folder in the prefix.
3. **WR-02 failure-path** — confirm a mid-switch failure clears the stale active flag (no OLD profile active over a purged deployment).
4. **WR-05 failure-path** — confirm a plugins.txt write failure leaves the DB plugin_state untouched (no partial save).

### Gaps Summary

No gaps. All 21 must-haves verified against the actual codebase with behavioral test evidence; all 9 requirement IDs SATISFIED; both safety BLOCKERs fixed and regression-locked; the byte-for-byte pristine safety invariant holds for both conflict-redeploy and profile-switch (BLOCKING-PRISTINE tests pass). Status is `human_needed` (not `passed`) solely because four items require manual/in-game UAT that cannot be verified programmatically: real Proton in-game plugin loading, the live AppData folder name, and the two error-path fixes (WR-02/WR-05) the code-fixer itself flagged as reasoned-through but not failure-injection-tested. None of these are blockers — they are confirmation checks on already-implemented, wired behavior.

---

_Verified: 2026-06-21_
_Verifier: Claude (gsd-verifier)_
