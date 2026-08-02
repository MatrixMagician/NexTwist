# Phase 6: Starfield Detection & CE2 Path Resolution - Pattern Map

**Mapped:** 2026-07-07
**New artifacts analyzed:** 6
**Analogs found:** 6 / 6 (all in-repo, all exact/role-match)

Every Phase-6 artifact has a live in-repo analog. There is **no net-new architecture** —
the only genuinely new *shape* is the `Documents/My Games/Starfield` path tail and the
`Ce2ConfigState` typed-state enum; everything else is a `match appid` arm or a fixture
variant. Excerpts below are the exact seams to copy from (verified against live source).

## File Classification

| New artifact | Role | Data flow | Closest analog | Match |
|--------------|------|-----------|----------------|-------|
| `steam::my_games_path(prefix)` + `resolve_ce2_config` (`crates/steam/src/ce2.rs`, NEW) | resolver/utility | file-I/O (read-only path probe) | `loadorder::appdata_local_path` (`loot.rs:67`) + `entry_ci` (`resolve.rs:323`) | exact seam + role-match |
| AppID 1716740 allow-list arms (6 sites) | config/mapping | transform (`match appid -> Option`) | `game_type_for`/`appdata_folder_name`/`game_id_for`/`game_slug`/`SUPPORTED_APPIDS`/`expected_exe`/`default_name` | exact |
| `buildid` read + `installed_build` (`resolve.rs`) | utility | file-I/O (ACF parse) | `AppManifest` keyvalues-serde seam (`resolve.rs:152/166`) | exact |
| `Ce2ConfigState` typed first-launch state (`ce2.rs`) | model/error | request-response (typed return) | `LoadOrderError::NoLocalAppData` (`error.rs:42`) — pattern to *diverge from* (state, not Err) | role-match |
| Bundled `assets/starfield/masterlist.yaml` + `VALIDATED_BUILD` (`masterlist.rs`) | config/asset | batch (`include_str!`) | `SKYRIMSE_SNAPSHOT`/`bundled_snapshot` (`masterlist.rs:42/55`) | exact |
| testkit My-Games + case-mismatch fixture + extended allow-list test | test | fixture builder | `fake_proton_prefix` (`testkit/lib.rs:79`) + `game_type_for_allow_lists_only_the_two_supported_games` (`loot.rs:463`) | exact |

---

## Pattern Assignments

### 1. `my_games_path(prefix) -> PathBuf` + case-fold (`crates/steam/src/ce2.rs`, NEW)

**Path-tail analog:** `appdata_local_path` — `crates/loadorder/src/loot.rs:67` (a fixed
`.join()` chain off the prefix root). The new resolver mirrors this exactly but with the
`Documents/My Games/<folder>` tail instead of `AppData/Local/<folder>`:

```rust
// loot.rs:67  — the seam to mirror
pub fn appdata_local_path(prefix: &Path, game_name: &str) -> PathBuf {
    prefix.join("drive_c").join("users").join("steamuser")
        .join("AppData").join("Local").join(game_name)
}
// NEW my_games_path tail:  .join("Documents").join("My Games").join("Starfield")
```

**Case-fold primitive — CORRECTION to the task brief:** `casing.rs` is a `Data/`-tree
*walker* (`canonical_data_casing` builds a whole `CasingMap`), NOT a per-component matcher.
The reusable seam is **`entry_ci` in `crates/steam/src/resolve.rs:323`**, currently a
**private `fn`** — lift to `pub(crate)` (RESEARCH §module-placement note) and call it
per existing path component:

```rust
// resolve.rs:323  — deterministic case-insensitive child match (WR-07)
fn entry_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut matches: Vec<String> = rd.flatten()
        .filter_map(|entry| {
            let n = entry.file_name().to_str()?.to_owned();
            n.eq_ignore_ascii_case(name).then_some(n)
        }).collect();
    if matches.is_empty() { return None; }
    matches.sort();                                   // exact-case wins, else smallest variant
    let chosen = matches.iter().find(|n| n.as_str() == name).unwrap_or(&matches[0]);
    Some(dir.join(chosen))
}
```

**Replicate:** the `.join()` prefix-tail shape; `entry_ci`'s exact deterministic choice
(do NOT re-implement — RESEARCH "Don't Hand-Roll"). Use `read_dir`/`entry_ci` with
`follow_links(false)` posture (symlink-safety, ASVS V12).
**Change:** tail is `Documents/My Games/Starfield`; walk each *existing* component through
`entry_ci` to get the real cased path; on any missing component, return the canonical
expected path (for `FirstLaunchPending`). Add the defensive `user.reg` `"Personal"` read
with the default `.../steamuser/Documents` as the load-bearing fallback (Pitfall 3);
verify the mapped path stays under `<prefix>/drive_c` before use (ASVS V5 traversal guard).

---

### 2. AppID 1716740 allow-list arms (6 sibling `match appid` sites)

Every site is a 2-arm `match appid { … => …, _ => None }`. Add one Starfield arm to each.
`STARFIELD: u32 = 1716740` should be a `pub const` in `resolve.rs` alongside `SKYRIM_SE`/
`FALLOUT4` (and mirrored as a local `const` in each `loadorder` module, matching the
existing `SKYRIM_SE`/`FALLOUT4` mirror-const convention — see `loot.rs:54`, `scan.rs:35`,
`masterlist.rs:28`).

```rust
// resolve.rs:23   pub const SUPPORTED_APPIDS: &[u32] = &[SKYRIM_SE, FALLOUT4];   // + STARFIELD
// resolve.rs:26   fn default_name(appid)  SKYRIM_SE => "Skyrim Special Edition", FALLOUT4 => "Fallout 4"
// resolve.rs:267  fn expected_exe(appid)  SKYRIM_SE => "SkyrimSE.exe", FALLOUT4 => "Fallout4.exe"
// loot.rs:81      game_type_for(appid)    SKYRIM_SE => Some(GameType::SkyrimSE), FALLOUT4 => Some(GameType::Fallout4)
// loot.rs:93      appdata_folder_name     SKYRIM_SE => "Skyrim Special Edition", FALLOUT4 => "Fallout4"   [Phase 7 plugins.txt]
// scan.rs:44      game_id_for(appid)      SKYRIM_SE => Some(GameId::SkyrimSE), FALLOUT4 => Some(GameId::Fallout4)
// masterlist.rs:46 game_slug(appid)       SKYRIM_SE => Some("skyrimse"), FALLOUT4 => Some("fallout4")
```

**Replicate:** the exact 2-arm shape at each site.
**Change (Starfield arm values, all HIGH-confidence per RESEARCH):**
`SUPPORTED_APPIDS` += `STARFIELD`; `default_name => "Starfield"`;
`expected_exe => "Starfield.exe"`; `game_type_for => Some(GameType::Starfield)`;
`appdata_folder_name => Some("Starfield")` (plugins.txt lives in AppData/Local — Phase 7);
`game_id_for => Some(GameId::Starfield)`; `game_slug => Some("starfield")`.
Tauri arm: `src-tauri/src/commands/plugins.rs` + `resolve_data_dir` in `lib.rs` consume
these — no new arm, they inherit the allow-list once the crates expose it.

---

### 3. `buildid` read (`installed_build`) — extend the `AppManifest` seam (`resolve.rs`)

**Analog:** the existing `keyvalues-serde` ACF deserialize — `resolve.rs:152` + struct at
`resolve.rs:166`. Add one `Option` field; add a small reader mirroring the existing
`read_to_string` + `from_str` + `NotFound => NotInstalled` error mapping (`resolve.rs:141`):

```rust
// resolve.rs:152 / 166  — the seam
let app: AppManifest = keyvalues_serde::from_str(&raw)
    .map_err(|e| SteamError::Locate(e.to_string()))?;

#[derive(Debug, Deserialize)]
struct AppManifest {
    installdir: String,
    name: Option<String>,   // + buildid: Option<String>   (ACF stores it as a quoted number)
}
```

**Replicate:** the ACF read + `keyvalues-serde` parse + fail-safe error mapping.
**Change:** add `buildid: Option<String>`; new `installed_build(library_root, appid) -> Option<u64>`
parses it to `u64` (any parse/read failure → `None`, non-blocking per Pitfall 4). Keep the
`AppState`-root behaviour (keyvalues-serde deserializes the inner object directly).

---

### 4. `Ce2ConfigState` typed first-launch state (`ce2.rs`)

**Analog to DIVERGE FROM:** `LoadOrderError::NoLocalAppData(PathBuf)` — `crates/loadorder/src/error.rs:42`.
That is the existing "unresolved path" representation, but it is an **`Err` variant** —
returning it would abort the add-game flow. SFDET-02 requires the game stay *addable*
while blocking load-order/INI ops, so use a **typed enum return, not an error**:

```rust
// error.rs:42  — the existing "path not resolved" shape (an Err — do NOT reuse verbatim)
#[error("no local AppData path resolved for the Proton prefix: {0}")]
NoLocalAppData(PathBuf),

// NEW ce2.rs — typed state carrying the path instead of an Err:
pub enum Ce2ConfigState {
    Ready(PathBuf),               // dir exists & non-empty → real cased path
    FirstLaunchPending(PathBuf),  // absent/empty → canonical expected path for UI guidance
}
```

**Replicate:** the "carry the `PathBuf` in the variant" idea from `NoLocalAppData`.
**Change:** it is an owned success-enum, not a `thiserror` arm; `resolve_ce2_config(prefix)`
returns it. Tauri adapter forwards the variant + build numbers to the UI (thin, per the
engine-boundary rule — no logic in the command layer).

---

### 5. Bundled `assets/starfield/masterlist.yaml` + `VALIDATED_BUILD` (`masterlist.rs`)

**Analog:** the existing `include_str!` snapshot pattern — `crates/loadorder/src/masterlist.rs:42`
+ `bundled_snapshot` arm at `:55`. The whole fetch/cache/offline pipeline is already
appid-generic behind `game_slug` + `bundled_snapshot`:

```rust
// masterlist.rs:42
const SKYRIMSE_SNAPSHOT: &str = include_str!("../assets/skyrimse/masterlist.yaml");
const FALLOUT4_SNAPSHOT: &str = include_str!("../assets/fallout4/masterlist.yaml");
// masterlist.rs:55
fn bundled_snapshot(appid: u32) -> Option<&'static str> {
    match appid { SKYRIM_SE => Some(SKYRIMSE_SNAPSHOT), FALLOUT4 => Some(FALLOUT4_SNAPSHOT), _ => None }
}
```

**Replicate:** add `STARFIELD_SNAPSHOT = include_str!("../assets/starfield/masterlist.yaml")`
+ a `bundled_snapshot` Starfield arm. The asset file **must physically exist** or the crate
won't compile (Wave-0 gap). `MASTERLIST_BRANCH = "v0.29"` (`masterlist.rs:34`) already
matches `loot/starfield@v0.29` — no URL-builder change.
**Change:** `VALIDATED_BUILD` is a *new* concept with no exact analog — RESEARCH places it
as a sibling constant near the Starfield asset (a plain `pub const VALIDATED_BUILD: u64`).
Seed with `0` (suppresses the notice until Phase 9 sets the real build). It is NOT part of
the masterlist snapshot; treat it as a bundled constant, drift-compare only.

---

### 6. testkit My-Games fixture + case-mismatch + extended allow-list test

**Fixture analog:** `fake_proton_prefix` — `crates/testkit/src/lib.rs:79`:

```rust
// testkit/lib.rs:79 — builds <root>/drive_c/users/steamuser/AppData/Local/<game_name>
pub fn fake_proton_prefix(root: &Path, game_name: &str, plugins_txt: Option<&str>) -> io::Result<PathBuf> {
    let appdata_local = root.join("drive_c").join("users").join("steamuser")
        .join("AppData").join("Local").join(game_name);
    fs::create_dir_all(&appdata_local)?;
    if let Some(contents) = plugins_txt { fs::write(appdata_local.join("Plugins.txt"), contents)?; }
    Ok(root.to_path_buf())
}
```

**Replicate:** the builder signature/shape.
**Change:** add a My-Games variant building `.../Documents/My Games/<folder>` (with an
optional case-variant path e.g. `documents/my games/starfield`, an optional seeded file to
flip `FirstLaunchPending`→`Ready`, and an optional `user.reg`). Case-mismatch fixture is
**mandatory** (SFDET-02 success criterion 2).

**Allow-list test analog:** `game_type_for_allow_lists_only_the_two_supported_games` —
`crates/loadorder/src/loot.rs:463`:

```rust
// loot.rs:463
#[test]
fn game_type_for_allow_lists_only_the_two_supported_games() {
    assert!(matches!(game_type_for(SKYRIM_SE), Some(GameType::SkyrimSE)));
    assert!(matches!(game_type_for(FALLOUT4), Some(GameType::Fallout4)));
    assert!(game_type_for(0).is_none());
    assert!(game_type_for(220).is_none());
}
```

**Replicate:** the "asserts only supported games pass, junk appids return `None`" structure.
**Change:** extend to **three** games (add the Starfield assertion) at each of the extended
allow-list sites (`loot`, `scan`, `masterlist`, `resolve`). The `220` negative assertion
stays — the point is the allow-list still rejects everything else.

---

## Shared Patterns

### Case-insensitive path resolution (ASVS V12)
**Source:** `entry_ci` (`crates/steam/src/resolve.rs:323`, lift to `pub(crate)`).
**Apply to:** every existing component of the My-Games path walk. Do not re-implement —
its deterministic exact-case-wins-else-smallest choice (WR-07) is load-bearing for
reversibility. `follow_links(false)` posture matches `casing.rs`/`scan.rs`.

### Untrusted-input fail-safe (ASVS V5/V12)
**Source:** the `NotFound => NotInstalled` / `map_err(... Locate)` mapping (`resolve.rs:141-153`).
**Apply to:** the ACF `buildid` read AND the `user.reg` read — any parse failure/absence →
`None`/default fallback, never panic, never path-escape `<prefix>/drive_c` (Pitfall 3/4).

### Mirror-const convention
**Source:** `const SKYRIM_SE`/`FALLOUT4` duplicated per crate module (`loot.rs:54`,
`scan.rs:35`, `masterlist.rs:28`) mirroring `resolve::SKYRIM_SE`.
**Apply to:** add a mirrored `const STARFIELD` in each `loadorder` module rather than a
cross-crate import (matches existing convention).

### Engine boundary (CLAUDE.md hard rule)
All detection/path/drift logic stays headless in `crates/steam` + `crates/loadorder`.
`src-tauri/src/commands/{plugins,games}.rs` only forwards the typed `Ce2ConfigState` +
build numbers — no logic in the adapter.

## No Analog Found

| Artifact | Role | Reason | Planner guidance |
|----------|------|--------|------------------|
| `VALIDATED_BUILD` constant | config | No existing "validated-against-a-build" baseline in the codebase | New `pub const … u64`, sibling of the Starfield asset; seed `0` (suppresses notice until Phase 9). Drift-compare helper is a small exhaustive-case unit (`installed > validated => is_newer`, else `None`). |
| `user.reg` "Personal" redirect read | utility | No registry reader exists (libloot/libloadorder don't read it) | Defensive-only per RESEARCH A1; lenient parse, default `.../steamuser/Documents` is the load-bearing fallback. Confirm on live prefix in Phase 9. |

## Metadata

**Analog search scope:** `crates/steam/src/{resolve,casing}.rs`,
`crates/loadorder/src/{loot,scan,masterlist,error}.rs`, `crates/testkit/src/lib.rs`.
**Files scanned:** 7 (all exact/role-match; early-stopped — no broader search needed).
**Key correction vs task brief:** the reusable case-fold seam is `entry_ci` in
`resolve.rs:323` (private → make `pub(crate)`), NOT `casing.rs` (which is a `Data/`-tree
walker producing a `CasingMap`, not a per-component matcher) — RESEARCH §module-placement
already flagged this.
**Pattern extraction date:** 2026-07-07
