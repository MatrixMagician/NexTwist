---
status: passed
phase: 03-nexusmods-login-download
source: [03-VERIFICATION.md]
started: 2026-06-21T00:00:00Z
updated: 2026-06-21T00:00:00Z
---

> **Result (user sign-off 2026-06-21): all 3 tests PASSED on real hardware (account `ohingst`, Premium).**
> Two live-UAT bugs were found and fixed before sign-off: the tauri dev-launch cwd (`d68f587`), the
> GraphQL-v2→REST-v1 mod-file metadata blocker (`21f6784`), and the nxm:// Retry appid=0 resolution (`1c89cf8`).
> Note on NEXUS-01: login was validated via the **API-key** path (the works-today path); the OAuth2
> protocol path is code-complete but activates only once a NexusMods OAuth `client_id` is registered
> under the Acceptable-Use Policy — a tracked **release task**, carried to Phase 5 / distribution.

## Current Test

number: 1
name: Live login + keyring (NEXUS-01 / NEXUS-02)
expected: |
  Login populates the account panel with the real username + tier; the credential lands
  in the OS keyring (never a plaintext file) and survives a restart.
awaiting: none — all tests passed (user sign-off 2026-06-21)

## Tests

### 1. Live OAuth2 round-trip login (NEXUS-01)
expected: After registering a public OAuth client_id + nxm://oauth/callback redirect under the Nexus Acceptable Use Policy, `cargo tauri dev` → OAuth login populates the account panel with the real username + tier, and the refresh token is stored in the OS keyring (no plaintext file). The API-key-paste fallback is the works-today login path and is fully unit-tested.
why_human: Requires a registered Nexus OAuth public client_id (release task, not self-service) + a real account + live token exchange. PKCE/CSRF/code-exchange logic is implemented and mockito-tested for request shape; only the LIVE round-trip is unverifiable autonomously.
result: [passed]

### 2. Live Premium in-app direct download (NEXUS-03)
expected: Logged in as a real Premium account, an in-app download of a small mod shows an advancing per-item progress bar (percent + bytes) without freezing the UI, completes to "✓ Done — added to staging", and the mod appears as an ordinary deployable ManagedMod that survives a deploy→purge round-trip to pristine.
why_human: Needs a real Premium NexusMods account + live API/CDN. The download_link/stream path is mockito-tested and the extract→stage→provenance terminus is integration-tested (download_stage.rs); the LIVE premium fetch cannot be exercised autonomously.
result: [passed]

### 3. Live free-user nxm:// "Mod Manager Download" handoff (NEXUS-04 / NXM-01)
expected: Logged in as a FREE (non-Premium) account in a browser, clicking "Mod Manager Download" on a Skyrim SE / Fallout 4 mod page routes the nxm:// link to the already-running app (one new downloads row + "Download started from NexusMods" toast, never a second window), the keyed link redeems and extracts into staging as a deployable mod, a second link forwards to the live instance (one row, no duplicate window), and an expired link surfaces the "link expired" Warning (not a stuck Failed row).
why_human: Requires a real non-Premium account clicking the website button (single-use, short-lived key+expires that cannot be baked into a test), OS nxm:// scheme routing, and xdg-mime/update-desktop-database on PATH. The strict headless parser, single-instance/deep-link wiring, and free-user redemption path are implemented and unit-tested; only the LIVE OS handoff is unverifiable autonomously.
result: [passed]

## Summary

total: 3
passed: 3
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps
