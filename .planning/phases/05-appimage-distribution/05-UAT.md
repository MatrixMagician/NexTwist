---
status: testing
phase: 05-appimage-distribution
source: [05-VERIFICATION.md]
started: 2026-06-22T00:00:00Z
updated: 2026-06-22T00:00:00Z
---

## Current Test

number: 1
name: Durable nxm:// handler Exec= path after a real AppImage first-run
expected: |
  After building the AppImage (`cargo tauri build --bundles appimage` or the
  release.yml run) and running it once, `~/.local/share/applications/nextwist-handler.desktop`
  shows a durable absolute `$APPIMAGE` Exec= path, NOT an ephemeral `/tmp/.mount_*` path.
awaiting: user response

## Tests

### 1. Durable nxm:// handler Exec= path after a real AppImage first-run
expected: Build the AppImage, run it once, then inspect `~/.local/share/applications/nextwist-handler.desktop` — `Exec=` is the durable absolute `$APPIMAGE` path, not an ephemeral `/tmp/.mount_*` path.
why_human: The `/tmp/.mount_*` path only exists at runtime; durable-Exec confirmation requires a real AppImage first-run on a desktop session (no built artifact / no GUI session in the headless dev env).
result: [pending]

### 2. nxm:// browser-click routes to the live instance
expected: With the AppImage running, click a Nexus "Mod Manager Download" (nxm://) button in a browser — the click routes to the live instance (single-instance forwards it) and triggers `on_open_url`; no duplicate window opens.
why_human: Browser → OS scheme handoff cannot be unit-tested; needs a real desktop session, a registered MIME handler, and a running AppImage instance.
result: [pending]

## Summary

total: 2
passed: 0
issues: 0
pending: 2
skipped: 0
blocked: 0

## Gaps
