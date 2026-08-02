---
phase: 6
slug: starfield-detection-ce2-path-resolution
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-07
---

# Phase 6 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Derived from `06-RESEARCH.md` § Validation Architecture. Phase 6 is fully
> headless-testable — no on-hardware step (that is Phase 9).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` + integration tests (cargo test) |
| **Config file** | none — workspace `Cargo.toml` |
| **Quick run command** | `cargo test -p nextwist-loadorder -p nextwist-steam --locked` |
| **Full suite command** | `cargo test --workspace --locked` |
| **Estimated runtime** | ~30–90 seconds (headless crates; no WebKitGTK needed for the engine crates) |

---

## Sampling Rate

- **After every task commit:** Run the quick run command for the touched crate(s)
- **After every plan wave:** Run `cargo test --workspace --locked` + `cargo clippy --workspace --all-targets -- -D warnings`
- **Before `/gsd-verify-work`:** Full suite green + clippy clean
- **Max feedback latency:** ~90 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| _(planner populates via must_haves)_ | — | — | SFDET-01/02/03 | — | non-destructive: Phase 6 performs no writes to the game/prefix | unit/fixture | `cargo test -p nextwist-loadorder` | — | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Per `06-RESEARCH.md` § Validation Architecture, the one infrastructure gap is a
`My Games` fixture variant in `testkit`:

- [ ] `crates/testkit/src/lib.rs` — add a `fake_proton_prefix` variant that seeds
      `drive_c/users/steamuser/Documents/My Games/Starfield/` (mirrors the existing
      `AppData/Local` fixture) for the `my_games_path` resolver tests
- [ ] A **case-mismatched** prefix fixture (e.g. `documents/my games/starfield`) —
      MANDATORY for SFDET-02 success criterion 2 (verify case-folded resolution)
- [ ] First-launch-not-done fixture: a prefix with the `My Games/Starfield` dir absent

*Existing cargo/`#[test]` infrastructure covers everything else — no framework install.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real Proton prefix Documents-redirection edge case | SFDET-02 | Requires a live Proton Starfield prefix; default `steamuser/Documents` path covers ~all installs, redirect read is defensive | Deferred to Phase 9 on-hardware validation |
| Exact `VALIDATED_BUILD` baseline `buildid` value | SFDET-03 | The real baseline is only knowable on the owner's installed build | Phase 6 ships compare logic + placeholder; Phase 9 sets the real constant |

*All other Phase 6 behaviors have automated (unit/fixture) verification.*

---

## Validation Sign-Off

- [ ] All tasks have automated verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers the `testkit` My-Games + case-mismatched fixtures
- [ ] No watch-mode flags
- [ ] Feedback latency < 90s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
