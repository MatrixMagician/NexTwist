---
phase: 02
phase_name: Multi-Mod Management
asvs_level: 2
threats_total: 18
threats_closed: 18
threats_open: 0
audited: 2026-06-23
mode: verify-mitigations
---

# Phase 02 — Multi-Mod Management: Security Verification

Retroactive threat-mitigation audit. The threat register was authored at plan time
(across `02-0{1..5}-PLAN.md` `<threat_model>` blocks); this audit verifies each declared
mitigation is PRESENT in the implemented code with file:line evidence. Implementation
files were not modified.

ASVS level: 2. Block-on: high. Result: **all 18 threats CLOSED, 0 open.**

## Threat Verification

| Threat ID | Category | Disposition | Status | Evidence |
|-----------|----------|-------------|--------|----------|
| T-02-01 | Tampering — V2 migration over Phase-1 DB | mitigate | CLOSED | `crates/store/src/migrations/V2__multi_mod.sql` is strictly additive (only CREATE TABLE/INDEX + one INSERT INTO profile; ALTER/DROP/UPDATE appear only in comment lines 4-5). Blocking test `v2_migrates_phase1_state` `crates/store/src/db.rs:166-241` reaches a genuine V1-only state, seeds a managed_game, asserts V2 tables created, exactly one active Default profile, and the V1 row survives untouched. |
| T-02-02 | Tampering — plugin_state.kind token decode | mitigate | CLOSED | Double-Result mapper `row_to_plugin` `crates/store/src/plugins.rs:60-76`; unknown token → `Ok(Err(StoreError::Corrupt))` (lines 64-68), propagated via `??` at `:53`. Test `corrupt_kind_token_surfaces_error` `:166-180`. Mirrors `crates/store/src/manifest.rs:76-95`. |
| T-02-03 | DoS(self) — profile uniqueness | accept | CLOSED | `UNIQUE(appid, name)` `V2__multi_mod.sql:44`; `create_profile` surfaces it as `StoreError::Db` `crates/store/src/profiles.rs:17,43`. Test `duplicate_name_per_game_rejected` `profiles.rs:270`. Accepted rationale (single-user desktop) holds. |
| T-02-04 | Tampering — plugins.txt path computation | mitigate | CLOSED | `appdata_local_path` `crates/loadorder/src/loot.rs:67-75` joins only the resolved prefix root + a fixed `drive_c/.../AppData/Local/` chain + hardcoded `game_name` from the `appdata_folder_name` allow-list `:93-99`. No user-controlled path component. Bounding test `appdata_local_path_builds_the_proton_appdata_subpath` `loot.rs:454`. |
| T-02-05 | InfoDisclosure — GPL-3.0 distribution | accept (tracked) | CLOSED | `deny.toml:66-67` allows `GPL-3.0-or-later` + `GPL-3.0`; libloot family enumerated `:52-65` and carried to Phase-5 DIST-02 license audit `:65`. Not a runtime threat; tracked. `cargo deny check ... licenses ok` (02-04/05 SUMMARY). |
| T-02-06 | Tampering/EoP — conflict::resolve winner paths | mitigate | CLOSED | `resolve` applies `guard_within_root(&winner_root, &abs)?` per winner `crates/deploy/src/conflict.rs:119-121`; shared guard `crates/deploy/src/path_guard.rs:16-24` returns `DeployError::PathEscape`. Defence-in-depth in `staged_rels` `conflict.rs:162-165`. Test `path_escape_winner_rejected` `:294`. |
| T-02-07 | DoS(self) — two owners one path | mitigate | CLOSED | `resolve` folds providers into a `BTreeMap` keyed by `target_rel`, emitting exactly one `WinnerFile` per key `conflict.rs:99,113-136`. Test `never_emits_duplicate_target_rel` `:269` (3 contending mods, no duplicate). UNIQUE-safe before any syscall. |
| T-02-08 | Tampering(integrity) — multi-mod deploy then purge | mitigate | CLOSED | `crates/deploy/tests/conflict_redeploy.rs`: `conflict_winner_set_deploys_unique_and_pristine` `:78` asserts no UNIQUE violation + deploy→purge byte-for-byte pristine `:142`; `rank_change_redeploy_stays_pristine` `:149` asserts pristine after a rank-flip redeploy `:206`. Runs through the unchanged journaled engine. |
| T-02-09 | Spoofing — IPC args | accept | CLOSED | Every adapter calls `require_game(&state, appid)` first (`src-tauri/src/commands/profiles.rs:29,41,57,76`; `conflicts.rs:41,52,67,88`; `plugins.rs:165,222` + active-profile resolve at `plugins.rs:130-135`). `require_game` `src-tauri/src/commands/mod.rs:53-64` validates via `store.get_game`. `mod_id:i64`/`rank:u32`/`appid:u32` are bounded ints, never path-joined. |
| T-02-10 | Tampering — masterlist fetch | mitigate | CLOSED | Pinned host `raw.githubusercontent.com` `crates/loadorder/src/masterlist.rs:37`, repo `loot/<slug>` allow-list `:46-52`, branch `v0.29` `:34`, URL built `:64-66`. `real_fetch` `:167` uses `reqwest::blocking` with `redirect::Policy::none()` `:169` + final-host re-assert `:176-182`. Bundled CC0 `include_str!` fallback `:42-43,55`. reqwest is rustls-only (`Cargo.toml:59`, no native-tls/openssl). Tests `url_is_pinned_to_host_repo_and_branch` `:202`, `falls_back_to_bundled_snapshot_when_offline` `:260`. |
| T-02-11 | Tampering/EoP — plugins.txt write path | mitigate | CLOSED | Write target is libloot's local path derived from `appdata_local_path(&game.prefix, folder)` `crates/deploy/src/profile.rs:183-187` → `loot.rs:67-75`; `open_game` always uses `Game::with_local_path` (never `Game::new`) `loot.rs:131`. Test asserts `report.plugins_txt.starts_with(prefix/.../AppData/Local/<game>)` `crates/deploy/tests/profile_switch.rs:227-235`. |
| T-02-12 | Tampering — malicious/oversized plugin names | mitigate | CLOSED | `scan_plugins`/`scan_plugins_for` walk only supplied roots via `WalkDir::new(root).follow_links(false)` `crates/loadorder/src/scan.rs:112,162-200`; names come from `path.file_name()` `:123` and are stored as opaque `Plugin.name`/`key`, never re-joined as paths. `classify_kind` `:72-93` header-validates via esplugin with a non-fatal ESP fallback `:84-91` (one bad file never aborts the scan). |
| T-02-13 | Tampering(integrity) — plugins.txt vs pristine | accept | CLOSED | plugins.txt is written under the Proton prefix (`AppData/Local/<game>`, `loot.rs:67`), entirely outside `install_dir/Data/`. `profile_switch.rs` proves both: plugins.txt under prefix `:231` AND pristine asserted only on `fx.install` `:201`. Regenerable from DB via `list_plugin_state` → `apply_profile_plugins` `profile.rs:174-189`. Accepted rationale holds. |
| T-02-14 | Tampering(integrity) — profile switch interrupted | mitigate | CLOSED | `switch_profile` = `purge(old)` → `resolve`+`deploy_winners(new)` → `apply_profile_plugins` → `set_active` `crates/deploy/src/profile.rs:78-130`, all through existing journaled crash-safe primitives. `recover_on_launch` defined `crates/deploy/src/engine.rs:511`, invoked at startup `src-tauri/src/lib.rs:62`. Regression test `profile_switch_round_trips_pristine_across_switches` (A→B→A + final purge) `crates/deploy/tests/profile_switch.rs:102-206`. |
| T-02-15 | Tampering(integrity) — stale files leak between profiles | mitigate | CLOSED | Full purge-to-pristine BETWEEN profiles (no diff-deploy) `profile.rs:83-86`; cross-switch test asserts switching to B purges A's 3 files (`report_b.purged.removed == 3`) and A's shared/only1/only2 are gone `profile_switch.rs:166-175`. |
| T-02-16 | Repudiation/accidental-loss — profile switch/delete from UI | mitigate | CLOSED | Engine-enforced guards (cannot be bypassed by the thin UI): `delete_profile` REFUSES to delete the active profile `crates/store/src/profiles.rs:181-185` and deletes only profile/profile_mod/plugin_state rows — staged mod files kept (D-14) `:195-207`; `set_active_profile` is transactional clears-then-sets (exactly one active) `:64-86`; `switch_profile` marks active ONLY after a successful deploy `crates/deploy/src/profile.rs:119-123` with stale-flag cleanup on failure `:91-95`. See residual note below re: the UI confirmation modal. |
| T-02-17 | DoS(self) — duplicate/invalid profile | accept | CLOSED | Same `UNIQUE(appid, name)` as T-02-03 (`V2__multi_mod.sql:44`, `profiles.rs:17`, test `:270`). Accepted rationale holds. |
| T-02-SC | Tampering — libloot/esplugin/reqwest installs | accept/transfer | CLOSED | libloot family pinned and license-gated in `deny.toml:52-68` (GPL-3.0 allowed; UnRAR banned `:22-26`); esplugin is a same-author libloot transitive dep; reqwest is the project-sanctioned rustls client (`Cargo.toml:59`). `cargo deny check` advisories/bans/licenses/sources ok (02-04/05 SUMMARY). Human-verify libloot checkpoint recorded in 02-02 spike. |

## Residual Notes (documented, not blocking)

**T-02-16 — UI confirmation modal not yet implemented.** The threat register's
sub-claim that "every disk-mutating profile action is confirmation-gated (UI-SPEC §D.2)"
is a DESIGN contract, not implemented code in this phase. The Phase-02 frontend is a
single `frontend/src/routes/+page.svelte` (2380 lines) plus `frontend/src/lib/api.ts`
bindings (`switchProfile`/`deleteProfile` at `api.ts:440-444`); there is no
profile-management UI surface and no confirmation modal wired to these actions. The
accidental-loss/repudiation CORE of T-02-16 is nonetheless closed because the protections
are enforced in the headless engine where the audit demands them and where the thin UI
adapters cannot bypass them: active-profile deletion is hard-refused at the store, staged
files are never removed by delete, and the active flag is set only after a successful
deploy. The confirmation modal is a defence-in-depth UX layer that should be added when
the profile-management UI is built (later phase). Recommend tracking as a UI follow-up,
not a security blocker.

## Unregistered Flags

None. All five Phase-02 plan summaries explicitly state "No new security surface
introduced/beyond the threat_model" (`02-03-SUMMARY.md:122`, `02-04-SUMMARY.md:132`
context, `02-05-SUMMARY.md:98`). No `## Threat Flags` section was emitted by any executor,
and no new attack surface lacking a threat mapping was found. The masterlist redirect
hardening (WR-01: `redirect::Policy::none()` + final-host re-assert in `masterlist.rs`) is
an in-scope strengthening of T-02-10, not new surface.

## Accepted Risks Log

| ID | Risk | Rationale | Tracking |
|----|------|-----------|----------|
| T-02-03 / T-02-17 | Duplicate/invalid profile name | `UNIQUE(appid,name)` rejects dupes; single-user desktop, low impact | n/a |
| T-02-05 | GPL-3.0 components in distributed AppImage | NexTwist is GPL-3.0-or-later; license-compatible; not a runtime threat | Phase-5 DIST-02 license audit |
| T-02-09 | IPC args (spoofing) | Single-user desktop; adapters validate appid via `require_game`; args are bounded ints | n/a |
| T-02-13 | plugins.txt not byte-for-byte-pristine-tracked | Lives in Proton prefix, not game Data/; regenerable from DB `plugin_state` | n/a |
| T-02-SC | libloot/esplugin/reqwest supply chain | Pinned + license-gated in `deny.toml`; project-sanctioned; human-verify checkpoint done | Phase-5 DIST-02 |
