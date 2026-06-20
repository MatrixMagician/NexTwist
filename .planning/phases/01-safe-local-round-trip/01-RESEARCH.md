# Phase 1: Safe Local Round-Trip - Research

**Researched:** 2026-06-20
**Domain:** Crash-safe, reversible filesystem deployment of mods into Steam Proton/Wine games on Linux (Rust headless core + thin Tauri shell)
**Confidence:** HIGH (crate versions/APIs verified against crates.io + docs.rs; platform facts cross-checked across multiple community/kernel sources)

<user_constraints>
## User Constraints (from CONTEXT.md)

All 16 decisions are LOCKED. Research below describes HOW to implement them, not whether to.

### Locked Decisions

**Architecture & Code Structure**
- Multi-crate Cargo workspace with a pure, headless Rust core under `crates/` (e.g. `deploy`, `steam`, `store`, `extract`) plus a thin Tauri shell. The safety-critical engine has **zero Tauri dependencies**, unit/property-testable headless in CI.
- Tauri commands are **thin 3–10 line adapters** that delegate to the headless core — no business logic in `#[tauri::command]` functions.
- Database: **rusqlite (bundled SQLite) + refinery** versioned migrations. Statically linked. Per-file deploy ledger lives here. (sqlx rejected as overkill.)
- **Tests are a first-class Phase 1 deliverable:** property/integration tests on temp dirs — round-trip-pristine assertions, plus fixtures for EXDEV (cross-FS), zip-slip/malicious archives, and crash-mid-deploy recovery.

**Deployment Engine Safety Model**
- **Crash-safety via write-ahead journal in SQLite + idempotent replay/rollback on next launch.** A verify/repair pass auto-runs after abnormal exit.
- **Vanilla-file backup:** any pre-existing game file about to be overwritten is first copied into a **per-game original-store under the app data dir**, content-hashed, recorded in the manifest. Purge restores from this store.
- **Staged-file integrity:** staged files marked **read-only**; deployment **prefers reflink** (independent inode) else falls back to hardlink.
- **Purge verification:** purge does a **hash-diff of the game folder against the recorded manifest + vanilla store**, asserts byte-for-byte pristine, reports orphans rather than trusting manifest deletion blindly.

**Deployment Method & Filesystem Handling**
- **Per-target runtime method probe: reflink → hardlink → symlink → copy.** Chosen per-target at deploy time (never globally), based on empirical fs-capability probe (st_dev, link() capability, reflink support).
- **Cross-FS / EXDEV policy:** detect at setup; **warn and recommend same-filesystem staging**. If user proceeds cross-FS, automatically fall back to symlink/copy for those targets (never silently fail a hardlink).
- **Case-sensitivity (Proton/Wine):** **normalize mod path casing against the per-game canonical `Data/` directory casing** on deploy. (ext4 casefold +F is a deferred alternative.)
- **Default staging location:** auto-suggest a directory on the **same filesystem as the game install**, capability-probed at setup.

**Detection, Archives & UI**
- **Game detection:** steamlocate **auto-detect + manual "add game by folder" fallback**.
- **Supported games:** **Skyrim SE (AppID 489830) + Fallout 4 (AppID 377160)**, detecting **Flatpak and Snap Steam roots** in addition to native Steam.
- **Proton prefix resolution:** derive `compatdata/<appid>/pfx/...` from `libraryfolders.vdf` + `appmanifest_<id>.acf`; re-resolve each session.
- **Archive handling:** `.zip` + `.7z` native (zip + sevenz-rust2); `.rar` via system `unrar`/`7z` if present else clear error — **no non-free RAR code bundled**. Extraction canonicalizes + bounds-checks every entry, rejects `..`/absolute/symlink, extracts to temp then moves into staging. Crafted-malicious-archive test fixture included.
- **UI: functional-minimal Svelte 5** — game list with paths, install-mod-from-archive, deploy/purge buttons, clear surfacing of warnings.

### Claude's Discretion
- Exact crate names/boundaries within the workspace, module layout, error-type design (thiserror in libs / anyhow at app boundary), and logging (tracing) setup.
- Manifest/ledger schema details and migration versioning specifics.
- Precise capability-probe implementation and how warnings are phrased in the UI.
- Whether a single managed mod is represented with full priority scaffolding now or minimal single-mod state (priority/load-order UI proper is Phase 2).

### Deferred Ideas (OUT OF SCOPE)
- ext4 casefold (+F) deployment tree as a stronger case-handling method — revisit later if path-casing normalization proves insufficient.
- Reflink (CoW) validated per-game as a first-class default (v2: DEPV2-02).
- Multi-mod conflict resolution, mod priority/load order, plugin (.esp/.esm/.esl) management + LOOT, multiple profiles — Phase 2.
- Any NexusMods networking (auth, download, nxm://) — Phase 3.
- Polished/styled UI.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| ENV-01 | Auto-detect installed Steam games on Linux | steamlocate 2.1 `SteamDir::locate_all()` + `libraries()`/`apps()`; manual-add fallback. §Steam/Proton Discovery |
| ENV-02 | Resolve a game's install dir and Proton/Wine prefix | `find_app(appid) -> (App, Library)`; derive install dir = `library.path()/steamapps/common/<App.install_dir>`; prefix = `library.path()/steamapps/compatdata/<appid>/pfx`. §Steam/Proton Discovery |
| ENV-03 | Add/manage supported Bethesda games (Skyrim SE 489830, FO4 377160) | AppID allow-list; persist managed-game rows in SQLite. §Steam/Proton Discovery |
| ENV-04 | Detect fs capabilities (same-device, case-folding) and warn | st_dev compare via `MetadataExt::dev()`; `reflink_copy::check_reflink_support`; FS_IOC_GETFLAGS casefold probe; empirical hard_link probe catching `CrossesDevices`. §Filesystem Capability Probe |
| STAGE-01 | Install a mod from .zip/.7z into staging store | zip 8.x `ZipArchive` + sevenz-rust2 0.21 `ArchiveReader`; extract-to-temp-then-move. §Safe Archive Extraction |
| STAGE-02 | Safely extract rejecting zip-slip | `enclosed_name()` + re-canonicalize-under-root + reject symlink/`..`/absolute entries. §Safe Archive Extraction |
| STAGE-03 | Install .rar via system unrar/7z (no bundled non-free code) | `std::process::Command` shelling to `unrar`/`7z`; detect presence; clear error otherwise. §Safe Archive Extraction |
| DEPLOY-01 | Deploy mods without modifying original game files | Link staging→`Data/` per-file; never write into base game files except via backup-then-overwrite. §Deployment Engine |
| DEPLOY-02 | Record every deployed file in per-game manifest/ledger | `deployed_file` table (target relpath, source mod, method, hash, pre_existing flag). §Manifest & Journal Schema |
| DEPLOY-03 | Purge/uninstall restoring pristine vanilla state | Manifest-driven purge + restore from vanilla store; hash-diff assert pristine. §Purge & Verify |
| DEPLOY-04 | Back up overwritten original before deployment | Content-hashed per-game original-store; copy-before-overwrite recorded in manifest. §Vanilla Backup Store |
| DEPLOY-05 | Per-target method selection (reflink→hardlink→symlink→copy) accounting for fs boundaries | Per-target probe + method ladder; EXDEV fallback. §Deployment Method Ladder |
| DEPLOY-06 | Crash-safe (journaled) deploy/purge recoverable | Operation-journal table (intent rows pending→done), idempotent replay on launch. §Crash-Safe Journaling |
| DEPLOY-07 | verify/repair detecting manifest-vs-disk drift | Hash + provenance per file; scan diff vs manifest; report orphans/missing. §Purge & Verify |
| DEPLOY-08 | Resolve case-sensitivity mismatches for Proton | Normalize mod path casing against per-game canonical `Data/` casing map. §Case-Sensitivity Handling |
</phase_requirements>

## Summary

Phase 1 builds the entire reversible-deployment safety core end-to-end on local archives. The dominant technical risk is **crash-safety of filesystem mutations**: SQLite gives you ACID *inside the database*, but a `link()`/`copy()` syscall and the DB row recording it are **two separate operations that cannot be made atomic together**. The correct pattern is therefore an explicit **operation-journal table** (intent recorded `pending` *before* the syscall, flipped to `done` *after*), wrapped in WAL mode, combined with **idempotent file operations** so that replaying a journal after a crash is always safe — re-doing a completed link or re-deleting a missing file is a no-op, never an error. On next launch the app scans for any non-`done` journal rows and rolls the operation forward (finish it) or back (undo it) to reach a consistent state.

The second risk cluster is **Linux/Proton platform reality**. The dev machine here is **btrfs** — which is the highest-risk filesystem for this product because btrfs treats every subvolume as a separate device for `link()`, producing `EXDEV` (errno 18, `io::ErrorKind::CrossesDevices`, stable since Rust 1.85) even on the same physical disk. The engine must therefore probe capabilities **per-target-pair** (staging dir → game `Data/`) at deploy time, never globally: compare `st_dev`, call `reflink_copy::check_reflink_support`, and attempt a real throwaway `hard_link` to catch `CrossesDevices` before the user installs anything. Proton path resolution must be derived from Steam's own `libraryfolders.vdf` + `appmanifest_<appid>.acf` (steamlocate gives you the library + install dir but **does not expose compatdata** — you join the prefix path manually), and must handle Flatpak (`~/.var/app/com.valvesoftware.Steam`) and Snap roots. Wine is case-sensitive on Linux, so mod paths must be normalized against the game's canonical `Data/` casing or mods silently fail to load.

All pinned crate versions and the safety-critical APIs (`reflink_copy::check_reflink_support` / `ReflinkSupport`, steamlocate `find_app -> (App, Library)`, sevenz-rust2 `ArchiveReader`/`ArchiveEntry`, zip `enclosed_name`) are verified current against crates.io and docs.rs. The zip-slip CVE-2025-29787 was fixed in `zip` 2.3.0, so the pinned 8.x line is safe — but extraction must still use `enclosed_name()` AND reject symlink entries explicitly, because the CVE was specifically a symlink-write-through bypass.

**Primary recommendation:** Build the `deploy` crate as a state machine over an explicit SQLite operation-journal (WAL mode, intent-before-act, idempotent replay), with a per-target capability probe driving a `DeploymentMethod` trait ladder, a content-hashed vanilla backup store, and a hash-diff verify/repair pass. Make the crash-mid-deploy recovery test the centerpiece of the suite, run it on tmpfs/ext4/btrfs-subvolume fixtures, and treat every file syscall as idempotent. Everything else (steam discovery, extraction, Svelte UI) orbits this core and is lower-risk, well-documented work.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Steam/Proton path discovery | `crates/steam` (headless core) | — | Quarantines all Proton/Steam-layout knowledge; hands engine resolved absolute paths |
| Filesystem capability probe | `crates/deploy` (or `crates/fsprobe`) | `crates/steam` (captures fs/st_dev at game-add) | Probe is a deploy-engine concern; game registry stores the captured result |
| Archive extraction + zip-slip defense | `crates/extract` (headless core) | — | Pure transform: archive bytes → validated staging tree; no UI/Tauri |
| Staging store management | `crates/store` (headless core) | `crates/extract` | Owns on-disk per-mod trees + read-only marking; extract writes into it |
| Deploy / purge / verify (crown jewel) | `crates/deploy` (headless core) | `crates/store` (manifest+journal persistence) | The reversibility guarantee; must be heavily property-tested headless |
| Manifest + operation journal persistence | `crates/store` (SQLite via rusqlite) | `crates/deploy` (writes intents/results) | DB layer owns schema/migrations; deploy owns the protocol |
| Vanilla backup store | `crates/deploy` + `crates/store` | — | Backup-before-overwrite is a deploy step; store owns the content-addressed dir |
| Case normalization | `crates/deploy` (`casefold.rs`) | `crates/steam` (canonical Data/ casing map per game) | Deploy rewrites paths; steam crate knows per-game canonical casing |
| Tauri command adapters | `src-tauri/` (thin adapter) | all `crates/` | 3–10 line wrappers; zero business logic (anti-pattern to violate) |
| Svelte UI (functional-minimal) | `frontend/` (webview) | `src-tauri/` (invoke/emit) | UI talks to backend only via `invoke` + event listeners |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rust | 1.85+ (2024 ed.) | Backend language | Project constraint; `io::ErrorKind::CrossesDevices` stable since 1.85 (needed for EXDEV detection) `[VERIFIED: rust release notes]` |
| tauri | 2.11.3 | Desktop shell (thin adapter) | Pinned in STACK.md; system WebView + native AppImage `[VERIFIED: crates.io]` |
| rusqlite | 0.40.1 (feature `bundled`) | Embedded DB: manifest + operation journal + game registry | Statically links SQLite → clean AppImage; transactional ledger `[VERIFIED: crates.io]` |
| refinery | 0.9.2 (feature `rusqlite`) | Versioned schema migrations | refinery rusqlite feature targets rusqlite major; ledger schema will evolve `[VERIFIED: crates.io]` |
| reflink-copy | 0.1.30 | CoW deploy primitive + reflink-support probe | `check_reflink_support` probes without copying; `reflink`/`reflink_or_copy` for deploy `[VERIFIED: crates.io + docs.rs]` |
| steamlocate | 2.1.0 | Steam library + app discovery | `SteamDir::locate_all`, `find_app(appid) -> (App, Library)` `[VERIFIED: crates.io + docs.rs]` |
| keyvalues-serde | 0.2.4 | Parse `appmanifest_<id>.acf` fields steamlocate doesn't expose | VDF/ACF parsing for prefix derivation `[VERIFIED: crates.io]` |
| zip | 8.6.0 | `.zip` extraction | CVE-2025-29787 fixed in 2.3.0; 8.x safe + `enclosed_name()` `[VERIFIED: crates.io]` |
| sevenz-rust2 | 0.21.0 | `.7z` extraction (maintained fork) | `ArchiveReader`/`ArchiveEntry` low-level API enables per-entry validation `[VERIFIED: crates.io + docs.rs]` |
| walkdir | 2.5.0 | Recursive staging traversal + conflict/orphan scan | Standard `[VERIFIED: crates.io]` |
| serde + serde_json | 1.0.228 | Manifest/registry (de)serialization | Universal `[VERIFIED: crates.io]` |
| tracing + tracing-subscriber | 0.1.44 / 0.3.x | Structured logging (diagnose deploy/Proton issues) | Essential for field debugging `[VERIFIED: crates.io]` |
| thiserror + anyhow | 2.0.18 / 1.0.102 | Lib error types / app-boundary context | Standard pairing per Claude's-discretion error design `[VERIFIED: crates.io]` |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| blake3 | 1.8.5 | Content hashing for vanilla store + verify/repair | Fast, modern; use for content-addressed backup keys + pristine assertion `[VERIFIED: crates.io]` |
| sha2 | 0.11.0 | Alt hashing if a published reference hash is SHA-based | Only if interop requires SHA-256 (Phase 3 NexusMods); blake3 preferred internally `[VERIFIED: crates.io]` |
| tempfile | 3.27.0 | Temp dirs for extract-to-temp-then-move + tests | Test fixtures (temp game/staging dirs) + safe extraction staging `[VERIFIED: crates.io]` |
| proptest | 1.11.0 | Property/integration tests (first-class deliverable) | Round-trip-pristine, randomized file trees, crash-injection `[VERIFIED: crates.io]` |
| rusqlite_migration *(alt)* | — | (Alternative to refinery if embedded-SQL migrations preferred) | Not needed; refinery is the locked choice |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| rusqlite + refinery | sqlx (SQLite) | Locked decision: rejected as overkill for single-user embedded DB |
| blake3 | sha2 / xxhash | blake3 fastest for large texture files; sha2 only where external reference hash demands it; xxhash non-cryptographic (fine for change-detection, not for trust) |
| `reflink_or_copy` | manual `check_reflink_support` then `reflink` | Use `check_reflink_support` for the **probe/UI warning**; use `reflink` (not `reflink_or_copy`) for **deploy** so you control the method-ladder fallback explicitly rather than silently copying |

**Installation:**
```bash
# Workspace member crates (crates/deploy, crates/store, etc.)
cargo add rusqlite --features bundled
cargo add refinery --features rusqlite
cargo add reflink-copy steamlocate keyvalues-serde
cargo add zip sevenz-rust2 walkdir
cargo add serde --features derive && cargo add serde_json
cargo add blake3 tracing tracing-subscriber thiserror anyhow
cargo add --dev proptest tempfile
# Tauri shell (src-tauri/) — scaffold first:
npm create tauri-app@latest    # pick Svelte, TypeScript
cargo add tauri --features protocol-asset      # in src-tauri/
cargo install cargo-deny                        # license audit (enforce no-unrar)
```

**Version verification:** All versions above confirmed against `crates.io/api/v1/crates/<name>` max_stable_version on 2026-06-20 and match the pinned values in `.planning/research/STACK.md` and `.claude/CLAUDE.md`. No drift detected.

## Package Legitimacy Audit

Ran `gsd-tools query package-legitimacy check --ecosystem crates …` on 2026-06-20.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| rusqlite | crates | ~11 yrs | 1.68M/wk | github.com/rusqlite/rusqlite | OK | Approved |
| reflink-copy | crates | ~3 yrs | 337K/wk | github.com/cargo-bins/reflink-copy | OK | Approved |
| steamlocate | crates | ~5 yrs | 1.7K/wk | github.com/WilliamVenner/steamlocate-rs | OK | Approved (low downloads expected for niche Steam crate) |
| keyvalues-serde | crates | ~5 yrs | 3.6K/wk | codeberg.org/CosmicHarper/vdf-rs | OK | Approved (niche VDF parser) |
| sevenz-rust2 | crates | ~1 yr | 19K/wk | github.com/hasenbanck/sevenz-rust | OK | Approved (maintained fork of abandoned sevenz-rust) |
| zip | crates | ~11 yrs | 3.7M/wk | github.com/zip-rs/zip2 | OK | Approved (≥8.x; CVE-2025-29787 fixed in 2.3.0) |
| refinery | crates | ~10 yrs | 147K/wk | github.com/rust-db/refinery | OK | Approved |
| walkdir | crates | — | 8.5M/wk | github.com/BurntSushi/walkdir | OK | Approved |
| tempfile | crates | — | 10.9M/wk | github.com/Stebalien/tempfile | OK | Approved |
| proptest | crates | — | 2.8M/wk | github.com/proptest-rs/proptest | OK | Approved |
| blake3 | crates | — | 2.4M/wk | github.com/BLAKE3-team/BLAKE3 | OK | Approved |
| sha2 | crates | — | 13.8M/wk | github.com/RustCrypto/hashes | OK | Approved |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none
**License note:** `cargo-deny` MUST be configured in Phase 1 to fail the build if any transitive dependency pulls in the non-free UnRAR source (the `unrar`/`unrar_sys` crates). This satisfies the "no non-free RAR code bundled" constraint and pre-positions DIST-02.

## Architecture Patterns

### System Architecture Diagram

```
  [User picks local .zip/.7z/.rar]        [User clicks "Add game" / auto-detect]
            │                                          │
            ▼                                          ▼
  ┌───────────────────┐                    ┌──────────────────────────┐
  │ extract crate     │                    │ steam crate              │
  │ • temp extract    │                    │ • SteamDir::locate_all   │
  │ • per-entry valid │                    │ • find_app(appid)        │
  │   (enclosed_name, │                    │ • derive install dir     │
  │    reject symlink/│                    │   = library/steamapps/   │
  │    abs/..)        │                    │     common/<install_dir> │
  │ • move→staging    │                    │ • derive prefix          │
  └─────────┬─────────┘                    │   = library/steamapps/   │
            │ staged tree (read-only)      │     compatdata/<id>/pfx  │
            ▼                              │ • Flatpak/Snap roots     │
  ┌───────────────────┐                    │ • canonical Data/ casing │
  │ store crate       │◄───────────────────┤ • capture fs/st_dev      │
  │ • SQLite (WAL)    │   resolved paths   └────────────┬─────────────┘
  │ • game registry   │                                 │
  │ • manifest table  │                                 │
  │ • op-journal table│                                 ▼
  │ • vanilla store   │                    ┌──────────────────────────┐
  │   (content-hashed)│◄───────────────────┤ deploy crate (CROWN JEWEL)│
  └─────────┬─────────┘  read/write        │  per-target probe:       │
            │            manifest+journal   │   st_dev? reflink? link? │
            │            in DB tx           │  method ladder:          │
            ▼                               │   reflink→hardlink→      │
  ┌───────────────────────────────┐        │   symlink→copy           │
  │ Target: <game>/Data/          │◄───────┤  DEPLOY:                 │
  │  links to staged mod files    │ writes │   1 journal intent(pending)│
  │  + backed-up vanilla originals│ links  │   2 backup-if-pre-existing│
  └───────────────────────────────┘        │   3 do file op (idempotent)│
            ▲                               │   4 journal done + manifest│
            │ on next launch                │  PURGE: reverse via manifest│
            │ scan non-done journal rows ───┤  VERIFY/REPAIR: hash-diff   │
            │ → roll forward/back           │   manifest vs disk + restore│
            └───────────────────────────────┘
                         ▲
       invoke()/emit()   │  (thin #[tauri::command] adapters in src-tauri/)
                         │
            ┌────────────┴─────────────┐
            │ Svelte 5 UI (functional) │
            │ game list+paths · install│
            │ deploy/purge · warnings  │
            └──────────────────────────┘
```

### Recommended Project Structure
```
nextwist/
├── Cargo.toml                  # [workspace] members = ["crates/*", "src-tauri"]
├── crates/
│   ├── core/                   # shared domain types: Game, ManagedMod, FileEntry, DeployMethod, error enums
│   ├── steam/                  # Steam/Proton discovery; fs/st_dev capture; canonical Data/ casing
│   ├── extract/                # zip + sevenz-rust2 + system-unrar; zip-slip defense; temp→staging
│   ├── store/                  # rusqlite + refinery; manifest, op-journal, game registry, vanilla store
│   └── deploy/                 # THE crown jewel
│       ├── probe.rs            # per-target capability probe (st_dev, reflink, hardlink, casefold)
│       ├── method/             # trait DeploymentMethod { deploy_file, remove_file, is_applicable }
│       │   ├── reflink.rs  hardlink.rs  symlink.rs  copy.rs
│       ├── journal.rs          # write-ahead operation journal protocol + idempotent replay
│       ├── manifest.rs         # record/load deployed-file rows
│       ├── backup.rs           # vanilla backup-before-overwrite + restore
│       ├── casefold.rs         # normalize mod path casing vs canonical Data/
│       ├── verify.rs           # hash-diff manifest vs disk; pristine assertion; repair
│       └── engine.rs           # deploy() / purge() / recover_on_launch() orchestration
├── src-tauri/                  # Tauri shell + thin command adapters ONLY
│   └── src/ (main.rs, commands/{games,mods,deploy}.rs, state.rs)
└── frontend/                   # Svelte 5 SPA (adapter-static)
```

### Pattern 1: Explicit Operation Journal (Intent-Before-Act) over WAL
**What:** SQLite WAL gives ACID *inside* the DB, but you cannot make a `link()` syscall and its recording row atomic. So record the *intent* of each file op as a `pending` journal row, `COMMIT` it, perform the (idempotent) syscall, then `UPDATE … done` + write the manifest row in one tx. A crash leaves a `pending` row whose effect on disk is either absent or already complete — both recoverable because the op is idempotent.
**When to use:** Every deploy and purge file operation. This is DEPLOY-06.
**Example:**
```rust
// Source: synthesized from SQLite WAL semantics (sqlite.org/pragma.html) + idempotent-op design
// conn: WAL mode set once at open: PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL (or FULL for safety)
fn deploy_one(conn: &Connection, op: &FileOp) -> Result<()> {
    // 1. record intent BEFORE touching disk, commit so it survives a crash
    let tx = conn.unchecked_transaction()?;
    tx.execute("INSERT INTO op_journal(target, method, source_hash, kind, state)
                VALUES (?1,?2,?3,'deploy','pending')", params![op.target, op.method, op.hash])?;
    let jid: i64 = tx.last_insert_rowid();
    tx.commit()?;                       // <-- durable intent

    // 2. backup pre-existing vanilla file (idempotent: skip if already backed up by hash)
    if op.pre_existing { backup_vanilla_if_absent(conn, op)?; }

    // 3. perform the IDEMPOTENT file op (re-running a completed deploy is a no-op)
    apply_method_idempotent(op)?;       // reflink/hardlink/symlink/copy; remove-then-link if target exists & is ours

    // 4. mark done + record manifest atomically
    let tx = conn.unchecked_transaction()?;
    tx.execute("UPDATE op_journal SET state='done' WHERE id=?1", params![jid])?;
    tx.execute("INSERT INTO deployed_file(target,source_mod,method,hash,pre_existing)
                VALUES(?1,?2,?3,?4,?5)", params![op.target, op.mod_id, op.method, op.hash, op.pre_existing])?;
    tx.commit()?;
    Ok(())
}

// On launch, BEFORE serving any UI command:
fn recover_on_launch(conn: &Connection) -> Result<RecoveryReport> {
    // any row not 'done' = interrupted; roll forward (finish) or roll back (undo) idempotently
    for row in pending_journal_rows(conn)? {
        match row.kind {
            Deploy => { apply_method_idempotent(&row.into_op())?; mark_done(conn,row.id)?; }
            Purge  => { remove_idempotent(&row.target)?; restore_vanilla_if_recorded(conn,&row)?; mark_done(conn,row.id)?; }
        }
    }
    verify::full_pristine_or_report(conn)   // hash-diff after recovery
}
```
**Why idempotency matters:** It removes the need for true syscall+DB atomicity. The only invariant required is "applying op N a second time yields the same disk state" — true for link/copy (remove-if-ours-then-recreate) and for delete (missing = no-op).

### Pattern 2: Per-Target Capability Probe → DeploymentMethod Ladder
**What:** For each (staging dir, game-target dir) pair, probe at deploy time and pick the strongest applicable method. Never decide globally.
**When to use:** DEPLOY-05, ENV-04.
**Example:**
```rust
// Source: synthesized from std::os::unix::fs::MetadataExt + reflink_copy::check_reflink_support docs
use std::os::unix::fs::MetadataExt;          // .dev()  -> st_dev
use reflink_copy::{check_reflink_support, ReflinkSupport};

fn choose_method(staging: &Path, target_dir: &Path) -> DeployMethod {
    let same_dev = same_st_dev(staging, target_dir).unwrap_or(false);
    if !same_dev {
        // cross-device: reflink & hardlink impossible -> warn + symlink/copy
        return DeployMethod::SymlinkOrCopy;
    }
    match check_reflink_support(staging, target_dir) {
        Ok(ReflinkSupport::Supported)  => DeployMethod::Reflink,
        Ok(ReflinkSupport::Unsupported)=> DeployMethod::Hardlink,   // same-dev, no CoW (ext4)
        Ok(ReflinkSupport::Unknown) | Err(_) => DeployMethod::Hardlink, // try, fall back on EXDEV
    }
}
fn same_st_dev(a: &Path, b: &Path) -> std::io::Result<bool> {
    Ok(std::fs::metadata(a)?.dev() == std::fs::metadata(b)?.dev())
}
// EXDEV at apply time (e.g. btrfs subvolume crossing st_dev missed it): catch and downgrade
fn try_hardlink(src: &Path, dst: &Path) -> Result<DeployMethod> {
    match std::fs::hard_link(src, dst) {
        Ok(()) => Ok(DeployMethod::Hardlink),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices
                  || e.raw_os_error() == Some(18) => { symlink_or_copy(src, dst)?; Ok(DeployMethod::SymlinkOrCopy) }
        Err(e) => Err(e.into()),
    }
}
```
**btrfs caveat:** `check_reflink_support` is the right empirical probe — on btrfs subvolumes `st_dev` may differ even on the same disk (so reflink/hardlink may be impossible across them), which is exactly why the empirical probe and the EXDEV catch are both required, not just the `st_dev` compare.

### Pattern 3: Backup-Before-Overwrite via Content-Addressed Vanilla Store
**What:** Before deploying a file whose target path already exists in the game tree **and was not deployed by NexTwist**, copy the original into a per-game store keyed by content hash, record `(target_relpath → hash)` in the manifest. Purge restores it. This is the single most important safety mechanism — corruption here is unrecoverable except via Steam re-verify.
**When to use:** DEPLOY-04, DEPLOY-01. Always, on every overwrite.
**Example:**
```rust
// vanilla store layout:  <app_data>/originals/<appid>/<blake3-hex>
fn backup_vanilla_if_absent(conn:&Connection, op:&FileOp) -> Result<()> {
    if !op.target.exists() { return Ok(()); }            // pure add, nothing to back up
    if is_ours(conn, &op.target)? { return Ok(()); }     // we deployed it; not vanilla
    let hash = blake3_file(&op.target)?;
    let store_path = originals_dir(op.appid).join(hash.to_hex().as_str());
    if !store_path.exists() { std::fs::copy(&op.target, &store_path)?; } // content-addressed dedupe
    conn.execute("INSERT OR IGNORE INTO vanilla_backup(appid,target,hash) VALUES(?1,?2,?3)",
                 params![op.appid, op.target_rel, hash.to_hex().as_str()])?;
    Ok(())
}
```

### Anti-Patterns to Avoid
- **Business logic inside `#[tauri::command]`:** untestable headless; violates locked decision. Commands are 3–10 line adapters.
- **Purging by directory scan ("delete what looks like a mod"):** deletes user/vanilla files. Purge ONLY manifest-recorded paths; scan is for *drift warnings* only.
- **Writing the manifest row *after* the syscall with no journal:** any crash → orphans + un-restorable purge. Always intent-before-act.
- **Global method choice ("this machine uses hardlinks"):** btrfs subvolumes / second-drive libraries break per-machine assumptions. Probe per target pair.
- **Symlinking whole directories into `Data/`:** a Steam update can write *through* the link into staging; Wine path translation mishandles dir symlinks. Deploy per-file.
- **`reflink_or_copy` for deployment:** it silently copies on failure, hiding the method from your manifest/ladder. Use explicit `reflink` and control fallback yourself.
- **Extracting directly into the game tree or staging without temp-then-move:** a malicious or partially-extracted archive corrupts state. Extract to temp, validate, move.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Reflink/CoW support detection | Custom FICLONE ioctl wrappers | `reflink_copy::check_reflink_support` + `reflink` | Handles FICLONE/FICLONERANGE, per-OS quirks, Btrfs/XFS/bcachefs; battle-tested |
| Steam library enumeration | Hand-parsing `libraryfolders.vdf` | `steamlocate` 2.1 (`locate_all`, `find_app`) | Multi-library, multi-drive, returns `(App, Library)`; you only hand-derive compatdata |
| ACF/VDF field parsing | Regex over `.acf` | `keyvalues-serde` | Valve KeyValues is nested + quoted; serde-deserialize the fields steamlocate omits |
| Zip path-traversal defense | Manual `..` string checks | `ZipFile::enclosed_name()` + re-canonicalize-under-root + reject symlink entries | Naive `..` checks miss symlink-write-through (CVE-2025-29787); enclosed_name is the vetted primitive |
| 7z decoding | Anything | `sevenz-rust2` `ArchiveReader` | Pure-Rust LZMA/LZMA2/BCJ; iterate `ArchiveEntry` to validate before writing |
| Content hashing | Custom | `blake3` | Fast on multi-GB textures; one-call file hashing |
| Schema migrations | Ad-hoc `CREATE TABLE IF NOT EXISTS` | `refinery` | Versioned, ordered, protects existing user DBs across releases |
| Crash-safe DB | Custom journal file | SQLite **WAL mode** (for DB) + your op-journal table (for *fs* ops) | SQLite's WAL handles DB durability; you only add the fs-intent layer it can't cover |
| RAR extraction | Bundling `unrar`/`unrar_sys` | Shell out to system `unrar`/`7z` | Non-free license blocks AppImage distribution (DIST-02); enforce ban with cargo-deny |

**Key insight:** The product's entire value is *correctness under failure*. The crates above are mature and audited; the thing you genuinely must hand-write — and test exhaustively — is the **journaling/idempotency protocol gluing fs syscalls to the DB**, because no crate can know your safety invariant ("game returns byte-for-byte pristine").

## Runtime State Inventory

> This is a greenfield phase (repo contains only `.planning/`, `.claude/`, `LICENSE`, `README.md`). No rename/refactor/migration. The categories below are documented for completeness because the *engine being built* creates persistent runtime state Phase 2+ must respect.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | None pre-existing. Phase 1 *creates*: SQLite DB (game registry, manifest, op-journal, vanilla_backup), per-game original-store dir, per-mod staging trees | New schema via refinery migration v1; design for forward-compat |
| Live service config | None | None — no external services in Phase 1 (no NexusMods networking) |
| OS-registered state | None in Phase 1 (nxm:// MIME handler + AppImage are Phase 3/5) | None; workspace layout must not preclude later `.desktop` registration |
| Secrets/env vars | None (no auth until Phase 3). Respect `STEAM_COMPAT_DATA_PATH` env if present when resolving prefix | Read-only consumption of Steam env vars; no secrets stored |
| Build artifacts | None pre-existing (no Rust toolchain even installed on this machine) | Phase 1 scaffolds `Cargo.lock`, `target/`, `node_modules/`; add to `.gitignore` |

**Nothing found in:** all categories pre-existing — confirmed greenfield by `ls`. The note is that Phase 1 *introduces* the canonical persistent state (DB schema, original-store, staging layout) that all later phases depend on, so schema/path decisions here are load-bearing.

## Common Pitfalls

(These are the Phase-1-relevant subset of `.planning/research/PITFALLS.md`, deepened with verified detail.)

### Pitfall 1: btrfs subvolume / second-drive EXDEV breaks hardlink — invisible on a single ext4 partition
**What goes wrong:** `std::fs::hard_link` returns `EXDEV` (errno 18) across filesystems — and **btrfs treats each subvolume as a separate device**, so even same-disk `@home`-staging → `@`-game fails. The dev machine here is btrfs, so this *will* surface in dev.
**Why it happens:** Devs test on one ext4 partition; the modal Linux-gamer setup (separate mods drive, btrfs subvolumes on Fedora/SteamOS) is the failure case.
**How to avoid:** Per-target `st_dev` compare (`MetadataExt::dev()`) + `check_reflink_support` + a real throwaway `hard_link` probe at game-add; catch `ErrorKind::CrossesDevices` / `raw_os_error()==Some(18)` and downgrade to symlink/copy; auto-suggest staging on the same fs as the game.
**Warning signs:** `EXDEV` / "Invalid cross-device link" in logs; deploy works in CI on tmpfs but not on the dev's btrfs home.

### Pitfall 2: Non-atomic manifest → orphans → purge doesn't restore pristine (the product-defining failure)
**What goes wrong:** App crashes mid-deploy; some files written but not recorded (orphans on purge) or recorded but not written (purge errors). "Purge" then leaves a non-pristine game — the exact thing the product exists to prevent. Vortex ships a "Repair" feature precisely because this happens.
**How to avoid:** Operation-journal intent-before-act (Pattern 1); content hash + provenance per file; idempotent purge (missing = no-op); auto verify/repair after abnormal exit.
**Warning signs:** files left after full purge; manifest rows pointing at non-existent files; deploy and manifest-write are separate non-atomic steps.

### Pitfall 3: Overwriting a vanilla file with no backup = unrecoverable corruption
**What goes wrong:** A mod replaces a base asset (very common: textures, meshes, `.ini`, base ESMs). Overwrite-in-place with no prior backup → purge has nothing to restore; only Steam re-verify fixes it. With hardlinks, replacing the original can also affect the staged copy depending on order — which is exactly why staged files are marked read-only and reflink (independent inode) is preferred.
**How to avoid:** Backup-before-overwrite into content-addressed vanilla store (Pattern 3); distinguish vanilla / NexTwist-deployed / user-added in the manifest.

### Pitfall 4: Wine case-sensitivity — mod ships `Textures/`, game opens `textures/`, file "not found"
**What goes wrong:** Wine/Proton does NOT abstract the filesystem; a Windows `open("Data\\Textures\\x.dds")` maps to a Linux `open()` on case-sensitive ext4/btrfs. Creation Engine mods are notoriously mixed-case (authored on case-insensitive NTFS), so some load and some silently don't.
**How to avoid (locked approach):** Normalize each incoming mod path's casing against the per-game **canonical `Data/` directory casing** at deploy time (the `steam` crate provides the canonical casing map: `Data`, `Textures`, `Meshes`, `Scripts`, `Interface`, etc.). Build a case map and rewrite. (ext4 casefold +F is the deferred stronger alternative.)
**Warning signs:** "deploy succeeded" but mod has no in-game effect; works for correctly-cased mods only.

### Pitfall 5: Wrong Proton prefix — steamlocate does NOT give you compatdata
**What goes wrong:** Bethesda reads its `plugins.txt`/AppData from inside the Proton prefix, not native HOME. steamlocate returns the library + install dir but **exposes no compatdata API**, so a dev may hardcode a wrong path. Flatpak relocates everything under `~/.var/app/com.valvesoftware.Steam`. (plugins.txt management is Phase 2, but **prefix resolution is Phase 1 / ENV-02**.)
**How to avoid:** Derive `prefix = <library.path>/steamapps/compatdata/<appid>/pfx` manually from the `(App, Library)` steamlocate returns; handle Flatpak/Snap roots; respect `STEAM_COMPAT_DATA_PATH`; re-resolve each session (Steam can rebuild prefixes). The Bethesda AppData path inside the prefix is `pfx/drive_c/users/steamuser/Local Settings/Application Data/<Game>/` (with `AppData/Local/<Game>/` typically a Wine junction to it) — relevant for Phase 2; in Phase 1 you only need to *resolve and display* the prefix root.
**Warning signs:** hardcoded `steamuser` or `~/.local/share/Steam`; Flatpak users see no resolved prefix.

### Pitfall 6: Zip-slip / malicious symlink archive (RCE / arbitrary file write)
**What goes wrong:** A mod archive contains `../../../.bashrc`, an absolute path, or a symlink entry that *later* entries write through (CVE-2025-29787, the symlink-write-through bypass — fixed in `zip` 2.3.0, so 8.x is patched, but you must still defend).
**How to avoid:** For every entry: get `enclosed_name()` (rejects `..`/absolute), then re-canonicalize the joined destination and assert it's still under the extraction root; **reject symlink entries explicitly** (do not create them, do not follow them when writing later entries); extract to a temp dir, validate, then move into staging; include a crafted zip-slip + symlink test fixture. For 7z, use `sevenz-rust2`'s `ArchiveReader`/`ArchiveEntry` iteration to validate each entry path before decoding it.
**Warning signs:** extraction joins entry paths without re-canonicalizing; no malicious-archive fixture; following symlinks during unpack.

## Code Examples

### Resolve a managed Bethesda game (install dir + Proton prefix)
```rust
// Source: steamlocate 2.1 docs.rs (find_app -> Result<Option<(App, Library)>>)  [CITED: docs.rs/steamlocate]
use steamlocate::SteamDir;

const SKYRIM_SE: u32 = 489830;
const FALLOUT4:  u32 = 377160;

struct ResolvedGame { appid: u32, install_dir: PathBuf, prefix: PathBuf }

fn resolve_game(appid: u32) -> anyhow::Result<ResolvedGame> {
    let steam = SteamDir::locate()?;                       // also: locate_all() for Flatpak/Snap roots
    let (app, library) = steam.find_app(appid)?
        .ok_or_else(|| anyhow::anyhow!("app {appid} not installed"))?;
    let lib = library.path();                              // .../steamapps' parent
    let install_dir = lib.join("steamapps/common").join(&app.install_dir);
    // steamlocate exposes NO compatdata API — derive the prefix path ourselves:
    let prefix = lib.join("steamapps/compatdata").join(appid.to_string()).join("pfx");
    Ok(ResolvedGame { appid, install_dir, prefix })
}
// Flatpak root:  ~/.var/app/com.valvesoftware.Steam/.steam/steam
// Snap root:     ~/snap/steam/common/.steam/steam (verify on a Snap install)
// Also honor $STEAM_COMPAT_DATA_PATH if set.
```

### Safe zip extraction (zip 8.x)
```rust
// Source: zip 8.x enclosed_name + CVE-2025-29787 guidance  [CITED: github.com/advisories/GHSA-94vh-gphv-8pm8]
use zip::ZipArchive;
fn extract_zip_safe(archive: &Path, temp_root: &Path) -> anyhow::Result<()> {
    let mut zip = ZipArchive::new(std::fs::File::open(archive)?)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        // enclosed_name() returns None for absolute paths / paths escaping via ..
        let rel = entry.enclosed_name().ok_or_else(|| anyhow::anyhow!("unsafe entry path"))?;
        // explicitly reject symlink entries (the CVE-2025-29787 vector)
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            if mode & 0o170000 == 0o120000 { anyhow::bail!("symlink entry rejected"); }
        }
        let dest = temp_root.join(&rel);
        // belt-and-braces: assert the canonicalized parent stays under temp_root
        let parent = dest.parent().unwrap();
        std::fs::create_dir_all(parent)?;
        let canon_parent = parent.canonicalize()?;
        anyhow::ensure!(canon_parent.starts_with(temp_root.canonicalize()?), "escapes root");
        if entry.is_dir() { std::fs::create_dir_all(&dest)?; }
        else { let mut out = std::fs::File::create(&dest)?; std::io::copy(&mut entry, &mut out)?; }
    }
    // caller then validates & moves temp_root -> staging
    Ok(())
}
```

### 7z extraction with per-entry validation (sevenz-rust2 0.21)
```rust
// Source: sevenz-rust2 0.21 docs.rs (ArchiveReader / ArchiveEntry / decompress_file_with_extract_fn)  [CITED: docs.rs/sevenz-rust2]
// Prefer the low-level ArchiveReader to inspect ArchiveEntry paths before writing,
// or decompress_file_with_extract_fn(...) to interpose a per-entry validate-then-write closure
// that applies the SAME enclosed-name/symlink-reject checks as the zip path.
```

### Detect filesystem capabilities at game-add
```rust
// Source: std::os::unix::fs::MetadataExt + reflink_copy::check_reflink_support  [CITED: docs.rs]
use std::os::unix::fs::MetadataExt;
struct FsCaps { same_device: bool, reflink: ReflinkSupport, hardlink_ok: bool }
fn probe(staging: &Path, game_data: &Path) -> std::io::Result<FsCaps> {
    let same_device = std::fs::metadata(staging)?.dev() == std::fs::metadata(game_data)?.dev();
    let reflink = check_reflink_support(staging, game_data).unwrap_or(ReflinkSupport::Unknown);
    // real throwaway hardlink probe (most reliable for btrfs-subvolume EXDEV):
    let hardlink_ok = try_throwaway_hardlink(staging, game_data).is_ok();
    Ok(FsCaps { same_device, reflink, hardlink_ok })
}
// casefold probe (ext4 +F): FS_IOC_GETFLAGS ioctl on the dir, test FS_CASEFOLD_FL (0x40000000).
// If casefolded, case normalization may be unnecessary — but normalize anyway for portability.
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| EXDEV detection via `raw_os_error()==18` only | `io::ErrorKind::CrossesDevices` | Rust 1.85 (stable) | Portable, readable; still keep raw_os_error as belt-and-braces |
| `sevenz-rust` (original) | `sevenz-rust2` | 2025 fork | Original abandoned; use the maintained fork |
| `zip` 1.3.0–2.2.x (zip-slip symlink bypass) | `zip` ≥2.3.0 (8.x) | 2025 (CVE-2025-29787 fix) | 8.x is patched; still validate entries + reject symlinks |
| MO2 USVFS virtual filesystem | Vortex-model real link deployment + manifest | — | USVFS is Windows-only; not viable on Linux (locked project decision) |
| `reflink` crate | `reflink-copy` | — | reflink-copy is maintained, adds `check_reflink_support` |

**Deprecated/outdated:**
- `unrar`/`unrar_sys` crates — non-free RAR source; never bundle (cargo-deny ban).
- `reflink_or_copy` for deployment — silently copies, hiding method from manifest; use only as a deliberate last-rung fallback, never as the probe.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `ReflinkSupport` variants are `Supported` / `Unsupported` / `Unknown` | Pattern 2 / probe | LOW — exact variant names must be confirmed against `reflink-copy` 0.1.30 source at code time; match-arm names may differ (e.g. `NotSupported`). Behavior/intent verified. |
| A2 | Snap Steam root is `~/snap/steam/common/.steam/steam` | resolve_game comment | MEDIUM — Flatpak root (`~/.var/app/com.valvesoftware.Steam`) is cross-confirmed; Snap path is less-attested. Verify on a real Snap install or treat Snap as manual-add-only for Phase 1. |
| A3 | sevenz-rust2 0.21 exposes `decompress_file_with_extract_fn` for per-entry interposition | 7z example | LOW–MEDIUM — `ArchiveReader`/`ArchiveEntry`/`decompress_file*` confirmed via docs.rs summary; the exact extract-fn signature must be read from docs at code time. Fallback: iterate `ArchiveReader` entries manually. |
| A4 | Skyrim SE = 489830, Fallout 4 = 377160 | resolve_game | LOW — widely attested; both confirmed in multiple community sources. |
| A5 | btrfs subvolumes report differing `st_dev` (so the st_dev compare catches them) | Pitfall 1 / probe | LOW — well-documented btrfs behavior; the empirical hard_link probe is the authoritative backstop regardless. |
| A6 | FS_CASEFOLD_FL = 0x40000000 via FS_IOC_GETFLAGS | probe casefold comment | MEDIUM — flag value/ioctl path should be confirmed against `<linux/fs.h>` at code time; casefold detection is a *nice-to-have* warning in Phase 1 (normalization is the locked primary approach), so low blast radius. |
| A7 | `synchronous=NORMAL` is sufficient with WAL for crash-safety of the journal | Pattern 1 | MEDIUM — NORMAL is safe against app crashes but a power-loss corner case may want FULL for the journal commits. Recommend FULL for the intent-commit, NORMAL elsewhere; confirm with a power-loss-simulation test. |

## Open Questions (RESOLVED)

> All three are dispositioned with a concrete recommendation and threaded into executable Phase 1 tasks. None blocks the phase goal.

1. **Exact `ReflinkSupport` enum variant names (0.1.30).**
   - What we know: `check_reflink_support(from, to) -> Result<ReflinkSupport>` exists; an "Unknown" variant is referenced in docs.
   - What's unclear: whether the negative variant is `Unsupported` or `NotSupported`.
   - Recommendation: read `reflink-copy` 0.1.30 source when writing `probe.rs`; trivial to confirm, no plan impact.
   - **RESOLVED:** carried as an in-code TODO in Plan 01-04 (`probe.rs`) per Assumption A1 — confirm at code time; not a plan blocker.

2. **Does Skyrim/FO4 under Proton tolerate symlinked *mod asset files* (not plugins.txt)?**
   - What we know: plugins.txt specifically must NOT be symlinked (Skyrim won't follow it) — but that's Phase 2. Hardlink/reflink are preferred for assets anyway.
   - What's unclear: empirical confirmation that symlinked loose texture/mesh files load under Proton for these two games.
   - Recommendation: keep symlink as the cross-FS fallback rung; add a Phase-1 manual UAT "known test mod loads in-game" check on a real Proton install. Reflink/hardlink (same-fs) is the happy path and avoids the question entirely.
   - **RESOLVED:** symlink retained as the cross-FS fallback rung in the method ladder (Plan 01-04); empirical confirmation deferred to Plan 01-06 manual UAT. Same-fs reflink/hardlink is the happy path and sidesteps the question.

3. **synchronous PRAGMA level for the operation journal.**
   - What we know: WAL + NORMAL survives app crashes; FULL adds power-loss durability at a perf cost.
   - Recommendation: FULL for the intent-commit transaction, NORMAL for bulk manifest writes; validate with a crash/power-loss-simulation test (kill -9 mid-deploy, then relaunch and assert pristine).
   - **RESOLVED:** `synchronous=FULL` set for the intent-commit path in `store::open` (Plan 01-01); validated by the `crash_recovery` centerpiece test (Plan 01-04).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain (rustc/cargo) | All backend build/test | ✗ | — | **Install via rustup (≥1.85 for `CrossesDevices`)** — blocking for build, trivial to install |
| Node.js / npm | Tauri scaffold + Svelte frontend | ✓ | node v22.22.2 / npm 10.9.7 | — |
| `7z` binary | `.rar` extraction (STAGE-03), 7z fallback | ✓ | /usr/bin/7z present | — |
| `unrar` binary | `.rar` extraction (preferred) | ✗ | — | Use system `7z` (present) or clear "install unrar" error — by design (no bundled non-free code) |
| btrfs filesystem (dev `/home`) | EXDEV/reflink testing | ✓ | btrfs on /dev/nvme1n1p3[/home] | — (this is an asset: dev machine exercises the hardest fs case) |
| `libwebkit2gtk-4.1-dev` etc. | Tauri build (Linux) | ? | not checked (Phase 1 backend can be built/tested headless without webview) | Install at Tauri-shell build time; core crates need none of it |

**Missing dependencies with no fallback (blocking):**
- **Rust toolchain not installed** — the planner must include an install/setup step (rustup, toolchain ≥1.85) before any crate work. This is the only hard blocker and is trivially resolved.

**Missing dependencies with fallback:**
- `unrar` absent → system `7z` (present) handles `.rar`; if neither present, clear error (the locked design).
- WebKitGTK dev libs not verified → only needed for the Tauri *shell* build, not the headless core; install when wiring the UI.

## Validation Architecture

> nyquist_validation is enabled (config). This phase is safety-critical; tests are a locked first-class deliverable.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + `proptest` 1.11 (property tests) + `tempfile` 3.27 (isolated temp dirs) |
| Config file | none — Cargo test discovery (`crates/*/tests/` + inline `#[cfg(test)]`) — see Wave 0 |
| Quick run command | `cargo test -p deploy --lib` (per-crate fast unit/property loop) |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DEPLOY-01/02/03 | Deploy→purge leaves game byte-for-byte pristine | integration (proptest over random file trees) | `cargo test -p deploy round_trip_pristine` | ❌ Wave 0 |
| DEPLOY-04 | Replaced vanilla file is backed up + restored on purge | integration | `cargo test -p deploy vanilla_restore` | ❌ Wave 0 |
| DEPLOY-05 | Per-target method ladder + EXDEV fallback | unit + integration (tmpfs vs second temp fs) | `cargo test -p deploy method_ladder` | ❌ Wave 0 |
| DEPLOY-06 | Crash-mid-deploy → relaunch recovers to consistent/pristine | integration (kill/abort injection mid-op + replay) | `cargo test -p deploy crash_recovery` | ❌ Wave 0 (CENTERPIECE) |
| DEPLOY-07 | verify/repair detects manifest-vs-disk drift + orphans | unit | `cargo test -p deploy verify_drift` | ❌ Wave 0 |
| DEPLOY-08 | Mixed-case mod path normalized vs canonical Data/ | unit | `cargo test -p deploy casefold_normalize` | ❌ Wave 0 |
| STAGE-02 | Crafted zip-slip + symlink archive rejected | unit (fixture archives) | `cargo test -p extract zip_slip_rejected` | ❌ Wave 0 |
| STAGE-01/03 | .zip/.7z extract to staging; .rar via system tool/clear error | integration | `cargo test -p extract extract_formats` | ❌ Wave 0 |
| ENV-01/02/03 | Resolve Skyrim SE/FO4 install dir + prefix from fixture Steam layout | integration (synthetic libraryfolders.vdf/acf fixtures) | `cargo test -p steam resolve_game` | ❌ Wave 0 |
| ENV-04 | fs-capability probe reports same-device/reflink/casefold correctly | unit | `cargo test -p deploy fs_probe` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p <crate> --lib` for the touched crate (fast).
- **Per wave merge:** `cargo test --workspace`.
- **Phase gate:** Full suite green + the crash-recovery and round-trip-pristine integration tests passing on **at least tmpfs + the dev btrfs** before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `crates/deploy/tests/round_trip_pristine.rs` — DEPLOY-01/02/03 (proptest harness asserting hash-equal vanilla after install→purge)
- [ ] `crates/deploy/tests/crash_recovery.rs` — DEPLOY-06 (abort-injection mid-op; relaunch replay; assert pristine) — the centerpiece
- [ ] `crates/deploy/tests/method_ladder.rs` — DEPLOY-05 (create a second temp filesystem/loopback to force EXDEV)
- [ ] `crates/deploy/tests/vanilla_restore.rs`, `verify_drift.rs`, `casefold_normalize.rs`, `fs_probe.rs`
- [ ] `crates/extract/tests/zip_slip_rejected.rs` + crafted-archive fixtures (`tests/fixtures/*.zip`, `*.7z` with `..`, absolute, symlink entries)
- [ ] `crates/steam/tests/resolve_game.rs` + synthetic Steam-layout fixture dirs (libraryfolders.vdf, appmanifest_489830.acf, compatdata/)
- [ ] Shared test helper crate or module: builds a fake vanilla game tree, a staged mod, asserts byte-for-byte equality via blake3.
- [ ] Framework install: `rustup` + toolchain ≥1.85 (blocking — see Environment Availability).

## Security Domain

> security_enforcement enabled, ASVS level 1. The threat surface in Phase 1 is **untrusted local mod archives** (third-party content) and **filesystem safety** — there is no network, no auth, no user accounts in this phase.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | No auth until Phase 3 (NexusMods OAuth) |
| V3 Session Management | no | N/A this phase |
| V4 Access Control | partial | App writes only under resolved game dir + app-data dir; never outside; vanilla store is app-private |
| V5 Input Validation | **yes** | Archive entry path validation (`enclosed_name` + canonicalize-under-root + reject symlink/abs/`..`); validate user-supplied "add game by folder" path is a real game dir |
| V6 Cryptography | partial (integrity, not secrecy) | `blake3`/`sha2` for content integrity/pristine assertion — never hand-roll hashing; no secrets stored |
| V12 File & Resources | **yes** | Extract-to-temp-then-move; reject path traversal; do not follow symlinks; least-privilege file writes |

### Known Threat Patterns for {Rust headless core + local archive ingestion}

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Zip-slip path traversal (`../`, absolute paths) | Tampering / Elevation | `ZipFile::enclosed_name()` + re-canonicalize joined dest under extraction root |
| Symlink-write-through in archive (CVE-2025-29787) | Tampering / Elevation | Reject symlink entries; never create/follow them during extraction; `zip` ≥8.x (patched) |
| Malicious archive overwrites files outside staging | Tampering | Extract to temp, validate every entry, then move into staging — never extract into game tree |
| Crash-induced game corruption (lost vanilla file) | Denial of Service (to the user's game) | Backup-before-overwrite + journaled idempotent recovery (DEPLOY-04/06) |
| `.rar` via system tool command injection | Elevation | Pass archive path as an argv element via `Command` (no shell string interpolation); validate path |
| TARmageddon-class tar traversal (if tar added) | Tampering | Apply same per-entry validation if/when tar formats are supported (rare; not in Phase 1 scope) |
| cargo-deny bypass → non-free/vulnerable dep slips in | Compliance | `cargo-deny` advisories + licenses check in CI; ban `unrar`/`unrar_sys`; pin `zip` ≥8 |

## Sources

### Primary (HIGH confidence)
- crates.io API (`api/v1/crates/*`) — verified current max_stable versions for all 16 crates on 2026-06-20 (match pinned STACK.md exactly)
- `gsd-tools query package-legitimacy check` — all 12 audited crates verdict OK
- docs.rs/steamlocate 2.1.0 — `SteamDir::locate`/`locate_all`, `find_app(appid) -> Result<Option<(App, Library)>>`, no compatdata API
- docs.rs/reflink-copy 0.1.30 — `reflink`, `reflink_or_copy`, `check_reflink_support(from,to) -> Result<ReflinkSupport>`
- docs.rs/sevenz-rust2 0.21.0 — `ArchiveReader`, `ArchiveEntry`, `decompress_file*` family
- Rust release notes / rust-lang#130209 — `io::ErrorKind::CrossesDevices` stabilized in 1.85 (EXDEV / errno 18)

### Secondary (MEDIUM confidence)
- GitHub Advisory GHSA-94vh-gphv-8pm8 / CVE-2025-29787 — zip path-traversal via symlink, fixed in zip 2.3.0
- kernel.org ext4 admin guide + LWN "Case-insensitive ext4" + Collabora casefold blog — casefold (+F) detection/semantics
- ValveSoftware/Proton #7418 + Steam Community + ProtonDB 489830 — Flatpak compatdata relocation, prefix layout
- Nexus/Step Mods + MO2 docs — Bethesda mod archive `Data/`-rooted layout; loose-file structure
- sqlite.org/pragma.html + sqlite.org/tempfiles.html — WAL mode crash recovery semantics

### Tertiary (LOW confidence)
- Steam Community threads — Snap Steam root path (flagged A2; verify or treat Snap as manual-add in Phase 1)
- WineHQ/forum precedent — plugins.txt-not-symlinkable (Phase 2 relevance only)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all versions verified against crates.io + legitimacy-audited; match pinned STACK.md.
- Architecture/journaling design: HIGH — synthesized from SQLite WAL semantics + established idempotent-op pattern + NexusMods.App/Vortex precedent; the *protocol* is sound, exact PRAGMA tuning flagged (A7).
- Crate APIs: HIGH for shapes verified on docs.rs; two exact-name details flagged (A1 ReflinkSupport variants, A3 7z extract-fn signature) for code-time confirmation.
- Platform facts (Proton/EXDEV/casefold): MEDIUM-HIGH — cross-checked across kernel docs + multiple community sources; Snap root LOW (A2).

**Research date:** 2026-06-20
**Valid until:** ~2026-07-20 (crate ecosystem moves; re-verify zip/sevenz-rust2/reflink-copy versions if planning slips a month)
