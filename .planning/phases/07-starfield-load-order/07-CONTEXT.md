# Phase 7: Starfield Load Order - Context

**Gathered:** 2026-07-07
**Status:** Ready for planning

<domain>
## Phase Boundary

Users can enable/disable, order, and LOOT-sort Starfield plugins safely, with the CE2
medium-master tier and protected base masters handled correctly **via libloot** (never
hard-coded). Reuses the shipped v1.0 libloot sort/apply/`plugins.txt` machinery verbatim;
the only new inputs are `GameType::Starfield` (wired in Phase 6), the bundled Starfield
masterlist (bundled in Phase 6), the medium-master-tier classification, protected-master
locking, and the SFLO-04 on-launch-rewrite reconciliation.

Starfield's `plugins.txt` lives in `AppData/Local/Starfield/` (the `appdata_folder_name`
seam wired in Phase 6 → `open_game(1716740)` / `appdata_local_path`), NOT in `My Games`.
(The `My Games/StarfieldCustom.ini` write is Phase 8.)

In scope: SFLO-01 (asterisk `plugins.txt` write at the CE2 AppData path), SFLO-02 (LOOT
sort via bundled masterlist), SFLO-03 (protected masters + medium-master tier via
libloot), SFLO-04 (verify/repair treats the game's on-launch rewrite as expected).

Out of scope: `StarfieldCustom.ini` loose-file activation (Phase 8), on-hardware LOOT-vs-
desktop parity + in-game load confirmation (Phase 9), `.ba2` archive inspection (deferred).

</domain>

<decisions>
## Implementation Decisions

### Protected & Medium-Master Handling (SFLO-03)
- Protected base masters (`Starfield.esm`, `SFBGS0xx.esm`, `BlueprintShips-*`, …) are
  determined **via libloot** (implicitly-active / base set) — NEVER hard-coded in NexTwist.
- Shown as **locked, non-editable rows** with a "protected" badge + a tooltip explaining
  why they can't be moved.
- The CE2 **medium-master tier** is classified via the esplugin/libloot header flag and
  surfaced as a distinct "medium master" badge, ordered per libloot's rules.
- A user attempt to reorder/disable a protected master is **blocked in the UI AND rejected
  defensively in the engine** (defense-in-depth — never rely on the UI alone).

### LOOT Sort & Masterlist Currency (SFLO-01/02)
- Masterlist source is the **bundled Phase-6 asset** (`crates/loadorder/assets/starfield/
  masterlist.yaml`) — offline-first; its commit date/age is surfaced.
- Sort application **previews the proposed order and requires an explicit Apply** (reuse
  the v1.0 `propose_sort` / preview pattern — no silent auto-apply).
- A **"masterlist from {date} — may be stale"** note appears near the Sort action.
- Parity with LOOT desktop is **deterministic via the same libloot version/inputs**;
  real-input parity is validated on hardware in Phase 9.

### On-Launch plugins.txt Rewrite Reconciliation (SFLO-04)
- Verify/repair **derives intent from the recorded plugin state in the store**, not the
  last-written raw file. The game's known on-launch rewrite (`.ccc` entries, stripped
  implicit ESMs, `BlueprintShips-*`) is treated as **non-drift**.
- When on-disk differs only by those known-expected deltas, surface **"in sync — game
  applied its expected on-launch changes"**, not a scary raw diff.
- Deltas **beyond** the known rewrite surface as a **real discrepancy** for the user.

### Claude's Discretion
- Exact module placement of the medium-master classification + protected-master query.
- Precise UI copy; detailed visual design deferred to the UI-SPEC.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets (v1.0 load-order machinery — reuse verbatim)
- `crates/loadorder/src/scan.rs` — `scan_plugins`, `classify_kind`,
  `collects_plugins_and_classifies_master_vs_regular` (extend classification with the
  medium-master tier).
- `src-tauri/src/commands/plugins.rs` — `sort_with_loot`, `list_plugins`,
  `save_plugin_order` (the Tauri command layer; `open_game(1716740)` already works).
- `crates/loadorder/src/masterlist.rs` — `ensure_masterlist`, `masterlist_url`,
  `game_slug` (Starfield slug + bundled snapshot wired in Phase 6).
- `crates/store/src/plugins.rs` — `list_plugin_state`, `plugin` (recorded plugin state =
  the SFLO-04 reconciliation baseline).
- `crates/loadorder/src/loot.rs` — `open_game`, `appdata_folder_name(1716740)="Starfield"`
  (Phase 6), `appdata_local_path` (writes `plugins.txt` under `AppData/Local/Starfield`).

### Established Patterns
- Asterisk `plugins.txt` round-trip already tested:
  `set_order_round_trip_writes_asterisk_plugins_txt_under_the_fixture_appdata`,
  `writes_asterisk_masters_first` — extend these with Starfield fixtures.
- `open_game` always uses `Game::with_local_path`; libloot owns protected/implicit-master
  determination.

### Integration Points
- Load-order UI view (frontend) — add protected-lock + medium-master badge + masterlist-age
  note. Tauri commands stay thin forwarders.

</code_context>

<specifics>
## Specific Ideas

- "Determined via libloot, never hard-coded" is a hard requirement for BOTH the protected
  masters and the medium-master tier (SFLO-03 criterion 3) — the plan must query libloot,
  not embed a name list.
- SFLO-04 reconciliation is the subtle one: the game rewrites `plugins.txt` on launch;
  NexTwist must not treat that expected rewrite as corruption/drift.

</specifics>

<deferred>
## Deferred Ideas

- LOOT-vs-desktop parity on real inputs and in-game load confirmation → Phase 9.
- `.ba2` archive conflict-awareness → future (v1.x/v2).

</deferred>
