---
phase: 5
slug: appimage-distribution
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-06-22
---

# Phase 5 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` (workspace) + `cargo-deny` + manual/real-hardware UAT for the AppImage |
| **Config file** | none (cargo workspace); `deny.toml` for the supply-chain gate |
| **Quick run command** | `cargo test -p nextwist --locked` |
| **Full suite command** | `cargo test --workspace --locked && cargo deny check advisories bans licenses sources` |
| **Estimated runtime** | ~60–120 seconds (workspace test); deny ~10s |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p nextwist --locked`
- **After every plan wave:** Run `cargo test --workspace --locked && cargo deny check`
- **Before `/gsd-verify-work`:** Full suite green + `release.yml` produces an AppImage + `DIST-AUDIT.md` recorded
- **Max feedback latency:** ~120 seconds (automated portion)

---

## Per-Task Verification Map

> Per-task IDs are assigned by the planner; this map is keyed by requirement until plans exist.

| Req | Behavior | Wave | Test Type | Automated Command | File Exists | Status |
|-----|----------|------|-----------|-------------------|-------------|--------|
| DIST-01 | Icon ≥128×128 present in bundle set | 0 | source | `test -f src-tauri/icons/128x128.png && file src-tauri/icons/128x128.png \| grep -q '128 x 128'` | ❌ W0 (regen via `cargo tauri icon`) | ⬜ pending |
| DIST-01 | AppImage builds with `--bundles appimage` | 1 | CI/manual | `cargo tauri build --bundles appimage` (in `release.yml`) | ❌ W0 (release.yml) | ⬜ pending |
| DIST-01 | `is_registered("nxm")` self-test wired, non-fatal | 1 | unit (shell crate) | `cargo test -p nextwist --locked` (asserts wiring compiles + non-fatal path) | ⚠️ OS-shelling; assert wiring, true E2E is manual UAT | ⬜ pending |
| DIST-01 | Durable `Exec=` (not `/tmp/.mount_`) after first run | 1 | manual UAT | inspect `~/.local/share/applications/nextwist-handler.desktop` post-launch | manual-only (real AppImage launch) | ⬜ pending |
| DIST-01 | `nxm://` browser click routes to live instance | 1 | manual UAT | click a Nexus "Mod Manager Download" with the AppImage running | manual-only (real hardware) | ⬜ pending |
| DIST-02 | cargo-deny passes (licenses/bans/sources/advisories) | 1 | CI | `cargo deny check advisories bans licenses sources` | ✅ exists (ci.yml) | ⬜ pending |
| DIST-02 | No UnRAR / non-free `.so` bundled in AppImage | 1 | release-time audit | `--appimage-extract` + `find -iname '*unrar*'` (expect empty) + `ldd`/`find usr/lib` review | ❌ W0 (audit helper + DIST-AUDIT.md) | ⬜ pending |
| DIST-02 | `.rar` shells out to system unrar/7z (no bundled RAR) | 1 | unit (existing) + audit | existing extract tests + bundled-binary review | ✅ extract tests exist; audit ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src-tauri/icons/128x128.png` (+ full icon set) — regenerate via `cargo tauri icon` (covers DIST-01 icon gap; current `icon.png` is 32×32 and linuxdeploy needs ≥128×128)
- [ ] `.github/workflows/release.yml` — tag-triggered AppImage build + upload (covers DIST-01)
- [ ] `DIST-AUDIT.md` — checked-in audit record: cargo-deny result + bundled-binary review (covers DIST-02)
- [ ] Audit helper (script or documented commands) for `--appimage-extract` + `ldd`/`find usr/lib` enumeration
- [ ] Self-test wiring in `src-tauri/src/lib.rs` — call `is_registered("nxm")` (non-fatal warn path)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Durable `Exec=` survives AppImage remount | DIST-01 | Needs a real AppImage launch (the `/tmp/.mount_*` path only exists at runtime) | Build the AppImage, run it, inspect `~/.local/share/applications/nextwist-handler.desktop` — `Exec=` must be the stable `$APPIMAGE` path, not `/tmp/.mount_*` |
| `nxm://` click routes to running instance | DIST-01 (NXM-01) | Browser → OS scheme handoff cannot be unit-tested | With the AppImage running, click a Nexus "Mod Manager Download" button; it must route to the live instance (single-instance) and trigger `on_open_url` |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references (icons, release.yml, DIST-AUDIT.md, audit helper, self-test wiring)
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
