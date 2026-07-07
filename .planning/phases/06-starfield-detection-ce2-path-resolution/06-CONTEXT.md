# Phase 6: Starfield Detection & CE2 Path Resolution - Context

**Gathered:** 2026-07-07
**Status:** Ready for planning

<domain>
## Phase Boundary

NexTwist detects Starfield (Steam AppID 1716740) under Steam/Proton and resolves its
Creation Engine 2 (CE2) config location — `Documents/My Games/Starfield` **inside the
Proton prefix** — so the game can be managed exactly like the existing Bethesda titles.
This phase **gates** Phase 7 (load order) and Phase 8 (reversible INI).

In scope: allow-listing AppID 1716740 across the `steam`/`loadorder`/Tauri seams; a new
`my_games_path` resolver mirroring the existing `appdata_local_path` seam (CE2 uses
`Documents/My Games`, not `AppData/Local`); first-launch-not-done detection + guidance;
version-drift detection + warning; bundling `assets/starfield/masterlist.yaml`.

Out of scope: writing `plugins.txt` (Phase 7), any `StarfieldCustom.ini` edit (Phase 8),
on-hardware in-game validation (Phase 9). No `core` change, no DB migration, no
dependency/MSRV bump (libloot 0.29.5 already exposes `GameType::Starfield`).

</domain>

<decisions>
## Implementation Decisions

### Version-Drift Warning (SFDET-03)
- Store the "last-validated build" baseline as a constant bundled alongside
  `assets/starfield/`, updatable when Phase 9 validates on real hardware (treat as
  validated-against-a-build, not a permanent constant).
- Warning is **advisory / non-blocking** — the safety and byte-for-byte reversibility
  guarantee holds regardless of build; drift only affects the MEDIUM-confidence CE2
  loose-file / load-order recipe, so it must never block management.
- Trigger when the installed Starfield build is **newer than the validated build** (any
  newer build) — honest, avoids false confidence; no "known-breaking" list is available.
- Surface as a **persistent notice on the Starfield game view** with an "installed vs
  validated build" line — visible but non-intrusive.

### First-Launch-Not-Done State (SFDET-02)
- Detect via the **`My Games/Starfield` config dir being absent/empty** (the game creates
  it on first launch).
- **Block** load-order/INI management for Starfield until resolved, but still allow the
  user to add/detect the game — SFDET-02 explicitly forbids "silently writing to a
  useless path".
- Guide with an **actionable message**: "Launch Starfield once via Steam so it creates its
  config folder, then re-check." Do not attempt to auto-launch the Proton game.
- Provide a **manual "Re-check" action** that re-runs detection — explicit and
  predictable, no surprise writes on app focus.

### Detection Scope & Masterlist (SFDET-01/02)
- **Bundle** `assets/starfield/masterlist.yaml` at build time (offline-first, matches the
  v1.0 pattern); surface its age/currency.
- Resolve the Documents path **Wine-redirection-aware** (read the prefix's user
  shell-folder mapping rather than hard-coding `Documents/My Games`), reusing the existing
  `casing.rs` case-folding seam — avoids Pitfall 6 (wrong/absent/mis-cased prefix path).
- **Reuse the existing Bethesda add-game flow verbatim** — Starfield is just a new
  allow-listed AppID (1716740); SFDET-01 requires it behave "exactly like Skyrim SE /
  Fallout 4 today".
- Config folder / slug = **`Starfield` / `starfield`**; the researcher confirms the exact
  CE2 folder name during plan-phase research.

### Claude's Discretion
- Exact module placement of the `my_games_path` resolver, the version-drift constant, and
  the first-launch state enum — follow existing `steam`/`loadorder` conventions.
- Precise wording of user-facing messages.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `appdata_local_path(prefix, game_name) -> PathBuf` (`crates/loadorder/src/loot.rs:67`) —
  the seam the new `my_games_path` resolver mirrors (`drive_c/users/steamuser/...`).
- `game_type_for(appid) -> Option<GameType>` (`loot.rs:81`) and
  `appdata_folder_name(appid)` (`loot.rs:93`) — the two-arm allow-list to extend with a
  Starfield arm (`GameType::Starfield`).
- `game_id_for(appid) -> Option<GameId>` (`crates/loadorder/src/scan.rs:44`),
  `game_slug(appid)` (`crates/loadorder/src/masterlist.rs:46`),
  `expected_exe(appid)` (`crates/steam/src/resolve.rs:267`) — sibling allow-list arms.
- `proton_prefix` / `proton_prefix_from_install` (`crates/steam/src/resolve.rs`) — prefix
  resolution already shipped.
- `testkit::fake_proton_prefix` (`crates/testkit/src/lib.rs:79`) — fixture builder for the
  case-mismatched-prefix regression test SFDET-02 requires.

### Established Patterns
- AppID allow-lists are pure `match appid { … }` functions returning `Option`, with a test
  (`game_type_for_allow_lists_only_the_two_supported_games`) asserting only supported games
  pass — the Starfield arm must extend that test.
- `open_game` always uses `Game::with_local_path` (never `Game::new`) and pre-creates the
  AppData parent dirs — CE2's `My Games` path plugs into the same construction seam.

### Integration Points
- `src-tauri/src/commands/plugins.rs` (`sort_with_loot`, `save_plugin_order_inner`) and
  `resolve_data_dir` (`src-tauri/src/lib.rs`) consume the resolved paths — the Tauri arm
  of the ~6 allow-list changes.

</code_context>

<specifics>
## Specific Ideas

- The CE2 config path is `Documents/My Games/Starfield` (both `plugins.txt` and
  `StarfieldCustom.ini` live here) — distinct from Skyrim SE/FO4's `AppData/Local`. This
  is why a new resolver is needed rather than a new `appdata_folder_name` arm.
- Success criterion 2 requires verification against **both** a real prefix and a
  **case-mismatched fixture** — the regression test is mandatory, not optional.

</specifics>

<deferred>
## Deferred Ideas

- Exact CE2 loose-file INI keys, masterlist currency, and `.ba2` v3 opacity remain
  MEDIUM-confidence — deliberately deferred to Phase 9's on-hardware validation. Phase 6
  only owns the version-drift *signal*, not the validated recipe.

</deferred>
