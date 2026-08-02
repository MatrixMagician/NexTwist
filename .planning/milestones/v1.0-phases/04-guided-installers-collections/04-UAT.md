---
status: partial
phase: 04-guided-installers-collections
source: [04-VERIFICATION.md]
started: 2026-06-21
updated: 2026-06-23
---

## Current Test

[testing complete]

## Tests

### 1. Live FOMOD wizard click-through in `cargo tauri dev`
expected: |
  Install a real conditional-installer mod; navigate steps Back/Next; change a flag-setting
  option; observe step/option visibility + type-state re-evaluate live; see the dry-run
  conflict-preview panel; then apply against a real archive. Wizard renders one step per screen;
  radio/checkbox groups honor min/max + Required/NotUsable; visibility re-evaluates on flag
  changes; conflict-preview shows the resolved plan before any staging write; apply stages with
  no Data/ double-nesting and the mod appears in the mod list.
why_human: |
  WebView render, step navigation, live visual conditional re-eval, and conflict-preview display
  cannot be exercised headlessly. The headless engine (resolve/validate/visible-step) IS verified
  by passing corpus tests; only the live UI interaction layer is open.
result: pass

### 2. Live Premium NexusMods Collection end-to-end
expected: |
  Real Premium account → fetch a real collectionRevision archive → bulk-download the pinned mods
  → deploy → launch the modded game in-game → uninstall the Collection. Premium session
  bulk-downloads the available set with per-mod + overall progress; the modded game launches;
  uninstall restores the game byte-for-byte pristine. A free account sees the Premium-required
  notice and no download starts.
why_human: |
  Requires a live Premium NexusMods account + real network + in-game launch (NEXUS-01 live-account
  gate, deferred since Phase 3). The GraphQL collectionRevision.downloadLink archive-fetch network
  seam is intentionally not implemented — adapters consume an already-fetched manifest. The
  reversibility guarantee itself IS proven headlessly by the pristine round-trip test; only the
  live-account/live-hardware fetch+launch path is open.
result: blocked
blocked_by: third-party
reason: "NexusMods site expects to use Vortex to install collections. It will not allow another client to download the collection file."
note: |
  External platform-policy gate, NOT a NexTwist code defect. NexusMods restricts Collection
  archive download to its own Vortex client; a third-party client cannot fetch the collection
  file. This is the same boundary documented in 04-VERIFICATION.md: the GraphQL
  collectionRevision.downloadLink archive-fetch seam was intentionally NOT implemented (adapters
  consume an already-fetched manifest). The headless Collection engine — apply pinned FOMOD
  choices + load order, deploy, and byte-for-byte reversible uninstall — IS verified by corpus
  + pristine round-trip tests. Live end-to-end Collection download from nexusmods.com is not
  achievable for v1.0 due to this restriction; revisit for v2 (COLLV2 / manifest-import path).

## Summary

total: 2
passed: 1
issues: 0
pending: 0
skipped: 0
blocked: 1

## Gaps

[none — Test 2 is an external platform gate (blocked), not a code issue]
