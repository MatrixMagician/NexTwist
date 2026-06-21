# Phase 4: Guided Installers & Collections - Research

**Researched:** 2026-06-21
**Domain:** FOMOD `ModuleConfig.xml` parsing + headless resolver, NexusMods Collection revision manifest parsing/resolve, pure-Rust XML, replaying FOMOD choices from a manifest, Collection-as-profile orchestration over the proven Phase 1-3 engine (Rust + Tauri 2.11, Linux/Proton)
**Confidence:** MEDIUM-HIGH (FOMOD schema HIGH — fetched authoritative XSD; Collection manifest MEDIUM-HIGH — fetched Vortex's own TypeScript interfaces; XML crate versions HIGH/registry-verified; the Collection *download/resolve API endpoint shape* MEDIUM — cross-checked against Nexus's GraphQL docs + node-nexus-api, not a live call this session)

## Summary

Phase 4 adds the two highest-complexity install features on top of the proven engine, and the research confirms the CONTEXT thesis precisely: **almost all of this phase is orchestration of existing primitives; the only genuinely new engine code is (1) a `crates/fomod` parser+resolver and (2) a Collection manifest parser+resolver.** Everything downstream — staging, conflict/priority, profiles, plugin load order, deploy, purge-to-pristine, the Nexus download client, the shared `governor` limiter — is reused verbatim. The two new crates produce data the existing engine already consumes: the FOMOD resolver emits a set of (source, destination) file installs that land in a `Data/`-rooted staging tree (exactly what `extract`/`conflict::resolve` expect), and the Collection resolver emits a pinned-mod list whose per-mod FOMOD `choices`, `rules`, and `load order` map onto the Phase-2 conflict-rank + plugin-order model.

The FOMOD unknown is **fully resolved**: I fetched the canonical schema (`GandaG/fomod-schema/ModuleConfig.xsd`, FOMOD 5.x) and enumerated the complete element tree — `moduleDependencies`, `requiredInstallFiles`, ordered `installSteps` → `optionalFileGroups` → `group` (5 selection types) → `plugin` (static `<type>` OR conditional `<dependencyType>` with `defaultType`+`patterns`), `conditionFlags`, per-plugin `files`, the `conditionalFileInstalls` pattern engine, and the composite `dependencies` operators (`And`/`Or`, nested, `fileDependency`/`flagDependency`/`gameDependency`/`fommDependency`). The 5-state plugin type enum (`Required`/`Optional`/`Recommended`/`NotUsable`/`CouldBeUsable`) and the `order` enum (`Ascending`/`Descending`/`Explicit`) are confirmed from the XSD. The Collection unknown is resolved against **Vortex's own `ICollection.ts`**: `collection.json` = `{ info, mods[], modRules[], collectionConfig? }`; each mod carries `source` (`type: nexus|bundle|direct|browse|manual`, `modId`, `fileId`, `md5`, `fileSize`), `version`, `optional`, `choices` (the FOMOD replay = `{type:"fomod", options: IChoices}`), `phase`, `fileOverrides`, `patches`; `modRules[]` are `{source, type: before|after|requires|conflicts, reference}` over an `IModReference` (matched by `tag`/`md5`/`logicalFileName`+`versionMatch`/`fileExpression`).

**Primary recommendation:** Build **`crates/fomod`** (headless, Tauri-free) with **`quick-xml` 0.40 + serde** (`serialize` feature) as the parser, splitting into `parse` (XSD → typed AST), `condition` (flag set + composite-dependency evaluator), and `resolve` (choices + flags → ordered `(source→destination, priority, alwaysInstall)` file-install plan with a **dry-run** that surfaces the plan before any staging write). Build the Collection resolver as a second concern (a module in `crates/nexus` or a thin `crates/collection` crate) that parses `collection.json`, resolves every pinned mod's availability against the Nexus API **before any download**, and emits a resolve report. Reuse `crates/fomod`'s resolver to replay each mod's `choices` headlessly. Map `modRules` (`after`/`before`) → mod `rank`; map `conflicts`/`fileOverrides` → the existing winner resolution. Persist the Collection + per-mod FOMOD choices in a new **V5** refinery migration. Deploy/uninstall = Phase-2 profile-switch / purge-to-pristine, regression-locked by the testkit blake3 pristine harness. **Close the carried Phase-2 archive-root-detection gap inside the new staging path** (see Pitfall 1) — FOMOD `destination` resolution and Collection installs make a wrapper-folder double-nest acute.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**FOMOD Engine & Parsing (FOMOD-01/02) — USER OVERRIDE: full spec**
- Implement the **FULL** FOMOD `ModuleConfig.xml` specification, not a subset: install steps + ordering, all 5 group types (`SelectExactlyOne`/`SelectAtMostOne`/`SelectAtLeastOne`/`SelectAll`/`SelectAny`), option type descriptors + conditional type states (`type` vs `typeDescriptor`/`dependencyType` — Optional/Required/Recommended/NotUsable/CouldBeUsable), flag set/conditions, the full `conditionalFileInstalls` pattern engine, and composite dependency operators (`And`/`Or`, nested, `fileDependency`/`flagDependency`/`gameDependency`/`fommDependency`). Genuinely malformed/unsupported constructs **fail with a clear, specific error** rather than silently mis-installing.
- The FOMOD engine lives in a **new headless `crates/fomod` crate** (Tauri-free, pure): parse `ModuleConfig.xml` → expose ordered steps/groups/options + visibility/type conditions → given user choices (+ accumulated flags), **resolve the concrete file-install plan**. Wizard UI lives in the shell; no FOMOD logic in the adapter.
- **XML parsing uses a pure-Rust crate** (`quick-xml` with serde, or `roxmltree`) — no native dep, rustls/AppImage-friendly, cargo-deny-clean. Exact crate confirmed by research.
- **Dry-run-resolve-then-apply is a HARD safety gate**: resolve the full file-install plan + surface conflicts **before touching staging**, then apply. Applying routes every file through the validated staging/extract path; the round-trip-pristine guarantee is untouched.

**FOMOD Wizard UX (FOMOD-01)**
- Step-by-step wizard, Back/Next, one install step per screen. `SelectExactlyOne`/`SelectAtMostOne` → radio; `SelectAny`/`SelectAtLeastOne`/`SelectAll` → checkbox, honoring min/max + required/notusable states.
- Each option shows its image + description from the staged archive; missing image degrades to text.
- Live conditional re-evaluation: choices set/unset flags → step + option visibility and type state re-evaluate live; resolved conditional file installs update.
- Re-installing the FOMOD installer **re-stages** with fresh choices; editing choices in place is deferred.

**Collections — Download & Resolve (COLL-01/02) — USER OVERRIDE: Premium-only**
- Parse the NexusMods **Collection revision manifest** (pinned mod list — game, mod id, file id, version — plus per-mod FOMOD choices, load order, rules, bundled config/patch files).
- **Resolve the FULL manifest first and report archived / unavailable / off-Nexus mods BEFORE any download or disk write** (success criterion 2). No partial disk mutation before the user accepts the resolution report.
- **Collections are Premium-only in v1.** Bulk in-app download requires a Premium account (the API direct-download path from Phase 3). A free user gets a clear "Collections require a NexusMods Premium account" notice — **no** per-mod free-user `nxm://` fallback this phase.
- Download orchestration **reuses the Phase-3 client + the single shared `governor` rate limiter** (WR-03), bounded concurrency, per-mod + overall progress. Off-Nexus deps are **detected + surfaced as required manual steps**, never auto-fetched.

**Collections — Apply, Deploy, Uninstall (COLL-03/04/05)**
- A Collection installs into its **own dedicated Phase-2 profile** — isolated, switchable, cleanly removable. Each pinned mod's **FOMOD choices are replayed headlessly from the manifest** through the `crates/fomod` resolver (no interactive wizard per mod).
- The Collection's rules **map onto the existing Phase-2 conflict/priority + plugin load-order model** — no new parallel rules engine. "X loads after Y" / explicit file overrides → mod rank + load order; the deterministic winner resolution from Phase 2 applies.
- **Deploying a Collection = activating its profile + deploy-winners + apply-load-order via the existing Phase-2 profile-switch path** — no new deploy primitive.
- **Uninstalling a Collection is fully reversible**: purge-to-pristine via the deploy engine + drop the profile + remove the Collection's staged mods, leaving the game byte-for-byte vanilla (reuse the Phase-1/2 guarantee + testkit pristine harness).

### Claude's Discretion
- Exact `crates/fomod` module split, the chosen XML crate, the precise V5 (or later) store schema for Collection + FOMOD-choice persistence, the manifest/bundle/patch parsing details, and the wizard component structure — settled by this research (FOMOD spec corpus + a real Collection revision).

### Deferred Ideas (OUT OF SCOPE)
- Free-user (non-Premium) bulk Collection download (per-mod `nxm://` orchestration).
- Edit-FOMOD-choices-in-place on an already-staged mod (v1 re-installs).
- Collection authoring/publishing (COLLV2-01).
- Auto-fetching off-Nexus dependencies (detect + surface only).
- Mod-update / Collection-revision-update tracking (NEXV2-01).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| FOMOD-01 | User can install mods with FOMOD scripted installers, making option choices through a guided UI | Full `ModuleConfig.xml` schema enumerated from the canonical XSD (*FOMOD Schema Reference*); `crates/fomod` `parse`→`condition`→`resolve` split (*Pattern 1/2/3*); the resolver exposes ordered steps/groups/options + live conditions for the wizard (*Architecture*). |
| FOMOD-02 | FOMOD conditional/option-driven file installation is applied correctly to staging | The condition engine (composite `And`/`Or` deps, flags, `conditionalFileInstalls` patterns) + the dry-run file-install plan resolved BEFORE any staging write (*Pattern 2/3*, *Pitfall 2*); plan feeds the existing `extract`+`conflict` path. |
| COLL-01 | User can browse and select a NexusMods Collection for a managed game | `collectionRevision(slug, revision, domainName)` GraphQL query returns the collection archive `downloadLink`; `domainName` gated by the Phase-3 `appid_for_domain` map (*Pattern 4*, *Collection Manifest Reference*). |
| COLL-02 | NexTwist downloads all mods in a Collection revision per its manifest | Parse `collection.json` (`mods[].source.{modId,fileId,md5}`), resolve availability per mod via the Phase-3 client BEFORE download, then bulk-download the available set reusing `NexusClient::download` + the shared `governor` limiter (*Pattern 4/5*, *Pitfall 4*). |
| COLL-03 | NexTwist applies the Collection's FOMOD choices, load order, and rules automatically | `mods[].choices` = `{type:"fomod", options: IChoices}` replayed headlessly through `crates/fomod::resolve` (*Pattern 6*); `modRules[]` (`after`/`before`/`conflicts`) mapped → mod `rank` + plugin load order (*Pattern 7*). |
| COLL-04 | User can deploy an installed Collection so the modded game launches | Collection = a Phase-2 `Profile`; deploy = `deploy::switch_profile` (purge→deploy_winners→apply_load_order→set_active) verbatim — no new primitive (*Pattern 8*). |
| COLL-05 | User can cleanly uninstall a Collection (fully reversible) | Uninstall = `deploy::purge` (purge-to-pristine) + `store::delete_profile` + remove staged mods; asserted by the testkit blake3 pristine harness (*Pattern 9*, *Validation Architecture*). |
</phase_requirements>

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Parse `ModuleConfig.xml` → typed AST | API/Backend (`crates/fomod`) | — | Pure XML transform; no OS/Tauri dep. Unit-testable against a fixture corpus headless. |
| FOMOD condition/flag evaluation + file-install resolve (dry-run) | API/Backend (`crates/fomod`) | — | Pure logic over the AST + a choice set; the safety-critical "resolve before touching disk" gate. |
| FOMOD wizard rendering + live re-evaluation | Frontend (Svelte shell) | API/Backend (calls resolver per choice) | UI-SPEC §A — the wizard is presentation; it calls the headless resolver to get current visibility/type states. No FOMOD logic in the adapter. |
| Apply resolved file-install plan → staging | API/Backend (`crates/extract`, extended) | — | The plan lands files in a `Data/`-rooted staging tree via the validated extract path (zip-slip defense unchanged). |
| Parse `collection.json` + resolve mod availability | API/Backend (`crates/nexus` or `crates/collection`) | — | Pure JSON + API metadata reads; the "resolve before download" gate. |
| Bulk Collection download (Premium) | API/Backend (`crates/nexus`) | Shell (re-emit per-mod + overall progress events) | Reuses `NexusClient::download` + shared `governor`; progress→Tauri event conversion is the shell's job. |
| Replay FOMOD choices headlessly | API/Backend (`crates/fomod`) | — | The Collection install path calls the SAME resolver with manifest-supplied choices (no wizard). |
| Map Collection rules → rank + load order | API/Backend (`crates/store` + `crates/loadorder`) | — | Reuses the Phase-2 `rank`/`profile_mod` model + `apply_load_order`; no new rules engine. |
| Collection deploy / uninstall | API/Backend (`crates/deploy`) | — | `switch_profile` / `purge` verbatim — the safe engine is never bypassed. |
| Collection + FOMOD-choice persistence | Database (`crates/store`, V5 migration) | — | Additive migration; no `rusqlite` in the public API (existing invariant). |
| Premium-tier gate | Shell (`src-tauri` commands) | API/Backend (`UserInfo.is_premium` from `crates/nexus`) | The shell reads the cached tier and shows the notice; the tier value comes from the headless client. |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `quick-xml` | `0.40.1` | Parse `ModuleConfig.xml` (+ `info.xml`) into a typed AST | Highest-download pure-Rust XML crate (~5.7M/wk), MIT, no native dep. The `serialize` feature gives serde `Deserialize` so the XSD maps to Rust structs/enums directly. Handles BOM, namespace-ignorant local-name reads, and entity decoding — the FOMOD quirks. `[VERIFIED: crates.io 0.40.1, MIT]` |
| `serde` | `1` (workspace) | Derive `Deserialize` for the FOMOD AST + (de)serialize `collection.json` + persisted choices | Already pinned; pairs with quick-xml's `serialize` feature and serde_json for the manifest. `[VERIFIED: workspace Cargo.toml]` |
| `serde_json` | `1` (workspace) | Parse `collection.json` revision manifest | Already pinned; the manifest is plain JSON. `[VERIFIED: workspace Cargo.toml]` |
| `nextwist-nexus` (existing) | — | Collection download + mod-availability metadata reads + shared `governor` limiter | Phase-3 crate reused verbatim: `NexusClient::download`, the REST-v1 file-info/download-link path, `Arc<RateLimiter>` from `AppState`. `[VERIFIED: crates/nexus/src/lib.rs]` |
| `nextwist-extract` (existing, extended) | — | Land the resolved FOMOD file-install plan + Collection mods into staging | The validated extract→staging pipeline; extended with archive-root-detection (Pitfall 1) + FOMOD-plan application. `[VERIFIED: crates/extract/src/staging.rs]` |
| `nextwist-deploy` / `nextwist-store` / `nextwist-loadorder` / `nextwist-testkit` (existing) | — | Profile switch deploy, purge-to-pristine, conflict rank, plugin load order, pristine assertion | All reused; Collection = profile, deploy = `switch_profile`, uninstall = `purge`. `[VERIFIED: crates/deploy/src/profile.rs, crates/store/src/profiles.rs]` |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `thiserror` | `2` (workspace) | `crates/fomod` (+ Collection) error enums (`anyhow` only at the shell boundary) | Existing convention; `FomodError` with specific malformed-construct variants (the "clear specific error" requirement). `[VERIFIED: workspace]` |
| `tracing` | `0.1` (workspace) | Structured logging of parse/resolve/download steps | Existing convention. `[VERIFIED: workspace]` |
| `walkdir` | `2.5` (workspace) | Case-insensitive lookup of the `fomod/` folder + source-path resolution inside the staged archive | Already pinned; used for the case-folding match (Pitfall 3). `[VERIFIED: workspace]` |
| `tempfile` (existing in extract) | — | Dry-run / extract-to-temp before the validated move into staging | Already used by `install_archive`. `[VERIFIED: crates/extract]` |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `quick-xml` 0.40 + serde | `roxmltree` 0.21 (DOM tree) | `roxmltree` is a read-only DOM with great ergonomics for ad-hoc traversal and is also MIT/legit (~1M/wk). For FOMOD's **deeply nested, optional, attribute-heavy** schema with `xsi:noNamespaceSchemaLocation` noise, **serde-deriving the XSD into typed structs (quick-xml) is less error-prone** than hand-walking a DOM — the type system enforces the schema. Pick `roxmltree` only if the team prefers manual traversal or hits a quick-xml serde edge case with mixed content. Both pass cargo-deny. `[VERIFIED: crates.io roxmltree 0.21.1]` |
| New `crates/collection` crate | A `collection` module inside `crates/nexus` | The Collection resolver is mostly Nexus-API-bound (availability reads, download-link gen) so it can live in `crates/nexus`; a separate crate is cleaner if the FOMOD-replay coupling grows. **Claude's discretion** — either keeps the engine headless. `[ASSUMED]` |
| `quick-xml` `serialize` feature | `quick-xml` raw reader (event-based) | Event-based is faster but far more code for a config-sized file; serde is the right tradeoff for correctness over a one-shot parse. `[VERIFIED: quick-xml docs — serialize feature]` |

**Installation (additions to root `[workspace.dependencies]` + the new crate):**
```toml
# root [workspace.dependencies]
quick-xml = { version = "0.40", features = ["serialize"] }

# crates/fomod/Cargo.toml
quick-xml.workspace = true
serde.workspace = true
thiserror.workspace = true
tracing.workspace = true
walkdir.workspace = true
nextwist-core = { workspace = true }      # speaks core types in/out
# dev:
nextwist-testkit = { workspace = true }   # fixture corpus + pristine assertions

# crates/nexus (or crates/collection) — collection.json parsing reuses existing deps:
serde_json.workspace = true               # already present
```
> `quick-xml`'s `serialize` feature pulls only `serde` (already in the graph) — **no native/C dependency, no OpenSSL**, so the AppImage rustls-only rule and `cargo-deny` licenses/sources gates are unaffected (MIT). `[VERIFIED: crates.io feature list]`

**Version verification:** `quick-xml` 0.40.1 (MIT, ~5.69M weekly downloads, repo `tafia/quick-xml`) and `roxmltree` 0.21.1 (MIT, ~1.03M weekly, repo `RazrFalcon/roxmltree`) verified against crates.io on 2026-06-21 via the legitimacy seam.

## Package Legitimacy Audit

> Verified via `gsd-tools query package-legitimacy check --ecosystem crates quick-xml roxmltree` on 2026-06-21.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| `quick-xml` | crates | since 2016-02 | ~5.69M/wk | github.com/tafia/quick-xml | OK | **Approved** (the recommended parser) |
| `roxmltree` | crates | since 2018-08 | ~1.03M/wk | github.com/RazrFalcon/roxmltree | OK | Approved (alternative; only one XML crate is added) |

**Packages removed due to [SLOP] verdict:** none.
**Packages flagged as suspicious [SUS]:** none.
**Provenance note:** `quick-xml` was named in the CONTEXT locked decision (an authoritative project source) AND returns `OK` from the legitimacy seam → `[VERIFIED: crates.io]`. The only NEW crate this phase adds is the chosen XML parser; every other dependency (`nexus`, `extract`, `deploy`, `store`, `loadorder`, `serde`, `serde_json`, `walkdir`, `thiserror`, `tracing`, `testkit`) is already in the workspace graph and previously audited. No `checkpoint:human-verify` install gate is required; the planner SHOULD confirm the **`serialize` feature** is enabled (quick-xml's serde support is feature-gated).

## Architecture Patterns

### System Architecture Diagram

```
  ┌──────────────────────── FOMOD INSTALL (FOMOD-01/02) ────────────────────────┐
  │                                                                              │
  archive w/ fomod/ModuleConfig.xml                                             │
        │ install                                                               │
        ▼                                                                       │
  extract → temp tree (validated, zip-slip defense)  ◀── crates/extract (reused)│
        │                                                                       │
        ▼  locate fomod/ case-insensitively                                     │
  crates/fomod::parse(ModuleConfig.xml)  ──quick-xml+serde──▶ typed AST         │
        │                                                                       │
        ▼                                                                       │
  ┌── WIZARD LOOP (shell, UI-SPEC §A) ───────────────────────────────┐         │
  │  render step → user picks options → set flags                    │         │
  │      │                                                           │         │
  │      ▼  per choice                                               │         │
  │  fomod::condition::evaluate(flags) ──▶ step visibility +         │         │
  │      option type-states (Required/NotUsable/…) + visible steps   │         │
  └──────────────────────────────────────────────────────────────────┘         │
        │ user presses Install                                                  │
        ▼  DRY-RUN (HARD GATE)                                                  │
  fomod::resolve(choices, flags) ──▶ ordered Vec<FileInstall {                  │
        src_in_archive, dest_rel, priority, always_install }>                   │
        │  + conflict preview (which src lands where; blocking?)                │
        ▼ on accept                                                            │
  apply plan → Data/-rooted staging tree  ──▶ crates/store ManagedMod ──▶ deploy│
        (root-detection applied here — Pitfall 1)                              │
  └──────────────────────────────────────────────────────────────────────────┘

  ┌──────────────────────── COLLECTION (COLL-01..05) ───────────────────────────┐
  │  collectionRevision(slug,revision,domainName) GraphQL ──▶ collection archive │
  │        │ download + extract                                                  │
  │        ▼                                                                     │
  │  parse collection.json  ──serde_json──▶ { info, mods[], modRules[] }         │
  │        │                                                                     │
  │        ▼  RESOLVE (HARD GATE — before any mod download)                      │
  │  for each mod: classify source.type + availability via Phase-3 client        │
  │     nexus → check file exists (available/archived/unavailable)               │
  │     bundle → bundled in the collection archive                               │
  │     direct/browse/manual → OFF-NEXUS → "manual step required" (never fetch)  │
  │        │                                                                     │
  │        ▼ resolve report (UI-SPEC §B) → user accepts                          │
  │  bulk download available set  ──NexusClient::download + shared governor──────┤
  │        │  (Premium-only; per-mod + overall progress)                         │
  │        ▼ per mod                                                             │
  │  stage → if mod.choices(fomod) → fomod::resolve(choices) headless (COLL-03)  │
  │        │                                                                     │
  │        ▼                                                                     │
  │  create dedicated Profile; set profile_mod ranks from modRules (after/before)│
  │  apply patches/fileOverrides; record manual steps                           │
  │        │                                                                     │
  │        ▼ deploy = switch_profile (purge→deploy_winners→load_order→active)    │
  │  uninstall = purge-to-pristine + delete_profile + remove staged mods         │
  │             (testkit blake3 pristine assertion — COLL-05)                    │
  └──────────────────────────────────────────────────────────────────────────┘
```

### Recommended Project Structure
```
crates/fomod/
├── src/
│   ├── lib.rs        # public API: parse_module_config, FomodModule, resolve, FileInstall; thiserror FomodError
│   ├── error.rs      # FomodError — Xml(parse), MalformedSchema(specific construct), MissingSource, …
│   ├── model.rs      # the typed AST: FomodModule, InstallStep, Group(GroupType), Plugin, TypeDescriptor,
│   │                 #   PluginType, Dependency(operator), FileItem, Pattern — serde-derived from the XSD
│   ├── parse.rs      # quick-xml+serde: locate fomod/ case-insensitively, deserialize ModuleConfig.xml
│   ├── condition.rs  # flag set + composite-dependency evaluator; step visibility; live plugin type-state
│   └── resolve.rs    # (choices, flags) → ordered Vec<FileInstall>; conditionalFileInstalls + requiredInstallFiles;
│                     #   priority/alwaysInstall/installIfUsable ordering; the DRY-RUN entry point
└── tests/
    └── corpus.rs     # fixture ModuleConfig.xml samples (simple, flags, conditional, nested deps, malformed)

crates/nexus/src/        (or a new crates/collection/)
├── collection.rs    # parse collection.json (serde_json) → Collection { info, mods, mod_rules }
├── resolve.rs       # per-mod availability resolution → ResolveReport (available/archived/unavailable/manual)
└── (download reused) # NexusClient::download for the bulk path

crates/store/src/migrations/
└── V5__collections.sql   # collection + collection_mod + fomod_choice tables (additive). NOTE: V5, V4 exists.

src-tauri/src/commands/
├── fomod.rs         # parse_fomod, resolve_fomod (dry-run), apply_fomod — thin adapters
└── collections.rs   # resolve_collection, download_collection, deploy/uninstall — thin adapters

frontend/src/        # FOMOD wizard view + Collections browse/resolve/progress view (UI-SPEC §A/§B/§C)
```

### FOMOD Schema Reference (the full spec — authoritative, from `GandaG/fomod-schema/ModuleConfig.xsd`, FOMOD 5.x)

Root `<config>` (type `moduleConfiguration`), children **in this order**:
1. `moduleName` (`moduleTitle`: text + `position` Left/Right/RightOfImage, `colour` hex)
2. `moduleImage` (`headerImage`: `path`, `showImage`, `showFade`, `height`) — optional
3. `moduleDependencies` (`compositeDependency`) — optional; gate whether the module is installable at all
4. `requiredInstallFiles` (`fileList`) — optional; files installed **unconditionally**
5. `installSteps` (`stepList`, `order` = Ascending|Descending|Explicit) — optional; the wizard pages
6. `conditionalFileInstalls` (`conditionalFileInstallList`) — optional; the post-choice pattern engine

**`installStep`** (in `stepList`): `name` (req), `visible` (`compositeDependency`, optional — step shown only if deps hold), `optionalFileGroups` (`groupList`, req, `order`).

**`group`** (in `groupList`): `name` (req), **`type` (req)** ∈ `{SelectExactlyOne, SelectAtMostOne, SelectAtLeastOne, SelectAll, SelectAny}`, `plugins` (`pluginList`, `order`).

**`plugin`** (in `pluginList`): `name` (req attr), `description` (text), `image` (optional `path`), `files` (`fileList`, optional), `conditionFlags` (`conditionFlagList`, optional — flags this option SETS when selected), **`typeDescriptor`** (req).

**`typeDescriptor`** is EITHER:
- `<type name="…">` — a **static** plugin type, OR
- `<dependencyType>` — a **conditional** type: `<defaultType name="…">` + `<patterns>` (each `<pattern>` = `<dependencies>` + `<type name="…">`); the first pattern whose dependencies hold sets the plugin's type live.

**`pluginType` enum:** `Required` (pre-selected, locked) · `Optional` (free) · `Recommended` (pre-selected, unlockable) · `NotUsable` (disabled) · `CouldBeUsable` (selectable but warns). **(All 5 confirmed in the XSD.)**

**`fileList`** = any number of `<file>` and `<folder>` (`fileSystemItem`): `source` (req — path inside the archive), `destination` (optional — **absent ⇒ install to the Data root**), `priority` (int, default 0 — higher wins when two installs target the same dest), `alwaysInstall` (bool, default false), `installIfUsable` (bool, default false — install when the owning option is not NotUsable even if unselected).

**`conditionFlags`** = `<flag name="…">value</flag>` set by a selected option.

**`conditionalFileInstalls`** → `<patterns>` → `<pattern>` = `<dependencies>` + `<files>` (`fileList`): after the wizard, **every** pattern whose `dependencies` (matched against the accumulated flags + installed files) hold contributes its files.

**`compositeDependency`** (`<dependencies operator="And|Or">`, default `And`): any mix of
- `fileDependency` (`file`, `state` ∈ `{Missing, Inactive, Active}`)
- `flagDependency` (`flag`, `value`)
- `gameDependency` (`version`) / `fommDependency` (`version`) (`versionDependency`)
- nested `dependencies` (`compositeDependency`) — recursion.

### Collection Manifest Reference (authoritative — from Vortex `extensions/collections/src/types/ICollection.ts`)

`collection.json` (inside the collection archive returned by `collectionRevision.downloadLink`):
```ts
ICollection = { info: ICollectionInfo; mods: ICollectionMod[]; modRules: ICollectionModRule[]; collectionConfig?: … }

ICollectionInfo  = { author, authorUrl, name, description, installInstructions, domainName, gameVersions? }

ICollectionMod   = {
  name; version; optional: boolean; domainName;
  source: ICollectionSourceInfo;
  hashes?;                       // per-file md5 list (file-matching)
  choices?;                      // FOMOD replay: { type:"fomod", options: IChoices }   ← COLL-03
  patches?: { [filePath:string]: string };   // binary patches keyed by file path
  instructions?;                 // manual instructions shown to the user
  phase?: number;                // install ordering phase (0-based)
  fileOverrides?: string[];      // file paths this mod force-wins
}

ICollectionSourceInfo = {
  type: "browse" | "manual" | "direct" | "nexus" | "bundle";
  modId?: number; fileId?: number;            // nexus source identity
  md5?: string; fileSize?: number;
  url?; instructions?; updatePolicy?: "exact"|"latest"|"prefer";
  logicalFilename?; fileExpression?; tag?;
}

ICollectionModRule = { source: IModReference; type: "before"|"after"|"requires"|"conflicts"|"recommends"|"provides"; reference: IModReference }

IModReference (matching predicates) = { tag?; md5Hint?; idHint?; archiveId?;
  fileExpression?; logicalFileName?; versionMatch?; fileMD5?;
  repo?: { gameId; modId; fileId } }   // a rule's source/reference identifies a mod by tag, md5, logicalFileName+versionMatch, or repo modId/fileId
```

**The FOMOD replay encoding (`IChoices`, authoritative):**
```ts
choices = { type: "fomod", options: [ { name: <stepName>,
   groups: [ { name: <groupName>,
      choices: [ { name: <optionName>, idx: <number> } ] } ] } ] }
```
i.e. an ordered list of **steps by name**, each with **groups by name**, each with the **chosen options by name+index**. `crates/fomod::resolve` is driven directly by this (match step→group→option by name; the headless path skips the wizard).

**Source-type → resolve classification (COLL-02 resolve report):**
| `source.type` | Meaning | Resolve report status |
|---------------|---------|------------------------|
| `nexus` | Pinned Nexus file (`modId`+`fileId`) | check file exists → **Available** / **Archived** (file archived) / **Unavailable** (removed) |
| `bundle` | File bundled inside the collection archive itself | **Available** (no download) |
| `direct` / `browse` / `manual` | Off-Nexus / externally hosted (script extenders, etc.) | **Manual step required** — never auto-fetched |

### Pattern 1: Parse `ModuleConfig.xml` with quick-xml + serde
**What:** Locate `fomod/ModuleConfig.xml` case-insensitively in the extracted tree, then deserialize it into a typed AST.
**When:** First step of any FOMOD install (FOMOD-01).
```rust
// crates/fomod/src/parse.rs — quick-xml 0.40 serde
use quick_xml::de::from_str;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename = "config")]
pub struct FomodModule {
    #[serde(rename = "moduleName")] pub module_name: ModuleTitle,
    #[serde(rename = "moduleDependencies", default)] pub module_deps: Option<CompositeDependency>,
    #[serde(rename = "requiredInstallFiles", default)] pub required: Option<FileList>,
    #[serde(rename = "installSteps", default)] pub steps: Option<StepList>,
    #[serde(rename = "conditionalFileInstalls", default)] pub conditional: Option<ConditionalFileInstalls>,
}
// Locate the file case-insensitively (Pitfall 3): walk for a dir == "fomod" (any case)
// then a file == "moduleconfig.xml" (any case); strip a UTF-8 BOM before from_str.
let xml = read_to_string_strip_bom(&module_config_path)?;
let module: FomodModule = from_str(&xml).map_err(|e| FomodError::Xml(e.to_string()))?;
```
`[VERIFIED: GandaG/fomod-schema ModuleConfig.xsd element tree; CITED: docs.rs/quick-xml de]`

### Pattern 2: Condition / flag evaluation (live + dry-run)
**What:** Evaluate a `compositeDependency` against the accumulated flag set (+ installed-file state) — drives step `visible`, live plugin type-state, and `conditionalFileInstalls` selection.
**When:** Every wizard choice (live, FOMOD-01) AND the final resolve (FOMOD-02).
```rust
pub fn eval(dep: &CompositeDependency, flags: &FlagSet, files: &InstalledFiles) -> bool {
    let results = dep.items.iter().map(|d| match d {
        Dependency::Flag { flag, value } => flags.get(flag).map_or(false, |v| v == value),
        Dependency::File { file, state }  => files.state(file) == *state, // Active/Inactive/Missing
        Dependency::Game { version }      => game_version_satisfies(version),
        Dependency::Nested(inner)         => eval(inner, flags, files),
    });
    match dep.operator { Op::And => results.all(|r| r), Op::Or => results.any(|r| r) }
}
```
The plugin type-state is resolved by walking `dependencyType.patterns` in order and taking the first whose `dependencies` hold, else `defaultType`. `[VERIFIED: XSD compositeDependency + dependencyPluginType]`

### Pattern 3: Resolve the file-install plan (the DRY-RUN gate, FOMOD-02)
**What:** Given the user's selected options + accumulated flags, produce the ordered concrete `(source-in-archive → destination-rel, priority, alwaysInstall)` list — **without writing anything** — so the UI can preview conflicts (UI-SPEC §A.6) before apply.
**When:** When the user presses Install (and headlessly during a Collection install, Pattern 6).
```rust
pub struct FileInstall { pub src: PathBuf, pub dest_rel: PathBuf, pub priority: i32, pub always: bool }

pub fn resolve(module: &FomodModule, sel: &Selection) -> Result<Vec<FileInstall>, FomodError> {
    let mut plan = Vec::new();
    // 1. requiredInstallFiles — unconditional
    // 2. files from each SELECTED plugin (or alwaysInstall, or installIfUsable && !NotUsable)
    // 3. conditionalFileInstalls — every pattern whose dependencies hold (sel.flags)
    // destination absent ⇒ Data root; a <folder> expands to its tree.
    // sort by (dest_rel, priority desc) so the highest-priority src wins a dest (dedup to one).
    Ok(dedup_by_priority(plan))
}
```
The plan is then applied through the validated extract→staging move (NOT a new write path) and into the Phase-2 `conflict::resolve` for cross-mod conflicts. `[VERIFIED: XSD fileSystemItem priority/alwaysInstall/installIfUsable; VERIFIED: crates/deploy/src/conflict.rs expects Data/-rooted trees]`

### Pattern 4: Resolve a Collection BEFORE downloading (COLL-01/02 — the hard gate)
**What:** Fetch the collection archive (`collectionRevision(slug,revision,domainName).downloadLink`), parse `collection.json`, then classify every `mod` by `source.type` + Nexus availability — emitting a report — **before any mod download or staging write**.
```rust
pub enum ModStatus { Available, Archived, Unavailable, Manual }  // UI-SPEC §B.3
pub struct ResolvedMod { pub name: String, pub version: String, pub status: ModStatus }
// for source.type == "nexus": reuse the Phase-3 client's file-info read to confirm the
// pinned fileId still exists / is not archived (NexusClient REST-v1 file-info path).
// bundle ⇒ Available (in-archive). direct/browse/manual ⇒ Manual (never fetched).
```
No partial disk mutation occurs until the user accepts the report. `[CITED: graphql.nexusmods.com collectionRevision; VERIFIED: Vortex ICollectionSourceInfo source types; VERIFIED: crates/nexus client REST-v1 file-info]`

### Pattern 5: Bulk download reusing the Phase-3 client + shared limiter (COLL-02)
**What:** Download the **available** set with bounded concurrency, the single shared `governor` limiter, per-mod + overall progress.
```rust
// Reuse verbatim: AppState's Arc<RateLimiter> + NexusClient::with_limiter(...).download(uri, dest, cancel, on_progress)
// Bound concurrency with a small semaphore (e.g. 2-3) so the shared limiter governs the global rate (WR-03).
// Premium gate: read UserInfo.is_premium first; non-Premium ⇒ show the notice, do NOT start (no nxm:// fallback).
```
`[VERIFIED: crates/nexus/src/client.rs with_limiter + download; VERIFIED: locked decision Premium-only]`

### Pattern 6: Replay FOMOD choices headlessly from the manifest (COLL-03)
**What:** For a Collection mod with `choices.type == "fomod"`, drive `crates/fomod::resolve` from the manifest's `IChoices` (step→group→option by name) instead of the interactive wizard.
```rust
// Convert IChoices → fomod::Selection by matching names against the parsed FomodModule,
// accumulating the flags each chosen option sets, then call the SAME resolve() (Pattern 3).
// A name that no longer matches the mod's current ModuleConfig.xml ⇒ a specific, surfaced error
// (the mod was updated since the collection pinned it) — never a silent mis-install.
```
`[VERIFIED: Vortex IChoices encoding {name,groups[{name,choices[{name,idx}]}]}]`

### Pattern 7: Map Collection rules → rank + load order (COLL-03 — no new engine)
**What:** Translate `modRules[]` into the Phase-2 `rank` model + plugin load order; reuse the deterministic winner resolution.
- `type: "after"` (source loads after reference) ⇒ source gets a **higher rank number** (lower priority) than reference for file conflicts; plugin order places source's plugins later.
- `type: "before"` ⇒ inverse.
- `type: "conflicts"` ⇒ surfaced in the existing conflict view; winner decided by the resulting rank.
- `fileOverrides[]` ⇒ the mod force-wins those `dest_rel` paths (a per-file rank override consumed by `conflict::resolve`).
Then `loadorder::apply_load_order` writes `plugins.txt` exactly as Phase 2 does. `[VERIFIED: Vortex ICollectionModRule types; VERIFIED: crates/deploy/src/conflict.rs rank model; VERIFIED: crates/loadorder apply_load_order]`

### Pattern 8: Deploy a Collection = profile switch (COLL-04 — no new primitive)
```rust
// 1. create_profile(appid, collection_name) → profile_id
// 2. for each staged collection mod: store.set_profile_mod(profile_id, mod_id, rank_from_rules, enabled)
// 3. deploy::switch_profile(store, game, profile_id)  // purge→deploy_winners→apply_load_order→set_active
```
`[VERIFIED: crates/store/src/profiles.rs create_profile/set_profile_mod; crates/deploy/src/profile.rs switch_profile]`

### Pattern 9: Reversible Collection uninstall (COLL-05)
```rust
// 1. deploy::purge(store, game)               // purge-to-pristine (if the collection profile is active)
// 2. store.delete_profile(profile_id)         // drops profile + profile_mod rows
// 3. remove the collection's staged mod trees + their managed_mod + V5 collection rows
// Regression: testkit snapshot(game) BEFORE install == snapshot(game) AFTER uninstall (blake3 + DIR_SENTINEL).
```
`[VERIFIED: crates/deploy engine purge; crates/store delete_profile; crates/testkit snapshot_tree/assert_trees_identical]`

### Anti-Patterns to Avoid
- **Putting any FOMOD/Collection logic in the Tauri command adapters.** The adapters lock `AppState` and call the headless crates; the wizard re-evaluation calls `fomod::condition`.
- **Building a parallel Collection rules engine.** Rules map onto the EXISTING rank + load-order model (Pattern 7).
- **A new deploy/purge primitive for Collections.** Collections deploy via `switch_profile` and uninstall via `purge` — the safe engine is never bypassed.
- **Writing to staging before the dry-run/resolve report.** Both gates (FOMOD dry-run, Collection resolve) must complete and be accepted first.
- **Trusting `destination` / `source` paths case-sensitively.** Proton/Wine + author-authored XML are case-inconsistent; resolve source paths into the staged tree case-insensitively (Pitfall 3) and normalize destinations through the existing DEPLOY-08 casefold.
- **Silently dropping an unsupported XML construct.** Malformed/unknown ⇒ a specific `FomodError` (the locked "fail clearly, never mis-install").
- **Auto-fetching `direct`/`browse`/`manual` Collection mods.** Off-Nexus deps are surfaced as manual steps only.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| XML parsing | A hand-rolled tokenizer for `ModuleConfig.xml` | `quick-xml` 0.40 + serde (`serialize`) | BOM, entities, namespace-ignorant local names, attribute decoding — all solved; the XSD maps to typed structs. |
| Conflict winner resolution | A new priority engine for FOMOD/Collection | `crates/deploy::conflict::resolve` (rank-based) | The deduped-winner-per-path fold is already UNIQUE-safe + path-escape-guarded. |
| Deploy / purge for Collections | A bespoke Collection deployer | `deploy::switch_profile` / `deploy::purge` | Journaled, crash-safe, byte-for-byte pristine — the whole safety guarantee. |
| Plugin load order from rules | A custom plugins.txt writer | `loadorder::apply_load_order` | Asterisk-format round-trip at the Proton AppData path is already correct. |
| Collection mod download | A new HTTP/download path | `NexusClient::download` + shared `governor` | Streaming, rustls, rate-limit-safe, cancel-aware — Phase 3 proved it. |
| Pristine assertion | A new tree-diff for the uninstall test | `testkit::snapshot_tree` + `assert_trees_identical` | blake3 + DIR_SENTINEL catches orphan empty dirs too. |
| Archive→staging | A FOMOD-specific extractor | `extract::install_archive` + the validated move | zip-slip/symlink defense + read-only locking already enforced. |

**Key insight:** The only new *engine* code is two pure parser+resolvers (`crates/fomod`, the Collection resolver). Every write path, conflict decision, deploy, purge, download, and load-order write is an existing, safety-reviewed primitive. The phase's real work is **schema fidelity + the two dry-run gates + correct rule→rank mapping**, not new filesystem machinery.

## Common Pitfalls

### Pitfall 1: Archive root-detection — the carried Phase-2 gap, made acute by FOMOD destinations
**What goes wrong:** Many mod archives wrap their payload in a single top-level folder (`MyMod/Data/foo.esp` instead of `Data/foo.esp`). `extract::install_archive` lists files relative to the staging root **verbatim** — there is **no** wrapper-folder flattening (confirmed: no `flatten`/`root_detect` logic exists in `crates/extract`). The result deploys to `Data/MyMod/Data/foo.esp` — double-nested, game can't see it. FOMOD `<folder source="…">` paths and Collection installs reference archive-internal paths, so a mis-detected root corrupts the FOMOD `source`→staged-tree resolution too.
**Why it happens:** Authors package inconsistently; the wrapper folder is cosmetic but indistinguishable from a real `Data/`-sibling without a heuristic.
**How to avoid:** Add root-detection to the staging path (or a pre-pass): if the extracted tree's top level is a **single directory** that itself contains the recognizable game root (`Data/`, or known top-level game files like `SKSE/`, `*.esp` under a `Data`), treat that subdirectory as the staging root. For FOMOD, resolve every `<file>/<folder>` `source` against the **detected** archive root, case-insensitively. Make the detection explicit and unit-tested (wrapper / no-wrapper / nested-wrapper / FOMOD-with-wrapper fixtures).
**Warning signs:** Deployed files appear under `Data/<ModName>/…`; the game doesn't load a mod that "installed fine"; a FOMOD `source` path 404s against the staged tree.
`[VERIFIED: crates/extract/src/staging.rs list_files_rel has no flattening; STATE Phase-2 install-root gap carried to Phase 4]`

### Pitfall 2: Resolving/applying before the dry-run gate
**What goes wrong:** Writing files to staging during wizard navigation or before the Collection resolve report means a cancel/blocking-conflict leaves partial state.
**How to avoid:** `fomod::resolve` is a **pure** function returning a plan; nothing touches disk until the user accepts the conflict preview. The Collection resolve report is computed entirely from `collection.json` + metadata reads (no mod download) before the "Download Collection" CTA. (This is the STATE Phase-4 blocker mitigation and the UI-SPEC's two hard gates.)
**Warning signs:** Partial staged trees after a cancelled wizard; a downloaded mod present after the user declined the resolve report.

### Pitfall 3: `fomod/` folder + source-path casing under Wine/authoring
**What goes wrong:** The `fomod` folder may be `Fomod`/`FOMOD`; `ModuleConfig.xml` may be `moduleconfig.xml`; a `<file source="textures/x.dds">` may be `Textures/X.DDS` in the archive. Case-sensitive Linux extraction + case-inconsistent authoring ⇒ "file not found" for files that are present.
**How to avoid:** Locate `fomod/` and `ModuleConfig.xml`/`info.xml` with a case-insensitive walk; resolve every FOMOD `source` against the staged tree case-insensitively; normalize destinations through the existing DEPLOY-08 casefold map at deploy time. (The fomod folder is documented as case-insensitive by the spec.)
**Warning signs:** Works on a case-insensitive FS, fails on ext4; intermittent "source not found" per-author.
`[CITED: nexus-mods.github.io AboutFomod / fomod-docs — fomod folder not case sensitive]`

### Pitfall 4: Collection mod identity / availability drift
**What goes wrong:** A pinned `fileId` may be archived or removed since the collection revision was authored; a `modRule` reference may point at a mod no longer in the collection (Vortex keeps stale rules). Assuming every pinned file resolves leads to a mid-batch failure or a rule that matches nothing.
**How to avoid:** Resolve **every** mod's availability up front (Pattern 4) and classify Archived/Unavailable into the report; when mapping `modRules`, skip references that don't match any resolved mod (don't error the whole install). A failed per-mod download does not abort the batch (UI-SPEC §B.4 Retry).
**Warning signs:** Batch aborts on one archived file; a rank derived from a rule that references a missing mod.
`[CITED: Nexus-Mods/Vortex issues #10841/#14666 — stale/missing collection mod rules]`

### Pitfall 5: quick-xml serde + the `xsi:noNamespaceSchemaLocation` attribute / mixed content
**What goes wrong:** `<config xmlns:xsi="…" xsi:noNamespaceSchemaLocation="…">` and occasional mixed text/element content can trip a naive serde struct.
**How to avoid:** Deserialize against **local names** (quick-xml's serde is namespace-ignorant by default — match `config`, not `{ns}config`); use `#[serde(default)]` on all optional elements (most FOMOD elements are optional); ignore unknown attributes. Test against the fixture corpus including a namespaced and a BOM-prefixed sample.
**Warning signs:** `from_str` errors on real-world files that validate against the XSD; missing-field errors for legitimately-absent optional elements.
`[CITED: docs.rs/quick-xml de; VERIFIED: XSD marks moduleDependencies/requiredInstallFiles/installSteps/conditionalFileInstalls minOccurs=0]`

## Code Examples
(See Patterns 1–9 — each carries a verified or cited snippet. The FOMOD Schema Reference + Collection Manifest Reference sections above are the authoritative field-level specs the planner should encode as the `crates/fomod` AST and the `Collection` struct.)

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| C#/.NET XML installer engines (NMM/Vortex/MO2) | Pure-Rust FOMOD parse+resolve (quick-xml serde) | This phase | Headless, AppImage-friendly, unit-testable without a webview. |
| Collections REST-only | `collectionRevision` GraphQL v2 returns the archive `downloadLink` | v2 GA ~2023–2025 | Use GraphQL for the collection archive; REST-v1 still load-bearing for per-mod download links (Phase 3). |
| Interactive-only FOMOD | Headless replay from `choices` (IChoices) | Collections | A Collection install drives the same resolver with recorded choices — no per-mod wizard. |

**Deprecated/outdated:**
- Don't target the FOMOD 1.0/4.0 schema URLs (`ModConfig5.0.xsd` namespace string is cosmetic) — parse by local element name against the 5.x tree; older files are a subset and parse fine.

## Runtime State Inventory

> Greenfield feature addition (not a rename/refactor). The standard categories:
- **Stored data:** New V5 tables only (`collection`, `collection_mod`, `fomod_choice`). No existing key/collection/user_id string is renamed. **None to migrate.**
- **Live service config:** None — this phase registers no new OS handler (the `nxm://` handler is Phase 3; Collections reuse it for nothing new). **None.**
- **OS-registered state:** None new. **None.**
- **Secrets/env vars:** None — Collections reuse the Phase-3 keyring token; no new secret key. **None.**
- **Build artifacts / installed packages:** Adding `crates/fomod` as a workspace member changes the build graph (a new crate dir + `Cargo.lock` update) — a normal additive change, no stale artifact. The new `quick-xml` dep enters `Cargo.lock`. **Rebuild only.**

## Common Pitfalls — quick verification checklist (for the planner's verification steps)
- [ ] `crates/fomod` has **no** `tauri`/`reqwest`/`keyring` dependency (grep its Cargo.toml) — headless invariant.
- [ ] `quick-xml` is added with the **`serialize`** feature; `cargo deny check licenses sources` passes (MIT, no native dep).
- [ ] The store migration is `V5__*.sql` (V4 exists), additive; **no `rusqlite` type in the store public API** for the new Collection/choice queries.
- [ ] Archive root-detection covered by a wrapper / no-wrapper / nested / FOMOD-with-wrapper fixture (Pitfall 1).
- [ ] `fomod::resolve` is pure (returns a plan; no `std::fs` write) — the dry-run gate is provable.
- [ ] Collection resolve runs with **zero mod downloads** before the report is accepted.
- [ ] Off-Nexus (`direct`/`browse`/`manual`) mods are classified Manual and never downloaded.
- [ ] Premium gate checked (`UserInfo.is_premium`) before any Collection download; non-Premium ⇒ notice, no fallback.
- [ ] Collection deploy goes through `switch_profile`; uninstall through `purge` + `delete_profile`; pristine asserted by testkit.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `quick-xml` 0.40 (`serialize`) | FOMOD parse | crates.io | 0.40.1 | `roxmltree` 0.21 (DOM) — both MIT, deny-clean |
| Existing engine crates (extract/deploy/store/loadorder/nexus/testkit) | All of Phase 4 | in-repo | — | none needed — all present |
| A real NexusMods **Premium** account + a real published Collection | manual UAT of bulk download + end-to-end install | manual only | — | No automated substitute (mockito covers the API shape; the live Collection install is manual UAT) |
| A corpus of real `ModuleConfig.xml` files (simple + conditional + nested-dep + malformed) | FOMOD resolver fixtures | gather at plan/Wave-0 time | — | hand-author representative fixtures from the XSD if real samples are scarce |
| WebKitGTK 4.1 dev libs | building `src-tauri` (existing) | per CI apt list | — | Headless `crates/fomod` needs none |

**Missing dependencies with no fallback:** The live Premium Collection end-to-end install is manual-UAT-only (no automated substitute). **With fallback:** XML crate (roxmltree if quick-xml serde hits an edge); real `ModuleConfig.xml` corpus (synthesize from the XSD if needed).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` / `#[tokio::test]` (workspace standard; no external runner) |
| Config file | none — `cargo test --workspace --locked` (CLAUDE.md) |
| Quick run command | `cargo test -p nextwist-fomod` (and `-p nextwist-nexus` for Collection resolve) |
| Full suite command | `cargo test --workspace --locked` |
| HTTP mock (existing) | `mockito` 1.7 dev-dependency in `crates/nexus` (Collection availability/download reuse it) |
| Fixture corpus (new) | `crates/fomod/tests/fixtures/*.xml` (real + synthesized `ModuleConfig.xml`) + a real `collection.json` fixture |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| FOMOD-01 | Parse every schema construct (5 group types, static + dependencyType plugins, ordered steps) into the AST; expose live step visibility + plugin type-state for a given flag set | unit (fixture corpus) | `cargo test -p nextwist-fomod parse` + `…condition` | ❌ Wave 0 |
| FOMOD-02 | `resolve(choices, flags)` produces the correct ordered file-install plan (requiredInstallFiles + selected files + conditionalFileInstalls; priority/alwaysInstall/installIfUsable); malformed XML ⇒ specific `FomodError`, never a silent install; plan is pure (no disk write) | unit (fixture corpus) | `cargo test -p nextwist-fomod resolve` | ❌ Wave 0 |
| FOMOD-01/02 (integration) | A FOMOD archive with a wrapper folder resolves sources correctly and stages a `Data/`-rooted tree (root-detection) | integration (temp extract + real `extract`) | `cargo test -p nextwist-fomod stage` | ❌ Wave 0 |
| COLL-01/02 | Parse a real `collection.json`; classify each mod (nexus available/archived/unavailable, bundle, off-Nexus manual) into the resolve report with **zero downloads**; bulk download hits the shared limiter | unit (mockito + real fixture) | `cargo test -p nextwist-nexus collection_resolve` | ❌ Wave 0 |
| COLL-03 | Replay `IChoices` headlessly through `fomod::resolve` (name-matched); `modRules` after/before map to ranks; a stale rule reference is skipped, not fatal | unit | `cargo test -p nextwist-nexus collection_apply` + `-p nextwist-fomod replay` | ❌ Wave 0 |
| COLL-04 | A Collection profile deploys via `switch_profile` (purge→deploy→load-order→active) | integration (temp DB + temp game tree) | `cargo test -p nextwist-deploy collection_deploy` | ❌ Wave 0 (reuses profile_switch harness) |
| COLL-05 | Install→uninstall a Collection leaves the game byte-for-byte pristine (blake3 + DIR_SENTINEL) | integration (testkit pristine) | `cargo test -p nextwist-deploy collection_round_trip` | ❌ Wave 0 (reuses pristine harness) |

**Manual-only (real-account UAT — no automated substitute):**
- A real **Premium** account bulk-downloading a **real published Collection** for Skyrim SE / Fallout 4 end-to-end (resolve report → download → apply choices → deploy → launch → purge-to-pristine). This is the **hard UAT for the phase** (analogous to prior phases' in-game round-trip).
- A FOMOD install of a **real-world complex mod** (e.g. a conditional-heavy installer) through the wizard, confirming the choices, dry-run preview, and in-game result.

### Sampling Rate
- **Per task commit:** `cargo test -p <crate-touched>` (e.g. `-p nextwist-fomod`) + `cargo clippy -p <crate> -- -D warnings`.
- **Per wave merge:** `cargo test --workspace --locked` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo deny check advisories bans licenses sources` (the new `quick-xml` license/source must pass).
- **Phase gate:** full suite green + the manual-UAT items above signed off before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] Scaffold `crates/fomod` (member crate, `error.rs` + `model.rs` + `parse.rs` + `condition.rs` + `resolve.rs`) and add `quick-xml = { version = "0.40", features = ["serialize"] }` to `[workspace.dependencies]`.
- [ ] Gather/author a `crates/fomod/tests/fixtures/` corpus: simple, flag-driven, `conditionalFileInstalls`, nested-`And`/`Or` deps, `dependencyType` plugin, wrapper-folder, malformed (each pins a test).
- [ ] Obtain a real `collection.json` fixture (export a small Collection or fetch a public revision) for the Collection resolver tests.
- [ ] Add root-detection + a wrapper/no-wrapper/nested fixture to the staging path (closes the carried Phase-2 gap).
- [ ] Add the `V5__collections.sql` migration + a store round-trip test for the Collection + FOMOD-choice tables.
- [ ] Reuse the existing `profile_switch` / pristine harnesses for COLL-04/05 (no new harness needed).

## Security Domain

> `security_enforcement: true`, ASVS Level 1. This phase parses **untrusted XML** and **untrusted JSON** and drives the existing write path — the new surface is parser robustness + path safety, not auth (auth is Phase 3, reused).

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V5 Input Validation | **yes** | `quick-xml`/`serde_json` parse untrusted `ModuleConfig.xml` / `collection.json`; reject malformed with a specific error (no silent fallthrough); validate FOMOD `source`/`destination` and Collection paths through the EXISTING zip-slip/path-escape guards (`extract::validate_entry`, `conflict::guard_within_root`). FOMOD `destination` must not escape `Data/`. |
| V6 Cryptography | partial (delegated) | No new crypto. Collection `hashes` (md5) are integrity hints only — not a security boundary; TLS stays `rustls` (Phase-3 client). |
| V9 Communications | yes (reused) | Collection archive + mod downloads reuse the Phase-3 `rustls`-only, redirect-disabled, `error_for_status` client. |
| V12 File & Resources | **yes** | The resolved FOMOD plan + Collection mods route through `extract::install_archive` (symlink/zip-slip defense) and `conflict::resolve` (per-winner path-escape guard); no new file primitive bypasses these. `patches`/`fileOverrides` paths validated the same way. |

### Known Threat Patterns for FOMOD/Collection parsing
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Malicious FOMOD `source`/`destination` path traversal (`../`, absolute) | Tampering | Resolve `source` inside the validated staged tree only; validate `destination` stays under `Data/` via the existing path-escape guard; reuse `extract` zip-slip defense. |
| XML entity-expansion / billion-laughs DoS | Denial of Service | `quick-xml` does not expand external entities by default; cap input size; the file is config-sized (KB), not arbitrary. |
| Collection `patches`/`fileOverrides` writing outside the mod | Tampering | Patch/override target paths validated through the same path-escape guard before any write. |
| Off-Nexus `direct`/`browse` URL auto-fetch (SSRF) | Tampering/SSRF | **Never auto-fetch** off-Nexus sources (locked decision) — surfaced as manual steps only. |
| Stale/spoofed `modRule` reference manipulating load order | Tampering | Rules only reorder rank within the user's own collection; a reference matching no resolved mod is ignored, not trusted. |
| Untrusted archive content (downloaded Collection mods) | Tampering | Reuse `extract` validation unchanged — every mod archive is zip-slip/symlink-validated. |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The collection archive (`collectionRevision.downloadLink`) contains `collection.json` matching Vortex's `ICollection` shape | Collection Manifest Reference | Field-name drift in the resolver; mitigated by parsing the real fixture at Wave 0. **MEDIUM** — confirm against a real download. |
| A2 | `collectionRevision(slug, revision, domainName)` is the correct GraphQL query + `downloadLink` field name for the archive | Pattern 4 | Wrong query → can't fetch the collection; confirm against graphql.nexusmods.com live. **MEDIUM.** |
| A3 | Premium download of Collection mods uses the same REST-v1 `download_link.json` path as Phase-3 single-mod (no key/expires for Premium) | Pattern 5 | If a different bulk endpoint exists, the download wiring changes (the limiter/streaming core still applies). **LOW-MEDIUM.** |
| A4 | The `IChoices` step/group/option-by-name encoding is what current Collections store for FOMOD replay | Pattern 6 | A name-keyed match fails if Nexus changed the format; fall back to surfacing the mod for manual FOMOD choice. **MEDIUM.** |
| A5 | Mapping `after`/`before` rules to rank numbers (after ⇒ higher rank number) matches the intended "loser/winner" semantics | Pattern 7 | An inverted mapping flips conflict winners; confirm with a real Collection's expected outcome in UAT. **MEDIUM.** |
| A6 | `installIfUsable` semantics = install when the owning option is not `NotUsable` even if unselected | FOMOD Schema Reference | Edge-case files mis-included/excluded; low frequency in real installers. **LOW.** |
| A7 | quick-xml 0.40 serde handles the FOMOD optional-element + namespaced-root shape cleanly with `#[serde(default)]` | Pattern 1, Pitfall 5 | If a real file trips serde, switch that struct to a manual reader or roxmltree for that node. **LOW-MEDIUM.** |
| A8 | A single new `quick-xml` dep is the only crate added (Collection resolver reuses existing crates) | Standard Stack | If a `crates/collection` split needs an extra dep, re-run the legitimacy gate. **LOW.** |

**These `[ASSUMED]` items (esp. A1, A2, A4, A5) should be confirmed during planning against a real Collection revision + the live GraphQL schema, or gated behind a `checkpoint:human-verify` before the Collection resolve/replay path is locked.**

## Open Questions (RESOLVED)

1. **The exact `collectionRevision` GraphQL query + the collection archive format.**
   - What we know: `collectionRevision(slug, revision, domainName)` returns `downloadLink` + `fileSize`; the archive contains `collection.json` (Vortex `ICollection`).
   - What's unclear: whether the archive is a tarball/zip and any auth header needed for the download.
   - RESOLVED: treat the collection archive as a zip via the existing extract path; fetch a real public Collection revision at Wave 0 of 04-03 and pin the parsed `collection.json` as a fixture (mockito covers the API shape); confirm the container format + any download auth header at the Wave-0 fetch and at the `checkpoint:human-verify` gate before the live query is locked.

2. **Whether Premium bulk download differs from single-mod `download_link.json`.**
   - RESOLVED: build on the Phase-3 Premium `run_download_to_window` path unchanged; confirm bulk vs single-mod equivalence with a real Premium account at the 04-04 human-verify checkpoint (UAT).

3. **The real-world `IChoices` ↔ current ModuleConfig matching when a pinned mod was updated.**
   - RESOLVED: name-match step→group→option; on a miss, return a specific error and surface the mod for a manual wizard pass rather than silently mis-installing.

## Sources

### Primary (HIGH confidence)
- **`GandaG/fomod-schema/ModuleConfig.xsd` (FOMOD 5.x)** — the canonical schema; full element/attribute/enum tree fetched and enumerated (group types, plugin type enum, dependency operators, fileSystemItem attributes, conditionalFileInstalls, order enum). `[VERIFIED via gh API 2026-06-21]`
- **Nexus-Mods/Vortex `extensions/collections/src/types/ICollection.ts`** — `ICollection`/`ICollectionMod`/`ICollectionSourceInfo`/`ICollectionModRule`, `SourceType`/`UpdatePolicy`/`RuleType`, `choices`/`patches`/`phase`/`fileOverrides` fields. `[VERIFIED via gh API 2026-06-21]`
- **Vortex `installer_fomod_shared/types/interface.ts`** — `IChoices` replay encoding (step→group→option by name+idx), `GroupType`/`PluginType` enums. `[VERIFIED via gh API 2026-06-21]`
- **Vortex `vortex-api/lib/api.d.ts`** — `IModReference` matching fields (tag/md5Hint/idHint/archiveId/repo). `[VERIFIED via gh API 2026-06-21]`
- **Local codebase** — `crates/extract/src/staging.rs` (no root-detection → Pitfall 1), `crates/deploy/src/conflict.rs` (Data/-rooted rank fold), `crates/deploy/src/profile.rs` (switch_profile), `crates/store/src/profiles.rs` (create/set_profile_mod/delete_profile), `crates/loadorder/src/lib.rs` (apply_load_order), `crates/nexus/src/{lib,client,download}.rs` (reusable download + shared limiter), `crates/testkit/src/lib.rs` (pristine harness), `crates/store/src/migrations/` (V4 exists → V5 next), `deny.toml`, `Cargo.toml`. `[VERIFIED]`
- **crates.io via legitimacy seam** — `quick-xml` 0.40.1 (MIT, ~5.69M/wk, OK), `roxmltree` 0.21.1 (MIT, ~1.03M/wk, OK). `[VERIFIED 2026-06-21]`

### Secondary (MEDIUM confidence)
- fomod-docs.readthedocs.io (specs/tutorial) — FOMOD structure + the case-insensitive `fomod` folder + `info.xml` vs `ModuleConfig.xml`.
- graphql.nexusmods.com + forums.nexusmods.com (GraphQL examples) — `collectionRevision(slug,revision,domainName)` → `downloadLink`/`fileSize`/`revisionNumber`.
- github.com/Nexus-Mods/node-nexus-api — file-info / download-link client shape reused for per-mod availability.
- help.nexusmods.com + modding.wiki Collections — manifest/checksum/install behavior, off-site dependency handling.

### Tertiary (LOW confidence)
- WebSearch summaries for FOMOD parsing quirks (BOM, namespace) and the exact collection-archive container format — cross-checked but need a live download to confirm (A1, A2).
- nexus-mods.github.io AboutFomod — high-level FOMOD architecture (the page did not expose implementation specifics).

## Metadata

**Confidence breakdown:**
- **FOMOD schema fidelity:** HIGH — enumerated from the canonical XSD (every element/attribute/enum).
- **Collection manifest shape:** MEDIUM-HIGH — from Vortex's own `ICollection.ts`/`IChoices`/`IModReference`; the live archive container + exact GraphQL query need one real fetch (A1/A2).
- **Reuse mapping (deploy/conflict/profile/loadorder/download/testkit):** HIGH — verified against the actual codebase APIs.
- **XML crate choice + legitimacy:** HIGH — registry-verified + legitimacy-audited; only one new crate.
- **Rule→rank + choices-replay semantics:** MEDIUM — the mapping is sound but the exact after/before winner direction (A5) and updated-mod replay (A4) warrant a real-Collection UAT.

**Research date:** 2026-06-21
**Valid until:** ~2026-07-21 for crate versions; the NexusMods Collections GraphQL surface is "in flux" → re-verify the `collectionRevision` query + a real `collection.json` against a live revision at plan time if more than ~2 weeks elapse. The FOMOD 5.x XSD is stable (long-frozen) → no expiry concern.
