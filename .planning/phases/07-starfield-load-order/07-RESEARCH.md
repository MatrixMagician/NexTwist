# Phase 7: Starfield Load Order - Research

**Researched:** 2026-07-07
**Domain:** libloot/esplugin plugin classification + load-order reconciliation (REUSE-heavy over v1.0)
**Confidence:** HIGH on the reuse surface and the medium-master API; MEDIUM on the protected-master proxy and the exact on-launch rewrite deltas (Phase 9 confirms on hardware).

## Summary

Phase 7 is an EXTENSION, not a rewrite. Every load-order primitive already ships in v1.0:
`scan_plugins_for` (esplugin header classify), `apply_load_order` (asterisk `plugins.txt`
seed → libloot reconcile → persist), `propose_sort` (LOOT sort via bundled masterlist),
and the `open_game(1716740)` seam that Phase 6 already wired to
`AppData/Local/Starfield/plugins.txt`. `GameType::Starfield` / `GameId::Starfield` are
already in every allow-list arm. The new work is three narrow additions, all driven by the
**pinned** libloot 0.29.5 / esplugin 6.1.4 public API — no `core` change, no DB migration,
no dependency/MSRV bump.

The three additions: (1) a **medium-master tier** on the plugin model, classified via
`esplugin::Plugin::is_medium_plugin()` (which libloot re-exposes as
`libloot::Plugin::is_medium_plugin()`); (2) a **protected flag** for implicitly-active base
masters, derived from `libloot::Game::is_plugin_active()` (the only public proxy — libloot
does NOT expose the early-loader list) plus defensive engine rejection of any reorder/disable
of a protected plugin; (3) an **SFLO-04 reconciler** that compares on-disk `plugins.txt`
against the recorded `plugin_state` and classifies the game's known on-launch rewrite deltas
as EXPECTED rather than drift.

**Primary recommendation:** Add a `medium: bool` + `protected: bool` pair to the wire model
in the Tauri/scan layer (NOT `core::Plugin` — see User Constraints), classify medium via
esplugin's header flag and protected via `is_plugin_active`, extend `scan_plugins_for` +
`save_plugin_order`/`apply_load_order` with a defensive protected guard, and add a
`loadorder::reconcile_plugins_txt` function feeding a new Starfield-only reconciliation field
on the verify report. Reuse `propose_sort`/`apply_load_order`/`ensure_masterlist` verbatim.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
**Protected & Medium-Master Handling (SFLO-03)**
- Protected base masters (`Starfield.esm`, `SFBGS0xx.esm`, `BlueprintShips-*`, …) are
  determined **via libloot** (implicitly-active / base set) — NEVER hard-coded in NexTwist.
- Shown as **locked, non-editable rows** with a "protected" badge + tooltip.
- The CE2 **medium-master tier** is classified via the esplugin/libloot header flag and
  surfaced as a distinct "medium master" badge, ordered per libloot's rules.
- A user attempt to reorder/disable a protected master is **blocked in the UI AND rejected
  defensively in the engine** (defense-in-depth — never rely on the UI alone).

**LOOT Sort & Masterlist Currency (SFLO-01/02)**
- Masterlist source is the **bundled Phase-6 asset**
  (`crates/loadorder/assets/starfield/masterlist.yaml`) — offline-first; its commit date/age
  is surfaced.
- Sort application **previews the proposed order and requires an explicit Apply** (reuse the
  v1.0 `propose_sort` / preview pattern — no silent auto-apply).
- A **"masterlist from {date} — may be stale"** note appears near the Sort action.
- Parity with LOOT desktop is **deterministic via the same libloot version/inputs**; real-input
  parity is validated on hardware in Phase 9.

**On-Launch plugins.txt Rewrite Reconciliation (SFLO-04)**
- Verify/repair **derives intent from the recorded plugin state in the store**, not the
  last-written raw file. The game's known on-launch rewrite (`.ccc` entries, stripped implicit
  ESMs, `BlueprintShips-*`) is treated as **non-drift**.
- When on-disk differs only by those known-expected deltas, surface **"in sync — game applied
  its expected on-launch changes"**, not a scary raw diff.
- Deltas **beyond** the known rewrite surface as a **real discrepancy** for the user.

### Claude's Discretion
- Exact module placement of the medium-master classification + protected-master query.
- Precise UI copy; detailed visual design deferred to the UI-SPEC.

### Deferred Ideas (OUT OF SCOPE)
- `StarfieldCustom.ini` loose-file activation → Phase 8.
- LOOT-vs-desktop parity on real inputs + in-game load confirmation → Phase 9.
- `.ba2` archive conflict-awareness → future (v1.x/v2).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| SFLO-01 | Enable/disable + order Starfield plugins, written as asterisk `plugins.txt` at the CE2 prefix path | Q4: `open_game(1716740)` → `appdata_local_path` → `AppData/Local/Starfield/plugins.txt` already resolved (Phase 6) and the asterisk round-trip (`apply_load_order`) is game-agnostic — verbatim reuse |
| SFLO-02 | Auto-sort via LOOT using libloot's Starfield masterlist | Q4/Q6: `propose_sort` + `ensure_masterlist(1716740)` already resolve the bundled `assets/starfield/masterlist.yaml` (tested in Phase 6) — verbatim reuse; only the masterlist-age date needs plumbing |
| SFLO-03 | Never reorder/disable/write protected base masters; correctly classify the CE2 medium-master tier — both via libloot | Q1/Q2: `esplugin::Plugin::is_medium_plugin()` for the medium tier; `libloot::Game::is_plugin_active()` for the protected/implicitly-active set; defensive engine guard in `apply_load_order`/`save_plugin_order` |
| SFLO-04 | Verify/repair treats the on-launch `plugins.txt` rewrite as expected, deriving intent from recorded plugin state | Q3: new `reconcile_plugins_txt` comparing on-disk vs `list_plugin_state`, classifying known deltas (`.ccc`, stripped implicit ESMs, re-added blueprint masters) as EXPECTED |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

- **Headless engine boundary:** all safety/classification logic lives in `crates/*` (pure Rust,
  zero Tauri deps). `src-tauri/src/commands/plugins.rs` stays a thin forwarder. New medium/
  protected classification MUST live in `crates/loadorder`, not the command adapter.
- **Errors:** `thiserror` enums in engine crates (`LoadOrderError`, `DeployError`); `anyhow`
  only at the Tauri boundary. New engine functions return the existing typed errors.
- **No `rusqlite` type in `store` public API** — SFLO-04 reads `list_plugin_state` (already
  returns `core` types); no new SQL leaks.
- **Pinned deps:** `libloot = "0.29"` (0.29.5), `esplugin = "6.1"` (6.1.4). Both already
  expose everything Phase 7 needs — **no version bump, no new crate, no MSRV change**.
- **`cargo-deny` load-bearing:** no new dependency, so no `deny.toml` impact.
- **TLS:** masterlist fetch already uses `reqwest` + `rustls` (unchanged).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Medium-master classification | `crates/loadorder` (scan.rs) | — | Header-flag read via esplugin; belongs beside `classify_kind` |
| Protected/implicitly-active query | `crates/loadorder` (loot.rs) | — | Needs a live libloot `Game` + `is_plugin_active`; engine-only |
| Defensive reorder/disable rejection | `crates/loadorder` (loot.rs `apply_load_order`) | frontend (disabled controls) | Defense-in-depth: engine is the authority, UI is the courtesy |
| SFLO-04 reconciliation | `crates/loadorder` (new fn) + `crates/deploy` verify surface | frontend (calm vs amber) | Compare-and-classify is engine logic; UI only renders the verdict |
| Asterisk `plugins.txt` write | `crates/loadorder` (`apply_load_order`) | — | Verbatim reuse — game-agnostic |
| LOOT sort + masterlist | `crates/loadorder` (`propose_sort`/`ensure_masterlist`) | — | Verbatim reuse |
| Masterlist-age date | `crates/loadorder` (new const) → Tauri → frontend | — | Bundled snapshot has no runtime file date; needs a recorded const |
| Badges / locked rows / reconciliation copy | frontend `+page.svelte` | — | Pure render of engine-supplied booleans/enum (per UI-SPEC) |

## Standard Stack

No new libraries. Phase 7 uses the already-pinned, already-vendored crates.

### Core (already in the workspace — verbatim reuse)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `libloot` | 0.29.5 | Game handle, LOOT sort, active-plugin query, medium/blueprint classification | The LOOT project's own Rust API; already de-risked by `libloot_spike` |
| `esplugin` | 6.1.4 | Header-flag plugin classification (ESM/ESL/ESP/**medium**/blueprint) | LOOT author's pure-Rust header parser; already a direct dep |

### Version verification (against the pinned, vendored source — not training data)
```
Cargo.lock: libloot 0.29.5, esplugin 6.1.4 (both from crates.io) — VERIFIED
esplugin 6.1.4 src/game_id.rs:40   GameId::supports_medium_plugins() -> true only for Starfield — VERIFIED
esplugin 6.1.4 src/plugin.rs:259   Plugin::is_medium_plugin() (medium flag set && !is_light) — VERIFIED
esplugin 6.1.4 src/plugin.rs:273   Plugin::is_blueprint_plugin() — VERIFIED
libloot  0.29.5 src/plugin/mod.rs:192  Plugin::is_medium_plugin() delegates to esplugin — VERIFIED
libloot  0.29.5 src/plugin/mod.rs:206  Plugin::is_blueprint_plugin() delegates to esplugin — VERIFIED
libloot  0.29.5 src/game.rs:529     Game::is_plugin_active(name) -> bool (public) — VERIFIED
libloot  0.29.5 src/game.rs:534     Game::load_order() -> Vec<&str> (public) — VERIFIED
```

### Package Legitimacy Audit
**N/A** — Phase 7 installs no new package. Both `libloot` and `esplugin` were legitimacy-gated
and approved at their Phase-2 introduction (verified crates.io + `github.com/loot/libloot`,
allowed by `deny.toml`'s libloot-family GPL allowance). No registry action required.

## Architecture Patterns

### System data flow (the plugin/load-order path this phase extends)

```
                          ┌─────────────────────────────────────────────┐
  enabled mods' staging   │ crates/loadorder                            │
  roots + game Data/  ───▶│  scan_plugins_for(GameId::Starfield, …)      │
                          │    esplugin header parse per file:          │
                          │      is_master_file / is_light_plugin        │
                          │      + NEW: is_medium_plugin  ───────────────┼─▶ Plugin{kind, medium}
                          └─────────────────────────────────────────────┘
                                     │ merged with store plugin_state (enable/order)
                                     ▼
  Tauri commands/plugins.rs (thin)   │
    list_plugins ─────────────────────┘  + NEW: protected flag from a live Game probe
    save_plugin_order ─┐
                       ▼
        loadorder::apply_load_order(1716740, …)
          open_game(1716740) → with_local_path(AppData/Local/Starfield)
          seed asterisk plugins.txt → load_canonical_order → reconcile_order → set_load_order
          + NEW defensive guard: reject if a protected plugin is moved/disabled
                       │
                       ▼
        <prefix>/drive_c/users/steamuser/AppData/Local/Starfield/plugins.txt

  ── SFLO-04 (verify surface) ──
  recorded plugin_state (store) ─┐
  on-disk plugins.txt          ──┴─▶ NEW reconcile_plugins_txt()
                                       classify deltas: EXPECTED (.ccc, stripped
                                       implicit ESMs, re-added blueprint) vs REAL drift
                                       │
                                       ▼  ReconcileState → verify report → UI calm/amber
```

### Pattern 1: Medium-master classification (extends `classify_kind`)
**What:** Add a `medium: bool` alongside the existing ESM/ESL/ESP `kind` on the scanned
plugin. The medium flag is orthogonal to `kind` — a Starfield medium master is a master
(`is_master_file() == true` → `PluginKind::Esm`) that *also* has the medium header flag.
**When:** During `scan_plugins_for` header parse, and again when building the wire model.
**Evidence:**
```rust
// esplugin 6.1.4 src/plugin.rs:259  [VERIFIED: vendored source]
pub fn is_medium_plugin(&self) -> bool {
    // If the medium flag is set in a light plugin then the medium flag is ignored.
    self.is_medium_flag_set() && !self.is_light_plugin()
}
// src/game_id.rs:40 — the flag is only meaningful for Starfield:
pub fn supports_medium_plugins(self) -> bool { self == GameId::Starfield }
```
The existing `classify_kind` (scan.rs:75) already parses the header with `header_only`; add a
sibling read of `plugin.is_medium_plugin()` inside the same `Ok(())` arm. For SkyrimSE/FO4 the
flag is never set, so a single classifier path stays correct across all three games.

### Pattern 2: Protected / implicitly-active determination (via libloot, no name list)
**What:** libloot 0.29.5 exposes **no public getter for the early-loader / implicitly-active
list** — `game_settings().early_loading_plugins()` is private (`src/game.rs:478`, reached only
through the private `load_order` field). The only public, libloot-driven proxy is:
```rust
// libloot 0.29.5 src/game.rs:529  [VERIFIED: vendored source]
pub fn is_plugin_active(&self, plugin_name: &str) -> bool
```
After `load_current_load_order_state()`, libloot marks the game's hardcoded early-loaders
(base master `Starfield.esm`, the `SFBGS0xx.esm` Creation masters, and every `.ccc`-listed
plugin including `BlueprintShips-Starfield.esm`) **active even though they are never written to
`plugins.txt`** (loot.rs already documents this at lines 40-45). So:

> **protected(name) = `game.is_plugin_active(name)` is true AND NexTwist did not itself enable
> it** (i.e. it is active without being a user `*Name` line).

This is 100% libloot-derived — no embedded name list, satisfying SFLO-03 criterion 3. The
`SFBGS0xx.esm` example names in CONTEXT/ROADMAP are illustrative only; the code must never
match on them.
**Defensive engine guard:** `apply_load_order` (and `save_plugin_order` before it) must reject
any request that reorders or disables a plugin the live `Game` reports as implicitly active.
libloot ALREADY rejects reordering the pinned early-loader prefix (`set_load_order` →
`"load order interaction failed"`, documented at loot.rs:26-28), so a large class of this is
enforced for free; the new explicit guard makes the rejection a typed, testable
`LoadOrderError` before the libloot call rather than an opaque libloot string.

### Pattern 3: Medium-master ordering (defer to libloot, do not hand-roll)
Medium masters load **between full masters and regular plugins**. libloot's own sort +
`load_canonical_order` already place them correctly (blueprint masters sort at the END of the
master block — `libloot 0.29.5 src/sorting/validate.rs`). `reconcile_order` (loot.rs:189)
already keeps every master-group plugin at its canonical libloot position, so medium masters
need **no new ordering code** — only the `is_master_group` predicate must continue to treat a
medium plugin as master-group (it does: a medium master is `is_master_file()` → `Esm`).

### Pattern 4: SFLO-04 reconciliation (compare-and-classify, never a raw diff)
**What:** A new pure function in `crates/loadorder`:
```
reconcile_plugins_txt(recorded: &[Plugin], on_disk_txt: &str, protected: &HashSet<String>)
    -> ReconcileState { InSync, Drift(Vec<String>) }
```
Parse the on-disk asterisk `plugins.txt`, diff against the recorded `plugin_state` order/active
set, and bucket each delta:
- an entry present on disk but not recorded, whose name is in the `protected`/implicitly-active
  set (a `.ccc` entry, a re-added blueprint master) → **EXPECTED**;
- a recorded implicit ESM absent from disk (the game strips implicitly-active masters from the
  file — they don't belong there) → **EXPECTED**;
- anything else (a user mod that vanished, a reordered regular `.esp`, an unknown entry) →
  **REAL drift**, collected into `Drift(names)`.
The result is a classification, not a text diff — the UI renders the calm "in sync" line when
`InSync`, and the amber `.warn` list only for `Drift`.

### Anti-Patterns to Avoid
- **Hard-coding `Starfield.esm`/`SFBGS0xx.esm`/`BlueprintShips-*`** — explicitly forbidden by
  SFLO-03. Always go through `is_plugin_active` / esplugin flags.
- **Adding `medium`/`protected` to `core::Plugin`** — the DB `plugin_state` schema and the
  `core` contract are frozen this phase (no migration). Medium is a *scan-derived* property and
  protected is a *live-game-derived* property; both belong on the wire/view model, not the
  persisted `core::Plugin`. (See Landmines.)
- **Hand-rolling the early-loader/medium order** — v1.0's RC1 bug proved this; defer to
  `load_canonical_order` + `reconcile_order`.
- **Rendering a raw `plugins.txt` diff in the UI** — SFLO-04 requires a classified verdict.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Medium-master detection | A header byte-flag parser | `esplugin::Plugin::is_medium_plugin()` | Correct light/medium/inject interaction is subtle (plugin.rs:259/244) |
| Protected-master list | An embedded `Starfield.esm`/CCC name table | `libloot::Game::is_plugin_active()` | SFLO-03 forbids hard-coding; libloot owns the hardcoded list |
| Early-loader ordering | An alphabetical masters-first sort | `load_canonical_order` + `reconcile_order` | v1.0 RC1: hand-rolled order → `"load order interaction failed"` |
| Asterisk `plugins.txt` format | A line writer | `apply_load_order` (seeds libloot, libloot rewrites) | Already round-trip-tested for SSE/FO4 |
| Masterlist fetch/cache/bundled fallback | HTTP + cache logic | `ensure_masterlist(app_data, 1716740, …)` | Already tested for Starfield in Phase 6 |

**Key insight:** every "new" behavior this phase needs is a *read* of an existing libloot/
esplugin signal plus a *classification*, not new I/O. The reversible-write path is untouched.

## Runtime State Inventory

Phase 7 is not a rename/refactor/migration, but it touches persisted plugin state, so the
inventory is worth an explicit pass:

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | `plugin_state` rows store `kind` (esm/esl/esp), `enabled`, `order_index` per profile. Medium/protected are NOT persisted and MUST NOT be (frozen schema). | None — derive medium at scan time, protected at query time |
| Live service config | Starfield's real `plugins.txt` lives in the Proton prefix (`AppData/Local/Starfield/`), rewritten by the game on launch — this is the SFLO-04 subject, not git-tracked. | Read-only compare in reconcile; never treat its rewrite as corruption |
| OS-registered state | None | None — verified: no OS registrations involve load order |
| Secrets/env vars | None | None |
| Build artifacts | Bundled `assets/starfield/masterlist.yaml` is `include_str!`'d into the binary (no runtime file → no runtime mtime). | Add a recorded snapshot-date const for the masterlist-age note |

## Common Pitfalls

### Pitfall 1: Assuming a public libloot getter for implicitly-active plugins
**What goes wrong:** A plan that says "call `game.early_loading_plugins()`" will not compile —
that method is private in 0.29.5.
**Why:** libloot only re-exports `Game`, `Plugin`, `Database`; the settings/load-order internals
are not public.
**How to avoid:** Use `is_plugin_active` as the proxy (Pattern 2). Verified against
`src/game.rs` and `src/lib.rs` public surface.
**Warning sign:** a compile error `no method named early_loading_plugins`.

### Pitfall 2: Treating "medium" as a fourth `PluginKind` variant
**What goes wrong:** Adding `PluginKind::Medium` changes the frozen `core` enum + the DB token
set, forcing a migration and breaking the "no core change" constraint.
**How to avoid:** Keep `kind` as ESM/ESL/ESP (a medium master is an ESM) and carry `medium` as a
separate boolean on the wire model only.
**Warning sign:** a `V*.sql` migration appearing in the plan.

### Pitfall 3: Masterlist-age date with no date source
**What goes wrong:** The UI needs a masterlist date, but `masterlist.yaml` embeds none and the
bundled snapshot has no runtime file to `stat`.
**How to avoid:** Record the snapshot date as a `const` beside the bundled include (the file's
git commit is 2026-07-07) and forward it through the sort/status path.
**Warning sign:** a plan that reads `masterlist.yaml` metadata at runtime.

### Pitfall 4: Verify surface built as a raw text diff
**What goes wrong:** Reusing the generic `VerifyReport` file-drift buckets for `plugins.txt`
surfaces the game's expected on-launch rewrite as red "changed"/"orphan" noise.
**How to avoid:** SFLO-04 is a SEPARATE classification (`ReconcileState`) on a Starfield-only
field, not a `missing/changed/orphan` entry. `deploy::verify` hashes `Data/` files; `plugins.txt`
lives in the prefix AppData, outside the deploy root, so it is *not even seen* by the current
verify walk — SFLO-04 is additive, not a modification of the file-hash logic.

## Code Examples

Verified signatures from the pinned, vendored crates (not training data):

### Medium classification (extend scan.rs:75 `classify_kind`)
```rust
// esplugin 6.1.4 — header_only parse already done in classify_kind.
// [VERIFIED: ~/.cargo/.../esplugin-6.1.4/src/plugin.rs:259]
let medium = plugin.is_medium_plugin();   // true only for Starfield medium-flagged masters
let kind = if plugin.is_light_plugin() { Esl }
           else if plugin.is_master_file() { Esm }   // a medium master is also a master
           else { Esp };
```

### Protected probe (new helper in loot.rs, uses the existing open_game seam)
```rust
// [VERIFIED: ~/.cargo/.../libloot-0.29.5/src/game.rs:499,529]
let mut game = open_game(1716740, &install_dir, &appdata_local)?;
game.load_current_load_order_state()?;               // populates active state
let protected = game.is_plugin_active(name)           // implicitly active …
             && !nextwist_enabled_names.contains(name); // … but not a user *Name line
```

### SFLO-04 reconcile (new pure fn — unit-testable, no I/O)
```rust
// classify each on-disk vs recorded delta; return a verdict, never a raw diff.
pub enum ReconcileState { InSync, Drift(Vec<String>) }
pub fn reconcile_plugins_txt(
    recorded: &[Plugin],
    on_disk_txt: &str,
    protected: &std::collections::HashSet<String>,
) -> ReconcileState { /* bucket per Pattern 4 */ }
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| ESM/ESL/ESP only (Bethesda pre-CE2) | + medium-master tier (CE2/Starfield) | Starfield (2023), esplugin ≥ 6 | A master can be full or medium; scale affects FormID space — classification must read the flag |
| Hard-coded implicit-master lists in old managers | libloot/libloadorder own the hardcoded early-loader list | libloot 0.18+ | Consumers query, never embed — matches SFLO-03 |

**Deprecated/outdated:** none relevant — the pinned versions are current for the CE2 feature set.

## Analog Map (folded in — no separate pattern-mapper this phase)

Each new artifact and the closest existing analog the executor should read/mirror:

| New artifact | Closest analog `file:line` | How to mirror |
|--------------|----------------------------|---------------|
| Medium-master classification | `crates/loadorder/src/scan.rs:75` `classify_kind` | Add `is_medium_plugin()` read inside the same `Ok(())` arm; thread a `medium` bool out beside `kind` |
| Wire model `medium`/`protected` fields | `crates/loadorder/src/scan.rs:139` `Plugin { name, kind, enabled, order }` construction + `frontend/src/lib/api.ts:100` `PluginInfo` | New struct in loadorder/Tauri (NOT `core::Plugin`); mirror the serde-derive + api.ts interface shape |
| Protected-master query | `crates/loadorder/src/loot.rs:152` `load_canonical_order` (opens game, loads state) | New fn opens the game the same way, then calls `is_plugin_active` per name |
| Defensive reorder/disable guard | `crates/loadorder/src/loot.rs:305` `apply_load_order` (drops missing-on-disk plugins before libloot) | Add a symmetric guard that returns a typed `LoadOrderError` if a protected plugin was moved/disabled |
| SFLO-04 reconciler | `crates/deploy/src/verify.rs:75` `verify` (compare recorded manifest vs on-disk, classify buckets) | Mirror the "recorded-is-truth, classify deltas, never mutate" shape; source of truth is `store::list_plugin_state` not the file manifest |
| Reconciliation report field | `crates/deploy/src/verify.rs:36` `VerifyReport` + `frontend/src/lib/api.ts:73` `VerifyReport` | Add a `Option<ReconcileState>` Starfield field; mirror serde + api.ts interface |
| Masterlist-age date | `crates/loadorder/src/masterlist.rs:46` `STARFIELD_SNAPSHOT` include + `loot.rs:356` `SortProposal` | Add a snapshot-date `const` beside the include; add a field to `SortProposal` or a small status |
| Testkit Starfield plugin fixtures | `crates/loadorder/tests/plugins.rs:28` `write_min_plugin` + `crates/loadorder/tests/libloot_spike.rs:43` `write_min_plugin` | Extend to write a medium-flagged TES4 header; add a Starfield prefix via `testkit::fake_proton_prefix` (lib.rs:79) |
| Asterisk round-trip test | `crates/loadorder/tests/plugins.rs:46` `writes_asterisk_masters_first` / `:259` `fo4_multi_master_game_master_first_active_survives` | Clone for Starfield with `STARFIELD` appid + `"Starfield"` folder |

## Validation Architecture

> nyquist_validation assumed enabled (no `.planning/config.json` override observed).

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + integration tests under `crates/*/tests/`; `testkit` fixtures |
| Config file | none — cargo test |
| Quick run command | `cargo test -p nextwist-loadorder` |
| Full suite command | `cargo test --workspace --locked` |

### Phase Requirements → Test Map
| Req | Behavior | Type | Automated command | File exists? |
|-----|----------|------|-------------------|-------------|
| SFLO-03 | Medium-flagged Starfield master classifies with `medium == true`; SSE/FO4 masters do not | unit | `cargo test -p nextwist-loadorder medium` | ❌ Wave 0 (extend scan.rs tests) |
| SFLO-03 | Protected/implicitly-active plugin reported by `is_plugin_active` is flagged protected | integration | `cargo test -p nextwist-loadorder protected` | ❌ Wave 0 (new, Starfield fixture) |
| SFLO-03 | **Protected-master immutability** — engine rejects a reorder or disable of a protected plugin with a typed error (defense-in-depth, not UI-only) | integration | `cargo test -p nextwist-loadorder protected_reorder_rejected` | ❌ Wave 0 |
| SFLO-02 | **Sort determinism** vs the bundled masterlist — same inputs → same `propose_sort` order across runs | integration | `cargo test -p nextwist-loadorder starfield_sort_determinism` | ❌ Wave 0 (mirror `propose_sort_returns_order_without_writing`) |
| SFLO-01 | Asterisk `plugins.txt` round-trip under `AppData/Local/Starfield` (enabled `*Name`, implicit masters omitted) | integration | `cargo test -p nextwist-loadorder starfield_asterisk` | ❌ Wave 0 (clone `writes_asterisk_masters_first`) |
| SFLO-04 | Reconcile a synthetic on-launch-rewritten `plugins.txt` (added `.ccc`, stripped implicit ESM, re-added blueprint) → `InSync` | unit | `cargo test -p nextwist-loadorder reconcile_expected_deltas` | ❌ Wave 0 |
| SFLO-04 | A beyond-expected delta (a user `.esp` vanished / reordered) → `Drift([names])` | unit | `cargo test -p nextwist-loadorder reconcile_real_drift` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p nextwist-loadorder` (+ `-p nextwist-deploy` if verify field touched)
- **Per wave merge:** `cargo test --workspace --locked`
- **Phase gate:** full workspace suite green + `cargo clippy --workspace --all-targets -- -D warnings` before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `crates/loadorder/src/scan.rs` unit tests — medium classification (needs a medium-flag TES4 header fixture; the 0x0400 medium flag beyond the current 24-byte master stub)
- [ ] `crates/loadorder/tests/plugins.rs` — Starfield asterisk round-trip + protected-immutability + sort-determinism (Starfield fixture via `write_min_plugin` + `fake_proton_prefix`)
- [ ] `crates/loadorder` reconcile unit tests — synthetic rewritten `plugins.txt` fixtures (expected-delta vs real-drift)
- [ ] `crates/testkit` — a `fake_starfield_*` / medium-flag plugin-header builder if the medium fixture is reused across crates

## Environment Availability

No new external dependency. The Phase-7 test suites run headless (`crates/*`, no WebKitGTK).
The only external data source — the LOOT Starfield masterlist — already has an offline bundled
fallback (`assets/starfield/masterlist.yaml`), so tests never require network.

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| libloot | classification/sort | ✓ (vendored) | 0.29.5 | — |
| esplugin | header classify | ✓ (vendored) | 6.1.4 | — |
| Starfield masterlist | LOOT sort | ✓ bundled | Phase-6 snapshot (git 2026-07-07) | bundled snapshot IS the fallback |

## Security Domain

`security_enforcement` not disabled in config → included. Phase 7 adds no new trust boundary:
it reads existing on-disk plugins (already header-parsed defensively — scan.rs treats names as
opaque filenames, T-02-12) and the masterlist (already pinned-host + no-redirect fetched,
T-02-10). No new network, no new file writes beyond the already-guarded `plugins.txt` seam.

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V5 Input Validation | yes | esplugin `header_only` parse tolerates corrupt files (falls back to ESP, never aborts); reconcile parses `plugins.txt` as opaque lines, no path join |
| V6 Cryptography | no | — |
| V12 File handling | yes | `plugins.txt` written only via the existing libloot seam under the resolved prefix; reconcile is read-only |

| Threat | STRIDE | Mitigation |
|--------|--------|------------|
| Malicious plugin filename in `plugins.txt` | Tampering | Names treated as opaque strings; never joined as a path outside `Data/`; libloot header-parses each (existing behavior) |
| Protected master silently reordered/disabled | Tampering | Defense-in-depth: engine-side typed rejection in addition to UI disable (SFLO-03) |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The exact on-launch rewrite delta set is `.ccc` entries + stripped implicit ESMs + re-added blueprint masters (`BlueprintShips-*`) | Pattern 4 / SFLO-04 | If Starfield rewrites additional entries, reconcile flags a false "drift". Phase 9 confirms on hardware; keep the EXPECTED set data-driven from libloot's active set + esplugin blueprint flag so it self-corrects rather than being a fixed list |
| A2 | `is_plugin_active && !nextwist_enabled` is a sufficient proxy for "protected/implicitly-active" | Pattern 2 | A regular master the user enabled that libloot also treats as implicitly active could be mis-tagged; low risk because libloot omits implicit plugins from the user `*` file. Phase 9 confirms against a real prefix |
| A3 | Blueprint masters are the re-added-on-launch entries and sort at the end of the master block | Pattern 3 / A1 | Verified in libloot `sorting/validate.rs`, but real-game re-add behavior is hardware-confirmed in Phase 9 |
| A4 | Masterlist-age date can be a recorded const (git commit 2026-07-07) since the snapshot is `include_str!`'d | Pitfall 3 | If a fresher masterlist is fetched at runtime, the const would understate currency; acceptable — the note says "may be stale", and refresh path already caches a dated file |

## Landmines / MEDIUM-confidence

The two items Phase 9 (on-hardware) confirms:

1. **Protected-master mechanism (MEDIUM).** libloot 0.29.5 exposes **no public early-loader/
   implicitly-active getter** (`game_settings().early_loading_plugins()` is private — VERIFIED
   against `src/game.rs:478` + the `src/lib.rs` public re-export list). The recommended proxy is
   `Game::is_plugin_active(name)` after `load_current_load_order_state()`: implicitly-active
   base masters and `.ccc` plugins report active without appearing as `*` lines. This is
   correct by construction from the documented libloot behavior (loot.rs:40-45) and the FO4
   integration test (`plugins.rs:309` asserts `is_plugin_active("Fallout4.esm")` while the file
   omits it), but the *Starfield-specific* implicit set (which `SFBGS0xx.esm` masters, which
   `.ccc` entries) is only observable against a real Starfield prefix — Phase 9.

2. **Exact on-launch rewrite deltas (MEDIUM).** The EXPECTED-delta set for SFLO-04 (`.ccc`
   additions, stripped implicit ESMs, re-added blueprint masters) is drawn from libloot's
   active-set + esplugin's `is_blueprint_plugin` flag rather than a fixed name list, so it
   adapts to the real game. The precise entries Starfield writes/strips on launch are confirmed
   on hardware in Phase 9; until then, reconcile should err toward classifying a
   libloot-implicit/blueprint delta as EXPECTED (avoid false alarms) while still surfacing any
   *user-mod* delta as real drift.

Everything else (medium classification API, asterisk write path, LOOT sort, masterlist bundling,
the `open_game(1716740)` prefix seam) is HIGH confidence — verified against the pinned vendored
source and already covered by shipped v1.0 tests.

## Sources

### Primary (HIGH confidence)
- `~/.cargo/registry/src/.../esplugin-6.1.4/src/plugin.rs` (219,259,273), `src/game_id.rs:40` — medium/master/blueprint flag logic
- `~/.cargo/registry/src/.../libloot-0.29.5/src/game.rs` (163,201,499,524,529,534,543), `src/plugin/mod.rs` (171,185,192,206), `src/lib.rs` (public re-exports), `src/sorting/validate.rs` — libloot public API + private early-loader confirmation
- `Cargo.lock` — pinned libloot 0.29.5 / esplugin 6.1.4
- NexTwist v1.0 source: `crates/loadorder/src/{scan.rs,loot.rs,masterlist.rs}`, `crates/store/src/plugins.rs`, `crates/deploy/src/verify.rs`, `src-tauri/src/commands/plugins.rs`, `frontend/src/lib/api.ts`, `crates/loadorder/tests/{plugins.rs,libloot_spike.rs}`, `crates/testkit/src/lib.rs`

### Secondary (MEDIUM confidence)
- CONTEXT.md / UI-SPEC.md / ROADMAP.md / REQUIREMENTS.md (Phase 7) — locked decisions and success criteria
- graphmind priority memory: Starfield two-path split (plugins.txt → `AppData/Local/Starfield/` via Phase-6 seam; `StarfieldCustom.ini` → My Games is Phase 8)

## Metadata

**Confidence breakdown:**
- Standard stack / medium API: HIGH — verified against vendored pinned source
- Reuse surface (asterisk write, sort, masterlist, prefix seam): HIGH — shipped + tested in v1.0/Phase 6
- Protected-master proxy: MEDIUM — sound from libloot's documented behavior; Starfield-specific set is Phase-9 hardware-confirmed
- On-launch rewrite deltas: MEDIUM — Phase-9 hardware-confirmed

**Research date:** 2026-07-07
**Valid until:** libloot/esplugin are pinned, so stable indefinitely for this phase; re-check only on a deliberate 0.29→0.30 bump.

## RESEARCH COMPLETE

- **Medium-master API (HIGH):** `esplugin 6.1.4 Plugin::is_medium_plugin()` (medium flag set & not light; Starfield-only via `GameId::supports_medium_plugins`), re-exposed by `libloot 0.29.5 Plugin::is_medium_plugin()` — carry as a `medium` bool on the wire model, NOT a new `PluginKind` variant.
- **Protected-master mechanism (MEDIUM):** libloot exposes no public early-loader getter (`early_loading_plugins()` is private); use `Game::is_plugin_active(name)` after `load_current_load_order_state()` as the libloot-driven proxy (implicit masters are active without a `*` line) plus a defensive typed engine rejection — never a hard-coded name list.
- **SFLO-04 design:** a new pure `reconcile_plugins_txt(recorded, on_disk, protected) -> {InSync | Drift(names)}` comparing recorded `plugin_state` against the on-disk file, classifying `.ccc`/stripped-implicit/blueprint deltas as EXPECTED and everything else as real drift — a classified verdict, never a raw diff; lives outside `deploy::verify`'s `Data/`-hash walk.
- **Reuse surface:** `apply_load_order`, `propose_sort`, `ensure_masterlist`, and the `open_game(1716740)` → `AppData/Local/Starfield/plugins.txt` seam are reused verbatim; the only additions are two derived booleans, a reconcile fn + report field, and a masterlist-date const — no `core` change, no DB migration, no dep/MSRV bump.
