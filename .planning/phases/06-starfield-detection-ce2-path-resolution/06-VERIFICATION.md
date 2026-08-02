---
phase: 06-starfield-detection-ce2-path-resolution
verified: 2026-07-07T00:00:00Z
status: passed
score: 4/4 success criteria verified (SFDET-01/02/03 satisfied)
behavior_unverified: 0
overrides_applied: 0
deferred:
  - truth: "Criterion 2 'verified against a REAL prefix' half + Criterion 4 real installed-build baseline + live in-app pending→Ready / drift confirmation"
    addressed_in: "Phase 9 (on-hardware validation)"
    evidence: "06-CONTEXT deferred section + 06-01/06-03 SUMMARY 'Notes for Later Phases': no live Proton Starfield prefix on this host; VALIDATED_BUILD=0 keeps the drift notice intentionally dormant until Phase 9 seeds the real baseline. Phase 6's contract = resolver + first-launch state + drift-compare ship and are proven via fixtures (incl. mandatory case-mismatch) + unit tests."
---

# Phase 6: Starfield Detection & CE2 Path Resolution Verification Report

**Phase Goal:** NexTwist detects Starfield (Steam AppID 1716740) under Steam/Proton and resolves its CE2 config location (`Documents/My Games/Starfield` inside the Proton prefix) so it can be managed like the existing Bethesda titles. Gates Phases 7 and 8.
**Verified:** 2026-07-07
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths (Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Add Starfield (1716740) as a managed Bethesda game, auto-detected with install dir + Proton prefix, exactly like SSE/FO4 (SFDET-01) | ✓ VERIFIED | `resolve.rs`: `STARFIELD=1716740` in `SUPPORTED_APPIDS`; `is_supported`/`default_name`="Starfield"/`expected_exe`="Starfield.exe" arms; test `is_supported_allow_lists_the_three_supported_games` + `resolve_from_root_resolves_starfield_positively` asserts positive install-dir + `steamapps/compatdata/1716740/pfx` resolution (not NotInstalled). `loadorder`: `game_type_for`→`GameType::Starfield`, `game_id_for`→`GameId::Starfield`, `game_slug`→"starfield", `appdata_folder_name`→"Starfield", all with 3-game allow-list tests rejecting 0/220. Frontend add-title list `+page.svelte:36` `{ appid: STARFIELD_APPID, name: "Starfield" }`. |
| 2 | Resolves CE2 `Documents/My Games/Starfield` inside the prefix, case-folded + Documents-redirection-aware; verified against a case-mismatched fixture (SFDET-02) | ✓ VERIFIED | `ce2.rs::my_games_path` mirrors the `appdata_local_path` join-chain with the `Documents/My Games/Starfield` tail, walking each existing component through shared `entry_ci` (lifted `pub(crate)`, body unchanged — no re-implementation). Test `my_games_case_mismatch_resolves_real_on_disk_casing` builds a mis-cased prefix and asserts the resolved path is the real on-disk `.../documents/my games/starfield` (`assert resolved.is_dir()`), NOT canonical casing — the MANDATORY fixture. `documents_redirect_is_honored_when_in_prefix` + `documents_redirect_traversal_is_rejected` + `malformed_user_reg_falls_back_to_default` cover the `user.reg` `"Personal"` redirect with an ASVS traversal guard (`windows_path_to_components` rejects non-`C:`, `.`/`..`/empty). Real-prefix half → Phase 9 (deferred). |
| 3 | First-launch-not-done detected (My Games absent) → user guided to launch once, not silently written to a useless path (SFDET-02) | ✓ VERIFIED | `Ce2ConfigState::{Ready, FirstLaunchPending}` is a typed SUCCESS enum (no thiserror derive) — `resolve_ce2_config` returns `FirstLaunchPending(expected)` when the dir is absent or empty, `Ready(real)` when it has any file. Tests `first_launch_pending_when_absent_or_empty` + `ready_when_config_dir_has_a_file`. UI: `+page.svelte:1677` `{#if isStarfield && starfieldPending}` wraps the entire "Plugins & load order" section — pending renders launch-once guidance + expected config path + a **Re-check** button (`onclick=loadStarfieldStatus`, explicit click only, no write-on-focus); `{:else}` holds the actual management toolbar (Refresh / Sort with LOOT / Save plugin order / reorder / enable toggles). Game stays addable/detectable while pending. |
| 4 | Installed build reported + version-drift warning when installed build is newer than last validated build (SFDET-03) | ✓ VERIFIED | `resolve.rs::installed_build` reads `buildid` from `appmanifest_<appid>.acf`, fail-safe `None` on any missing/unparseable/absent case (test `installed_build_reads_buildid_and_is_fail_safe` → `Some(18901529)` and `None`). `ce2.rs::drift_notice` returns `Some{is_newer:true}` only when `installed > validated > 0`, else `None` (test covers equal/older/None/`VALIDATED_BUILD=0`). `StarfieldStatus` aggregate + persistent `.drift-notice` block in `+page.svelte:1480` rendering installed-vs-validated as plain text, independent of the pending gate (never blocks management). Notice is intentionally DORMANT (`VALIDATED_BUILD=0`) until Phase 9 seeds the real baseline. |

**Score:** 4/4 truths verified (0 present, behavior-unverified)

### Deferred Items (Phase 9 — on-hardware)

| # | Item | Addressed In | Evidence |
|---|------|-------------|----------|
| 1 | "Verified against a REAL prefix" half of criterion 2; real installed-build baseline for criterion 4; live in-app pending→Ready + drift confirmation | Phase 9 | 06-CONTEXT deferred + 06-01/06-03 SUMMARY: no live Proton Starfield prefix on this host; `VALIDATED_BUILD=0` keeps drift dormant by design. Acknowledged as a Phase-9 item, not a Phase-6 gap. |

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/steam/src/ce2.rs` | CE2 resolver + first-launch enum + drift + aggregate | ✓ VERIFIED | New module, 368 lines, 9 tests; re-exported in `lib.rs`. Read-only, no prefix writes. |
| `crates/steam/src/resolve.rs` | STARFIELD const, allow-list arms, `installed_build`, `entry_ci` pub(crate), `starfield_status_for` | ✓ VERIFIED | All present + tested; `entry_ci` lifted to `pub(crate)` body-unchanged. |
| `crates/loadorder/src/loot.rs` + `scan.rs` + `masterlist.rs` | 4 allow-list arms + snapshot wiring | ✓ VERIFIED | game_type/id/slug/appdata arms + `STARFIELD_SNAPSHOT` include_str!; 3-game tests. |
| `crates/loadorder/assets/starfield/masterlist.yaml` | Bundled CC0 loot/starfield@v0.29 snapshot | ✓ VERIFIED | 979 lines / 31 KB, valid YAML, include_str! target present (crate compiles). |
| `crates/testkit/src/lib.rs` | `fake_my_games_prefix` + `MyGamesOpts` (canonical/case-variant/marker/user.reg) | ✓ VERIFIED | Builder + self-test `fake_my_games_prefix_builds_shapes_on_request` asserts all shapes. |
| `src-tauri/src/commands/games.rs` + `lib.rs` | Thin `starfield_status` command, registered | ✓ VERIFIED | One-line forwarder `steam::starfield_status_for(appid).map_err(boundary_err)` — zero path/case-fold/drift logic; registered in `generate_handler!`. |
| `frontend/src/lib/api.ts` + `routes/+page.svelte` | Typed wrapper + first-launch gate + drift notice | ✓ VERIFIED | `starfieldStatus` wrapper + `StarfieldStatus`/`Ce2ConfigState`/`DriftNotice` types; svelte gate + notice wired (see note below). |

### Key Link Verification

| From | To | Via | Status |
|------|----|----|--------|
| `ce2.rs` | `resolve.rs::entry_ci` | shared case-fold, `pub(crate)`, not re-implemented | ✓ WIRED |
| `masterlist.rs` | `assets/starfield/masterlist.yaml` | `include_str!` (compiles) | ✓ WIRED |
| `games.rs::starfield_status` | `steam::starfield_status_for` | thin forwarder, no adapter logic | ✓ WIRED |
| `+page.svelte` | `api.starfieldStatus` → `starfield_status` command | `loadStarfieldStatus` on select + explicit Re-check | ✓ WIRED |
| `+page.svelte` first-launch gate | "Plugins & load order" management controls | `{#if isStarfield && starfieldPending}…{:else}…toolbar…{/if}` | ✓ WIRED — pending genuinely blocks management |
| `+page.svelte` drift notice | `starfield?.drift` | independent `{#if}`, never disables management | ✓ WIRED |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full workspace suite (orchestrator, once) | `cargo test --workspace --locked` | 284 passed, 0 failed | ✓ PASS |
| Case-mismatch resolves real on-disk casing | test `my_games_case_mismatch_resolves_real_on_disk_casing` | asserts mis-cased path + `is_dir()` | ✓ PASS |
| Frontend type/build | `npm run check` / `build` (orchestrator) | 0 errors, build OK | ✓ PASS |

### Requirements Coverage

| Requirement | Description | Status | Evidence |
|-------------|-------------|--------|----------|
| SFDET-01 | Add Starfield auto-detected with install dir + Proton prefix | ✓ SATISFIED | steam + loadorder allow-list arms + frontend title; positive `resolve_from_root` test |
| SFDET-02 | Resolve CE2 `Documents/My Games/Starfield`, Wine case-folding + pre-first-launch case | ✓ SATISFIED | `resolve_ce2_config` + case-mismatch fixture + first-launch typed state + UI gate |
| SFDET-03 | Detect installed version + version-drift warning | ✓ SATISFIED | `installed_build` + `drift_notice` + persistent notice; dormant at VALIDATED_BUILD=0 by design |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| — | — | No debt markers (TBD/FIXME/XXX), no stubs, no scope leakage | ℹ️ Info | Clean |
| `frontend/src/routes/+page.svelte` | — | File is non-UTF-8 (`file` reports `data`, pre-existing before this phase) — plain `grep` silently skips it as binary | ℹ️ Info | Not a Phase-6 regression (file was binary-flagged pre-phase); `grep -a` confirms full Starfield surfacing present + wired; `npm run check`/`build` pass. Flagged only as a tooling caveat. |

### Scope-Leakage Check

- Phase 7 (plugins.txt): `appdata_folder_name` arm added but NOTHING writes plugins.txt — no leakage. ✓
- Phase 8 (StarfieldCustom.ini): `ce2.rs` is read-only, resolves paths only — no INI writes. ✓
- Phase 9 (on-hardware): `VALIDATED_BUILD=0` dormant constant, no on-hardware value baked in. ✓

### Gaps Summary

No Phase-6 gaps. Every success criterion and requirement (SFDET-01/02/03) is delivered in shipped code with substantive, wired implementations and passing fixture/unit tests — including the MANDATORY case-mismatched-prefix test. The engine stays headless (steam owns all path construction), the Tauri command is a genuine thin forwarder, and the UI is presentation-only. The real-Proton-prefix confirmation, real installed-build baseline, and live drift/pending→Ready visual are the acknowledged, by-design Phase-9 on-hardware items (there is no live Starfield prefix on this host; the drift notice is intentionally dormant until Phase 9 seeds `VALIDATED_BUILD`).

One tooling caveat worth recording: `+page.svelte` is a non-UTF-8 file, so a naive `grep` returns a false-negative and appears to "miss" the surfacing. Forced-text inspection (`grep -a`) confirms the first-launch gate, Re-check action, and drift notice are all present and correctly wired.

---

_Verified: 2026-07-07_
_Verifier: Claude (gsd-verifier)_
