---
phase: 1
slug: safe-local-round-trip
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-06-20
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution. This phase is safety-critical; tests are a locked first-class deliverable (CONTEXT.md decision).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` + `proptest` 1.11 (property tests) + `tempfile` 3.27 (isolated temp dirs) |
| **Config file** | none — Cargo test discovery (`crates/*/tests/` + inline `#[cfg(test)]`); Wave 0 installs toolchain |
| **Quick run command** | `cargo test -p <crate> --lib` (touched crate, fast loop) |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30–90 seconds (workspace; grows with proptest cases) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p <crate> --lib` for the touched crate
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd-verify-work`:** Full suite green **+ the crash-recovery and round-trip-pristine integration tests passing on at least tmpfs AND the dev btrfs filesystem**
- **Max feedback latency:** ~90 seconds

---

## Per-Task Verification Map

| Requirement | Behavior | Test Type | Automated Command | File Exists | Status |
|-------------|----------|-----------|-------------------|-------------|--------|
| DEPLOY-01/02/03 | Deploy→purge leaves game byte-for-byte pristine | integration (proptest over random file trees) | `cargo test -p deploy round_trip_pristine` | ❌ W0 | ⬜ pending |
| DEPLOY-04 | Replaced vanilla file backed up + restored on purge | integration | `cargo test -p deploy vanilla_restore` | ❌ W0 | ⬜ pending |
| DEPLOY-05 | Per-target method ladder + EXDEV fallback | unit + integration | `cargo test -p deploy method_ladder` | ❌ W0 | ⬜ pending |
| DEPLOY-06 | Crash-mid-deploy → relaunch recovers to consistent/pristine | integration (abort injection + replay) | `cargo test -p deploy crash_recovery` | ❌ W0 (CENTERPIECE) | ⬜ pending |
| DEPLOY-07 | verify/repair detects manifest-vs-disk drift + orphans | unit | `cargo test -p deploy verify_drift` | ❌ W0 | ⬜ pending |
| DEPLOY-08 | Mixed-case mod path normalized vs canonical Data/ | unit | `cargo test -p deploy casefold_normalize` | ❌ W0 | ⬜ pending |
| ENV-04 | fs-capability probe reports same-device/reflink/casefold | unit | `cargo test -p deploy fs_probe` | ❌ W0 | ⬜ pending |
| STAGE-02 | Crafted zip-slip + symlink archive rejected | unit (fixture archives) | `cargo test -p extract zip_slip_rejected` | ❌ W0 | ⬜ pending |
| STAGE-01/03 | .zip/.7z extract to staging; .rar via system tool/clear error | integration | `cargo test -p extract extract_formats` | ❌ W0 | ⬜ pending |
| ENV-01/02/03 | Resolve Skyrim SE/FO4 install dir + prefix from fixture Steam layout | integration (synthetic vdf/acf fixtures) | `cargo test -p steam resolve_game` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `rustup` + Rust toolchain ≥1.85 (2024 edition) — **BLOCKING**, toolchain not currently installed
- [ ] Shared test-helper module/crate — builds a fake vanilla game tree + staged mod, asserts byte-for-byte equality via blake3
- [ ] `crates/deploy/tests/round_trip_pristine.rs` — DEPLOY-01/02/03 (proptest hash-equal vanilla after install→purge)
- [ ] `crates/deploy/tests/crash_recovery.rs` — DEPLOY-06 centerpiece (abort-injection mid-op; relaunch replay; assert pristine)
- [ ] `crates/deploy/tests/method_ladder.rs` — DEPLOY-05 (second temp fs / loopback to force EXDEV)
- [ ] `crates/deploy/tests/{vanilla_restore,verify_drift,casefold_normalize,fs_probe}.rs`
- [ ] `crates/extract/tests/zip_slip_rejected.rs` + crafted-archive fixtures (`..`, absolute, symlink entries in `.zip`/`.7z`)
- [ ] `crates/steam/tests/resolve_game.rs` + synthetic Steam-layout fixtures (libraryfolders.vdf, appmanifest_489830.acf, compatdata/)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Mod actually loads in-game under real Proton | DEPLOY-08 (case correctness) | Requires a real Skyrim SE/FO4 install + Proton launch; not reproducible in CI | Deploy a known test mod, launch via Steam Proton, confirm it loads; deferred validation also lands in later phases |
| Real Flatpak/Snap Steam root resolution | ENV-01/02 | Depends on the user's actual Steam packaging; Snap path is low-confidence (A2) | On a Flatpak/Snap Steam box, confirm detection or fall back to manual "add by folder" |
| Power-loss `synchronous` PRAGMA durability | DEPLOY-06 | True power-cut durability can't be simulated purely in-process | Validate FULL vs NORMAL with a power-loss-simulation / fsync-fault harness if feasible |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references (toolchain install is the blocking dep)
- [ ] No watch-mode flags
- [ ] Feedback latency < 90s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
