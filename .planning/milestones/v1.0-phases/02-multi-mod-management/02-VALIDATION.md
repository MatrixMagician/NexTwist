---
phase: 2
slug: multi-mod-management
status: approved
nyquist_compliant: true
wave_0_complete: false
created: 2026-06-20
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness (`cargo test`) + property/integration tests (Phase-1 pattern); `testkit` round-trip-pristine harness reused |
| **Config file** | none — workspace `Cargo.toml`; libloot/refinery deps added per plan |
| **Quick run command** | `cargo test -p <crate>` (the crate touched by the task) |
| **Full suite command** | `cargo test --workspace` |
| **Lint/deny gates** | `cargo clippy --workspace --all-targets -- -D warnings` and `cargo deny check` (license gate — note libloot is GPL-3.0-or-later) |
| **Estimated runtime** | ~30–90 seconds workspace (Phase-1 baseline: 82 tests) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p <crate>` for the touched crate.
- **After every plan wave:** Run `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings`.
- **Before `/gsd-verify-work`:** Full suite green + clippy 0 + `cargo deny check` ok.
- **Max feedback latency:** ~90 seconds.

---

## Per-Task Verification Map

> Authoritative per-task automated commands live in each PLAN.md `<verify><automated>` block. The map below summarizes the verification command set per plan/wave (extracted from the 5 plans). `[PRISTINE]` rows assert byte-for-byte round-trip pristine via the `testkit` DIR_SENTINEL harness — the safety invariant. `[BLOCKING]` = phase cannot pass without it.

| Plan | Wave | Requirements | Key automated commands | Pristine/Blocking | Status |
|------|------|--------------|------------------------|-------------------|--------|
| 02-01 | 1 | PROF-01, PROF-03, CONF-02 | `cargo test -p nextwist-core model::`; `cargo test -p nextwist-store` (incl. `migrations::v2_migrates_phase1_state`); `cargo deny check`; `cargo clippy -p nextwist-store -- -D warnings` | `[BLOCKING]` V2 migration applies-clean + Phase-1→Default migration | ⬜ pending |
| 02-02 | 1 | PLUGIN-01/02/03 | `cargo build -p nextwist-loadorder`; `cargo test -p nextwist-testkit fake_proton_prefix`; `cargo test -p nextwist-loadorder --test libloot_spike` | libloot install gated by human-verify checkpoint (autonomous:no) | ⬜ pending |
| 02-03 | 2 | CONF-01/02/03 | `cargo test -p nextwist-deploy conflict`; `cargo test -p nextwist-deploy --test conflict_redeploy`; `cargo build -p nextwist && (cd frontend && npm run check)` | `[PRISTINE]` conflict_redeploy | ⬜ pending |
| 02-04 | 2 | PLUGIN-01/02/03 | `cargo test -p nextwist-loadorder scan`; `cargo test -p nextwist-loadorder --test plugins`; `cargo deny check`; build + `npm run check` | — | ⬜ pending |
| 02-05 | 3 | PROF-01/02/03 | `cargo test -p nextwist-deploy --test profile_switch`; `cargo build -p nextwist && (cd frontend && npm run check)` | `[PRISTINE]` profile_switch (pristine across switches) | ⬜ pending |

Wave gate after each wave: `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo deny check`.

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] V2 refinery migration test fixture (schema applies cleanly over V1; Phase-1 single-mod state migrates into a "Default" profile).
- [ ] `testkit` extension if needed for multi-mod / profile-switch pristine snapshots (DIR_SENTINEL harness already exists — reuse).
- [ ] libloot spike test (de-risk A1/A3): one-library plugin enable + sort + `Game::with_local_path` round-trip against a real Proton-prefix AppData path.

*Existing Phase-1 infrastructure (`cargo test`, testkit pristine harness) covers most phase requirements; the above are the new fixtures.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| plugins.txt actually loads the chosen plugins/order in-game under Proton | PLUGIN-01/02 | Requires launching the real game via Steam Proton; cannot assert in-game load headlessly | Enable/order plugins → Deploy → launch Skyrim SE/FO4 via Steam → confirm plugins active + ordered in-game |
| Exact Proton-prefix `AppData/Local/<GameName>` folder name (A3) | PLUGIN-01/02 | Real prefix folder name (e.g. "Skyrim Special Edition" vs other) needs on-hardware confirmation | Resolve a real prefix, confirm `Game::with_local_path` path round-trips and `Plugins.txt` is written to the right place |
| LOOT auto-sort produces a sane order for a real load order | PLUGIN-03 | Masterlist-driven sort quality is best judged against a real mod list | Install several real plugins → "Sort with LOOT" → sanity-check order + warnings |

---

## Validation Sign-Off

- [x] All tasks have automated `cargo test` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers V2 migration + libloot spike
- [x] Conflict-resolution and profile-switch tasks assert round-trip-pristine (safety invariant)
- [x] `nyquist_compliant: true` set in frontmatter
- [x] No watch-mode flags

**Approval:** approved 2026-06-20
