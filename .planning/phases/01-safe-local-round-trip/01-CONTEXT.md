# Phase 1: Safe Local Round-Trip - Context

**Gathered:** 2026-06-20
**Status:** Ready for planning
**Mode:** Smart discuss (autonomous) — 16 decisions across 4 areas, all recommended answers accepted

<domain>
## Phase Boundary

This phase delivers the **complete reversible-deployment safety core** end-to-end on LOCAL mod archives, with no NexusMods networking. A user can:

1. Have NexTwist auto-detect installed Steam games, add a supported Bethesda game (Skyrim SE / Fallout 4) as managed, and see its resolved install directory and Proton prefix paths.
2. Install a local mod archive (.zip / .7z / .rar-via-system-tool) into a managed staging store, with malicious (zip-slip / symlink / `..`) entries safely rejected.
3. Deploy the enabled mod into the game with **zero original game files modified in place**, every deployed file recorded in a per-game manifest, and any overwritten vanilla file backed up first.
4. Purge/uninstall and verify (hash-diff) the game folder is **byte-for-byte pristine** — no orphans, originals restored — even after an interrupted (crash-mid-deploy) operation.
5. Be warned before deploying when the filesystem configuration is unsafe (cross-device/EXDEV, case-folding); NexTwist selects a safe method (reflink → hardlink → symlink → copy) and resolves case mismatches so the mod loads under Proton.

**Requirements covered:** ENV-01..04, STAGE-01..03, DEPLOY-01..08 (15 requirements).

**Explicitly out of scope for this phase:** multi-mod conflict resolution / load order / profiles (Phase 2), any NexusMods auth/download/nxm:// (Phase 3), FOMOD/Collections (Phase 4), AppImage packaging (Phase 5), plugin (.esp/.esm/.esl) load-order management and LOOT (Phase 2). Single mod at a time is sufficient to prove the round-trip.

</domain>

<decisions>
## Implementation Decisions

### Architecture & Code Structure
- **Multi-crate Cargo workspace** with a pure, headless Rust core under `crates/` (e.g. `deploy`, `steam`, `store`, `extract`) plus a thin Tauri shell. The safety-critical engine has **zero Tauri dependencies** so it is unit/property-testable headless in CI without a webview.
- **Tauri commands are thin 3–10 line adapters** that delegate to the headless core — no business logic in `#[tauri::command]` functions.
- **Database layer: rusqlite (bundled SQLite) + refinery** versioned migrations. Statically linked for a clean AppImage; the per-file deploy ledger lives here. (sqlx was considered and rejected as overkill for a single-user embedded DB.)
- **Tests are a first-class Phase 1 deliverable:** property/integration tests on temp dirs — round-trip-pristine assertions, plus fixtures for EXDEV (cross-FS), zip-slip/malicious archives, and crash-mid-deploy recovery.

### Deployment Engine Safety Model (the crown jewel)
- **Crash-safety via write-ahead journal in SQLite + idempotent replay/rollback on next launch.** A verify/repair pass auto-runs after an abnormal exit so an interrupted deploy or purge is always recoverable to a consistent state.
- **Vanilla-file backup:** any pre-existing game file about to be overwritten is first copied into a **per-game original-store under the app data dir**, content-hashed, and recorded in the manifest. Purge restores from this store. Backup-before-overwrite is the single most important safety mechanism (corruption here is otherwise unrecoverable except via Steam re-verify).
- **Staged-file integrity:** staged files are marked **read-only**, and deployment **prefers reflink** (independent inode — edits to the deployed file can't corrupt staging) where the filesystem supports it, falling back to hardlink.
- **Purge verification:** purge does a **hash-diff of the game folder against the recorded manifest + vanilla store**, asserts byte-for-byte pristine, and reports any orphans rather than trusting manifest deletion blindly.

### Deployment Method & Filesystem Handling
- **Per-target runtime method probe: reflink → hardlink → symlink → copy.** Method is chosen per-target at deploy time (never globally), based on an empirical filesystem-capability probe (st_dev, link() capability, reflink support). This is required by success criterion #5.
- **Cross-FS / EXDEV policy:** detect at setup; **warn and recommend same-filesystem staging**. If the user proceeds with cross-FS staging anyway, automatically fall back to symlink/copy for those targets (never silently fail a hardlink).
- **Case-sensitivity (Proton/Wine):** **normalize mod path casing against the per-game canonical `Data/` directory casing** on deploy so mods load under Wine's case-sensitive view. (ext4 casefold (+F) considered as an alternative/future enhancement.)
- **Default staging location:** auto-suggest a directory on the **same filesystem as the game install**, capability-probed at setup, to keep hardlink/reflink viable.

### Detection, Archives & UI
- **Game detection:** steamlocate **auto-detect + a manual "add game by folder" fallback** for non-standard installs.
- **Supported games in Phase 1:** **Skyrim Special Edition (AppID 489830) + Fallout 4 (AppID 377160)**, detecting **Flatpak and Snap Steam roots** in addition to native Steam.
- **Proton prefix resolution:** derive `compatdata/<appid>/pfx/...` from `libraryfolders.vdf` + `appmanifest_<id>.acf`; re-resolve each session (paths can move).
- **Archive handling:** `.zip` + `.7z` extracted natively (zip + sevenz-rust2 crates); `.rar` via the **system `unrar`/`7z` binary if present**, otherwise a clear "install unrar" error — **no non-free RAR code bundled**. Extraction canonicalizes + bounds-checks every entry, rejects `..`/absolute/symlink entries, and extracts to temp then moves into staging. A crafted-malicious-archive test fixture is included.
- **Phase 1 UI depth: functional-minimal Svelte 5 UI** — game list with resolved paths, install-mod-from-archive, deploy/purge buttons, and clear surfacing of conflicts/warnings (e.g. unsafe FS config). Visual polish is deferred; engineering effort concentrates on the engine and its test suite.

### Claude's Discretion
- Exact crate names/boundaries within the workspace, module layout, error-type design (thiserror in libs / anyhow at the app boundary), and logging (tracing) setup.
- Manifest/ledger schema details and migration versioning specifics.
- The precise capability-probe implementation and how warnings are phrased in the UI.
- Whether a single managed mod is represented with full priority scaffolding now or minimal single-mod state (priority/load-order UI proper is Phase 2).

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- **Greenfield.** Repo currently contains only `.planning/`, `.claude/`, `LICENSE`, `README.md` — no `src-tauri/`, no Rust crates, no frontend yet. Phase 1 scaffolds the project from scratch.
- Rich research already exists in `.planning/research/` (ARCHITECTURE.md, FEATURES.md, PITFALLS.md, STACK.md, SUMMARY.md) — the planner should consume these, especially PITFALLS.md (the 6 critical safety pitfalls) and ARCHITECTURE.md (component/crate layout).

### Established Patterns
- Stack and library choices are pinned in `.claude/CLAUDE.md` (Recommended Stack table) and `.planning/research/STACK.md`: Rust 1.85+ (2024 edition), Tauri 2.11.x, Svelte 5, tokio 1.52, rusqlite 0.40 (bundled), reflink-copy, steamlocate 2.1, keyvalues-serde, zip 8, sevenz-rust2 0.21, walkdir, serde, tracing, thiserror/anyhow, refinery.
- "What NOT to Use" guidance is authoritative: no unrar bundling, no sled, no Electron, no OpenSSL/native-tls (use rustls-tls), no copy-only deployment, no overwriting game files without a ledger.

### Integration Points
- `create-tauri-app` (Svelte + TypeScript template) scaffolds the frontend + Tauri shell; Cargo workspace wraps the headless `crates/` core under `src-tauri/` or a top-level workspace.
- AppImage bundling and nxm:// MIME registration are Phase 5 concerns — not wired here, but the workspace layout should not preclude them.

</code_context>

<specifics>
## Specific Ideas

- Success criterion #4 ("byte-for-byte pristine even after an interrupted crash-mid-deploy operation") is the hardest, most important assertion — the test suite must include a crash-recovery fixture that simulates an aborted deploy and proves recovery to pristine.
- Pitfalls to design against from day one (from `.planning/research/PITFALLS.md` / SUMMARY.md): (1) overwrite destroys vanilla → backup-before-overwrite; (2) non-atomic manifest/orphans → WAL journal + verify/repair; (3) hardlink EXDEV across fs/btrfs-subvolume/Proton boundaries → empirical probe + per-target method; (4) Wine case-sensitivity mismatch → casing normalization; (5) wrong Proton prefix → derive from VDF/ACF; (6) zip-slip/malicious-symlink extraction → canonicalize + bounds-check + reject.
- STATE.md flags Phase 1's safety-critical engine (crash-safe journaling, EXDEV probe, vanilla-backup, casefold) for **deeper research at plan time** — plan-phase should run phase research for the deployment engine.

</specifics>

<deferred>
## Deferred Ideas

- ext4 casefold (+F) deployment tree as an alternative/stronger case-handling method — revisit in a later phase if path-casing normalization proves insufficient under Proton.
- Reflink (CoW) validated per-game as a first-class default (v2: DEPV2-02).
- Multi-mod conflict resolution, mod priority/load order, plugin (.esp/.esm/.esl) management + LOOT, and multiple profiles — Phase 2.
- Any NexusMods networking (auth, download, nxm://) — Phase 3.
- Polished/styled UI — after the core loop is proven.

</deferred>
