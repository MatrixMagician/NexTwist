---
phase: 8
slug: reversible-starfieldcustom-ini-activation
status: draft
nyquist_compliant: true
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
| 8-01-01 | 01 | 1 | SFINI-03 | T-08-02 | Surgical merge preserves BOM/CRLF/comments/order; one `[Archive]`; new file CRLF+no-BOM; conflict on non-empty user `sResourceDataDirsFinal` | unit | `~/.cargo/bin/cargo test -p nextwist-deploy gameconfig` | ❌ W0 (created in-task) | ⬜ pending |
| 8-01-02 | 01 | 1 | SFINI-01, SFINI-02, SFINI-04 | T-08-01, T-08-02, T-08-04, T-08-05, T-08-06 | Preview reports intent w/o writing; provenance restore (bytes) or absence+prune (created); atomic write; KIND_INI rides `my_games_path`, Data/ guard untouched | unit | `~/.cargo/bin/cargo test -p nextwist-deploy gameconfig && ~/.cargo/bin/cargo test -p nextwist-deploy journal` | ❌ W0 (created in-task) | ⬜ pending |
| 8-02-01 | 02 | 2 | SFINI-04, SFINI-05 | T-08-03, T-08-04, T-08-05, T-08-07 | Reversibility suite (both provenance branches, idempotency, crash-recovery, conflict, CRLF/BOM, KIND_INI-path guard, non-Starfield control) — RED-first | integration | `~/.cargo/bin/cargo test -p nextwist-deploy --test ini_activation` | ❌ W0 (this task creates it) | ⬜ pending |
| 8-02-02 | 02 | 2 | SFINI-04, SFINI-05 | T-08-03, T-08-07 | `is_starfield`-gated hooks at deploy/deploy_winners/purge tails → suite GREEN; Data/ path regression-free | integration | `~/.cargo/bin/cargo test -p nextwist-deploy --test ini_activation && ~/.cargo/bin/cargo test -p nextwist-deploy` | ❌ W0 | ⬜ pending |
| 8-03-01 | 03 | 2 | SFINI-01, SFINI-03 | T-08-06, T-08-08 | Thin `preview_ini_activation` / `apply_ini_activation` adapter + api.ts bindings | compile | `~/.cargo/bin/cargo check -p nextwist && npm --prefix frontend run check` | ❌ W0 | ⬜ pending |
| 8-03-02 | 03 | 2 | SFINI-01, SFINI-03 | T-08-06, T-08-09 | No-silent-edit preview modal + amber conflict box (keep-mine / use-NexTwist), existing classes only | compile | `npm --prefix frontend run check && npm --prefix frontend run build` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky. Wave-0 test scaffolds are created inside the same task/plan (unit tests co-located with code; the integration suite is 8-02-01, RED-first before the 8-02-02 wiring).*

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
