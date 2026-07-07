# Roadmap: NexTwist

## Milestones

- ✅ **v1.0 MVP** — Phases 1–5 (shipped 2026-06-23) — full detail in [`milestones/v1.0-ROADMAP.md`](milestones/v1.0-ROADMAP.md)
- 🚧 **v1.1 Starfield Support** — Phases 6–9 (planning) — bring safe, reversible mod management to Starfield (CE2, AppID 1716740)

## Phases

<details>
<summary>✅ v1.0 MVP (Phases 1–5) — SHIPPED 2026-06-23</summary>

- [x] Phase 1: Safe Local Round-Trip (7 plans) — completed 2026-06-20
- [x] Phase 2: Multi-Mod Management (5 plans) — completed 2026-06-21
- [x] Phase 3: NexusMods Login & Download (3 plans) — completed 2026-06-21
- [x] Phase 4: Guided Installers & Collections (4 plans) — completed 2026-06-21
- [x] Phase 5: AppImage Distribution (2 plans) — completed 2026-06-22

Delivered: safe, fully-reversible, conflict-aware mod deployment for Windows games on Linux via Steam Proton/Wine, with NexusMods login + download, FOMOD installers, one-click Collections, and a license-clean AppImage. 40/40 v1 requirements satisfied; all phases secured.

</details>

### 🚧 v1.1 Starfield Support (Phases 6–9)

Extend the shipped v1.0 headless engine to Starfield (Creation Engine 2, Steam AppID 1716740). A data-and-mapping extension, not a re-architecture: reuse the deploy ladder, journal, conflicts, profiles, and load-order seams; add CE2-specific handling so mods actually **load in-game**. The v1.0 safety guarantee (non-destructive, byte-for-byte reversible, conflict-aware) must extend verbatim to the new `StarfieldCustom.ini` write target.

**Dependency shape:** Phase 6 gates everything (detection + CE2 path). Phases 7 and 8 fan out from 6 (8 sequenced soft-after 7 to hook a working deploy). Phase 9 is a validation gate requiring both 7 and 8.

```
Phase 6  Detection & CE2 path  ──┬──▶ Phase 7  Load order ──┐
  (gate)                         │                          ├──▶ Phase 9  On-hardware verify (GATE)
                                 └──▶ Phase 8  Reversible INI ┘
```

- [x] **Phase 6: Starfield Detection & CE2 Path Resolution** — Detect Starfield under Proton; resolve the CE2 `My Games/Starfield` config path (case-folded, first-launch-aware) + version-drift signal (completed 2026-07-07)
- [ ] **Phase 7: Starfield Load Order** — Asterisk `plugins.txt` at the CE2 path + LOOT sort via `GameType::Starfield`; protected base masters + medium-master tier handled via libloot
- [ ] **Phase 8: Reversible StarfieldCustom.ini Activation** — Reversible loose-file activation (provenance-driven restore-vs-delete, surgical byte-fidelity merge, journaled idempotency) upholding the byte-for-byte safety guarantee
- [ ] **Phase 9: On-Hardware In-Game Verification** — Prove a real Starfield mod deploys AND is visible/loaded in-game on the owner's live Proton install; surface the deployed-vs-loaded distinction

## Phase Details

### Phase 6: Starfield Detection & CE2 Path Resolution

**Goal**: NexTwist detects Starfield under Steam/Proton and resolves its CE2 config location so the game can be managed like the existing Bethesda titles. Gates both Phase 7 and Phase 8.
**Depends on**: Nothing new (builds on the shipped v1.0 steam/casing/registry seams)
**Requirements**: SFDET-01, SFDET-02, SFDET-03
**Success Criteria** (what must be TRUE):

  1. User can add Starfield (AppID 1716740) as a managed Bethesda game; NexTwist auto-detects it under Steam/Proton with install dir and Proton prefix resolved, exactly like Skyrim SE / Fallout 4 today.
  2. NexTwist resolves the CE2 `Documents/My Games/Starfield` path **inside the Proton prefix** (never a Linux-side `~/Documents`), correctly case-folded and Documents-redirection-aware — verified against a real prefix and a case-mismatched fixture.
  3. When `My Games/Starfield` does not yet exist (game not launched once), NexTwist detects this first-launch-not-done state and guides the user to launch the game once, rather than silently writing to a useless path.
  4. NexTwist reports the installed Starfield build and warns the user when it is newer than the last build the loose-file / load-order behavior was validated against (version drift).

**Scope notes**: ~6 allow-list `match appid` arms across `steam`/`loadorder`/Tauri + a bundled `assets/starfield/masterlist.yaml`; new `steam::my_games_path` resolver mirroring the existing `appdata_local_path` seam. No `core` change, no DB migration. Avoids Pitfall 6 (wrong/absent/mis-cased prefix path); owns the version-drift signal for Pitfall 8.
**Plans**: 3 plans

- [x] 06-01-PLAN.md — Steam detection core: allow-list AppID 1716740, `ce2.rs` My-Games resolver + `Ce2ConfigState` + drift compare, testkit fixtures (wave 1)
- [x] 06-02-PLAN.md — Loadorder Starfield allow-list arms + bundled `assets/starfield/masterlist.yaml` (wave 1)
- [x] 06-03-PLAN.md — Thin Tauri `starfield_status` command + Starfield game-view first-launch guidance/Re-check + drift notice (wave 2)

### Phase 7: Starfield Load Order

**Goal**: Users can enable, order, and LOOT-sort Starfield plugins safely, with the CE2 medium-master tier and protected base masters handled correctly via libloot.
**Depends on**: Phase 6 (needs a resolved `Game`)
**Requirements**: SFLO-01, SFLO-02, SFLO-03, SFLO-04
**Success Criteria** (what must be TRUE):

  1. User can enable/disable and reorder Starfield plugins; NexTwist writes them as asterisk-format `plugins.txt` at the resolved CE2 prefix location.
  2. User can auto-sort the Starfield load order via LOOT using the bundled Starfield masterlist, and the result agrees with LOOT desktop on the same inputs (masterlist age surfaced).
  3. NexTwist never writes, disables, or reorders Starfield's protected base masters (`Starfield.esm`, `SFBGS0xx.esm`, `BlueprintShips-*`, …) — determined via libloot, never hard-coded — and correctly classifies the medium-master tier; protected masters appear locked/non-editable in the load-order view.
  4. Verify/repair treats the game's on-launch rewrite of `plugins.txt` (`.ccc` entries, stripped implicit ESMs, `BlueprintShips-*`) as expected, deriving intent from recorded plugin state rather than flagging a raw on-disk diff.

**Scope notes**: Reuses the v1.0 libloot sort/apply machinery verbatim; only new inputs are `GameType::Starfield` + slug + bundled masterlist. Avoids Pitfalls 5 (protected masters), 7 (medium tier), 9 (masterlist currency).
**Plans**: 3 plans
**UI hint**: yes

- [ ] 07-01-PLAN.md — Engine: medium classification + PluginView, protected-master probe + defensive guard, SFLO-04 reconcile, masterlist-date (wave 1)
- [ ] 07-02-PLAN.md — Thin Tauri wiring: list_plugins → PluginView (medium/protected), reconcile_plugins command (wave 2)
- [ ] 07-03-PLAN.md — Frontend surfacing: protected locked rows, MEDIUM/PROTECTED badges, masterlist-age note, SFLO-04 states (wave 3)

### Phase 8: Reversible StarfieldCustom.ini Activation

**Goal**: Users can enable loose-file loading for Starfield through a reversible `StarfieldCustom.ini` edit that upholds the non-destructive, byte-for-byte reversible safety guarantee. Highest-risk new safety surface of the milestone.
**Depends on**: Phase 6 (needs the CE2 prefix path); soft-after Phase 7 (hook the lifecycle against a working Starfield deploy rather than blind)
**Requirements**: SFINI-01, SFINI-02, SFINI-03, SFINI-04, SFINI-05
**Success Criteria** (what must be TRUE):

  1. User can enable loose-file loading; NexTwist writes `[Archive] bInvalidateOlderFiles=1` and `sResourceDataDirsFinal=` into `StarfieldCustom.ini` at the CE2 prefix path, after showing the user the exact reversible change (no silent edit).
  2. On purge, NexTwist restores the INI exactly to its pre-mod state — byte-for-byte original bytes if it pre-existed, or **deletes** the file (and any NexTwist-created now-empty parent dirs) if NexTwist created it (restore-absence) — driven by recorded `PreExisting`-vs-`CreatedByNexTwist` provenance.
  3. NexTwist merges its two owned keys surgically into an existing user `StarfieldCustom.ini` without clobbering other sections/keys, preserving byte fidelity (CRLF line endings, BOM); an existing user `sResourceDataDirsFinal` value surfaces as a conflict rather than being overwritten.
  4. The INI edit is journaled and idempotent — running deploy twice, a profile switch, or a crash-replay via `recover_on_launch` yields one `[Archive]` section and identical bytes.
  5. The INI activation participates in the same reversibility guarantee as deployment — covered by crash recovery (`recover_on_launch`) and verify/repair, and regression-locked by a testkit reversibility suite (extending `DIR_SENTINEL`) across both provenance branches.

**Scope notes**: New `crates/deploy/src/gameconfig.rs` (~120 LOC) reusing `backup::backup_vanilla_if_absent` / `restore_vanilla` + the journal verbatim, wired at the single engine choke point (end of `deploy`/`deploy_winners`/`purge` + `recover_on_launch`), gated on `is_starfield`. INI rides its own sentinel key outside the `Data/` deploy root — leave the deploy-root path guard untouched. Avoids Pitfalls 1–4 (absence-restore, clobbering, byte-fidelity, idempotency). **Warrants its own SECURITY.md** (`/gsd-plan-phase --research-phase 8` recommended).
**Plans**: TBD
**UI hint**: yes

### Phase 9: On-Hardware In-Game Verification

**Goal**: Confirm on the owner's real Proton install that a Starfield mod deploys AND loads/is visible in-game, and surface the deployed-vs-loaded distinction so users understand deployment alone does not guarantee in-game loading. A validation GATE, not a large code phase.
**Depends on**: Phase 7 AND Phase 8 (in-game visibility requires both correct plugin activation and the loose-file INI edit)
**Requirements**: SFVER-01, SFVER-02
**Success Criteria** (what must be TRUE):

  1. On the owner's live Proton Starfield install, a real Nexus loose-file + plugin mod deploys and is confirmed **visible/loaded in-game**; purge afterward leaves the game and `StarfieldCustom.ini` byte-for-byte pristine.
  2. NexTwist's LOOT sort output agrees with LOOT desktop on the same inputs, and the game does not strip/reorder what NexTwist wrote beyond the expected on-launch rewrite.
  3. NexTwist surfaces a "deployed vs actually loaded in-game" distinction in the UI, so the user understands successful deployment does not by itself guarantee in-game loading.
  4. The exact CE2 INI keys and masterlist currency are validated against the installed game build and recorded as validated-against-a-build (not a permanent constant) — closing the MEDIUM-confidence assumptions Phases 7–8 encode.

**Scope notes**: The only way to close the MEDIUM-confidence CE2 loose-file specifics (exact INI keys, loose-file loading, `.ba2` v3 opacity). Owns the deployed-vs-loaded gap (Pitfall 8). Treat the INI recipe as validated-against-a-build; re-verify after Starfield patches.
**Plans**: TBD
**UI hint**: yes

## Progress

| Phase | Milestone | Plans | Status | Completed |
|-------|-----------|-------|--------|-----------|
| 1. Safe Local Round-Trip | v1.0 | 7/7 | Complete | 2026-06-20 |
| 2. Multi-Mod Management | v1.0 | 5/5 | Complete | 2026-06-21 |
| 3. NexusMods Login & Download | v1.0 | 3/3 | Complete | 2026-06-21 |
| 4. Guided Installers & Collections | v1.0 | 4/4 | Complete | 2026-06-21 |
| 5. AppImage Distribution | v1.0 | 2/2 | Complete | 2026-06-22 |
| 6. Starfield Detection & CE2 Path Resolution | v1.1 | 3/3 | Complete    | 2026-07-07 |
| 7. Starfield Load Order | v1.1 | 0/3 | Not started | - |
| 8. Reversible StarfieldCustom.ini Activation | v1.1 | 0/? | Not started | - |
| 9. On-Hardware In-Game Verification | v1.1 | 0/? | Not started | - |
