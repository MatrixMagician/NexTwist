# Architecture Research

**Domain:** Integrating a new Bethesda game (Starfield, Creation Engine 2) into an existing Rust + Tauri Linux mod-manager engine
**Researched:** 2026-07-07
**Confidence:** HIGH for game registration + load order (verified against source + dep versions); MEDIUM for the reversible-INI design (primitives verified reusable; exact CE2 INI keys and libloot Starfield early-loader completeness need on-hardware verification)

## TL;DR for the Roadmapper

Adding Starfield is a **data-and-mapping extension, not a re-architecture**. Three facts drive everything:

1. **`core::Game` is game-agnostic** — a plain struct keyed by `appid: u32`. No enum, no per-game variant. Nothing in `core` changes.
2. **The store is fully appid-generic** — `managed_game`, `vanilla_backup`, `deployed_file`, journal, profiles all key on `appid`/`(appid, target_rel)` as opaque values. **No refinery migration is needed** (registry is data-driven; current migrations stop at V5).
3. **The dependencies already support Starfield** — `libloot 0.29` has `GameType::Starfield`, `esplugin 6.1` has `GameId::Starfield` (with light-plugin support). **No dependency bump.**

"Supported game" is expressed as **allow-list `match appid` arms and `const` AppIDs duplicated across ~6 sites.** Adding Starfield = adding one arm/const at each. The one genuinely NEW capability is reversible `StarfieldCustom.ini` management — and it can **reuse the existing `backup.rs` + journal primitives verbatim**, because those primitives are path-generic; only the deploy *engine orchestration* is bounded to `Data/`.

---

## Existing Architecture (integrate WITH this)

```
┌──────────────────────── Tauri shell (src-tauri/) — thin adapters ────────────────────────┐
│  commands/{games,mods,deploy,conflicts,plugins,profiles,collections}.rs  lib.rs (startup) │
│  appid_for_domain()  require_game()  recover_on_launch()-per-game-before-UI               │
└───────────────────────────────────────────┬───────────────────────────────────────────────┘
                                             │ speaks core::Game only
┌──────────────────────────── headless crates/* engine (ZERO Tauri deps) ───────────────────┐
│  core   Game{appid,name,install_dir,prefix,staging_dir}  ManagedMod Profile Plugin FileEntry│
│  steam  resolve_game/detect_games (allow-list) · appdata paths · casing.rs                 │
│  extract  archive → validated Data/-rooted staging tree                                    │
│  loadorder  libloot seam: game_type_for · appdata_folder_name · game_id_for · game_slug    │
│  deploy  CROWN JEWEL: probe→method-ladder→journal(intent-before-act)→backup→purge          │
│          engine.rs {deploy, deploy_winners, redeploy_winners, purge, recover_on_launch}    │
│          backup.rs {backup_vanilla_if_absent, restore_vanilla}  ← PATH-GENERIC             │
│          profile.rs {switch_profile}                                                       │
│  store  SQLite facade: managed_game · deployed_file · journal · vanilla_backup · profiles  │
│         (no rusqlite in public API; all keyed on appid — GAME-AGNOSTIC)                     │
└────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Question 1 — Game model & the exact extension points

### How a supported game is represented

- **`core::Game`** (`crates/core/src/model.rs:18-30`): a plain struct `{ appid: u32, name, install_dir, prefix, staging_dir }`. **No enum, no per-game type.** Serde round-trips any appid. → **NO CHANGE.**
- **Store registry** (`crates/store/src/registry.rs`): `add_managed_game(&Game)` / `get_game(appid)` / `list_managed_games()` — pure upsert on the `managed_game` table, appid opaque. `vanilla_backup` (`vanilla.rs`), `deployed_file`, journal all key on `appid`/`(appid, target_rel)` generically. → **NO CHANGE, NO MIGRATION.**
- **"Supported"** = an **allow-list of `const` AppIDs + `match appid` arms**, duplicated across crates (each redefines its own `SKYRIM_SE = 489830` / `FALLOUT4 = 377160`).

### The exact sites to add Starfield (`appid 1716740`)

| # | File / function | Current | Add for Starfield | Kind |
|---|-----------------|---------|-------------------|------|
| 1 | `steam/src/resolve.rs` — `SKYRIM_SE`/`FALLOUT4` consts, `SUPPORTED_APPIDS`, `default_name()`, `expected_exe()` | 489830 / 377160; exes `SkyrimSE.exe`/`Fallout4.exe` | `STARFIELD = 1716740`; add to `SUPPORTED_APPIDS`; name `"Starfield"`; exe `"Starfield.exe"` | modify (mapping) |
| 2 | `steam/src/discover.rs` — `detect_games()` | iterates `SUPPORTED_APPIDS` | automatic once #1 adds the const | no code change |
| 3 | `loadorder/src/loot.rs` — `game_type_for()`, `appdata_folder_name()` (+ local `SKYRIM_SE`/`FALLOUT4` consts) | `GameType::SkyrimSE`/`Fallout4`; folders `"Skyrim Special Edition"`/`"Fallout4"` | `1716740 => GameType::Starfield`; folder `"Starfield"` | modify (mapping) |
| 4 | `loadorder/src/scan.rs` — `game_id_for()` (+ local consts) | `GameId::SkyrimSE`/`Fallout4` | `1716740 => GameId::Starfield` | modify (mapping) |
| 5 | `loadorder/src/masterlist.rs` — `game_slug()`, `bundled_snapshot()`, snapshot `include_str!`s | slugs `"skyrimse"`/`"fallout4"`; `assets/{skyrimse,fallout4}/masterlist.yaml` | slug `"starfield"`; ship `assets/starfield/masterlist.yaml` (CC0, `loot/starfield`); add arm | modify + **new asset** |
| 6 | `src-tauri/src/commands/mod.rs` — `appid_for_domain()` | `"skyrimspecialedition"`/`"fallout4"` → appid | `"starfield" => Some(1716740)` (Nexus domain slug) | modify (mapping) |

**Resolved paths for Starfield** (all follow existing patterns):
- Install: `steamapps/common/Starfield`; prefix `steamapps/compatdata/1716740/pfx` (both derived by existing `steam::resolve`).
- `plugins.txt` (asterisk format): `<prefix>/drive_c/users/steamuser/AppData/Local/Starfield/Plugins.txt` — produced by the **existing** `appdata_local_path(prefix, "Starfield")`.
- `StarfieldCustom.ini`: `<prefix>/drive_c/users/steamuser/Documents/My Games/Starfield/StarfieldCustom.ini` — **NEW path root** (`Documents/My Games`, not `AppData/Local`); see Q3.

**Ponytail note — optional consolidation:** these mappings are duplicated across three `loadorder` modules (each with its own `SKYRIM_SE`/`FALLOUT4` const). Adding a third game is the moment a single `struct GameProfile { appid, game_type, game_id, appdata_folder, loot_slug, exe, nexus_domain }` table pays for itself. It is **not required** — three match arms work — but if the roadmap wants one, do it as a small refactor folded into game registration, not a separate epic. Recommend: **add the arms now, keep a `// ponytail:` note that a 4th game should trigger the table.**

---

## Question 2 — Load order: where the game-specific config is chosen

**Everything lives in `crates/loadorder`.** The generic sort/apply machinery (`apply_load_order`, `propose_sort`, `reconcile_order`, `asterisk_plugins_txt`, `masters_first_order`) is already game-agnostic and takes `appid` + resolved paths. Only the four mappings in Q1 rows 3–5 select per-game behaviour:

- `loot::game_type_for(appid)` → `libloot::GameType` (the game handle).
- `loot::appdata_folder_name(appid)` → the `AppData/Local/<folder>` segment for `plugins.txt`.
- `scan::game_id_for(appid)` → `esplugin::GameId` (header master/light classification).
- `masterlist::game_slug(appid)` + bundled snapshot → the LOOT masterlist repo/asset.

**What must change:** add the Starfield arm to each (done in Q1). **No new load-order logic.** Two Starfield facts confirm the existing code already handles it:

1. **Asterisk `Plugins.txt` format** — Starfield uses the same `*Enabled.esm` asterisk method as SkyrimSE/FO4; `asterisk_plugins_txt` applies unchanged.
2. **Large early-loader / CCC set** — Starfield ships many implicitly-active plugins (game master + hardcoded list + Creation-Club `*.ccc`). `reconcile_order` was **built to defer the early-loader prefix to libloot's canonical order** (it exists precisely because FO4's DLC list broke a hand-rolled master sort — see `loot.rs` RC1 comment). This is exactly the shape Starfield needs. No new handling.

**Flag for verification (Phase 7 UAT):** confirm libloot 0.29.x's Starfield masterlist branch (`v0.29`) exists on `loot/starfield` and that its early-loader handling matches the installed game version — Starfield's plugin list has churned across game updates. `ba2` v3 archive awareness is a plugin-content concern, not a load-order-code concern; note it but it does not change this crate.

---

## Question 3 — NEW component: reversible `StarfieldCustom.ini` management

This is the **only genuinely new capability.** Requirement: on activate, add the CE2 loose-file keys to `StarfieldCustom.ini`; on purge, restore the file **byte-for-byte** (or delete it if we created it) under the same reversibility guarantee as deployment.

### Why the existing `deploy()`/`purge()` engine can't cover it directly

- `resolve_target` / `guard_within_root` / `deploy_root` bound every deployed path to `<install_dir>/Data`. The INI lives in the **Proton prefix's `Documents/My Games/Starfield`**, outside that root.
- Deployed files are *linked from a mod staging tree*; the INI content is *generated by NexTwist* (merge `[Archive]` keys), so there is no source file to link.

### Why the existing PRIMITIVES cover it perfectly (the reuse win)

`backup::backup_vanilla_if_absent(store, game, target, target_rel)` and `backup::restore_vanilla(...)` (`crates/deploy/src/backup.rs`) are **path-generic** — they take an arbitrary absolute `target` and an opaque `target_rel` key, content-address the original with blake3 into `<staging>/../originals/<appid>/<hash>`, and record `(appid, target_rel, hash)` in the `vanilla_backup` ledger. They are **not** bounded to `Data/`. The content-addressed originals store + `vanilla_backup` table already give byte-for-byte restore of a pre-existing INI, deduped and idempotent. The `journal` (`begin_deploy`/`finish_deploy`/`begin_purge`/`finish_purge` + `replay`) is likewise keyed on an opaque `target_rel` string.

### Recommendation — a small `gameconfig` module INSIDE `crates/deploy` (NOT a new crate)

A new crate would be ceremony: the logic needs `store`, `core`, `backup`, and `journal` — all already siblings in `deploy`. A new `crates/deploy/src/gameconfig.rs` (sibling of `backup.rs`) is the laziest correct home and keeps it under the crown-jewel's test harness.

**Public surface (~120 LOC incl. the idempotent INI merge + one self-check):**

```
gameconfig::activate_loose_files(store, game)   // no-op unless is_starfield(game.appid)
gameconfig::deactivate_loose_files(store, game) // no-op unless is_starfield(game.appid)
```

**`activate` sequence** (mirrors `deploy_one_file`, reusing its primitives):
1. Resolve INI path from `game.prefix` via a **new tiny `steam` helper** `my_games_path(prefix, "Starfield")` — a 5-line sibling of `appdata_local_path`.
2. `journal::begin_deploy(...)` with a **sentinel key** (e.g. `"@gameconfig/StarfieldCustom.ini"` — cannot collide with any `Data/`-relative path). Durable intent BEFORE the write.
3. `backup::backup_vanilla_if_absent(store, game, &ini, &sentinel_key)` — **REUSED VERBATIM.** Backs up any existing INI byte-for-byte (or returns `false` = pure-add when absent). This is the reversibility guarantee, unchanged.
4. Idempotent INI key-merge: ensure `[Archive]` `bInvalidateOlderFiles=1` and `sResourceDataDirsFinal=STRINGS\` (the ~40 LOC of genuinely new code; leaves any other user keys intact so a re-run is a no-op).
5. `journal::finish_deploy(store, jid, appid, FileEntry{ target_rel: sentinel, source_mod: 0, method: Copy, hash, pre_existing: backed })` — the `pre_existing` bool is the one bit purge needs to decide restore-vs-delete. **Reuses `FileEntry` unchanged; no new table.**

**`deactivate` sequence** (its own tiny loop, because the INI is outside the Data/ purge root):
1. Read the recorded INI entry (by sentinel key); `journal::begin_purge`.
2. If `pre_existing`: `backup::restore_vanilla(...)` → **REUSED VERBATIM**, restoring exact original bytes. Else (pure-add): `method::remove_if_present(&ini)` to return to absent.
3. Drop the manifest row + `journal::finish_purge`. Crash mid-edit is replayed by `recover_on_launch` because we used the same journal.

**No migration required:** the sentinel key rides in the existing `vanilla_backup` / journal / `deployed_file` tables as opaque text. (An explicit `active_gameconfig` table would be *clearer* but is not needed for correctness — recommend skipping it; add only if a second game-config file appears.)

### Exact lifecycle integration points (single choke, inherited by every entry point)

Route the two calls through the engine functions, gated on `is_starfield`, so **all** command entry points (deploy, purge, conflicts, profile switch, collections) inherit the behaviour without per-caller edits — the "fix it once where all callers route through" move:

| Hook | Function (`crates/deploy/src/engine.rs`) | Add | Covers |
|------|-------------------------------------------|-----|--------|
| Activate | end of `deploy()` and `deploy_winners()` | `gameconfig::activate_loose_files(store, game)?` | single-mod deploy, conflict winner set, `redeploy_winners`, `switch_profile` (its `deploy_winners` step), collections apply |
| Deactivate | end of `purge()` | `gameconfig::deactivate_loose_files(store, game)?` | standalone purge, `redeploy_winners`'s purge, `switch_profile`'s purge, collection uninstall |
| Replay | `recover_on_launch()` | already replays the journal rows; ensure the sentinel op is handled in `journal::replay` | crash mid-INI-edit |

Net effect is correct and idempotent: a profile switch = `purge`(deactivate) → `deploy_winners`(activate) = net active; standalone `purge` = deactivate. Non-Starfield games short-circuit to a no-op. **Do not** wire these into the thin `src-tauri/commands/*` adapters — keep the logic in the engine (honours the zero-logic-in-adapters boundary).

**Flag for on-hardware verification (Phase 8/9):** the exact CE2 loose-file keys have been finicky across Starfield patches (`sResourceDataDirsFinal` contents, whether `StarfieldCustom.ini` vs `Starfield.ini`, `bInvalidateOlderFiles` behaviour). The owner has Starfield on Proton — verify the chosen keys actually make a loose-file mod load in-game before locking the merge logic. This is the one place a minimal model can be wrong against the real game.

---

## Question 4 — Suggested build order (phases, with dependencies)

```
Phase 6  Game registration + detection   ──┬──▶ Phase 7  Load order (Starfield)
  (no deps)                                 │
                                            └──▶ Phase 8  Reversible INI activation
                                                            │
                        Phase 9  On-hardware in-game verification ◀── (7 AND 8)
```

| Phase | Scope | Depends on | Done-when (verifiable) |
|-------|-------|-----------|------------------------|
| **6 — Game registration & detection** | Add `STARFIELD = 1716740` across the 6 allow-list sites (Q1 rows 1,3,4,5,6); name/exe; ship `assets/starfield/masterlist.yaml`. Store already generic. | none | `detect_games()` lists Starfield; `add_managed_game` + `get_game(1716740)` round-trip; resolve yields correct install/prefix. |
| **7 — Load order** | Verify the four mappings drive a full plugin scan → LOOT sort → `apply_load_order` for Starfield. | 6 (needs a resolved `Game`) | `Plugins.txt` written (asterisk) at `AppData/Local/Starfield`; `propose_sort` returns a Starfield-masterlist order; early-loader prefix accepted by libloot (no `"load order interaction failed"`). |
| **8 — Reversible INI activation** | New `deploy::gameconfig` module + `steam::my_games_path`; wire activate/deactivate into `deploy`/`deploy_winners`/`purge`/`recover_on_launch`; reuse `backup.rs` + journal. | 6 (needs prefix path); **soft-after 7** (cleaner once a real Starfield deploy exists to hook) | round-trip-pristine test (reuse `testkit` blake3 assertions) proves INI restored byte-for-byte on purge, and deleted when created pure-add; crash-injection replay via `recover_on_launch` converges. |
| **9 — On-hardware in-game verification** | Owner deploys a real Starfield mod on Proton; confirm it loads AND is visible in-game; confirm the INI keys are correct. | 7 AND 8 | a real loose-file + plugin mod is visible in-game; purge leaves the game + INI pristine. |

**Dependency notes:**
- 7 and 8 are **independent after 6** and *could* run in parallel; recommend 8 lands after 7 so its lifecycle hook is exercised against a working Starfield deployment (lower integration risk than wiring the hook blind).
- 9 is a **gate**, not code — it validates the CE2 domain assumptions (INI keys, loose-file loading, `ba2` v3) that Phases 7–8 encode. The owner's live Proton install is the only way to close the MEDIUM-confidence items.

---

## Anti-patterns to avoid (project-specific)

| Anti-pattern | Why bad here | Instead |
|--------------|--------------|---------|
| A new `crates/gameconfig` crate for the INI | Ceremony for ~120 LOC that needs `store`/`core`/`backup`/`journal` — all already in `deploy`; a new crate also escapes the crown-jewel test harness | `crates/deploy/src/gameconfig.rs` reusing `backup.rs` + journal |
| Hand-rolling INI backup/restore | Re-implements the proven content-addressed vanilla ledger; risks the reversibility guarantee | Call `backup::backup_vanilla_if_absent` / `restore_vanilla` verbatim (they're path-generic) |
| Generalizing `resolve_target`/`deploy_root` to reach outside `Data/` | Touches the crown-jewel path guard for one file; widens the containment invariant that protects every deploy | Keep the INI on its own tiny activate/deactivate path with a sentinel key; leave the Data/ guard untouched |
| A refinery migration for Starfield | Registry + ledgers are appid-generic; nothing schema-shaped changes | No migration; add data (const/arm) only |
| Bumping libloot/esplugin for Starfield | Both 0.29/6.1 already expose Starfield variants | Add the `match` arms; no version change |
| Wiring INI calls into `src-tauri/commands/*` | Violates the zero-logic-in-adapters boundary and forgets an entry point | Hook the engine `deploy`/`deploy_winners`/`purge` once; all commands inherit it |

## Sources

- Direct source read (HIGH): `crates/core/src/model.rs`, `crates/store/src/{registry,vanilla}.rs`, `crates/steam/src/{resolve,discover}.rs`, `crates/loadorder/src/{lib,loot,scan,masterlist}.rs`, `crates/deploy/src/{lib,engine,backup,profile}.rs`, `src-tauri/src/commands/{deploy,profiles}.rs`, migrations `V1..V5`.
- Dependency capability (HIGH): installed `libloot 0.29` `GameType::Starfield` and `esplugin 6.1` `GameId::Starfield` (source-inspected in `~/.cargo`).
- Starfield AppID 1716740, `Documents/My Games/Starfield/StarfieldCustom.ini`, CE2 loose-file `[Archive]` keys, `loot/starfield` masterlist (MEDIUM — community/modding knowledge; the INI keys and libloot Starfield early-loader completeness are flagged for on-hardware verification in Phase 9).
