---
phase: 01-safe-local-round-trip
asvs_level: 2
block_on: high
threats_total: 24
threats_open: 0
threats_closed: 24
audited: 2026-06-23
mode: verify-mitigations (register authored at plan time)
---

# SECURITY — Phase 01: Safe Local Round-Trip

Retroactive threat-mitigation verification for the deploy/extract safety engine (the
security-critical core of NexTwist). Each declared mitigation in the phase threat
register was verified to **exist in the implemented code** with file:line evidence —
documentation and intent were not accepted as proof. Implementation files were not
modified.

All 24 threats resolve to **CLOSED**. No open blockers. No unregistered flags.

## Verification Result: SECURED

| Threat ID | Category | Disposition | Status | Evidence (file:line) |
|-----------|----------|-------------|--------|----------------------|
| T-01-01 | Tampering (dep graph) | mitigate | CLOSED | `deny.toml:22-27` bans `unrar`/`unrar_sys` by name; license allow-list `deny.toml:31-68`; absent from `Cargo.lock` (ban effective in practice) |
| T-01-02 | Tampering (SQLite durability) | mitigate | CLOSED | `crates/store/src/db.rs:57` `journal_mode=WAL` (asserted L59-63) + `:65` `synchronous=FULL` + `:67` `foreign_keys=ON` |
| T-01-03 | InfoDisclosure (SQL injection) | accept | CLOSED | No string-built SQL anywhere; 40 `params!`/bound call sites; e.g. `crates/store/src/journal.rs:63-65,82-83`, `manifest.rs:68-69`. Only `format!` uses build test `/staging/{name}` PathBufs, not SQL. Accept rationale (truly parameterized) holds |
| T-01-04 | Tampering/Elevation (add_game_by_folder) | mitigate | CLOSED | `crates/steam/src/resolve.rs:219-264` validates supported AppID + dir metadata + case-insensitive `Data/` marker + game `.exe` marker before returning |
| T-01-05 | DoS (.acf/.vdf parse) | accept | CLOSED | `crates/steam/src/resolve.rs:152-153` keyvalues-serde parse maps to typed `SteamError::Locate`/`Io`, no panic/unwrap on untrusted parse. Steam-owned files. Accept rationale holds |
| T-01-06 | Spoofing (wrong-prefix resolution) | mitigate | CLOSED | `crates/steam/src/resolve.rs:84-125` re-resolves each call (no disk cache, L89); prefix from Steam layout + `$STEAM_COMPAT_DATA_PATH` (`proton_prefix` L196-207) |
| T-01-07 | Tampering/Elevation (zip-slip) | mitigate | CLOSED | `crates/extract/src/validate.rs:138-160` rejects RootDir/Prefix/ParentDir components; re-canonicalize-parent-under-root containment `:119-129`. Called by all handlers: zip `zip.rs:64`, 7z `sevenz.rs:44`, rar `rar.rs:55` |
| T-01-08 | Tampering/Elevation (symlink CVE-2025-29787) | mitigate | CLOSED | `crates/extract/src/validate.rs:99-102` rejects symlink entries first; zip detects via unix mode `zip.rs:45-51`; rar re-walks + rejects on-disk symlinks `rar.rs:120-121`; zip>=8 pinned |
| T-01-09 | Tampering (extract outside staging) | mitigate | CLOSED | `crates/extract/src/staging.rs:37-84` extracts to `tempfile::TempDir` (L54), validates per-entry during extract, only then `move_into_staging` (L72); never extracts into staging/game tree |
| T-01-10 | Elevation (.rar command injection) | mitigate | CLOSED | `crates/extract/src/rar.rs:41` archive `is_file()` check first; `Command::new("unrar"/"7z")` with path+outdir as discrete `.arg()` argv, no shell, `--` ends options `:75-88`; post-extract `revalidate_tree` |
| T-01-11 | DoS (crash-induced vanilla loss) | mitigate | CLOSED | Backup-before-overwrite into content-addressed store `crates/deploy/src/backup.rs:51-84` (`originals_dir` app-area L28-35); intent-before-act journal `journal.rs:34-49,70-79`; idempotent replay `journal.rs:113-170`; startup `recover_on_launch` wired before UI `src-tauri/src/lib.rs:97-98,62` |
| T-01-12 | Tampering (non-atomic purge) | mitigate | CLOSED | `crates/deploy/src/engine.rs:401-439` purge driven only by `list_deployed_files` manifest (L402), per-removal journaled, idempotent, orphans reported not deleted |
| T-01-13 | Tampering (overwrite vanilla no backup) | mitigate | CLOSED | `backup_vanilla_if_absent` always backs up pre-existing non-ours files `backup.rs:51-84` (`is_ours` from manifest L115); reflink preferred over hardlink `method/mod.rs:52-60`; staged tree marked read-only `extract/src/staging.rs:77` |
| T-01-14 | Elevation (dir-symlink write-through) | mitigate | CLOSED | Per-file deploy only; trait contract "Must NOT operate on directories" `method/mod.rs:37`; `method/symlink.rs:20-24` symlinks an individual file target, never a directory |
| T-01-15 | InfoDisclosure (writes outside game dir) | mitigate | CLOSED | `guard_within_root` lexical containment `path_guard.rs:16-24` called in `deploy_one_file` `engine.rs:354`; `resolve_target` under `deploy_root(install_dir)/Data` `lib.rs:96-100`; vanilla store under app-data `backup.rs:28-35` |
| T-01-16 | Tampering (repair deletes vanilla as orphan) | mitigate | CLOSED | `crates/deploy/src/verify.rs:123-150` repair only re-deploys manifest-recorded missing/changed; file orphans copied to report (L132), never deleted |
| T-01-17 | DoS (case mismatch) | mitigate | CLOSED | `normalize_to_canonical` called before target resolution `engine.rs:351`; `FsWarning::NotCasefolded` surfaced via `fs_warnings_from_caps` `engine.rs:50,65` into `DeployReport.fs_warnings` (L81,177) |
| T-01-18 | Tampering (undetected drift) | mitigate | CLOSED | `verify` blake3 hash-diffs manifest vs disk, classifies missing/changed/orphan by provenance (ours/vanilla/unmanaged) `verify.rs:75-114`; auto-runs in `recover_on_launch` after replay `engine.rs:523` |
| T-01-19a | Tampering/Elevation (logic in command) | mitigate | CLOSED | Phase-01 adapters 1-4 logic lines: `games.rs:16-45`, `mods.rs:17-25`, `deploy.rs:16-39`; all safety logic in tested headless crates; `require_game`/`boundary_err` thin `commands/mod.rs:28-30,53-61` |
| T-01-20a | Tampering (unvalidated user path) | mitigate | CLOSED | `mods.rs:24` forwards raw archive path to `extract::install_archive` (per-entry validation); `games.rs:36` forwards raw folder path to `steam::add_game_by_folder` (marker check); no path trust at command layer |
| T-01-21a | Tampering (CI bypass) | mitigate | CLOSED | `.github/workflows/ci.yml:4,6` on push+PR; `:65` `cargo deny check advisories bans licenses sources`; `:52` `cargo test --workspace --locked`; `:55` clippy `-D warnings` |
| T-01-19b | Tampering/Destruction (purge empty-dir cleanup) | mitigate | CLOSED | `engine.rs:462-507` candidates from manifest `removed_rels` only, strictly below `deploy_root` (stop-at-root L476); `std::fs::remove_dir` not `remove_dir_all` (L493); `DirectoryNotEmpty` benign; defence-in-depth root guard L490 |
| T-01-20b | Destruction (repair orphan-dir removal) | mitigate | CLOSED | `verify.rs:152-183` repair removes ONLY empty orphan dirs via `remove_dir` (L176, refuses non-empty); file orphans stay report-only; root/ancestor guard L173 |
| T-01-21b | DenialOfPristine (recover purge branch) | mitigate | CLOSED | `engine.rs:511-528` `recover_on_launch` runs same `remove_emptied_dirs` over journal-derived `outcome.purged_rels` (L517-518) after `journal::replay`; never disk-scans for candidates |
| T-01-SC | Tampering (supply chain) | mitigate | CLOSED | Versions pinned in `[workspace.dependencies]` (`Cargo.toml`: rusqlite 0.39, zip 8, sevenz-rust2 0.21, reqwest rustls-only); `unrar`/`unrar_sys` absent from `Cargo.lock`; `frontend/package-lock.json` committed |

## Accepted Risks Log

- **T-01-03 (SQL injection) — ACCEPTED.** Verified valid: every store write uses
  parameterized `rusqlite` `params!` bindings; no string-concatenated/`format!`-built
  SQL exists in `crates/store/src/*.rs`. The only `format!` uses construct test-fixture
  `/staging/{name}` `PathBuf`s, not SQL. No untrusted SQL surface remains.
- **T-01-05 (.acf/.vdf parse DoS) — ACCEPTED.** Verified valid: the parsed `.acf`/`.vdf`
  are Steam-owned local files, not third-party mod content. `keyvalues-serde` parse
  failures map to a typed `SteamError` (`resolve.rs:153`) with no panic/`unwrap`. Local-only,
  low-value target. Accept rationale holds.

## Notable Hardening Beyond the Register (informational, not gaps)

- **rar argv `--` terminator** (`rar.rs:77,86`): option-parsing terminator so a filename
  starting with `-` cannot be misread as a flag — stronger than the planned argv split.
- **deny.toml advisory-scope narrowing** (`deny.toml:81-84`, introduced Plan 06): a
  CI-policy change that downgrades only *informational* `unmaintained` advisories on the
  transitive Tauri/GTK3 tree to non-gating, scoped to `unmaintained = "workspace"`.
  Verified that **`yanked = "deny"` and vulnerability advisories still fail the build** —
  the security-relevant signal (real CVEs + yanked crates) is intact. This narrows
  informational noise only and does not weaken any T-01 mitigation.

## Unregistered Flags

None. SUMMARY.md `## Threat Flags` for plans 01-03 through 01-06 each report
"None — no new security surface beyond the plan's `<threat_model>`"; plan 01-07 (gap
plan) introduced no new dependencies (`std::fs` + existing `walkdir`) and no new attack
surface. No new attack surface appeared during implementation without a threat mapping.

## Scope Note

Phase 01's threat register covers the headless safety engine
(`crates/{store,steam,extract,deploy,loadorder}`) and the three Phase-01 Tauri adapters
(`games`, `mods`, `deploy`). The additional command files present on disk
(`collections`, `downloads`, `fomod`, `nexus`, `plugins`, `profiles`, `conflicts`) are
later-phase additions outside this register and are not audited here — they carry their
own phase threat models.
