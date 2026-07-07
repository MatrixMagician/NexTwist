---
phase: 7
slug: starfield-load-order
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-07
---

# Phase 7 — Validation Strategy

> Per-phase validation contract, derived from `07-RESEARCH.md` § Validation Architecture.
> Phase 7 is headless-testable via fixtures; LOOT-vs-desktop parity + real on-launch
> deltas are the Phase-9 on-hardware confirmations (MEDIUM-confidence, kept data-driven).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `#[test]` + integration tests (cargo test); `npm run check` for frontend |
| **Config file** | none — workspace `Cargo.toml` |
| **Quick run command** | `cargo test -p nextwist-loadorder --locked` |
| **Full suite command** | `cargo test --workspace --locked` (+ `cargo clippy --workspace --all-targets -- -D warnings`) |
| **Estimated runtime** | ~60–120 seconds |

---

## Sampling Rate

- **After every task commit:** quick run for the touched crate(s)
- **After every plan wave:** `cargo test --workspace --locked` + clippy; `npm --prefix frontend run check` for UI tasks
- **Before `/gsd-verify-work`:** full suite green + clippy clean
- **Max feedback latency:** ~120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Secure Behavior | Test Type | Automated Command | Status |
|---------|------|------|-------------|-----------------|-----------|-------------------|--------|
| _(planner populates via must_haves)_ | — | — | SFLO-01/02/03/04 | protected masters never written/reordered; plugins.txt write stays under prefix AppData | unit/fixture | `cargo test -p nextwist-loadorder` | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Per `07-RESEARCH.md` § Validation Architecture:

- [ ] `crates/testkit/src/lib.rs` — a Starfield plugin fixture: a fake `AppData/Local/Starfield/`
      with `plugins.txt` + synthetic plugins including a **medium-master** (esplugin medium
      flag set), a **protected/implicitly-active** master, and regular plugins
- [ ] A **synthetic on-launch-rewritten `plugins.txt`** fixture for SFLO-04 (contains `.ccc`
      entries, stripped implicit ESMs, re-added `BlueprintShips-*`) to prove
      `reconcile_plugins_txt` classifies those as EXPECTED (InSync), and a second fixture
      with an UNEXPECTED delta to prove it returns Drift(names)

*Existing cargo/`#[test]` infra + the v1.0 `fake_proton_prefix` builder cover the rest.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| LOOT sort output agrees with LOOT desktop on the same inputs | SFLO-02 | Requires LOOT desktop + a real Starfield load order | Phase 9 on-hardware |
| Exact on-launch `plugins.txt` rewrite delta set | SFLO-04 | Only the real game defines it | Phase 9 confirms; logic is data-driven (self-corrects) |
| Real Starfield implicitly-active/protected master set | SFLO-03 | libloot proxy validated against a live prefix | Phase 9; determination stays libloot-driven, never hard-coded |

*All other Phase 7 behaviors have automated (unit/fixture) verification.*

---

## Validation Sign-Off

- [ ] All tasks have automated verify or Wave 0 dependencies
- [ ] Protected-master immutability asserted (engine rejects reorder/disable — defense-in-depth)
- [ ] Medium-master classification asserted against a fixture with the esplugin medium flag
- [ ] Sort determinism asserted against the bundled masterlist
- [ ] SFLO-04 reconciliation asserted (InSync on expected deltas, Drift on unexpected)
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
