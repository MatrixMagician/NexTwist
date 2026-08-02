---
phase: 02-multi-mod-management
plan: 04
subsystem: plugins / LOOT (scan + libloot apply + masterlist + Tauri + Svelte view)
tags: [plugins, loot, libloot, esplugin, masterlist, plugins.txt, asterisk, masters-first, reqwest, rustls, svelte]
status: complete
requires:
  - "Plan 01: plugin_state CRUD + core Plugin/PluginKind + cargo-deny GPL allowance"
  - "Plan 02: libloot 0.29.5 wrapper (open_game with_local_path, appdata_local_path, set_order_and_save) + verified API surface + testkit::fake_proton_prefix"
  - "Phase-1 store (list_mods, active_profile, list_plugin_state, set_plugin_state) + core::Game"
provides:
  - "loadorder::scan_plugins / scan_plugins_for — plugin discovery + ESM/ESL/ESP badge from esplugin header flags"
  - "loadorder::apply_load_order(appid, install_dir, appdata_local, &[Plugin]) -> PathBuf — seed+persist asterisk plugins.txt in the prefix (Plan 05 profile-switch calls this)"
  - "loadorder::propose_sort(appid, install_dir, appdata_local, app_data, &[Plugin]) -> SortProposal — LOOT sort, writes nothing (D-12)"
  - "loadorder::ensure_masterlist(app_data, appid, refresh) -> PathBuf — pinned fetch + cache + bundled CC0 fallback"
  - "loadorder::{appdata_folder_name, masters_first_order, SortProposal}"
  - "Tauri commands: list_plugins / set_plugin_enabled / save_plugin_order / sort_with_loot"
  - "frontend Plugin manager view (UI-SPEC §B/§C): masters-first list, badges, LOOT propose/apply"
affects:
  - "Plan 05 (profile switch) — apply_load_order(appid, install_dir, appdata_local, &[Plugin]) is the per-profile plugins.txt write after deploy"
tech-stack:
  added:
    - "esplugin 6.1.4 (GPL-3.0) — direct dep (was already a libloot transitive dep; no new graph crate, no new legitimacy checkpoint)"
    - "reqwest 0.13.4 with the `rustls` feature (blocking; NO OpenSSL) — masterlist HTTPS fetch"
    - "transitive: webpki-root-certs 1.0.8 (CDLA-Permissive-2.0) — CA roots via reqwest rustls-platform-verifier"
  patterns:
    - "esplugin header-only classification (is_master_file/is_light_plugin), NOT extension"
    - "seed-then-load-then-set active-state seam (libloot 0.29.5 has no active setter)"
    - "injectable fetcher generic seam for offline/cache-path unit tests"
    - "bundled CC0 masterlist snapshots via include_str! for the offline fallback"
key-files:
  created:
    - crates/loadorder/src/scan.rs
    - crates/loadorder/src/masterlist.rs
    - crates/loadorder/tests/plugins.rs
    - crates/loadorder/assets/skyrimse/masterlist.yaml
    - crates/loadorder/assets/fallout4/masterlist.yaml
    - src-tauri/src/commands/plugins.rs
  modified:
    - Cargo.toml
    - Cargo.lock
    - deny.toml
    - crates/loadorder/Cargo.toml
    - crates/loadorder/src/lib.rs
    - crates/loadorder/src/error.rs
    - crates/loadorder/src/loot.rs
    - src-tauri/Cargo.toml
    - src-tauri/src/commands/mod.rs
    - src-tauri/src/lib.rs
    - frontend/src/lib/api.ts
    - frontend/src/routes/+page.svelte
decisions:
  - "esplugin added DIRECTLY for scan classification (header_only parse, no libloot Game/prefix needed). It is already a libloot 6.1.4 transitive dep (GPL-3.0, deny-allowed), so no new crate enters the graph and no new package-legitimacy checkpoint is required (T-02-SC)."
  - "reqwest uses the `rustls` feature name (NOT `rustls-tls` — that feature does not exist in reqwest 0.13.4) + `blocking`; loadorder stays runtime-free (no tokio). The masterlist fetch is a one-shot synchronous call behind a Tauri command."
  - "libloot 0.29.5 DOES expose structured warnings (A2 resolved): Database::general_messages -> Vec<Message> with message_type() {Say,Warn,Error} + content().text(). propose_sort surfaces Warn/Error-level general messages; no serde_yaml fallback was needed."
  - "libloot SkyrimSE/Fallout4 asterisk plugins.txt lists ONLY toggleable plugins (.esp / ESL-light); .esm masters are implicitly active and governed by the load order, NOT written to the active-plugins file (verified against the 0.29.5 writer output). Masters-first is therefore asserted via the load order (is_plugin_active / load_order()), not the file content."
  - "Active state enters via the seeded asterisk plugins.txt (seed -> load_current_load_order_state -> set_load_order), per the Plan-02 finding that 0.29.5 has no active-plugin setter."
  - "deny.toml gains CDLA-Permissive-2.0 (webpki-root-certs, the Mozilla CA root set via reqwest rustls) — a permissive, GPL-compatible data license."
metrics:
  duration_min: 15
  tasks: 3
  files: 18
  tests_added: 21
  completed: 2026-06-21
---

# Phase 2 Plan 04: Plugin + LOOT Vertical Slice Summary

Delivered the in-game-observable half of multi-mod management end-to-end: scan enabled mods' staged trees + game `Data/` for plugins (ESM/ESL/ESP-badged from esplugin header flags), enable/disable + reorder them, write a correct asterisk-format masters-first `plugins.txt` at the Proton-prefix AppData location via libloot (nothing hand-rolled), fetch/cache per-game LOOT masterlists over TLS with an offline CC0 fallback, and LOOT-sort with a propose -> review (diff + critical warnings) -> apply flow (no silent apply). Built entirely on the Plan-02 verified libloot seam.

## What Was Built

- **`scan.rs`** — `scan_plugins` / `scan_plugins_for(GameId, roots, data)`: walkdir collection of `.esp/.esm/.esl` (case-insensitive for Wine), de-dup by case-insensitive filename (staged/enabled copy wins), `PluginKind` from esplugin `is_light_plugin()`/`is_master_file()` header flags. A corrupt/unparsable plugin file falls back to ESP + logs, never aborting the scan.
- **`loot.rs` extension** — `apply_load_order` (seed asterisk file -> load -> set masters-first order -> persist; drops stale store rows whose file is absent on disk so libloot's header-parse never aborts the write), `propose_sort` -> `SortProposal { proposed, warnings }` (writes nothing, D-12), `masters_first_order`, `appdata_folder_name`, and `critical_warnings` (A2).
- **`masterlist.rs`** — `ensure_masterlist(app_data, appid, refresh)`: pinned HTTPS (`raw.githubusercontent.com/loot/<slug>/v0.29/masterlist.yaml`, T-02-10 / Pitfall 5) over reqwest `rustls` blocking, cache at `<app_data>/masterlists/<appid>/masterlist.yaml`, bundled CC0 snapshot fallback when offline. Injectable fetcher seam for headless cache/offline tests.
- **`src-tauri/commands/plugins.rs`** — four thin adapters: `list_plugins` (scan merged with per-profile `plugin_state`), `set_plugin_enabled`, `save_plugin_order` (persist order then `apply_load_order`), `sort_with_loot` (propose, no apply). Zero format/safety logic; errors surfaced verbatim for the UI-SPEC plugins.txt error copy.
- **frontend** — `PluginInfo`/`SortProposal` types + bindings; Plugin manager view (UI-SPEC §B/§C): masters-first grouped list with "Masters (load first)" / "Regular plugins" dividers, ESM/ESL/ESP badges, enable toggles, reorder controls disabled across the masters/regular boundary (with the §B.2 inline warning), read-only asterisk `plugins.txt` preview, and the LOOT propose -> review (moved-plugin diff + warnings) -> Apply/Discard flow with a primary "Save plugin order" CTA.

## API Surface Plan 05 Depends On

```rust
// The per-profile plugins.txt write Plan 05 calls after deploy in a profile switch:
loadorder::apply_load_order(
    appid: u32, install_dir: &Path, appdata_local: &Path, plugins: &[Plugin],
) -> Result<PathBuf, LoadOrderError>;   // returns the written active_plugins_file_path

// appdata_local is built from the resolved prefix:
loadorder::appdata_local_path(prefix, loadorder::appdata_folder_name(appid).unwrap())

loadorder::propose_sort(appid, install_dir, appdata_local, app_data, &[Plugin]) -> SortProposal;
```

Plugin command names: `list_plugins`, `set_plugin_enabled`, `save_plugin_order`, `sort_with_loot`.

## Plan-Required Recordings

- **esplugin added directly?** YES — as a direct workspace dep. It was already a libloot 6.1.4 transitive dep (GPL-3.0, deny-allowed), so this adds no new crate to the graph and required no new legitimacy checkpoint (T-02-SC).
- **reqwest blocking vs async?** BLOCKING (`features = ["rustls", "http2", "charset", "blocking"]`, `default-features = false`). loadorder stays runtime-free (no tokio); the fetch is a one-shot call behind a Tauri command. The feature is `rustls`, not `rustls-tls` (the latter does not exist in reqwest 0.13.4).
- **Does libloot expose structured warnings (A2)?** YES — `Database::general_messages(MergeMode, EvalMode) -> Vec<Message>`, each with `message_type()` (Say/Warn/Error) and `content()[].text()`. `propose_sort` surfaces Warn/Error general messages as `SortProposal.warnings`. No serde_yaml fallback was needed.
- **apply_load_order signature:** `apply_load_order(appid: u32, install_dir: &Path, appdata_local: &Path, plugins: &[Plugin]) -> Result<PathBuf, LoadOrderError>`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking gate] reqwest feature name `rustls-tls` does not exist in 0.13.4**
- **Found during:** Task 2 build.
- **Issue:** The plan specified `features = ["rustls-tls"]`, but reqwest 0.13.4 exposes the feature as `rustls` (the `rustls-tls` name was from an older reqwest line). The dependency failed to resolve.
- **Fix:** Used `default-features = false, features = ["rustls", "http2", "charset", "blocking"]` — still rustls, still no OpenSSL (T-02-10 honored).
- **Files modified:** `Cargo.toml`
- **Commit:** `9a51411`

**2. [Rule 3 - Blocking gate] cargo deny rejected CDLA-Permissive-2.0 (webpki-root-certs via reqwest rustls)**
- **Found during:** Task 2 `cargo deny check`.
- **Issue:** reqwest's rustls platform verifier pulls `webpki-root-certs` (the Mozilla CA root bundle), licensed `CDLA-Permissive-2.0` — not in the allow list, so the licenses gate failed (a plan verification gate).
- **Fix:** Added `"CDLA-Permissive-2.0"` to `deny.toml` `[licenses].allow` with a comment. It is a permissive, GPL-compatible data license (the same CA set browsers ship).
- **Files modified:** `deny.toml`
- **Commit:** `9a51411`

**3. [Rule 1 - Bug / API adaptation] libloot asterisk plugins.txt does not list .esm masters**
- **Found during:** Task 2 plugins integration test.
- **Issue:** The plan's behavior note implied masters would appear `*Skyrim.esm` in the file ahead of `.esp`. libloot's SkyrimSE/Fallout4 asterisk writer lists ONLY toggleable plugins (`.esp` / ESL-light); `.esm` masters are implicitly active and governed by the load order, not written to the active-plugins file (verified against the 0.29.5 output `*Mod.esp\nOff.esp\n`).
- **Fix:** Corrected the test assertions: enabled regular plugins are `*`-listed, masters are NOT in the file, and masters-first is asserted via the persisted load order (`is_plugin_active` / `load_order()`) instead of file content. No production-code change — `apply_load_order` already delegates the canonical write to libloot.
- **Files modified:** `crates/loadorder/tests/plugins.rs`
- **Commit:** `9a51411`

## Threat-Model Mitigations Applied

- **T-02-10 (masterlist tampering):** reqwest `rustls` (no OpenSSL) HTTPS to the pinned host `raw.githubusercontent.com`, pinned repo `loot/<slug>`, pinned branch `v0.29` (= libloot major); bundled CC0 snapshot fallback. Asserted by `url_is_pinned_to_host_repo_and_branch`.
- **T-02-11 (plugins.txt write path):** the write target is libloot's `active_plugins_file_path()` derived from the resolved prefix AppData; `writes_asterisk_masters_first` asserts the path stays under the fixture prefix root.
- **T-02-12 (malicious plugin names):** scan collects only files physically present under validated roots; names are opaque filenames, never joined as paths outside a root; libloot header-validates each plugin.
- **T-02-SC (esplugin/reqwest installs):** esplugin is a pre-existing libloot transitive dep (GPL-3.0 allowed); reqwest is the CLAUDE.md-sanctioned client. No new SLOP packages.

## ESL-Detection Limitation (recorded)

Header-only ESL classification depends on the plugin's TES4 record light flag, which a hand-built 24-byte TES4 header stub cannot set (the light flag is record-internal). The scan tests therefore cover ESM (master flag) vs ESP via real header bytes; ESL-flag detection is exercised against the same esplugin path but a true ESL fixture would need a real light-flagged plugin. The production path uses esplugin's `is_light_plugin()` against real plugin files, so this is a test-fixture limitation only — the non-negotiable (`.esp/.esm/.esl` collection + de-dup + header-based, not extension-based, classification) holds.

## Verification Results

- `cargo test -p nextwist-loadorder` — 34 passed (scan 8 unit + loot 5 unit + masterlist 7 unit + libloot_spike 4 + plugins 6 integration + 4 lib re-export/loot unit).
- `cargo test --workspace` — 140 passed, 0 failed (no Phase-1/Plan-01/02/03 regressions).
- `cargo build -p nextwist` — compiles (four plugin commands registered).
- `cd frontend && npm run check` — 0 errors, 0 warnings (141 files).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo deny check` — advisories ok, bans ok, licenses ok, sources ok.

## Self-Check: PASSED

- FOUND: crates/loadorder/src/scan.rs
- FOUND: crates/loadorder/src/masterlist.rs
- FOUND: crates/loadorder/src/loot.rs (apply_load_order, propose_sort)
- FOUND: crates/loadorder/tests/plugins.rs
- FOUND: crates/loadorder/assets/skyrimse/masterlist.yaml, crates/loadorder/assets/fallout4/masterlist.yaml
- FOUND: src-tauri/src/commands/plugins.rs
- FOUND: frontend/src/routes/+page.svelte (Plugin manager view)
- FOUND commit 0081805 (Task 1), 9a51411 (Task 2), 4e3860c (Task 3)
