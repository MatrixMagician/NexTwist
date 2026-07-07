# Phase 6: Starfield Detection & CE2 Path Resolution - Research

**Researched:** 2026-07-07
**Domain:** Adding Starfield (Steam AppID 1716740, Creation Engine 2) as a supported Bethesda game to NexTwist's existing headless Rust engine — detection + path resolution only (no writes).
**Confidence:** HIGH on stack, allow-list surface, and path mechanism (verified against libloadorder source + community). MEDIUM only on the exact behaviour of Wine Documents *redirection* in the wild (the default path is HIGH; the redirect edge case is defensive).

## Summary

Starfield detection is a **data-and-mapping extension**, exactly as SUMMARY.md predicted: the two hard external libraries already ship Starfield support at the versions v1.0 pins (`libloot 0.29.5 → GameType::Starfield`, `esplugin 6.1.4 → GameId::Starfield`, both verified via docs.rs this session), so there is zero MSRV move, zero dependency bump, no `core` change, and no DB migration. Adding Starfield is ~6 `match appid` allow-list arms plus one bundled masterlist snapshot plus **one genuinely new resolver**.

The one material correction this research makes to SUMMARY.md: **Starfield uses TWO different config directories, not one.** `Plugins.txt` (load order, Phase 7) lives in `AppData/Local/Starfield/` — the *same* subtree Skyrim SE / FO4 use — so it needs only a new `appdata_folder_name` arm and the existing `appdata_local_path` seam. `StarfieldCustom.ini` (loose-file activation, Phase 8) and the first-launch marker dir live in `Documents/My Games/Starfield/` — a subtree the v1.0 seam does not have, so *that* needs the new `my_games_path` resolver. SUMMARY.md and CONTEXT.md both stated "both files live in My Games", and CONTEXT framed the whole change as "a NEW resolver, not a new `appdata_folder_name` arm." The evidence says it is **both**: a new folder-name arm (for plugins.txt) AND a new My-Games resolver (for the INI + first-launch detection).

**Primary recommendation:** Register AppID 1716740 across the ~6 existing allow-list arms (folder name = `"Starfield"`, slug = `"starfield"`, exe = `"Starfield.exe"`, `GameType::Starfield`, `GameId::Starfield`); add a new `my_games_path(prefix) -> PathBuf` resolver in `steam` that mirrors libloadorder's own derivation (`<prefix>/drive_c/users/steamuser/Documents/My Games/Starfield`), redirection-aware via `user.reg` with the default path as the load-bearing fallback, case-folded per-component; return a typed first-launch state (not an error) when that dir is absent/empty; and read the installed build from `appmanifest_1716740.acf` `buildid` to compare against a bundled validated-build constant in `assets/starfield/`.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
**Version-Drift Warning (SFDET-03)**
- Store the "last-validated build" baseline as a constant bundled alongside `assets/starfield/`, updatable when Phase 9 validates on real hardware (treat as validated-against-a-build, not a permanent constant).
- Warning is **advisory / non-blocking** — the safety and byte-for-byte reversibility guarantee holds regardless of build; drift only affects the MEDIUM-confidence CE2 loose-file / load-order recipe, so it must never block management.
- Trigger when the installed Starfield build is **newer than the validated build** (any newer build) — honest, avoids false confidence; no "known-breaking" list is available.
- Surface as a **persistent notice on the Starfield game view** with an "installed vs validated build" line — visible but non-intrusive.

**First-Launch-Not-Done State (SFDET-02)**
- Detect via the **`My Games/Starfield` config dir being absent/empty** (the game creates it on first launch).
- **Block** load-order/INI management for Starfield until resolved, but still allow the user to add/detect the game — SFDET-02 explicitly forbids "silently writing to a useless path".
- Guide with an **actionable message**: "Launch Starfield once via Steam so it creates its config folder, then re-check." Do not attempt to auto-launch the Proton game.
- Provide a **manual "Re-check" action** that re-runs detection — explicit and predictable, no surprise writes on app focus.

**Detection Scope & Masterlist (SFDET-01/02)**
- **Bundle** `assets/starfield/masterlist.yaml` at build time (offline-first, matches the v1.0 pattern); surface its age/currency.
- Resolve the Documents path **Wine-redirection-aware** (read the prefix's user shell-folder mapping rather than hard-coding `Documents/My Games`), reusing the existing `casing.rs` case-folding seam — avoids Pitfall 6 (wrong/absent/mis-cased prefix path).
- **Reuse the existing Bethesda add-game flow verbatim** — Starfield is just a new allow-listed AppID (1716740); SFDET-01 requires it behave "exactly like Skyrim SE / Fallout 4 today".
- Config folder / slug = **`Starfield` / `starfield`**; the researcher confirms the exact CE2 folder name during plan-phase research.

### Claude's Discretion
- Exact module placement of the `my_games_path` resolver, the version-drift constant, and the first-launch state enum — follow existing `steam`/`loadorder` conventions.
- Precise wording of user-facing messages.

### Deferred Ideas (OUT OF SCOPE)
- Exact CE2 loose-file INI keys, masterlist currency, and `.ba2` v3 opacity remain MEDIUM-confidence — deliberately deferred to Phase 9's on-hardware validation. Phase 6 only owns the version-drift *signal*, not the validated recipe.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| SFDET-01 | User can add Starfield (AppID 1716740) as a managed Bethesda game, auto-detected under Steam/Proton with install dir + Proton prefix resolved | The v1.0 detection path (`steam::resolve_game` / `add_game_by_folder` / `detect_games`) is entirely appid-generic behind the `SUPPORTED_APPIDS` allow-list; adding `1716740` + `default_name`/`expected_exe` arms makes the whole flow work verbatim (§Allow-List Surface). Prefix derivation (`proton_prefix`) already appid-parameterised. |
| SFDET-02 | Resolve CE2 config location (`Documents/My Games/Starfield` inside the Proton prefix), handling Wine case-folding and the not-yet-created (pre-first-launch) folder case | New `my_games_path` resolver mirrors libloadorder's authoritative derivation (§Q1/Q2); case-fold via the `entry_ci` primitive; typed first-launch state returned instead of an error (§Q4). Mandatory case-mismatched fixture test (§Validation Architecture). |
| SFDET-03 | Detect installed Starfield build and warn on version drift (game update may have changed loose-file/load-order behaviour) | Read `appmanifest_1716740.acf` `buildid` via the existing `keyvalues-serde` `AppManifest` deserialize seam; compare against a bundled validated-build constant in `assets/starfield/`; advisory non-blocking warning when installed > validated (§Q3). |
</phase_requirements>

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| AppID → GameType/GameId/slug/folder/exe allow-list | `crates/steam` + `crates/loadorder` | — | Pure `match appid` mapping functions already live here; Starfield is one arm each. Headless, no Tauri. |
| Proton prefix resolution | `crates/steam` (`resolve.rs`) | — | Already shipped + appid-generic; Starfield inherits it unchanged. |
| CE2 `My Games/Starfield` path resolution + case-fold + first-launch state | `crates/steam` (new resolver, sibling of `casing.rs`/`resolve.rs`) | — | Prefix/Wine/case-fold knowledge is `steam`'s responsibility (per v1.0 Responsibility Map); the resolver is pure path logic over the prefix. |
| `Plugins.txt` location (AppData/Local/Starfield) | `crates/loadorder` (`appdata_local_path` + new folder-name arm) | libloot | **Phase 7.** Data-driven via the existing seam once the arm exists. |
| Installed build read + drift comparison | `crates/steam` (ACF read) + a small compare helper | — | ACF parsing already lives in `resolve.rs` (`AppManifest`); add a `buildid` field. Pure logic. |
| Surfacing first-launch state + drift notice to UI | `src-tauri` command adapter (thin) + frontend | — | Detection/logic stays headless; Tauri only forwards the typed state + build numbers. |
| Bundling `masterlist.yaml` | `crates/loadorder/assets/starfield/` (`include_str!`) | — | **Phase 7 uses it**; Phase 6 only adds the asset + slug/snapshot arms. |

## Standard Stack

### Core (delta only — everything else is the v1.0 stack, unchanged)
| Library | Version | Purpose | Why Standard / Status |
|---------|---------|---------|-----------------------|
| libloot | 0.29 (pinned; resolves 0.29.5) | `GameType::Starfield` — CE2 load-order semantics (Phase 7 consumes; Phase 6 only wires the arm) | **[VERIFIED: docs.rs]** `Starfield` variant present in `enum.GameType` at 0.29.5. No bump. |
| esplugin | 6.1 (pinned; resolves 6.1.4) | `GameId::Starfield` — CE2 header/medium-master parsing | **[VERIFIED: docs.rs]** `Starfield` variant present in `enum.GameId` at 6.1.4. Already a direct + transitive dep. No bump. |
| keyvalues-serde | (existing v1.0 dep) | Deserialize `buildid` from `appmanifest_1716740.acf` for SFDET-03 | **[VERIFIED: codebase]** `resolve.rs` already deserializes `AppManifest` from the ACF; add one field. No new dep. |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| walkdir | 2.x (existing) | Not required for Phase 6 core, but available if the case-fold resolver prefers it over `read_dir` | Reuse the `entry_ci`-style per-component match instead — see Don't Hand-Roll. |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Reading `buildid` from `appmanifest_*.acf` | Parsing `Starfield.exe` PE version resource | ACF `buildid` is already parsed by the existing seam, is what Steam actually tracks/updates, and needs no PE parser. EXE version is stable across many hotfixes so it would *miss* drift. **Recommend ACF `buildid`.** |
| A new `my_games_path` resolver in `steam` | Letting libloot derive My Games from the supplied `local_path` internally | libloot *does* derive it internally for its own use (`.ccc`, additional Data dirs), but Phase 6 needs the path *directly* (for first-launch detection now, and Phase 8's INI). We must compute it ourselves; we cannot read it out of libloot's private `GameSettings`. **Recommend explicit resolver.** |
| `rust-ini` | — | **Not needed in Phase 6** (no INI writes). It is a Phase 8 dependency. Do not add it here. |

**Installation:** No new crates for Phase 6. (`Cargo.toml` `[workspace.dependencies]` unchanged; `rust-ini` is deferred to Phase 8.)

**Version verification (this session):**
- `docs.rs/libloot/0.29.5/libloot/enum.GameType.html` → variants include `Starfield` (also `SkyrimSE`, `Fallout4`, `OpenMW`). **[VERIFIED: docs.rs]**
- `docs.rs/esplugin/6.1.4/esplugin/enum.GameId.html` → variants include `Starfield`. **[VERIFIED: docs.rs]**
- `api.github.com/repos/loot/starfield/branches/v0.29` → branch `v0.29` exists, last commit `2026-06-20`. **[VERIFIED: GitHub API]** (This is the same `MASTERLIST_BRANCH = "v0.29"` constant the SSE/FO4 masterlist fetch already uses — so the masterlist URL builder works for Starfield with only a slug arm.)

## Package Legitimacy Audit

No external packages are added in Phase 6. All libraries used (`libloot`, `esplugin`, `keyvalues-serde`, `walkdir`) are already pinned v1.0 dependencies that passed the v1.0 legitimacy checkpoint (`libloot`/`esplugin` verified against `github.com/loot/*`, allowed by `deny.toml`'s libloot-family allowance). **Package Legitimacy Gate: N/A — no installs.** (`rust-ini`'s audit belongs to Phase 8.)

## Architecture Patterns

### System Architecture Diagram (Phase 6 data flow)

```
                    User: "Add Starfield"  ┐
                                           ▼
  detect_games / add_game(1716740) ──► steam::resolve_game(1716740)
       (Tauri adapter, thin)                  │  allow-list: is_supported? default_name? expected_exe?
                                              ▼
                                     ResolvedGame { install_dir, prefix, prefix_exists }
                                              │
                          ┌───────────────────┼───────────────────────────┐
                          ▼                   ▼                           ▼
             appmanifest_1716740.acf    steam::my_games_path(prefix)   (Phase 7, later)
               `buildid` read                │                       loadorder::appdata_local_path
                    │                         │  1. user.reg "Personal" redirect (defensive)
                    ▼                         │  2. default: drive_c/users/steamuser/Documents
        drift compare vs bundled             │  3. join "My Games"/"Starfield"
        VALIDATED_BUILD (assets/starfield)   │  4. case-fold each existing component (entry_ci)
                    │                         ▼
                    │              Ce2ConfigState:
                    │                 Ready(real_cased_path)   ← dir exists & non-empty
                    │                 FirstLaunchPending(expected_path) ← absent/empty
                    ▼                         │
        DriftNotice { installed, validated, is_newer }   │
                          └───────────────────┼───────────► Tauri adapter forwards typed
                                              ▼             state + build numbers to UI
                                   Starfield game view:
                                   - persistent drift notice (if newer)
                                   - "launch once" guidance + Re-check (if FirstLaunchPending)
                                   - load-order/INI actions BLOCKED until Ready
```

File-to-implementation mapping is in the tables below, not the diagram.

### Recommended module placement (Claude's discretion per CONTEXT — this is the recommendation)
```
crates/steam/src/
├── resolve.rs      # + STARFIELD const, SUPPORTED_APPIDS, default_name, expected_exe arms
│                   # + AppManifest.buildid field, installed_build(library_root, appid) helper
├── casing.rs       # unchanged (Data/-specific); the general per-component case-fold lives in resolve.rs (entry_ci)
└── ce2.rs (NEW)    # my_games_path(prefix) -> PathBuf (redirection-aware, case-folded)
                    # Ce2ConfigState enum + resolve_ce2_config(prefix) -> Ce2ConfigState
                    # DriftNotice + drift compare helper (or keep drift in resolve.rs)

crates/loadorder/src/
├── loot.rs         # + game_type_for arm (Starfield), appdata_folder_name arm ("Starfield")  [Phase 7 uses]
├── scan.rs         # + game_id_for arm (GameId::Starfield)                                    [Phase 7 uses]
├── masterlist.rs   # + game_slug arm ("starfield"), bundled_snapshot arm, STARFIELD_SNAPSHOT include_str!
└── assets/starfield/masterlist.yaml (NEW, bundled CC0 snapshot)  [+ VALIDATED_BUILD constant sibling]
```
> Note: `entry_ci` in `resolve.rs` is currently private. Lift it to `pub(crate)` (or a tiny shared `casefold_component` helper) so `ce2.rs` reuses the *exact* deterministic case-fold logic (exact-case-wins, else lexicographically-smallest variant — WR-07) rather than re-implementing it. `casing.rs` itself is a `Data/`-tree walker and is **not** the reusable primitive despite CONTEXT's phrasing.

### Pattern 1: Mirror libloadorder's authoritative My-Games derivation
**What:** libloadorder (the crate libloot uses) derives the My Games path on non-Windows as `local_path.parent().parent().parent().join("Documents").join("My Games").join(folder)`, where `local_path` ends in `AppData/Local/<folder>` and `folder == "Starfield"`.
**When to use:** The default (non-redirected) case — which is what Proton produces essentially always.
**Evidence:**
```
// Source: github.com/Ortham/libloadorder/src/game_settings.rs (master)
// line 391-403  appdata_folder_name:  GameId::Starfield => Some("Starfield")
// line 470-473  my_games_path:  documents_path(local_path).map(|d| d.join("My Games").join(folder))
// line 480-483  my_games_folder_name:  _ => appdata_folder_name(...)  // Starfield folder = "Starfield"
// line 524-537  documents_path (non-Windows):
//   local_path.parent().and_then(parent).and_then(parent).map(|p| p.join("Documents"))
```
For NexTwist, computed **directly from the prefix** (we already know it, no need to go via `local_path`):
`<prefix>/drive_c/users/steamuser/Documents/My Games/Starfield`.

### Pattern 2: Typed state, not an error, for first-launch-pending
**What:** Return `Ce2ConfigState::{ Ready(PathBuf), FirstLaunchPending(PathBuf) }` from the resolver.
**Why:** SFDET-02 requires the game still be *addable* while blocking load-order/INI ops; an `Err` would abort the add flow. `Ready` carries the case-folded real path; `FirstLaunchPending` carries the canonical *expected* path (nothing on disk to case-fold yet) so the UI can name it in guidance.

### Anti-Patterns to Avoid
- **Using a Linux-side `~/Documents` path.** The CE2 config is *inside the prefix* (`compatdata/1716740/pfx/...`), never the host home dir (Pitfall 6). [VERIFIED: libloadorder derives it from the prefix-relative `local_path`.]
- **Treating `Plugins.txt` and `StarfieldCustom.ini` as co-located.** They are in different directories (see State of the Art). Deriving one from the other's parent will put a file in the wrong place.
- **Hard-coding the Starfield master/medium-master list.** `STARFIELD_HARDCODED_PLUGINS` grows per patch (12 entries as of this read, incl. `SFBGS0xx.esm` + `ShatteredSpace.esm`); delegate to libloot in Phase 7. Not a Phase 6 concern, but do not bake it in here.
- **Reading the build from `Starfield.exe` version.** Misses hotfixes that change loose-file behaviour without bumping the PE version.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Case-insensitive path-component match on a case-sensitive FS | A new lowercasing walker | The existing `entry_ci` in `resolve.rs` (make it `pub(crate)`) | Already solves WR-07 deterministically (exact-case wins, else smallest variant); a fresh impl risks nondeterminism the v1.0 code specifically fixed. |
| Documents/My Games derivation | A bespoke path guess | Mirror libloadorder's documented derivation (Pattern 1) | It is the *same* code path libloot will use internally for `.ccc`/additional Data dirs; matching it guarantees Phase 7/8 agree with libloot. |
| Steam build-id read | A VDF/ACF hand-parser | The existing `AppManifest` `keyvalues-serde` deserialize in `resolve.rs` (+ a `buildid` field) | The ACF read seam already exists and is tested; adding a field is a one-line change. |
| Masterlist URL / fetch / offline fallback | Anything new | The existing `masterlist.rs` pipeline (`masterlist_url` + `ensure_masterlist` + bundled snapshot) | Fully appid-generic behind `game_slug`/`bundled_snapshot`; Starfield is two arms + one asset. |

**Key insight:** Every path-, casing-, and manifest-handling problem Phase 6 faces was already solved for Skyrim SE / FO4. The *only* net-new logic is (a) the My-Games tail (`Documents/My Games/Starfield` instead of `AppData/Local/Starfield`), (b) the redirection read, and (c) the drift comparison. Everything else is a `match appid` arm.

## Runtime State Inventory

> Phase 6 is additive/greenfield within an existing app (no rename/refactor/migration), but it *reads* runtime state under the Proton prefix, so the relevant categories are inventoried here.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | `nextwist.db` `managed_games` gains a Starfield row via the existing `add_managed_game` — appid-generic, **no migration** (store is appid-keyed; verified in SUMMARY + v1.0 schema V1–V5). | Code: none beyond the add-game flow. No migration. |
| Live service config | Proton prefix `compatdata/1716740/pfx` — created by Steam on first *launch*, not install. `Documents/My Games/Starfield` created by the game on first *in-game* load. Neither is in git nor NexTwist-owned. | Read-only in Phase 6; detect absence → `FirstLaunchPending`. No writes. |
| OS-registered state | None. Phase 6 registers nothing (no `nxm://`, no Task Scheduler). | None — verified: Phase 6 touches only detection/read paths. |
| Secrets/env vars | Honors existing `$STEAM_COMPAT_DATA_PATH` override (already handled by `proton_prefix`). No new secrets. | None. |
| Build artifacts | New bundled asset `crates/loadorder/assets/starfield/masterlist.yaml` compiled via `include_str!`; a `VALIDATED_BUILD` constant. Both are source-tree artifacts. | Ship the asset; wire the `include_str!` + slug/snapshot arms. |

**Nothing found in category (OS-registered state):** None — verified Phase 6 performs no OS registration.

## Common Pitfalls

### Pitfall 1: Assuming Plugins.txt and StarfieldCustom.ini share a directory
**What goes wrong:** Deriving both paths from one root writes `Plugins.txt` into `My Games` (where the game won't read it) or the INI into `AppData/Local` (ditto).
**Why it happens:** SUMMARY.md/CONTEXT.md both state "both files live in `My Games/Starfield`." That is incorrect for `Plugins.txt`.
**How to avoid:** `Plugins.txt` → `AppData/Local/Starfield` (existing `appdata_local_path` + new folder-name arm, Phase 7). `StarfieldCustom.ini` + first-launch marker → `Documents/My Games/Starfield` (new `my_games_path`, Phase 6/8). Two seams, two directories.
**Warning signs:** A Phase 7 plugins.txt round-trip that "succeeds" but the game ignores the load order.

### Pitfall 2: Case-sensitive prefix mismatch (SFDET-02 mandate)
**What goes wrong:** `Documents/My Games/Starfield` computed with canonical casing fails `is_dir()` on a prefix that created `documents/my games/starfield` (or any variant), so a *created* config dir reads as `FirstLaunchPending`.
**Why it happens:** Wine/Proton does not case-fold; the on-disk casing depends on who created the tree.
**How to avoid:** Resolve each existing component case-insensitively via `entry_ci`. The **case-mismatched fixture test is mandatory** (SFDET-02 success criterion 2).
**Warning signs:** Detection flips between Ready/Pending depending on the machine.

### Pitfall 3: Wine Documents redirection ignored
**What goes wrong:** A non-default prefix redirects the "Personal" (Documents) shell folder elsewhere; the hard-coded `.../steamuser/Documents` misses it.
**Why it happens:** Wine stores the mapping in `<prefix>/user.reg` under `[Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\User Shell Folders]` value `"Personal"`; the default is `C:\users\steamuser\Documents` but it can be repointed (rare under Proton).
**How to avoid:** Read `user.reg` for `"Personal"` first; map the returned Windows path (`C:\users\steamuser\Documents` → `<prefix>/drive_c/users/steamuser/Documents`) back into the prefix; **fall back to the default** on any parse failure/absence. libloadorder itself does *not* read `user.reg` (it uses the default derivation), so treat the redirect read as defensive belt-and-suspenders honoring the CONTEXT lock — the default path is the load-bearing case. See Landmines.
**Warning signs:** Path resolves correctly for you but a user with a customized prefix reports "first launch not done" forever.

### Pitfall 4: Drift warning that blocks
**What goes wrong:** Treating "installed build newer than validated" as an error and refusing to manage the game.
**Why it happens:** Over-reading the safety mandate.
**How to avoid:** CONTEXT locks it **advisory / non-blocking**. Reversibility is build-independent (Phase 6 writes nothing). Warn only; never gate management on it.

## Code Examples

Recommended signatures (design, not implementation — the planner turns these into PLAN.md):

```rust
// crates/steam/src/resolve.rs  (allow-list additions)
pub const STARFIELD: u32 = 1716740;
pub const SUPPORTED_APPIDS: &[u32] = &[SKYRIM_SE, FALLOUT4, STARFIELD];
// default_name:  STARFIELD => "Starfield"
// expected_exe:  STARFIELD => "Starfield.exe"
// AppManifest { installdir, name, buildid: Option<String> }   // + one field
pub fn installed_build(library_root: &Path, appid: u32) -> Option<u64>;  // reads acf buildid

// crates/steam/src/ce2.rs  (NEW — Phase 6 core)
pub enum Ce2ConfigState { Ready(PathBuf), FirstLaunchPending(PathBuf) }
/// <prefix>/drive_c/users/steamuser/Documents/My Games/Starfield, redirection-aware + case-folded.
pub fn my_games_path(prefix: &Path) -> PathBuf;
/// absent/empty dir => FirstLaunchPending(expected); else Ready(real_cased).
pub fn resolve_ce2_config(prefix: &Path) -> Ce2ConfigState;
pub struct DriftNotice { pub installed: u64, pub validated: u64, pub is_newer: bool }
pub fn drift_notice(installed: Option<u64>, validated: u64) -> Option<DriftNotice>;

// crates/loadorder/src/loot.rs        game_type_for:  STARFIELD => Some(GameType::Starfield)
//                                     appdata_folder_name: STARFIELD => Some("Starfield")   [Phase 7]
// crates/loadorder/src/scan.rs        game_id_for:    STARFIELD => Some(GameId::Starfield)   [Phase 7]
// crates/loadorder/src/masterlist.rs  game_slug:      STARFIELD => Some("starfield")
//                                     bundled_snapshot: STARFIELD => Some(STARFIELD_SNAPSHOT)
```

## State of the Art

| Old Approach (SUMMARY.md / CONTEXT.md assumption) | Current Approach (verified this session) | Source | Impact |
|--------------|------------------|--------|--------|
| "Both `plugins.txt` and `StarfieldCustom.ini` live in `My Games/Starfield`" | `Plugins.txt` → `AppData/Local/Starfield/`; `StarfieldCustom.ini` → `Documents/My Games/Starfield/` | **[VERIFIED: libloadorder `game_settings.rs`]** `plugins_file_path` for Starfield = `local_path.join("Plugins.txt")` (AppData/Local); `my_games_path` used only for `.ccc`/additional Data dirs/INI. **[CITED]** community: Steam/Nexus confirm the split. | Phase 6 delivers **both** a folder-name arm (plugins.txt) and the My-Games resolver (INI/first-launch). Corrects CONTEXT's "new resolver, not a new folder-name arm." |
| Folder name "confirm during research" | Confirmed exactly `"Starfield"` for both AppData/Local and My Games | **[VERIFIED: libloadorder]** `appdata_folder_name(Starfield) => "Starfield"`, `my_games_folder_name` falls through to same. | Lock `"Starfield"` / slug `"starfield"`. |

**Deprecated/outdated:**
- The SUMMARY's "Gaps to Address → CE2 plugins.txt location: STACK/FEATURES/PITFALLS agree it is `My Games` (not AppData/Local)" is resolved in the **opposite** direction: the ARCHITECTURE doc's `AppData/Local/Starfield` mention for plugins.txt was correct.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Wine Documents *redirection* via `user.reg` "Personal" is worth reading; the default `.../steamuser/Documents` covers ~all Proton prefixes. | Q2 / Pitfall 3 | Low — if a redirect exists and we skip reading it, first-launch detection false-negatives for that user; the default path is correct for the overwhelming majority (libloot itself only uses the default). Mitigated by keeping the default as fallback. |
| A2 | `appmanifest_1716740.acf` exposes a monotonically-increasing `buildid` suitable for "newer than validated" comparison. | Q3 | Low-Med — `buildid` is Steam's depot build id (numeric, increases per update); if Steam ever changed the field name the read returns `None` and drift simply isn't surfaced (fail-safe, non-blocking). Validate the field name on the owner's live ACF in Phase 9. |
| A3 | The game creates `Documents/My Games/Starfield` (and its INI/Plugins.txt) on **first launch**, absent before. | Q4 | Low — consistent across all Bethesda CE/CE2 titles and community reports; the first-launch state is advisory guidance, and a wrong guess only shows/hides a "launch once" hint. |
| A4 | Bundling a `loot/starfield@v0.29` `masterlist.yaml` snapshot is legally safe (CC0-1.0, same as SSE/FO4). | Masterlist | Low — the SSE/FO4 snapshots are already bundled under this assumption; confirm the LICENSE in `loot/starfield` is CC0 when fetching the snapshot (it is, historically, for all `loot/*` masterlists). |

## Open Questions

1. **Does any real Proton Starfield prefix redirect Documents away from `steamuser/Documents`?**
   - What we know: libloadorder assumes it does not (uses the default derivation). Proton defaults keep it in-prefix.
   - What's unclear: Whether the owner's prefix (or Steam Deck / custom setups) ever repoints "Personal".
   - Recommendation: Implement the `user.reg` read as defensive with default fallback; **confirm on the owner's live prefix in Phase 9**.

2. **Exact `buildid` value of the "last-validated build" constant.**
   - What we know: It must be a real build the owner validates in Phase 9; Phase 6 ships a placeholder + the comparison logic.
   - Recommendation: Seed `VALIDATED_BUILD` with the current installed build at Phase 9 time; Phase 6 can seed it with the build present on the owner's machine now (or `0` to suppress the notice until Phase 9 sets it). Document it as "update in Phase 9."

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| libloot | Phase 7 (Phase 6 wires arm only) | ✓ (pinned) | 0.29.5 | — |
| esplugin | Phase 7 (Phase 6 wires arm only) | ✓ (pinned) | 6.1.4 | — |
| `loot/starfield` masterlist `v0.29` | Bundled asset | ✓ (reachable) | commit 2026-06-20 | Bundled snapshot is the offline fallback by design |
| A real Starfield Proton install (`compatdata/1716740`) | On-hardware verification of the path mechanism | ✗ (not on this CI/dev host) | — | Fixture-based tests via `testkit::fake_proton_prefix` (headless); real-prefix check deferred to **Phase 9** |

**Missing dependencies with no fallback:** None that block Phase 6 — all detection/resolution logic is unit/fixture-testable headlessly.
**Missing dependencies with fallback:** Live Starfield prefix → simulated by `testkit` fixtures; real-hardware confirmation is Phase 9's job (SFVER-01).

## Validation Architecture

> `workflow.nyquist_validation` is not disabled → this section is REQUIRED. Framework = the workspace's built-in `cargo test` (no external test framework); fixtures via `nextwist-testkit`.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness (`#[test]`) + `tempfile` fixtures + `nextwist-testkit` builders |
| Config file | none (cargo workspace); `crates/*` headless — no WebKitGTK needed |
| Quick run command | `cargo test -p nextwist-steam -p nextwist-loadorder` |
| Full suite command | `cargo test --workspace --locked` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| SFDET-01 | AppID 1716740 passes the allow-list (`is_supported`, `default_name`, `expected_exe`, `game_type_for`, `game_id_for`, `game_slug`) and only supported games pass | unit | `cargo test -p nextwist-steam is_supported` / `-p nextwist-loadorder allow_lists` | ❌ Wave 0 (extend the existing `game_type_for_allow_lists_only_the_two_supported_games` to assert three games + Starfield arms) |
| SFDET-01 | `resolve_from_root` builds install_dir + prefix for 1716740 from a synthetic library | fixture | `cargo test -p nextwist-steam resolve_from_root` (parametrize the existing synthetic-library test with Starfield) | ❌ Wave 0 |
| SFDET-02 | `my_games_path` resolves `<prefix>/drive_c/users/steamuser/Documents/My Games/Starfield` against a real-shaped prefix fixture | fixture (unit) | `cargo test -p nextwist-steam my_games_path` | ❌ Wave 0 (needs a `fake_proton_prefix` My-Games variant in testkit) |
| SFDET-02 | **Case-mismatched fixture**: prefix created as `documents/my games/starfield` resolves via case-fold to the real cased dir (MANDATORY per success criterion 2) | fixture (unit) | `cargo test -p nextwist-steam my_games_case_mismatch` | ❌ Wave 0 |
| SFDET-02 | Documents redirection: a `user.reg` with a non-default "Personal" is honored; malformed/absent `user.reg` falls back to default | unit (fixture) | `cargo test -p nextwist-steam documents_redirect` | ❌ Wave 0 |
| SFDET-02 | First-launch-not-done: absent dir → `FirstLaunchPending`; empty dir → `FirstLaunchPending`; dir with a file → `Ready(cased)` | fixture (unit) | `cargo test -p nextwist-steam first_launch` | ❌ Wave 0 |
| SFDET-03 | Drift compare: installed > validated → `Some{is_newer:true}`; equal/older → `None`; unreadable buildid (`None`) → `None` (fail-safe) | unit | `cargo test -p nextwist-steam drift` | ❌ Wave 0 |
| SFDET-03 | `installed_build` reads `buildid` from a synthetic `appmanifest_1716740.acf` | fixture | `cargo test -p nextwist-steam installed_build` | ❌ Wave 0 |
| SFDET-01 | Bundled `assets/starfield/masterlist.yaml` present + offline fallback seeds the cache (mirror `falls_back_to_bundled_snapshot_when_offline`) | fixture | `cargo test -p nextwist-loadorder starfield_snapshot` | ❌ Wave 0 |

**Test-type notes:** all are unit or tempdir/testkit **fixture** tests — headless, no property tests strictly required. The case-fold could be framed as a property ("any case permutation of the components resolves to the on-disk dir"), but 2–3 representative fixtures (all-lower, mixed, exact) satisfy the SFDET-02 mandate more legibly. Drift compare is a small exhaustive-case unit test.

### Sampling Rate
- **Per task commit:** `cargo test -p nextwist-steam -p nextwist-loadorder`
- **Per wave merge:** `cargo test --workspace --locked` + `cargo clippy --workspace --all-targets -- -D warnings`
- **Phase gate:** full suite green before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `crates/testkit/src/lib.rs` — add a `fake_proton_prefix` My-Games variant (build `drive_c/users/steamuser/Documents/My Games/<folder>`, with an optional case-variant + optional `user.reg`) — covers SFDET-02 fixtures. This mirrors the existing AppData/Local builder.
- [ ] `crates/steam/src/ce2.rs` test module — `my_games_path`, case-mismatch, redirect, first-launch, drift.
- [ ] `crates/steam/src/resolve.rs` — extend allow-list tests to three games; add `installed_build`/`buildid` test.
- [ ] `crates/loadorder/{loot,scan,masterlist}.rs` — extend the three existing allow-list tests to assert the Starfield arm; add the bundled-snapshot test.
- [ ] `crates/loadorder/assets/starfield/masterlist.yaml` — bundle the CC0 snapshot (`include_str!` target must exist or the crate won't compile).

*(No framework install needed — the Rust harness + `tempfile` + `testkit` already cover everything.)*

## Security Domain

> `security_enforcement` not disabled → included. Phase 6 is read-only (no writes to game/prefix), so the surface is narrow.

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V5 Input Validation | yes | The Proton prefix path and `user.reg`/ACF contents are **untrusted input** (attacker-influenced files under `compatdata`). Treat plugin/dir names as opaque; never join un-validated components outside the prefix root. Reuse the v1.0 marker-validation posture from `add_game_by_folder` (T-01-04). |
| V12 File Handling | yes | `user.reg` and `appmanifest_*.acf` are parsed read-only; a malformed file must fail safe (fall back to default path / `None` build), never panic or path-escape. Do not follow symlinks out of the prefix when case-folding (`follow_links(false)`, as `casing.rs`/`scan.rs` already do). |
| V6 Cryptography | no | No secrets/crypto in Phase 6. |
| V2/V3/V4 (auth/session/access) | no | No auth surface (detection only). |

### Known Threat Patterns for {Rust / Proton FS}
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Path traversal via a crafted `user.reg` "Personal" value (`..\..\..`) | Tampering / EoP | After mapping the Windows path into the prefix, canonicalize/verify the result stays under `<prefix>/drive_c`; reject and fall back to default otherwise. |
| Symlink in the prefix pointing case-fold resolution outside the prefix | Tampering | `follow_links(false)` + resolve components against `read_dir` entries only (as `entry_ci` does). |
| Malformed ACF/`user.reg` causing panic | DoS | Parse with the existing `keyvalues-serde` / a lenient reg read; map errors to `None`/default (fail-safe, non-blocking). |

## Sources

### Primary (HIGH confidence)
- `github.com/Ortham/libloadorder` `src/game_settings.rs` (master) — `appdata_folder_name`/`my_games_path`/`my_games_folder_name`/`documents_path`/`plugins_file_path`/`STARFIELD_HARDCODED_PLUGINS` — **the authoritative derivation libloot uses**. [VERIFIED]
- `docs.rs/libloot/0.29.5` `enum.GameType` → `Starfield`; `docs.rs/esplugin/6.1.4` `enum.GameId` → `Starfield`. [VERIFIED]
- `api.github.com/repos/loot/starfield/branches/v0.29` — branch live, last commit 2026-06-20. [VERIFIED]
- In-repo source: `crates/steam/src/{resolve.rs,casing.rs}`, `crates/loadorder/src/{loot.rs,scan.rs,masterlist.rs}`, `crates/testkit/src/lib.rs`, `src-tauri/src/{lib.rs,commands/{plugins.rs,games.rs}}` — allow-list surface, path seams, `with_local_path` invariant, `entry_ci` case-fold. [VERIFIED: codebase]

### Secondary (MEDIUM confidence)
- Steam Community discussion + Nexus "Plugins.txt Enabler" (mod 4157) + starfieldwiki.net — confirm `Plugins.txt` in `AppData/Local/Starfield` and `StarfieldCustom.ini` in `Documents/My Games/Starfield`. [CITED]
- SUMMARY.md (`.planning/research/SUMMARY.md`) — milestone research basis; corrected re plugins.txt location.

### Tertiary (LOW confidence)
- Wine `user.reg` "Personal" redirect behaviour under Proton — inferred from Wine registry conventions; validate on live hardware (Phase 9).

## Landmines / MEDIUM-confidence notes

- **CE2 loose-file INI keys, loose-file loading behaviour, `.ba2` v3 opacity, and masterlist currency are deliberately Phase 9.** Phase 6 owns **only the version-drift *signal*** — the ability to say "installed build N is newer than validated build M" — not the validated INI recipe itself. Do not let drift scope-creep into INI logic (that is Phase 8) or in-game verification (Phase 9).
- **Wine Documents redirection (Pitfall 3 / A1)** is the one MEDIUM-confidence path element. libloot/libloadorder do not read `user.reg`; they trust the default derivation. NexTwist honors the CONTEXT lock by reading `user.reg` "Personal" defensively, but the **default `.../steamuser/Documents` path is the load-bearing case** and must remain the fallback. Confirm the owner's live prefix does not redirect in Phase 9.
- **`buildid` field name/monotonicity (A2)** — verify against the owner's real `appmanifest_1716740.acf` in Phase 9; the read fails safe to "no drift shown" if the field is missing.

## Metadata

**Confidence breakdown:**
- Standard stack / allow-list surface: HIGH — enum variants verified on docs.rs; every allow-list site read directly from source.
- Path mechanism (My Games/Starfield + plugins.txt split): HIGH — matched against libloadorder source AND community sources; corrects a SUMMARY conflation.
- Wine redirection edge case: MEDIUM — default path HIGH, redirect handling defensive/inferred.
- Version-drift `buildid`: MEDIUM-HIGH — mechanism sound (reuses existing ACF seam); exact field validated in Phase 9.

**Research date:** 2026-07-07
**Valid until:** ~2026-08-07 (stable; libloot/esplugin pinned. Re-check `loot/starfield` masterlist currency and `STARFIELD_HARDCODED_PLUGINS` growth at Phase 7 planning.)
