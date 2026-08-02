---
status: complete
phase: 05-appimage-distribution
source: [05-VERIFICATION.md]
started: 2026-06-22T00:00:00Z
updated: 2026-06-22T17:00:00Z
---

## Current Test

[testing complete]

## Tests

### 1. Durable nxm:// handler Exec= path after a real AppImage first-run
expected: Build the AppImage, run it once, then inspect `~/.local/share/applications/nextwist-handler.desktop` — `Exec=` is the durable absolute `$APPIMAGE` path, not an ephemeral `/tmp/.mount_*` path.
why_human: The `/tmp/.mount_*` path only exists at runtime; durable-Exec confirmation requires a real AppImage first-run on a desktop session (no built artifact / no GUI session in the headless dev env).
result: pass
verified: 2026-06-22 on real hardware (KDE/Fedora 44). `nextwist-handler.desktop` Exec= resolved to `"…/NexTwist_0.1.0_amd64.AppImage" %u` (durable AppImage path, not /tmp/.mount_*). Local AppImage built with NO_STRIP=true (linuxdeploy strip vs Fedora .relr.dyn — see DIST-AUDIT.md).

### 2. nxm:// browser-click routes to the live instance
expected: With the AppImage running, click a Nexus "Mod Manager Download" (nxm://) button in a browser — the click routes to the live instance (single-instance forwards it) and triggers `on_open_url`; no duplicate window opens.
why_human: Browser → OS scheme handoff cannot be unit-tested; needs a real desktop session, a registered MIME handler, and a running AppImage instance.
result: pass
verified: 2026-06-22 on real hardware. Logs: `nxm:// handler self-test: PASS` → `validated NexusMods API key user_id=377708` → `starting streaming download total=361589`. The browser click reached the live instance and drove a real download. Prereq: removed a competing legacy NexusMods AppImage handler (com.nexusmods.app.desktop) that was winning the nxm:// default on KDE. An initial "link expired" was a stale/single-use NexusMods link, not a routing/redemption defect — a fresh click redeemed and downloaded.

## Summary

total: 2
passed: 2
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps
