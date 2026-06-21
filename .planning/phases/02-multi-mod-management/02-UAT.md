---
status: testing
phase: 02-multi-mod-management
source: [02-VERIFICATION.md]
started: 2026-06-21
updated: 2026-06-21
---

## Current Test

number: 1
name: In-game plugins.txt under Proton
expected: |
  After enabling/ordering plugins and deploying, launching the real game via Steam Proton
  loads exactly the enabled plugins in the chosen order (asterisk-format plugins.txt at the
  Proton-prefix AppData/Local/<GameName>/Plugins.txt path is honored by the Creation Engine).
awaiting: user response

## Tests

### 1. In-game plugins.txt under Proton
expected: Enabled set + load order written by NexTwist actually apply in-game (Skyrim SE / Fallout 4) launched via Steam Proton.
result: [pending]

### 2. Real Proton-prefix AppData folder name
expected: The per-game AppData/Local folder constants (e.g. "Skyrim Special Edition", "Fallout4") match the live Proton prefix on real hardware; libloot with_local_path round-trips to the correct Plugins.txt.
result: [pending]

### 3. WR-02 mid-switch failure clears stale active flag
expected: If a profile switch fails after the purge step, no profile is left falsely marked active (state/disk consistent). Happy path is test-covered; this is the failure-injection path the fixer flagged as not automatically tested.
result: [pending]

### 4. WR-05 plugins.txt write-failure leaves DB untouched
expected: If the plugins.txt write fails, the DB plugin_state is not persisted (write-before-persist ordering holds). Happy path is test-covered; this is the failure-injection path the fixer flagged as not automatically tested.
result: [pending]

## Summary

total: 4
passed: 0
issues: 0
pending: 4
skipped: 0
blocked: 0

## Gaps
