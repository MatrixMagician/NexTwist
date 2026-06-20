---
phase: 2
slug: multi-mod-management
status: draft
nyquist_compliant: false
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

> Populated by the planner per PLAN.md task (Task ID, automated `cargo test` command, requirement, status). Conflict-resolution and profile-switch tasks MUST include a round-trip-pristine assertion via the `testkit` harness (the safety invariant).

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 2-01-01 | 01 | 1 | CONF-01 | unit/integration | `cargo test -p <crate>` | ❌ W0 | ⬜ pending |

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

- [ ] All tasks have automated `cargo test` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers V2 migration + libloot spike
- [ ] Conflict-resolution and profile-switch tasks assert round-trip-pristine (safety invariant)
- [ ] `nyquist_compliant: true` set in frontmatter
- [ ] No watch-mode flags

**Approval:** pending
