---
phase: 8
slug: reversible-starfieldcustom-ini-activation
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-08
---

# Phase 8 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness (`cargo test`) + `nextwist-testkit` (blake3 `DIR_SENTINEL` pristine-tree assertions) |
| **Config file** | none — workspace `Cargo.toml`; toolchain pinned via `rust-toolchain.toml` |
| **Quick run command** | `~/.cargo/bin/cargo test -p nextwist-deploy` |
| **Full suite command** | `~/.cargo/bin/cargo test --workspace --locked` |
| **Estimated runtime** | ~60–120 seconds (workspace) |

---

## Sampling Rate

- **After every task commit:** Run `~/.cargo/bin/cargo test -p nextwist-deploy`
- **After every plan wave:** Run `~/.cargo/bin/cargo test --workspace --locked` + `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings`
- **Before `/gsd-verify-work`:** Full workspace suite must be green (cross-crate integration gate)
- **Max feedback latency:** ~120 seconds

---

## Per-Task Verification Map

*Planner fills one row per task. Every SFINI requirement below must map to at least one `<automated>` test.*

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 8-01-01 | 01 | 1 | SFINI-01 | — | Writes `[Archive]` keys only after confirm; no silent edit | unit | `cargo test -p nextwist-deploy gameconfig` | ❌ W0 | ⬜ pending |
| 8-01-02 | 01 | 1 | SFINI-02 | T-8-01 | Restore = original bytes (pre-existing) or delete+prune (created) | unit | `cargo test -p nextwist-deploy ini_restore` | ❌ W0 | ⬜ pending |
| 8-01-03 | 01 | 1 | SFINI-03 | T-8-02 | Surgical merge preserves CRLF/BOM + other keys; conflict on user `sResourceDataDirsFinal` | unit | `cargo test -p nextwist-deploy ini_merge` | ❌ W0 | ⬜ pending |
| 8-01-04 | 01 | 1 | SFINI-04 | — | Deploy×2 / profile-switch / crash-replay → one `[Archive]`, identical bytes | integration | `cargo test -p nextwist-deploy ini_idempotent` | ❌ W0 | ⬜ pending |
| 8-01-05 | 01 | 1 | SFINI-05 | — | `recover_on_launch` + verify/repair cover INI; `DIR_SENTINEL` pristine both branches | integration | `cargo test -p nextwist-deploy ini_reversibility` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky. Task IDs are indicative — planner reconciles to actual plan/wave layout.*

---

## Wave 0 Requirements

- [ ] Reversibility suite extending `nextwist-testkit` `DIR_SENTINEL` to cover a target OUTSIDE the `Data/` root (the `My Games/Starfield/StarfieldCustom.ini` sentinel path) — both provenance branches.
- [ ] Test fixtures: a Proton-prefix My-Games tree builder (pre-existing INI with CRLF+BOM+extra keys; and absent-INI first-launch case).

*Existing `nextwist-deploy` crash-recovery + vanilla-restore harnesses cover the journal/backup substrate; only the INI-specific cases above are new.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Exact CE2 INI-key recipe loads loose files in-game | SFINI-01 (recipe currency) | Requires the real game build on Proton hardware | Deferred to Phase 9 on-hardware gate (SFVER-01) — treat the key recipe as validated-against-a-build, not a constant |

*All reversibility/byte-fidelity behaviors have automated verification; only the in-game load-effect of the key recipe is manual (Phase 9).*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
