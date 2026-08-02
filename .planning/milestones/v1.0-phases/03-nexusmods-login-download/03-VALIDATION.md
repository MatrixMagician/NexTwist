---
phase: 3
slug: nexusmods-login-download
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-06-21
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (workspace) + `mockito` 1.7 (new `crates/nexus` dev-dep for HTTP mocking) |
| **Config file** | none — Cargo workspace; Wave 0 adds `crates/nexus` + mockito dev-dep |
| **Quick run command** | `cargo test -p nextwist-nexus` |
| **Full suite command** | `cargo test --workspace --locked` |
| **Estimated runtime** | ~60–120 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p <crate-touched>`
- **After every plan wave:** Run `cargo test --workspace --locked`
- **Before `/gsd-verify-work`:** Full suite must be green + `cargo clippy --workspace --all-targets -- -D warnings`
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| (planner fills) | | | | | | | | | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `mockito` 1.7 added as `crates/nexus` dev-dependency (no repo HTTP mock exists yet — RESEARCH gap)
- [ ] `crates/nexus` test module scaffold (auth/client/download unit stubs)

*Detailed Wave 0 list refined by planner.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Free-user `nxm://` "Mod Manager Download" handoff redemption | NEXUS-04, NXM-01 | Single-use key/expires from the live website button — unmockable end-to-end; needs a real **non-Premium** NexusMods account | Log in as free user; click "Mod Manager Download" on a mod page; confirm OS routes `nxm://` to the running app and the file downloads + extracts into staging |
| OAuth2 `client_id` registration + live login | NEXUS-01 | Requires a registered OAuth client under the Nexus Acceptable Use Policy + live token exchange | After client_id locked: run OAuth login, confirm token stored in keyring, access token works |
| Premium in-app direct download | NEXUS-03 | Needs a real Premium account + live API | Log in as Premium; download a mod in-app; confirm progress + staging |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
