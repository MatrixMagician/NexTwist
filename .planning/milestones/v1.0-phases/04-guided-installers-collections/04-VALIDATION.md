---
phase: 4
slug: guided-installers-collections
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-06-21
---

# Phase 4 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (workspace) + `mockito` 1.7 (HTTP) + a `crates/fomod` `ModuleConfig.xml` fixture corpus + a Collection-revision fixture |
| **Config file** | none — Cargo workspace; Wave 0 adds `crates/fomod` (quick-xml 0.40) + fixtures |
| **Quick run command** | `cargo test -p nextwist-fomod` |
| **Full suite command** | `cargo test --workspace --locked` |
| **Estimated runtime** | ~90–150 seconds |

---

## Sampling Rate

- **After every task commit:** `cargo test -p <crate-touched>`
- **After every plan wave:** `cargo test --workspace --locked`
- **Before `/gsd-verify-work`:** full suite green + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo deny check advisories bans licenses sources` (new XML crate) + `npm --prefix frontend run check`
- **Max feedback latency:** 150 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| (planner fills) | | | | | | | | | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/fomod` scaffold (quick-xml 0.40 dep, cargo-deny-clean) + a `ModuleConfig.xml` fixture corpus (incl. conditional type-state, conditionalFileInstalls, nested And/Or dependency, multi-step quirk cases)
- [ ] A real public Collection-revision fixture (`collection.json`) for the resolver/replay tests (Open Question A1/A2 — fetch one at Wave 0)
- [ ] HTTP mock (`mockito`) reused from Phase 3 for Collection bulk-download tests

*Detailed Wave 0 list refined by planner.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Install a real-world FOMOD mod through the wizard end-to-end | FOMOD-01/02 | Real `ModuleConfig.xml` variety + the staged-image wizard render are best confirmed interactively | In `cargo tauri dev`, install a known FOMOD mod (e.g. a Skyrim SE patcher with conditional options); confirm choices drive the staged file set + the dry-run conflict preview |
| Install a real NexusMods Collection end-to-end (Premium) | COLL-01..04 | Needs a real Premium account + a live public Collection revision + live CDN; exact GraphQL revision query/archive container confirmed here (A1/A2/A4/A5) | Browse/select a small public Collection; accept the resolve report; bulk-download; auto-apply choices/order/rules; deploy; launch the modded game |
| Reversible Collection uninstall → pristine | COLL-05 | The byte-for-byte pristine guarantee after a full Collection round-trip is the hard in-game UAT | After deploying a Collection, uninstall it; confirm the game folder is byte-for-byte vanilla (and the testkit pristine assertion in the automated suite) |
| Wrapper-folder mod root-detection (carried Phase-2 gap) | FOMOD-02 (acute) | Real archives with a wrapper dir vs Data/-rooted archives | Install a mod whose archive has a top wrapper folder; confirm it stages into Data/ correctly, not double-nested |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references (fixture corpus + Collection fixture)
- [ ] No watch-mode flags
- [ ] Feedback latency < 150s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
