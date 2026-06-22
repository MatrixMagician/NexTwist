---
phase: 05-appimage-distribution
verified: 2026-06-22T00:00:00Z
status: human_needed
score: 9/9 must-haves verified
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Build the AppImage (cargo tauri build --bundles appimage or the release.yml run), run it once, then inspect ~/.local/share/applications/nextwist-handler.desktop"
    expected: "Exec= is the durable absolute $APPIMAGE path, NOT an ephemeral /tmp/.mount_* path"
    why_human: "The /tmp/.mount_* path only exists at runtime; durable-Exec confirmation requires a real AppImage first-run on a desktop session (no built artifact / no GUI session in this headless env)"
  - test: "With the AppImage running, click a Nexus 'Mod Manager Download' (nxm://) button in a browser"
    expected: "The click routes to the live instance (single-instance forwards it) and triggers on_open_url; no duplicate window opens"
    why_human: "Browser -> OS scheme handoff cannot be unit-tested; needs a real desktop session, registered MIME handler, and a running AppImage instance"
---

# Phase 5: AppImage Distribution Verification Report

**Phase Goal:** A user (or distro) can download a single-file Linux AppImage, run NexTwist with no install friction, and have the nxm:// MIME handler registered automatically — with the distributed build passing a license-compliance audit so it contains no non-free bundled code.
**Verified:** 2026-06-22
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

All source-checkable and runnable must-haves for DIST-01 and DIST-02 are VERIFIED in the codebase. Two success-criterion behaviors are inherently manual/real-hardware (durable `Exec=` after a real AppImage first-run, and live `nxm://` browser-click routing) — they cannot be exercised in this headless environment with no built AppImage and no desktop session. They are routed to human verification, not failed.

### Observable Truths

| #   | Truth   | Status     | Evidence       |
| --- | ------- | ---------- | -------------- |
| 1   | Icon set is >=128x128 so AppImage bundling does not fail on a too-small icon [DIST-01] | ✓ VERIFIED | `file src-tauri/icons/128x128.png` -> `128 x 128, 8-bit RGBA`; icon.png is 512x512 (no longer 32x32); 128x128@2x is 256x256 |
| 2   | bundle.icon references the >=128x128 icon; no speculative bundle.linux.appimage; version 0.1.0 unchanged [DIST-01] | ✓ VERIFIED | tauri.conf.json:36 lists `["icons/32x32.png","icons/128x128.png","icons/128x128@2x.png","icons/icon.png"]`; no `"linux"` key present; `"version": "0.1.0"` intact; valid JSON |
| 3   | On startup the shell calls `is_registered("nxm")` after `register_all()`, logs PASS/WARN, never aborts when xdg-mime is missing [DIST-01] | ✓ VERIFIED | lib.rs:116 `nxm_self_test(app.deep_link().is_registered("nxm"))` inside the cfg(any(windows, target_os="linux")) block, after register_all() (110) and before on_open_url (121); `nxm_self_test` (lib.rs:36-46) matches all 3 arms via tracing, returns (), no `?`/unwrap/expect; 3-arm test passes (`cargo test -p nextwist --test nxm_self_test` -> 3 passed) |
| 4   | The self-test is wired through the plugin's own API (not a hand-rolled xdg-mime query) so the desktop-file name cannot drift [DIST-01] | ✓ VERIFIED | Call site uses `app.deep_link().is_registered("nxm")`; no `Command::new("xdg-mime")` anywhere in lib.rs; doc comment documents the no-hand-roll rationale |
| 5   | Pushing a `v*` tag triggers a CI job on ubuntu-22.04 that builds the AppImage with `--bundles appimage` and uploads it to a GitHub Release [DIST-01] | ✓ VERIFIED | release.yml: `on: push: tags: ["v*"]`; `runs-on: ubuntu-22.04`; `permissions: contents: write`; `tauri-apps/tauri-action@action-v0.6.2` with `tagName: ${{ github.ref_name }}`, `projectPath: src-tauri`, `args: --bundles appimage --locked`; apt set includes patchelf; test+clippy release gate present; valid YAML; no pull_request trigger; no duplicated 0.1.0 literal; ci.yml unchanged |
| 6   | The release workflow re-runs `cargo deny check advisories bans licenses sources` so the result is reproducible from the release run [DIST-02] | ✓ VERIFIED | release.yml installs cargo-deny via `taiki-e/install-action@v2` then runs `cargo deny check advisories bans licenses sources` as the DIST-02 evidence step |
| 7   | A reproducible bundled-binary audit enumerates the AppImage's `.so` deps and proves no UnRAR / non-free RAR code and no app-path system-OpenSSL ships [DIST-02] | ✓ VERIFIED | scripts/dist-audit.sh: `set -euo pipefail`, executable, `bash -n` clean; mktemp temp-dir isolated with EXIT trap; `--appimage-extract`; `ldd usr/bin/nextwist`; `find usr/lib -name '*.so*'`; UnRAR-absence via find + content grep over binary AND usr/lib; WebKitGTK-absence check |
| 8   | DIST-AUDIT.md records the cargo-deny result plus bundled-binary findings (UnRAR-absence named, rustls-only, WebKitGTK-uses-host) [DIST-02] | ✓ VERIFIED | DIST-AUDIT.md sections 1 (cargo-deny + deny.toml ban rationale) and 2.1-2.4 (TLS/rustls, bundled libs, UnRAR-absence by name, WebKitGTK-uses-host); honestly marks literal release-time output as a release-time/manual capture (no fabricated ldd/find output) |
| 9   | cargo-deny passes (advisories/bans/licenses/sources), bans unrar/unrar_sys, and the workspace test suite passes [DIST-02] | ✓ VERIFIED | Ran `cargo deny check advisories bans licenses sources` -> "advisories ok, bans ok, licenses ok, sources ok"; deny.toml denies `unrar` + `unrar_sys`; `cargo test --workspace --locked` -> all test results ok, 0 failures |

**Score:** 9/9 truths verified (0 present, behavior-unverified)

### Required Artifacts

| Artifact | Expected    | Status | Details |
| -------- | ----------- | ------ | ------- |
| `src-tauri/icons/128x128.png` | genuine >=128x128 icon | ✓ VERIFIED | 128 x 128 8-bit RGBA |
| `src-tauri/src/lib.rs` | is_registered("nxm") self-test, non-fatal | ✓ VERIFIED | nxm_self_test helper + call site at line 116; wired + used |
| `src-tauri/tauri.conf.json` | bundle.icon -> >=128x128 | ✓ VERIFIED | lists 128x128.png; version 0.1.0; valid JSON |
| `src-tauri/tests/nxm_self_test.rs` | headless non-fatal test | ✓ VERIFIED | 3 tests, all green against nextwist_lib |
| `.github/workflows/release.yml` | tag-triggered AppImage build + Release | ✓ VERIFIED | tauri-action pinned; --bundles appimage; deny gate |
| `scripts/dist-audit.sh` | reproducible bundled-binary audit | ✓ VERIFIED | executable, bash -n clean, temp-dir isolated |
| `DIST-AUDIT.md` | license + bundled-binary record | ✓ VERIFIED | both evidence streams, UnRAR named |

### Key Link Verification

| From | To  | Via | Status | Details |
| ---- | --- | --- | ------ | ------- |
| src-tauri/src/lib.rs | tauri-plugin-deep-link is_registered | `app.deep_link().is_registered("nxm")` after register_all() | ✓ WIRED | lib.rs:116, inside cfg block, before on_open_url |
| src-tauri/tauri.conf.json | src-tauri/icons/128x128.png | bundle.icon array entry | ✓ WIRED | tauri.conf.json:36 |
| .github/workflows/release.yml | src-tauri/tauri.conf.json | tauri-action projectPath src-tauri + --bundles appimage | ✓ WIRED | release.yml tauri-action step |
| .github/workflows/release.yml | deny.toml | cargo deny check re-run for DIST-02 evidence | ✓ WIRED | release.yml deny step |
| scripts/dist-audit.sh | DIST-AUDIT.md | --appimage-extract + ldd/find recorded as bundled-binary review | ✓ WIRED | DIST-AUDIT.md section 2 cites the script's exact invocation |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| nxm self-test non-fatal on all 3 arms | `cargo test -p nextwist --locked --test nxm_self_test` | 3 passed; 0 failed | ✓ PASS |
| Workspace test suite (release gate / VALIDATION full suite) | `cargo test --workspace --locked` | all results ok, 0 failures | ✓ PASS |
| Supply-chain gate (DIST-02) | `cargo deny check advisories bans licenses sources` | advisories ok, bans ok, licenses ok, sources ok | ✓ PASS |
| Shell crate lint | `cargo clippy -p nextwist --all-targets --locked -- -D warnings` | Finished, no warnings | ✓ PASS |
| release.yml valid YAML / dist-audit.sh parses | embedded grep + bash -n | YAML valid; bash -n clean; executable | ✓ PASS |
| Durable Exec= after real AppImage first-run | inspect nextwist-handler.desktop post-launch | no built AppImage / no desktop session | ? SKIP (human) |
| nxm:// browser click routes to live instance | click Nexus Mod-Manager-Download with AppImage running | no built AppImage / no desktop session | ? SKIP (human) |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| DIST-01 | 05-01, 05-02 | NexTwist is packaged and runnable as a Linux AppImage | ✓ SATISFIED (automated portion) | Icon blocker cleared; self-test wired+tested; tag-triggered release.yml builds + uploads the AppImage. Real-hardware run-and-register behaviors routed to human verification. REQUIREMENTS.md marks DIST-01 Complete. |
| DIST-02 | 05-02 | Distributed build passes a license-compliance audit (no non-free bundled code, e.g. UnRAR) | ✓ SATISFIED | cargo-deny passes (ran it); unrar/unrar_sys banned in deny.toml; release.yml re-runs the gate; dist-audit.sh + DIST-AUDIT.md record the bundled-binary review. |

Both requirement IDs declared in PLAN frontmatter (DIST-01, DIST-02) are accounted for and map to verified evidence. REQUIREMENTS.md maps exactly DIST-01 and DIST-02 to Phase 5 (lines 159-160, 174) — no orphaned requirements.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| (none) | - | No TBD/FIXME/XXX/TODO/HACK/PLACEHOLDER in any Phase 5 file | - | The "release-time/manual-UAT capture" markers in DIST-AUDIT.md are the plan-mandated honest representation of evidence that can only be captured against a built artifact — not stubs |

No `crates/*` engine crate was modified by any Phase 5 commit (021a73c, 2fe611d, 69a503b, e7eb6b1, 5b5c143) — the headless-engine boundary is honored.

### Human Verification Required

1. **Durable `Exec=` path** — Build the AppImage and run it once, then inspect `~/.local/share/applications/nextwist-handler.desktop`.
   - Expected: `Exec=` is the stable `$APPIMAGE` absolute path, not a `/tmp/.mount_*` path.
   - Why human: the `/tmp/.mount_*` path only exists at runtime; needs a real first-run on a desktop session.

2. **`nxm://` click routing** — With the AppImage running, click a Nexus "Mod Manager Download" button.
   - Expected: routes to the live instance (single-instance forwards) and triggers `on_open_url`; no duplicate window.
   - Why human: browser -> OS scheme handoff cannot be unit-tested; needs a registered MIME handler and a running AppImage.

### Gaps Summary

No gaps. Every automated/source-checkable must-have for DIST-01 and DIST-02 is verified against the codebase, and the three runnable gates (nxm_self_test, full workspace suite, cargo-deny) were executed and pass. The phase is not marked `passed` only because two success-criterion behaviors are inherently real-hardware/manual (durable `Exec=` and live `nxm://` routing) and require a built AppImage on a desktop session — these are captured as human-verification items per 05-VALIDATION.md's Manual-Only table, not failures.

---

_Verified: 2026-06-22_
_Verifier: Claude (gsd-verifier)_
