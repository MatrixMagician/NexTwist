# Phase 2: Multi-Mod Management - Context

**Gathered:** 2026-06-20
**Status:** Ready for planning
**Mode:** Smart discuss (autonomous) — 16 decisions across 4 areas, all recommended answers accepted

<domain>
## Phase Boundary

This phase turns the single-mod safe round-trip (Phase 1) into **real multi-mod management** for Bethesda Creation Engine games under Proton, still routing every disk change through the Phase-1 safety engine (staging → manifest → journal → deploy → purge-to-pristine). A user can:

1. **See file-level conflicts** between multiple staged/enabled mods (which mods overwrite which paths) and **set mod priority/order** so a chosen mod deterministically wins; deployment applies those winner choices (CONF-01/02/03).
2. **Enable/disable individual plugins** (.esp/.esm/.esl) and **adjust plugin load order**, written to `plugins.txt` in the correct Proton-prefix `AppData` location so it applies in-game (PLUGIN-01/02).
3. **Auto-sort plugins via LOOT** (PLUGIN-03).
4. **Create multiple independent profiles per game**, switch the active profile to change which mods/plugins/order are deployed, with each profile preserving its own enabled-mod set and load order (PROF-01/02/03).

**Requirements covered:** CONF-01..03, PLUGIN-01..03, PROF-01..03 (9 requirements).

**Explicitly out of scope for this phase:** any NexusMods auth/download/nxm:// (Phase 3); FOMOD guided installers + Collections (Phase 4); AppImage packaging + license audit (Phase 5); MO2-style save-game/INI redirection per profile (deferred — heavier, not required for v1 parity); full LOOT plugin-cleaning UI (only sort + critical warnings in v1).

</domain>

<decisions>
## Implementation Decisions

### Conflict Model & Priority (CONF-01/02/03)
- **Winner is decided by an explicit ordered priority list** — each managed mod has a rank (Vortex "deployment rank" / MO2 priority model); the higher-priority mod deterministically wins any file it shares with a lower-priority mod. (Vortex rule-based "X after Y" and pure per-file manual override were considered and rejected as heavier for v1.)
- **Conflict view shows both file-level and per-mod**: a file-level conflict list (for each contested `target_rel`, which mods provide it and who currently wins) plus a per-mod "overwrites / overwritten-by" summary. CONF-01 is explicit about *file-level* conflicts.
- **Winner convention: higher priority wins, and the winning (mod, file, hash) is recorded in the per-game manifest** so deployment is deterministic and fully reversible (purge still restores pristine). The conflict resolution is computed across all enabled mods' staged trees, not guessed at deploy time.
- **Priority changes are pending until an explicit "Deploy"** — mirrors the Phase-1 deploy/purge button model (safe, reviewable, no surprise disk mutation). A pending-changes indicator shows the deployed set is stale vs the chosen winner set. (Auto-redeploy-on-every-change and mandatory dry-run-diff were considered; explicit Deploy is the safety-consistent default.)

### Plugin Management — .esp/.esm/.esl (PLUGIN-01/02)
- **`plugins.txt` location is derived per-game from the resolved Proton prefix**, at `<prefix>/drive_c/users/steamuser/AppData/Local/<GameName>/Plugins.txt` (e.g. `Skyrim Special Edition`, `Fallout4`). Re-resolve from the prefix each session (consistent with Phase-1 prefix handling). A user override is a possible future enhancement, not v1.
- **Plugin discovery scans the enabled mods' staged file trees plus the game `Data/` directory** for `.esp`/`.esm`/`.esl` files, presenting them in a list with per-plugin enable toggles. Base-game master files are included.
- **`plugins.txt` is written in the asterisk-enabled format** (`*PluginName.esp` denotes an enabled plugin) which is the correct convention for Skyrim SE and Fallout 4. Disabled plugins are written without the leading `*` (or omitted) per the engine's expected format — confirm exact disabled-line handling in plan research.
- **Master-first ordering is enforced as a hard invariant**: master files (`.esm` and ESL-flagged plugins) are ordered before regular `.esp` plugins; LOOT refines ordering *within* that constraint. The engine requires masters-before-dependents, so this is non-negotiable structural ordering, not a preference.

### LOOT Auto-Sort (PLUGIN-03) — flagged for plan-time research
- **Integrate libloot (the sorting engine behind the LOOT app), via a Rust FFI binding** rather than re-implementing sorting. The exact crate/binding (e.g. an existing `libloot` Rust binding vs. building a thin FFI over the C++ library, vs. a maintained CLI fallback) is to be confirmed by plan-phase research — this is the single largest technical unknown in the phase. Shelling out to a LOOT CLI is the acceptable fallback if no viable in-process binding exists for an AppImage-friendly static build.
- **Masterlist is downloaded and cached per-game** from LOOT's official masterlist repository, with a manual refresh action; a bundled snapshot is the offline fallback. (Required because LOOT sorting is masterlist-driven.)
- **v1 LOOT scope = sort order + surfacing critical warnings** (dirty plugins needing cleaning, missing masters). A full in-app plugin-cleaning workflow is deferred.
- **Apply model: LOOT proposes a sorted order, the user reviews it, then applies** — the resulting order remains hand-editable afterward (LOOT is a one-click "Sort" that the user can still override). No silent auto-apply.

### Profiles (PROF-01/02/03)
- **A profile scopes, per game: the enabled-mod set, the mod priority order, and the plugin enable state + plugin load order.** Save-game and INI redirection (MO2-style) are explicitly deferred — not required for v1.
- **Single shared staging store; a profile is a lightweight set of references** (which mods are enabled + their priority + plugin order), never duplicated staged file trees. This matches the Phase-1 reflink/hardlink space-efficiency ethos and keeps profile switching cheap.
- **Switching the active profile reconciles deployment through the safe engine** — purge the previously-deployed set, then deploy the new profile's winner set, all behind the existing journal/manifest crash-safety, with a user confirmation before mutating disk. Switching is never a silent disk operation; the round-trip-pristine guarantee holds across switches.
- **Auto-create a "Default" profile per game; migrate the Phase-1 single-mod state into it.** Profiles, profile membership (enabled mods + priority), and plugin state persist in SQLite via a new **V2 refinery migration** (V1 has only `managed_game`, `deployed_file`, `op_journal`, `vanilla_backup` — no mod/profile/plugin tables yet).

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`crates/core/src/model.rs`** already defines the stable vocabulary: `Game`, `ManagedMod { id, name, staging_root, enabled }` (the `id`/`enabled` fields were deliberately scaffolded in Phase 1 "to pre-position Phase 2 multi-mod/load-order work"), `FileEntry { target_rel, source_mod, method, hash, pre_existing }`, and `DeployMethod`. Phase 2 extends these (add a priority/rank field; add Profile, Plugin types) — shapes are a contract, additive changes preferred.
- **`crates/store/`** is the rusqlite (bundled) + refinery DB layer: `db.rs` (open/migrate), `manifest.rs` (`list_deployed_files`, `FileEntry` rows), `registry.rs` (`list_managed_games`, `Game`), `journal.rs` (`OpIntent`, WAL pending-ops), `vanilla.rs` (backup ledger). Phase 2 adds a **V2 migration** + new query modules (mods, profiles, plugins).
- **`crates/deploy/src/engine.rs`** (`deploy(store, game, staged: &StagedFiles) -> DeployReport`) is the single deploy entry point; `method/` chooses reflink→hardlink→symlink→copy; `backup.rs` does vanilla backup; `verify.rs` does hash-diff verify/repair; `journal.rs` is crash recovery. Phase-2 conflict resolution must produce the winning `StagedFiles` set that this engine deploys — the engine itself stays the safe primitive.
- **`crates/steam/src/{resolve.rs,casing.rs}`** resolves install dir + Proton prefix and canonical `Data/` casing — reuse `resolve` to locate the prefix `AppData/.../Plugins.txt` path.
- **`crates/testkit/`** provides the round-trip-pristine harness (DIR_SENTINEL directory-shape snapshots) — Phase 2's profile-switch and conflict-redeploy tests should assert pristine via this harness.

### Established Patterns
- **Multi-crate workspace, headless safety core, zero Tauri deps in `crates/`**; Tauri commands are thin 3–10 line adapters (`src-tauri/src/commands/{mods,deploy,games}.rs`). Phase 2 adds `commands/{conflicts|plugins|profiles}.rs` style thin adapters delegating to new headless logic.
- **DB: rusqlite bundled + refinery versioned migrations** (`crates/store/src/migrations/V1__init.sql`). `deployed_file` enforces `UNIQUE(appid, target_rel)` — **one owner per deployed path**, which is exactly the invariant conflict-winner resolution must satisfy before calling deploy.
- **Frontend is SvelteKit** (`frontend/`, adapter-static → `frontend/build`; Tauri `frontendDist` points there; `beforeDevCommand` runs `npm --prefix ../frontend run dev`). Phase 1 UI is "functional-minimal Svelte 5". Phase 2 adds conflict / load-order / profile views (UI hint: yes → a UI-SPEC is generated before planning).
- **thiserror in libs / anyhow at app boundary; tracing for logs.** New crate errors follow the per-crate `error.rs` pattern.

### Integration Points
- New persistence: `crates/store/src/migrations/V2__*.sql` + query modules; new core types in `model.rs`.
- Conflict resolution is a **new headless module** (likely in `deploy` or a new `crates/conflict`/`crates/loadorder`) that consumes enabled mods' staged trees + priority and emits the winning file set for `engine::deploy`.
- Plugin management + LOOT live in a new headless module (plugin scan, `plugins.txt` writer to the prefix, libloot binding) — keep Tauri-free for headless testing.
- Tauri shell: new thin command modules + Svelte views; profile switch wires through the existing deploy/purge engine.

</code_context>

<specifics>
## Specific Ideas

- **The round-trip-pristine guarantee is the hard invariant that must survive every new operation**: changing priority + redeploying, switching profiles (purge old → deploy new), and enabling/disabling plugins must all leave the game folder restorable byte-for-byte to vanilla. Reuse the Phase-1 testkit pristine harness for profile-switch and conflict-redeploy regression tests.
- **`deployed_file UNIQUE(appid, target_rel)`** is the structural reason conflict resolution must pick a single winner per path *before* deploy — the schema will reject two owners. Design conflict resolution to emit a deduplicated winning `StagedFiles` set.
- **LOOT/libloot is the largest technical unknown** — plan-phase should run dedicated research on an AppImage-compatible, statically-linkable LOOT integration path for Linux (FFI binding availability, masterlist fetch, license compatibility) before committing the approach.
- **plugins.txt correctness is in-game-observable** — the asterisk-enabled format and masters-first ordering must match what the Creation Engine expects under Proton, or plugins silently don't load. This is a key manual-UAT item (analogous to Phase-1 UAT-3 in-game load).
- Bethesda specifics to honor: Skyrim SE / Fallout 4 use `*Plugin.esp` enabled markers in `Plugins.txt`; ESL-flagged plugins and `.esm` masters sort ahead of `.esp`; the file lives under `AppData/Local/<GameName>/` inside the Proton prefix, not the game install dir.

</specifics>

<deferred>
## Deferred Ideas

- MO2-style per-profile **save-game and INI redirection** — revisit in a later milestone if users need fully isolated profiles; not required for v1 conflict/order/profile parity.
- Full LOOT **plugin-cleaning workflow** (running xEdit-style cleaning on dirty plugins) — v1 only surfaces the warnings.
- **User-configurable `plugins.txt` / load-order path overrides** — v1 derives per-game from the prefix.
- Vortex **rule-based conflict resolution** ("mod X always loads after mod Y") as an alternative to the ordered priority list — possible future enhancement.
- Per-profile **staged copies** (vs shared store) — rejected for v1; revisit only if a use case requires fully independent mod binaries per profile.

</deferred>
