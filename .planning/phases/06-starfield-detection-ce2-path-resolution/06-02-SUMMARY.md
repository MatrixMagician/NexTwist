---
phase: 06-starfield-detection-ce2-path-resolution
plan: 02
subsystem: loadorder
tags: [starfield, allow-list, masterlist, loot, detection]
requires:
  - libloot 0.29.5 (GameType::Starfield — already pinned)
  - esplugin 6.1.4 (GameId::Starfield — already pinned)
provides:
  - "game_type_for(1716740) = Some(GameType::Starfield)"
  - "game_id_for(1716740) = Some(GameId::Starfield)"
  - "game_slug(1716740) = Some(\"starfield\")"
  - "appdata_folder_name(1716740) = Some(\"Starfield\")"
  - "bundled Starfield LOOT masterlist snapshot (offline-first)"
affects:
  - Phase 7 (load order — consumes appdata_folder_name + bundled masterlist)
  - Phase 8 (reversible INI)
tech-stack:
  added: []
  patterns:
    - mirror-const per loadorder module (STARFIELD = 1716740)
    - include_str! bundled CC0 masterlist snapshot
    - pure `match appid -> Option` allow-list arms
key-files:
  created:
    - crates/loadorder/assets/starfield/masterlist.yaml
  modified:
    - crates/loadorder/src/masterlist.rs
    - crates/loadorder/src/loot.rs
    - crates/loadorder/src/scan.rs
decisions:
  - "Bundle loot/starfield@v0.29 masterlist verbatim (CC0-1.0); no URL-builder change since MASTERLIST_BRANCH already v0.29"
  - "appdata_folder_name(Starfield) = \"Starfield\" — plugins.txt lives in AppData/Local/Starfield (Phase 7); the My Games/Starfield path for INI is the steam crate's job (06-01)"
metrics:
  duration: ~10m
  completed: 2026-07-07
status: complete
---

# Phase 6 Plan 02: Starfield loadorder allow-list + bundled masterlist Summary

Completed Starfield's (AppID 1716740) allow-list surface across the headless `loadorder`
engine and bundled its LOOT masterlist snapshot offline (SFDET-01). Starfield is now a
first-class managed Bethesda game everywhere the two existing titles are, with a
compiled-in CC0 masterlist for offline sorting in Phase 7.

## What was built

- **Bundled masterlist** — vendored `loot/starfield@v0.29/masterlist.yaml` (CC0-1.0, 979
  lines) as `crates/loadorder/assets/starfield/masterlist.yaml`, wired through
  `STARFIELD_SNAPSHOT` (`include_str!`), `game_slug -> "starfield"`, and the
  `bundled_snapshot` arm. `MASTERLIST_BRANCH` was already `v0.29`, so the appid-generic
  URL builder needed no change.
- **loot.rs** — `game_type_for(1716740) = GameType::Starfield`;
  `appdata_folder_name(1716740) = "Starfield"` (Starfield's `Plugins.txt` lives in
  `AppData/Local/Starfield` — consumed by Phase 7, not written here).
- **scan.rs** — `game_id_for(1716740) = GameId::Starfield` (esplugin header classifier).
- **Tests** — new `starfield_bundled_snapshot_is_present_and_nonempty`; the three
  allow-list tests (`game_type_for`, `appdata_folder_name`, `esplugin_game_id`) now assert
  all three games pass and junk appids (0, 220) still return `None`.

No dependency, MSRV, `core`, or DB-migration change: `GameType::Starfield` /
`GameId::Starfield` already exist in the pinned libloot 0.29.5 / esplugin 6.1.4.

## Verification

- `cargo test -p nextwist-loadorder --locked` — green (lib + integration).
- `cargo clippy -p nextwist-loadorder --all-targets --locked -- -D warnings` — clean.
- `cargo deny check bans licenses sources` — pass (no new deps; masterlist is a data asset).

## Deviations from Plan

None to the implementation. One out-of-scope discovery (below).

### Deferred Issues

`cargo deny check advisories` FAILED on **pre-existing, time-based** RUSTSEC advisories in
already-pinned transitive deps — NOT introduced by this plan (zero deps added):
RUSTSEC-2026-0190 (anyhow), -0204 (crossbeam-epoch), -0194/-0195 (quick-xml). Logged to
`deferred-items.md` per the executor scope boundary; `bans`/`licenses`/`sources` pass.
Resolve in a dedicated dependency-refresh task (`cargo update -p anyhow -p crossbeam-epoch -p quick-xml`).

## Threat Model Adherence

- T-06-02b (masterlist provenance): fetched only from official `loot/starfield@v0.29` raw
  HTTPS URL, LICENSE confirmed CC0-1.0, saved verbatim (no post-processing).
- T-06-SC (supply chain): no packages installed; the masterlist is a bundled data asset.

## Known Stubs

None.

## Commits

- `a211ee4` feat(06-02): bundle Starfield masterlist + game_slug/bundled_snapshot arms
- `4261c47` feat(06-02): allow-list Starfield in loot.rs + scan.rs (SFDET-01)
- `3fc0d73` chore(06-02): log pre-existing cargo-deny advisories to deferred-items

## Self-Check: PASSED

All 4 artifact files present; all 3 task commits found in git history.
