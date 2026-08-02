# Technology Stack — v1.1 Starfield Support

**Project:** NexTwist (adding Starfield / Creation Engine 2 support)
**Researched:** 2026-07-07
**Confidence:** HIGH (crate versions + enum variants verified against crates.io / docs.rs / GitHub API)
**Scope:** Stack DELTA only — what changes vs the shipped v1.0 engine. Existing capabilities (deploy ladder, journal, store, steam detection, NexusMods auth) are unchanged and out of scope.

## Headline

**Starfield needs essentially ZERO new crates and ZERO MSRV movement.** The two hard parts — LOOT sorting and plugin-header parsing — are already Starfield-capable in the exact crate versions v1.0 ships. The only genuinely new dependency is a small INI crate (`rust-ini`), and even that is optional (hand-rollable). The real v1.1 work is code (new AppID, a **different plugins.txt location**, a masterlist slug), not dependencies.

## Answers to the Four Questions

### 1. libloot — Starfield support & MSRV

**`libloot 0.29.5` (already pinned) ALREADY supports Starfield. No version change required, no MSRV impact.**

- `libloot::GameType::Starfield` is present in the enum today (verified against docs.rs source — variants include `Morrowind, Oblivion, OblivionRemastered, Skyrim, SkyrimSE, SkyrimVR, Fallout3, FalloutNV, Fallout4, Fallout4VR, Starfield, OpenMW`). The `game_type_for()` match in `crates/loadorder/src/loot.rs` just needs a `1716740 => Some(GameType::Starfield)` arm.
- **MSRV is unaffected.** Every `libloot 0.29.x` release (0.29.3 → 0.29.6) declares `rust-version = 1.89` — identical to the current pin. Bumping `libloot = "0.29"` from 0.29.5 to the latest **0.29.6** is a patch bump that keeps MSRV at 1.89. The bump is **optional** (0.29.5 already has Starfield); take 0.29.6 only for its bug fixes.
- **Starfield masterlist exists and matches the current branch pin.** `github.com/loot/starfield` ships a **`v0.29`** branch (last pushed 2026-06-20) — the *same* `MASTERLIST_BRANCH = "v0.29"` constant `crates/loadorder/src/masterlist.rs` already uses for Skyrim SE / Fallout 4. So `masterlist_url("starfield")` resolves with the existing host+branch pin; you only add a `1716740 => Some("starfield")` slug arm plus a bundled CC0 snapshot (`assets/starfield/masterlist.yaml`) for the offline fallback.

### 2. esplugin — Starfield plugin parsing

**`esplugin 6.1.4` (already a direct + transitive dep) ALREADY parses Starfield plugins. No version change required.**

- `esplugin::GameId::Starfield` is present today (verified against docs.rs: `Morrowind, Oblivion, Skyrim, SkyrimSE, Fallout3, FalloutNV, Fallout4, Starfield`). esplugin is authored by the LOOT author specifically to track Creation Engine; its Starfield handling covers the CE2 record-header changes and the **medium-master flag** semantics libloot relies on for sorting.
- It is already both a **direct dep** (`esplugin = "6.1"`) and the transitive parser inside `libloot 0.29.5` (esplugin 6.1.4), so Starfield header classification (`PluginKind::Esm/Esl/Esp`) works with no Cargo change. GPL-3.0, already whitelisted in `deny.toml`'s libloot-family allowance — **no cargo-deny change**.

### 3. `.ba2` version 3 archives

**Treat `.ba2` as an OPAQUE ordinary file. Add NO crate.**

- The deploy engine (`reflink→hardlink→symlink→copy` ladder, journal, purge) operates on whole files by path and never inspects archive *contents*. A Starfield mod's `Foo - Textures.ba2` is deployed and reverted exactly like any other file; `.ba2` v3's new internal format is irrelevant to linking, backup, or byte-for-byte purge.
- Conflict detection is already path-level: two mods each shipping `Textures.ba2` conflict at the *filename*, which the existing rank-based conflict engine resolves. You do **not** need to crack open the archive to know two files collide.
- A Rust crate reads BA2 v1/v2/v3 (`ba2` by Ryan-rsm-McKenzie, MIT) **but it is out of scope** — inspecting archive interiors would only matter for *intra-archive* conflict detection, which even Vortex/MO2 don't do for BA2s and which the v1.1 safety goal doesn't require. **Do not add it.** (ponytail: speculative; add only if a future milestone wants BA2-interior conflict analysis.)
- The one BA2-adjacent concern is **activation**, not parsing: Starfield reads `StarfieldCustom.ini` to pick up loose files / registered archives — that's question 4, an INI edit, not a BA2 reader.

### 4. INI read/modify/round-trip for `StarfieldCustom.ini`

**Add `rust-ini` 0.21.3 as ONE new dep (recommended), OR hand-roll a two-key edit. Byte-for-byte restore comes from the existing deploy backup ledger, NOT from the INI crate.**

Recommended: **`rust-ini = "0.21"`** (crate name `rust-ini`, imported as `ini`).
- **License MIT** → clean under `deny.toml`, no cargo-deny trip.
- **MSRV 1.64** → well under the 1.89 pin, no MSRV impact.
- Transitive deps `cfg-if` (MIT/Apache), `ordered-multimap` (MIT), optional `unicase` — all permissive, all common. No non-free surface.
- Handles the fiddly-but-important cases correctly: `[Archive]` section absent, `StarfieldCustom.ini` absent entirely (very common — must be *created*), key present-and-must-be-replaced, an existing user value in `sResourceDataDirsFinal` that must be merged rather than clobbered, CRLF/BOM. Editing `bInvalidateOlderFiles` / `sResourceDataDirsFinal` under `[Archive]`.

**Critical design note — reversibility is NOT the INI crate's job.** The core guarantee ("byte-for-byte restore on purge") is provided the same way deploy already provides it: **back up the original `StarfieldCustom.ini` bytes (or record "did-not-exist") in the store/backup ledger BEFORE the edit, then restore those exact bytes on purge.** rust-ini's write path *normalizes* formatting (it does not perfectly preserve comments / blank-line / key ordering), so it is **not** a byte-for-byte round-trip — and it doesn't need to be. Restore replays the saved original bytes; the forward edit only needs to be a *valid, correct* modification, which rust-ini gives you. Route the INI file through the **same journaled backup-before-write path deploy already uses** so a crash mid-edit is recoverable and purge is exact.

**Lazy alternative (ponytail):** the edit is literally two keys in one `[Archive]` section. A hand-rolled "ensure section, set/replace two keys, else append" is ~40 lines and avoids a dependency, and since the ledger (not the writer) owns reversibility, formatting fidelity of the forward write barely matters. **Recommendation: `rust-ini` — the "don't clobber the user's existing `sResourceDataDirsFinal`" merge is exactly the edge case a tested parser should own on a safety-critical path.** Hand-roll only if the team prefers zero new deps.

## Stack Delta Table

| Crate | Current (v1.0) | v1.1 action | New or bump? | MSRV impact | cargo-deny |
|-------|----------------|-------------|--------------|-------------|------------|
| **libloot** | `0.29` (0.29.5) | none required; optional patch bump to **0.29.6** | Bump (optional) | None — 0.29.x MSRV = 1.89 | Already allowed |
| **esplugin** | `6.1` (6.1.4) | none — `GameId::Starfield` already present | No change | None | Already allowed (GPL-3.0 libloot family) |
| **rust-ini** | — | add `rust-ini = "0.21"` for `StarfieldCustom.ini` | **NEW** | None — MSRV 1.64 | Clean (MIT + permissive deps) |
| BA2 reader (`ba2`) | — | **do NOT add** — `.ba2` is opaque to deploy | — | — | — |

## Non-Crate Changes the Roadmapper Must Plan (code, not dependencies)

Where the actual v1.1 effort and risk live — none require a crate, but STACK-adjacent so flagging for requirements:

1. **New AppID plumbing:** `1716740 → GameType::Starfield` in `game_type_for()`, a `"starfield"` masterlist slug + bundled snapshot, and Starfield in the steam crate's `SUPPORTED_APPIDS` / `appdata_folder_name`.
2. **⚠️ HIGHEST-RISK SEAM — plugins.txt lives in a DIFFERENT location for Starfield.** Skyrim SE / Fallout 4 use `AppData/Local/<Game>/Plugins.txt` (what `appdata_local_path()` builds today). **Starfield (CE2) uses `Documents/My Games/Starfield/Plugins.txt`** — i.e. `drive_c/users/steamuser/Documents/My Games/Starfield/` inside the Proton prefix, NOT `AppData/Local`. Because NexTwist always calls `libloot::Game::with_local_path` (the Linux seam — libloot can't derive this itself, Pitfall 1), the code MUST supply Starfield's `My Games` path explicitly. `appdata_local_path()` / `appdata_folder_name()` need a Starfield-specific branch (or a new resolver). This is the #1 place a wrong path silently breaks load order — verify on hardware against the owner's real Starfield prefix.
3. **INI activation path:** new reversible `StarfieldCustom.ini` writer routed through the deploy backup/journal ledger (see Q4).
4. **`.ba2` — nothing to build.** Opaque file; existing deploy + conflict engine already handle it.

## Version Compatibility

| Package | Compatible with | Note |
|---------|-----------------|------|
| libloot 0.29.5/0.29.6 | esplugin 6.1.4 (its transitive parser) | Keep the direct esplugin `6.1` pin aligned with whatever libloot 0.29.x vendors to avoid a duplicate esplugin in the tree. |
| libloot 0.29.x | Rust 1.89 (MSRV) | Whole 0.29 line is 1.89; no toolchain move for Starfield. |
| masterlist branch `v0.29` | libloot 0.29.x | `loot/starfield` publishes a `v0.29` branch — matches the existing `MASTERLIST_BRANCH` pin; no branch-constant change. |
| rust-ini 0.21 | Rust 1.64+ | Far under the 1.89 pin. |

## Installation

```toml
# root Cargo.toml [workspace.dependencies] — the ONLY new line:
rust-ini = "0.21"

# Optional (bug-fix patch bump; not required for Starfield):
# libloot = "0.29"   # already resolves 0.29.5+; latest is 0.29.6, MSRV unchanged
```

```
# New asset for the offline masterlist fallback:
crates/loadorder/assets/starfield/masterlist.yaml   # CC0 snapshot from loot/starfield @ v0.29
```

## Sources

- crates.io API (`/api/v1/crates/*`) — libloot **max_stable 0.29.6**, esplugin **6.1.4**, rust-ini **0.21.3**; per-version `rust_version` confirms libloot 0.29.3–0.29.6 = MSRV 1.89, rust-ini 0.21.3 = MSRV 1.64 / MIT / deps cfg-if, ordered-multimap, unicase — **HIGH** (authoritative registry).
- docs.rs source — `libloot::GameType` includes `Starfield`; `esplugin::GameId` includes `Starfield` — **HIGH** (published crate docs).
- GitHub API `repos/loot/starfield` — repo exists, default branch `v0.29`, pushed 2026-06-20; branches `v0.18 / v0.21 / v0.26 / v0.29` — **HIGH** (authoritative; matches existing masterlist pin).
- In-repo source (`crates/loadorder/src/loot.rs`, `masterlist.rs`, root `Cargo.toml`) — current seam shape, pins, and the `with_local_path` Linux invariant — **HIGH**.
- Starfield plugins.txt at `Documents/My Games/Starfield` (CE2, vs `AppData/Local` for Skyrim/FO4) — Bethesda/LOOT/libloadorder game-folder convention — **MEDIUM** (well-established modding convention; verify on hardware against the owner's prefix per PROJECT.md's live-verification plan).
