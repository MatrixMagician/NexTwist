---
phase: 8
slug: reversible-starfieldcustom-ini-activation
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (block_on: high)
threats_open: 0
asvs_level: 1
created: 2026-07-08
---

# Phase 8 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.
> Phase 8 adds the FIRST sanctioned engine write OUTSIDE the `Data/` deploy root
> (`StarfieldCustom.ini` under the Proton prefix). Verification config: `asvs_level: 1`,
> `block_on: high`. Register authored at plan time; this audit verifies each declared
> mitigation exists in the implemented code (mitigations, not intent).

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Proton prefix filesystem → engine | `My Games/Starfield` path derives from an attacker-influenceable `user.reg` `Personal` redirect + on-disk casing; untrusted until re-verified in-`drive_c` at the write site | resolved INI target path |
| existing `StarfieldCustom.ini` bytes → byte editor | A pre-existing user/third-party INI is untrusted input to the surgical merge — slice only, never execute/eval | INI file bytes (BOM/EOL/keys) |
| deploy/purge choke point → INI op | The INI hook runs at the deploy/purge tail; a Starfield gate decides whether the out-of-`Data/` write happens at all | `game.appid` gate |
| journal replay (crash recovery) → filesystem | `recover_on_launch` replays a pending `KIND_INI` row before the UI is served; must converge, never corrupt | journal intent + provenance row |
| webview (frontend) → Tauri command | IPC args (`appid`, `resolution`) cross the trust boundary; the adapter validates the game via `require_game` and forwards to the engine, which owns all path/write safety | `appid: u32`, `resolution` enum |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-08-01 | Tampering/Elevation | `my_games_path` result → INI write site (`ensure_ini_active`) | high | mitigate | `resolve_ini_target` re-verifies lexical `<prefix>/drive_c` containment at the write site → `PathEscape` on escape | closed |
| T-08-02 | Tampering | symlink at the `StarfieldCustom.ini` target | high | mitigate | `refuse_symlink` (`fs::symlink_metadata` + `is_symlink` refusal); writes a regular file only via temp+rename | closed |
| T-08-03 | Elevation | `KIND_INI` replay arm + choke wiring + verify/repair INI check | high | mitigate | `replay_ini` / `ini_drift` resolve via `my_games_path` only, never `resolve_target`/`guard_within_root`; `Data/`-root guard + orphan walk byte-for-byte untouched | closed |
| T-08-04 | DoS/Integrity | crash mid-INI-write / mid-purge replayed by `recover_on_launch` | medium | mitigate | Atomic temp-file + `fs::rename`; intent-before-act `begin_ini` durable before write; idempotent `replay_ini`; purge journals restore intent BEFORE the `Data/` loop (WR-01) | closed |
| T-08-05 | Tampering | purge/restore deleting a user file it did not create | high | mitigate | Three-valued provenance from `vanilla_backup` row (never inferred from disk); CR-01 guard forbids downgrading a real-hash row to `ABSENCE_MARKER`; `remove_dir` refuses non-empty | closed |
| T-08-06 | Tampering | clobbering a non-empty user `sResourceDataDirsFinal` | high | mitigate | `plan_merge` → `Conflict` under `Block` → `Ok(IniOutcome::Blocked)` (no write); overwrite only on `UseNexTwist` after whole-file backup; UTF-16 refused (WR-02) | closed |
| T-08-07 | Tampering | non-Starfield games gaining an unintended INI side-effect | high | mitigate | Every hook behind `game.appid == steam::STARFIELD` (deploy/deploy_winners/purge/verify/repair); non-Starfield control tests | closed |
| T-08-08 | Tampering/Spoofing | IPC args driving an out-of-scope write | medium | mitigate | Thin Tauri adapter: `require_game` + one `deploy::` call + `boundary_err`; zero path/IO/merge logic at the boundary; engine owns all safety | closed |
| T-08-09 | Repudiation/Integrity | a silent INI edit the user did not see | medium | mitigate | No-silent-edit: `openIniModal` opens the preview only; write authorized ONLY by the modal confirm (`onActivateIni`) / `onUseNexTwistValue`; preview writes nothing | closed |
| T-08-SC | Tampering | npm/pip/cargo installs | low | accept | Phase adds ZERO external crates (std-only INI editor); no `cargo-deny`/MSRV/AppImage surface change. See Accepted Risks Log | closed |

*Status: open · closed · open — below {block_on} threshold (non-blocking)*
*Severity: critical > high > medium > low — with `block_on: high`, only open threats at high or critical count toward `threats_open`*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

### Evidence (file:line, code-verified)

| Threat ID | Primary control | Regression test |
|-----------|-----------------|-----------------|
| T-08-01 | `crates/deploy/src/gameconfig.rs:435` (`resolve_ini_target`) → `:581-595` (`verify_contained` → `starts_with(drive_c)`, `PathEscape`) | `gameconfig.rs:990` `verify_contained_refuses_an_escaping_target` |
| T-08-02 | `crates/deploy/src/gameconfig.rs:436` (`refuse_symlink`) → `:600-608` (`symlink_metadata` + `is_symlink`) | `gameconfig.rs:973` `ensure_refuses_to_write_through_a_symlink` |
| T-08-03 | `crates/deploy/src/journal.rs:214-216` (`replay_ini` via `my_games_path`, no `resolve_target`); `crates/deploy/src/verify.rs:121-122, 214-222` (gated, delegates to `gameconfig::ini_drift`/`ensure_ini_active` only) | `journal.rs:239` `replay_ini_resolves_under_prefix_not_data_and_rolls_back`; `tests/ini_activation.rs:466` `ini_kind_path_under_prefix` |
| T-08-04 | `crates/deploy/src/gameconfig.rs:613-624` (`atomic_write` temp+rename), `:464/:478` (`begin_ini` before write, `mark_done` after); `crates/deploy/src/engine.rs:455-467` (purge journals restore intent BEFORE `Data/` loop — WR-01) | `tests/ini_activation.rs:369` `ini_crash_recovery_consistent`; `:416` `ini_purge_crash_window_recovers_via_journal` |
| T-08-05 | `crates/deploy/src/gameconfig.rs:556-576` (`restore_ini_at` provenance branch); `:471-474` (CR-01 guard `!pre_existing && vanilla_for(..).is_none()`); `:631-647` (`prune_created_dirs`, `remove_dir` refuses non-empty); `crates/store/src/vanilla.rs:57-65` (`remove_vanilla`) | `tests/ini_activation.rs:572` `ini_reactivation_of_absent_preexisting_preserves_original_on_purge` (CR-01); `gameconfig.rs:957` `restore_is_a_safe_noop_when_never_activated` |
| T-08-06 | `crates/deploy/src/gameconfig.rs:381-384` (`Conflict` under `Block`), `:445-452` (`Ok(Blocked)` / `UnsupportedEncoding`, no write), `:316-317` (UTF-16 refusal — WR-02) | `gameconfig.rs:892` `ensure_blocks_nonempty_user_value_and_use_nextwist_overwrites`; `tests/ini_activation.rs:285` `ini_utf16_bom_is_refused_no_write` |
| T-08-07 | `crates/deploy/src/engine.rs:125, 296, 455` + `crates/deploy/src/verify.rs:121, 216` (all behind `game.appid == steam::STARFIELD`) | `tests/ini_activation.rs:498` `nonstarfield_deploy_touches_no_ini`; `:657` `ini_verify_ignores_nonstarfield` |
| T-08-08 | `src-tauri/src/commands/gameconfig.rs:17-35` (`require_game` + one `deploy::` call + `boundary_err`; no path/IO/merge) | `cargo clippy -p nextwist` clean; adapter is 3-5 lines/command |
| T-08-09 | `frontend/src/routes/+page.svelte:246-247` (`openIniModal` sets `iniModalOpen` only), `:251-256/:262-264` (writes only via `onActivateIni`/`onUseNexTwistValue`), `:1600` (Enable button → `openIniModal`); engine `preview_ini_activation` writes nothing (`gameconfig.rs:502-516`) | svelte-check + build green (08-03-SUMMARY D3) |
| T-08-SC | `tech-stack.added: []` in 08-01/02/03 SUMMARYs; no `Cargo.toml` dependency added | `cargo tree -p nextwist-deploy` unchanged vs `main` |

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-08-SC | T-08-SC | Phase 8 adds ZERO external crates: the surgical `StarfieldCustom.ini` editor is std-only (`std::fs/io/path` + existing `serde`); the frontend reuses existing classes with no npm dependency. RESEARCH Package Legitimacy Audit = negative result. No `cargo-deny`/MSRV/AppImage surface change, so no supply-chain install surface exists to attack. Verified: `tech-stack.added: []` across all three plan summaries. | gsd-security-auditor (verified against implementation) | 2026-07-08 |

*Accepted risks do not resurface in future audit runs.*

---

## Unregistered Flags

No `## Threat Flags` section is present in 08-01-SUMMARY.md, 08-02-SUMMARY.md, or 08-03-SUMMARY.md — no new attack surface was flagged by the executors during implementation beyond the plan-time register. No unregistered flags.

Note: the deep code review (08-REVIEW.md) surfaced three security-relevant findings during implementation (CR-01 provenance-downgrade data-loss, WR-01 interrupted-purge crash window, WR-02 UTF-16 silent clobber). All three are FIXED and each fix is code-verified above (they strengthen T-08-05, T-08-04, T-08-06 respectively). IN-01 (`DeployError::IniConflict` is a dead, never-constructed variant — conflicts surface as the `Ok(IniOutcome::Blocked)` success value instead) and IN-02 (frontend `VerifyReport` TS type omits `ini_drift`) are non-security info items; IN-02 is Phase-9-scoped. Neither affects any threat disposition.

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-07-08 | 10 | 10 | 0 | gsd-security-auditor |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed (0 open threats at or above `high`; nothing below-threshold open either)
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-07-08
