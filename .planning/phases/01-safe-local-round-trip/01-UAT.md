---
status: testing
phase: 01-safe-local-round-trip
source: [01-VERIFICATION.md]
started: 2026-06-20
updated: 2026-06-20
---

# Phase 1 — Manual UAT Checklist

Automated verification passed **30/30 must-haves, 15/15 requirements** — all four core-value safety guarantees (no-original-modified, purge-to-pristine, malicious-archive-rejection, crash-recovery) are real, executed, passing tests. The items below are the only surfaces that **cannot** run headless and need you on a machine with a **display + real Steam (Proton) + a Bethesda game installed**.

## Prerequisites
- A graphical desktop session (the Tauri webview needs a display).
- `cargo tauri dev` — install the Tauri CLI if needed: `cargo install tauri-cli --version '^2'` (WebKitGTK 4.1 dev libs are already installed on this machine).
- Skyrim Special Edition (AppID 489830) and/or Fallout 4 (377160) installed via Steam, having run at least once under Proton (so `compatdata/<appid>/pfx` exists).
- A local mod archive (`.zip` or `.7z`) to test with.

## Test items

- [ ] **UAT-1 — GUI detect / add game**
  - Steps: `cargo tauri dev` → let auto-detect run (or use "add game by folder" for a non-standard/Snap install).
  - Expected: the managed game appears with the correct **install dir** and **`compatdata/<appid>/pfx` Proton prefix**; add-by-folder accepts a valid Bethesda dir (Data/ + game exe) and rejects a non-game folder.
  - Covered headless by: `crates/steam/tests/resolve_game.rs` (synthetic fixtures).

- [ ] **UAT-2 — GUI install → deploy → purge round-trip**
  - Steps: install a local `.zip`/`.7z` mod into staging → click **Deploy** → then **Purge**.
  - Expected: mod files link into `Data/`; the deploy report shows the chosen method(s) + any FS warnings (cross-device/EXDEV, non-casefolded); **Purge returns the game folder to byte-for-byte pristine with no orphans**.
  - Covered headless by: `round_trip_pristine.rs` (48-case proptest) + `crash_recovery.rs`.

- [ ] **UAT-3 — In-game Proton load (DEPLOY-08 case-correctness)**
  - Steps: deploy a mod → launch the game via real Steam Proton → confirm the mod is active in-game.
  - Expected: mod content is visible/active; mixed-case mod paths resolve under Wine's case-sensitive view.
  - Covered headless by: `casefold_normalize.rs` (unit) — but actual in-game load is empirical.

- [ ] **UAT-4 — Real Flatpak/Snap Steam packaging**
  - Steps: on a Flatpak- or Snap-packaged Steam, run detection.
  - Expected: Flatpak root auto-detected; Snap users fall back to add-by-folder (Snap root intentionally not auto-detected — A2 low-confidence, tested fallback).

## On completion
- All pass → Phase 1 is fully signed off. Resume the autonomous build with: `/gsd-autonomous --from 2`
- Any failure → describe it; it routes to gap closure (`/gsd-plan-phase 1 --gaps`) before Phase 2.
