# Walking Skeleton — NexTwist

**Phase:** 1
**Generated:** 2026-06-20

## Capability Proven End-to-End

> The smallest user-visible capability that exercises the full stack.

"A user launches NexTwist, detects (or adds-by-folder) a supported Bethesda Steam-Proton game, installs one local mod archive into staging, deploys it non-destructively into the game's `Data/` folder, then purges it and the game folder returns byte-for-byte to vanilla — surviving a crash mid-deploy."

This spine — **detect → install-to-staging → deploy → purge-to-pristine, crash-safe** — is the irreplaceable Core Value of the product. Every later phase thickens this spine; it does not replace it.

## Architectural Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Workspace layout | Multi-crate Cargo workspace: a pure headless core under `crates/*` + a thin `src-tauri/` shell | Locked (CONTEXT.md). The safety-critical engine has ZERO Tauri deps so it is unit/property-testable headless in CI without a webview. |
| Core crates | `core` (domain types), `store` (persistence), `steam` (Proton/Steam discovery), `extract` (archive→staging), `deploy` (the crown jewel), `testkit` (test helpers) | One concern per crate; `deploy` stays dependency-light and Proton-agnostic (`steam` quarantines all Proton-layout knowledge and hands it resolved absolute paths). |
| Data layer | SQLite via **rusqlite (bundled)** + **refinery** migrations, WAL mode, `synchronous=FULL` for journal commits | Locked. Statically links SQLite → clean AppImage (Phase 5). Holds the game registry, the per-file deploy **manifest**, the **operation journal**, and the content-addressed **vanilla backup** table. |
| Crash-safety model | Explicit **operation-journal** table (intent recorded `pending` before the syscall, flipped to `done` after) + **idempotent** file ops + **replay on launch** | A `link()` syscall and its DB row cannot be made atomic together; SQLite WAL alone is insufficient. Idempotency makes replay-after-crash always safe. This is DEPLOY-06 and the test centerpiece. |
| Deploy method | Per-target runtime probe → **reflink → hardlink → symlink → copy** ladder, chosen per (staging, target) pair, with EXDEV/`CrossesDevices` fallback | Locked. Dev machine is **btrfs** → `link()` returns EXDEV across subvolumes even same-disk; method must be chosen per-target, never globally. Per-file links only (never directory symlinks). |
| Vanilla safety | **Backup-before-overwrite** into a content-addressed (`blake3`) per-game original store; staged files marked **read-only**; **reflink preferred** (independent inode) | The single most important safety mechanism — corruption here is otherwise only fixable by Steam re-verify. |
| Archive extraction | `zip` 8 + `sevenz-rust2` 0.21 natively; `.rar` via **system `unrar`/`7z`** (argv, never shell); extract-to-temp-then-move; per-entry validation (`enclosed_name` + canonicalize-under-root + reject symlink/abs/`..`) | Locked + security-critical (untrusted third-party content is the entire Phase 1 threat surface). NO non-free UnRAR code bundled (cargo-deny ban) → pre-positions DIST-02. |
| Case handling | Normalize each mod path's casing against the per-game **canonical `Data/` casing map** at deploy time | Wine is case-sensitive on Linux; mixed-case Bethesda mods otherwise silently fail to load. ext4 casefold (+F) is a deferred stronger alternative. |
| Tauri boundary | Tauri 2.11 commands are **thin 3-10 line adapters** delegating to headless crates; zero business logic in `#[tauri::command]` | Locked. Keeps safety logic testable headless and out of the UI. |
| Frontend | **Svelte 5 + TypeScript**, SvelteKit adapter-static SPA, functional-minimal | Locked. Smallest bundle / fastest cold start on WebKitGTK. Visual polish deferred until the core loop is proven. |
| Toolchain | Rust **stable >= 1.85 (2024 edition)**, pinned via `rust-toolchain.toml` | `io::ErrorKind::CrossesDevices` stabilized in 1.85 (required for EXDEV detection). Toolchain was NOT installed → Plan 01 installs it (the one hard blocker). |
| Supply-chain gate | `cargo-deny` (`deny.toml`) bans `unrar`/`unrar_sys`, advisories + license allow-list; run in CI | Enforces "no non-free RAR code bundled" from day one; pre-positions the Phase 5 license-compliance audit (DIST-02). |

## Stack Touched in Phase 1

- [x] Project scaffold (Cargo workspace, rust-toolchain, deny.toml, CI, SvelteKit SPA) — Plan 01 + Plan 06
- [x] Routing — the single functional Svelte route wiring the round-trip — Plan 06
- [x] Database — real reads AND writes: game registry, manifest, op-journal, vanilla store (rusqlite + refinery V1) — Plan 01, exercised by Plans 04/05
- [x] UI — interactive Deploy/Purge/Install buttons wired to Tauri commands → headless engine — Plan 06
- [x] Local full-stack run — `cargo tauri dev` launches the app and exercises detect → install → deploy → purge (no remote deploy target this phase; AppImage is Phase 5)

## Out of Scope (Deferred to Later Slices)

> Explicit — prevents future phases from re-litigating Phase 1's minimalism.

- Multi-mod conflict resolution, mod priority / load order, multiple profiles per game — **Phase 2**
- Plugin (.esp/.esm/.esl) enable/disable, plugins.txt management in the prefix, LOOT auto-sort — **Phase 2**
- Any NexusMods networking: OAuth login, secure token storage, in-app + `nxm://` downloads, rate limiting — **Phase 3**
- FOMOD guided installers and end-to-end NexusMods Collection install/deploy/uninstall — **Phase 4**
- AppImage packaging + `nxm://` MIME handler registration + license-compliance audit run — **Phase 5** (cargo-deny ban is set up now; the audit *run* is Phase 5)
- ext4 casefold (+F) deploy tree (stronger case handling); reflink validated per-game as first-class default — **v2**
- Visual/styled UI polish — after the core loop is proven
- Archive-invalidation `.ini` handling, Steam-update staleness reconcile — **Phase 2 / later** (prefix resolution itself is Phase 1)

## Subsequent Slice Plan

Each later phase adds one vertical slice on top of this skeleton without altering its architectural decisions (the workspace layout, the journal/manifest/vanilla schema, and the deploy-method ladder are contracts):

- **Phase 2 — Multi-Mod Management:** conflict resolution + priority/load order + plugin ordering + per-game profiles, on top of the proven single-mod deploy engine.
- **Phase 3 — NexusMods Login & Download:** OAuth + keyring tokens + in-app/`nxm://` downloads feeding the existing `extract` → staging → `deploy` pipeline.
- **Phase 4 — Guided Installers & Collections:** FOMOD wizard + Collection install/deploy/uninstall, replaying choices through the Phase 2 conflict/order engine and Phase 3 downloads.
- **Phase 5 — AppImage Distribution:** package as a single-file AppImage with the `nxm://` handler registered; run the cargo-deny license-compliance audit (no non-free code) to ship.
