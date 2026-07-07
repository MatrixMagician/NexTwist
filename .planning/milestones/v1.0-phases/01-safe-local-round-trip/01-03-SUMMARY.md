---
phase: 01-safe-local-round-trip
plan: 03
subsystem: extraction
tags: [rust, zip, sevenz-rust2, archive-extraction, zip-slip, cve-2025-29787, system-rar, staging, security]

# Dependency graph
requires:
  - phase: 01-01
    provides: "crates/core domain types (ManagedMod), workspace + pinned deps (zip 8, sevenz-rust2 0.21), cargo-deny unrar ban"
provides:
  - "crates/extract: untrusted-archive -> validated read-only staging-tree transform"
  - "install_archive(archive, staging_root) -> StagedMod (detect format, extract-to-temp, validate every entry, move into staging, mark read-only, return file manifest)"
  - "validate::validate_entry — the single shared per-entry path validator (reject symlink / absolute / parent-escape + re-canonicalize-under-root)"
  - "ExtractError enum (UnsafeEntry, SymlinkEntry, RarToolMissing, UnsupportedFormat, ToolFailed, Io, Decode)"
  - "zip / sevenz-rust2 / system-rar format handlers all routing through the shared validator"
affects:
  - "Plan 04 (deploy): consumes StagedMod read-only staging trees as the deploy source"
  - "Plan 06 (tauri): install-mod-from-archive command wraps install_archive; ManagedMod row insert pairs with the returned StagedMod"

# Tech tracking
tech-stack:
  added:
    - "zip 8.6 (CVE-2025-29787-patched; enclosed_name + unix_mode + add_symlink)"
    - "sevenz-rust2 0.21 (ArchiveReader::for_each_entries per-entry interposition)"
    - "tempfile 3.27 (extract-to-temp staging)"
    - "walkdir 2.5 (read-only marking + rar output re-validation)"
  patterns:
    - "Single shared per-entry validator; format handlers never roll their own path checks"
    - "Validate the RAW archive entry name, not the post-sanitized enclosed_name (which silently relativizes absolute entries)"
    - "Extract-to-temp -> validate-whole-tree -> move-into-staging -> mark-read-only (never extract into staging/game tree directly)"
    - "System rar/7z via std::process::Command with path + outdir as separate argv elements and -- terminator (no shell string); re-validate the tool's output tree"
    - "Alias the core crate as nextwist_core (not core) in any crate that derives thiserror, so it does not shadow ::core"

key-files:
  created:
    - crates/extract/Cargo.toml
    - crates/extract/src/lib.rs
    - crates/extract/src/validate.rs
    - crates/extract/src/zip.rs
    - crates/extract/src/sevenz.rs
    - crates/extract/src/rar.rs
    - crates/extract/src/staging.rs
    - crates/extract/tests/zip_slip_rejected.rs
    - crates/extract/tests/extract_formats.rs
    - crates/extract/tests/fixtures/.keep
  modified: []

key-decisions:
  - "Validate raw entry.name() (zip) / entry.name() (7z) rather than enclosed_name(), so absolute-path entries are explicitly REJECTED rather than silently relativized into staging"
  - "Symlink defense uses the zip unix-mode S_IFLNK bits; the test builds a genuine symlink entry via the writer's add_symlink (plain unix_permissions forces regular-file bits, so it cannot author a real symlink entry)"
  - "rar handler prefers unrar, falls back to 7z; re-validates the whole extracted tree (a system tool will happily write symlink/traversal entries)"
  - "Aliased core dep as nextwist_core to avoid shadowing ::core, which breaks thiserror's derive in any crate that derives errors locally"

patterns-established:
  - "Shared validator pattern: one audited code path guarantees the zip-slip/symlink invariant across all formats"
  - "temp-then-move staging pipeline: the whole archive must validate before any byte lands in staging"

requirements-completed: [STAGE-01, STAGE-02, STAGE-03]

# Metrics
duration: 35min
completed: 2026-06-20
status: complete
---

# Phase 1 Plan 03: Safe Archive Extraction Summary

**`crates/extract` turns an untrusted local `.zip`/`.7z`/`.rar` into a validated read-only per-mod staging tree, with a single shared per-entry validator that rejects zip-slip / absolute / symlink entries (CVE-2025-29787) and a system-tool-only RAR path (no bundled non-free code).**

## Performance

- **Duration:** ~35 min
- **Started:** 2026-06-20
- **Completed:** 2026-06-20
- **Tasks:** 2 (both TDD)
- **Files modified:** 10 created

## Accomplishments

- **Shared per-entry validator** (`validate.rs`): `validate_entry(name, root, is_symlink)` rejects symlink entries outright, rejects absolute paths and parent-directory escape components via `Path::components()` inspection (not fragile string matching), then re-canonicalizes the created parent and asserts containment under the root — the one audited path all format handlers call.
- **Three format handlers** all funnel through that validator: `zip.rs` (validates the RAW entry name + detects symlinks via unix-mode `S_IFLNK`), `sevenz.rs` (`ArchiveReader::for_each_entries` interposition closure validating each entry before decoding), `rar.rs` (system `unrar`→`7z` via argv, then re-validates the extracted tree).
- **Extract-to-temp-then-move staging pipeline** (`staging.rs`): `install_archive` detects format (magic + extension), extracts into a `TempDir` near the staging root, moves the validated tree in (atomic rename fast-path, recursive cross-device fallback), marks every staged file read-only, and returns a `StagedMod { staging_root, files }`.
- **Crafted-malicious-archive rejection test** (`zip_slip_rejected.rs`): builds traversal, absolute-path, and genuine-symlink zips in-test and proves each is rejected with the correct `ExtractError` variant while nothing escapes; a benign control installs cleanly.
- **Multi-format extract test** (`extract_formats.rs`): authors `.zip` and `.7z` fixtures in-test, asserts both extract to staging with the expected relpaths and read-only bits, and exercises the `.rar` system-tool / `RarToolMissing` branch gated on tool presence.
- **7 extract tests green**; full workspace unit/integration suite green (53 tests); `cargo deny check bans` still passes (no non-free RAR crate added); clippy clean.

## Task Commits

1. **Task 1: Shared per-entry validator + crafted malicious-archive rejection test** - `b08c095` (feat, TDD)
2. **Task 2: zip / 7z / system-rar handlers + temp-then-move staging pipeline** - `35ca81c` (feat, TDD)

_Both tasks were TDD: the validator/handlers and their failing-first tests landed together within each task commit (RED→GREEN within the task)._

## Files Created/Modified

- `crates/extract/Cargo.toml` - crate manifest; `nextwist_core` path-aliased to avoid `::core` shadowing; zip/sevenz-rust2/walkdir/tempfile/thiserror/tracing deps.
- `crates/extract/src/lib.rs` - `ArchiveFormat::detect` (magic + extension), `mark_tree_readonly`, `list_files_rel`, public re-exports.
- `crates/extract/src/validate.rs` - the single shared `validate_entry` + `ExtractError` enum.
- `crates/extract/src/zip.rs` - `.zip` extraction; raw-name validation + unix-mode symlink rejection.
- `crates/extract/src/sevenz.rs` - `.7z` extraction via `ArchiveReader::for_each_entries`, per-entry validated.
- `crates/extract/src/rar.rs` - system `unrar`/`7z` via argv; output-tree re-validation; `RarToolMissing` fallback.
- `crates/extract/src/staging.rs` - `install_archive` orchestrator + `StagedMod`.
- `crates/extract/tests/zip_slip_rejected.rs` - crafted traversal/absolute/symlink rejection + benign control.
- `crates/extract/tests/extract_formats.rs` - `.zip`/`.7z`/`.rar` extraction correctness + read-only assertions.
- `crates/extract/tests/fixtures/.keep` - keeps the fixtures dir tracked (archives built programmatically).

## Decisions Made

- **Validate the raw entry name, not `enclosed_name()`.** Probing zip 8.6 showed `enclosed_name()` silently relativizes an absolute entry (`/abs/x` → `abs/x`) and returns `None` only for `..`-escapes. To *reject* absolute entries (per the plan) rather than quietly relativize them into staging, the validator inspects the raw `entry.name()` components directly.
- **Genuine symlink fixtures need `add_symlink`.** The zip writer forces regular-file mode bits when a symlink mode is passed via `unix_permissions`; the crafted-symlink test uses the writer's dedicated `add_symlink` so the entry carries real `S_IFLNK` bits the handler detects.
- **RAR tool order unrar→7z, with mandatory output re-validation**, because a system extractor can itself write symlink/traversal entries — the tree is re-walked and re-checked after the tool runs.
- **`nextwist_core` alias** (not `core`) for the internal dependency: a dependency literally named `core` shadows the std `::core` crate, which breaks `thiserror`'s derive macro (it emits `::core::fmt`). `extract` derives `ExtractError` locally, so it must not shadow it.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Aliased the internal `core` dependency as `nextwist_core`**
- **Found during:** Task 1 (`cargo build -p nextwist-extract`)
- **Issue:** Declaring `core = { ... }` (the workspace alias from Wave 1) makes a dependency literally named `core`, which shadows the std `::core` crate at the extern prelude. `thiserror`'s `#[derive(Error)]` expands to `::core::fmt`/`::core::write!`, so the derive failed to compile in `extract` (the first crate to BOTH depend on `core` AND derive thiserror locally — Wave 1's `store` keeps its errors in `nextwist-core` itself, so it never hit this).
- **Fix:** Declared the dep as `nextwist_core = { path = "../core", package = "nextwist-core" }` (the workspace alias only exposes the name `core`, so a direct path dep was used to choose a non-shadowing name). No version drift, no package substitution.
- **Files modified:** crates/extract/Cargo.toml
- **Verification:** `cargo build -p nextwist-extract` clean; clippy clean.
- **Committed in:** b08c095 (Task 1 commit)

**2. [Rule 1 - Bug] Validate the raw entry name so absolute-path entries are rejected**
- **Found during:** Task 1 (`zip_slip_rejected.rs` RED run)
- **Issue:** First implementation validated the post-`enclosed_name()` path. `enclosed_name()` relativizes an absolute entry instead of returning `None`, so the absolute-path archive was *accepted* (extracted under staging) rather than rejected — the test caught it. The symlink case also failed because the test author used `unix_permissions` (which the writer overrides to regular-file bits).
- **Fix:** zip handler now validates `entry.name()` (raw) through the shared component-checking validator (rejecting absolute + escape components), and the test builds the symlink fixture via `add_symlink` so a genuine `S_IFLNK` entry is exercised.
- **Files modified:** crates/extract/src/zip.rs, crates/extract/tests/zip_slip_rejected.rs
- **Verification:** all 4 `zip_slip_rejected` tests green (traversal/absolute/symlink rejected with correct variant; benign accepted).
- **Committed in:** b08c095 (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug). Both were necessary for correctness/security and are within the plan's scope. No scope creep.

## Issues Encountered

- **Pre-existing, out-of-scope:** `cargo test --workspace` surfaces a compile failure in `nextwist-core`'s **doc-tests** (`error[E0433]: cannot find 'fmt'/'write' in 'core'`) — the same `::core` shadowing, in Wave 1's `crates/core/src/error.rs` (commit `e24229e`), affecting doc-tests only. It is NOT in this plan's task scope (I never touched `crates/core`) and the lib/integration suites all pass (53 tests). Logged to `.planning/phases/01-safe-local-round-trip/deferred-items.md` (DEFER-01) with a suggested fix for a future plan; not fixed here per the executor scope boundary.
- **`.rar` round-trip not directly executed on this host:** `7z` is present (extraction works) but no tool can *author* a `.rar` here (`7z` lacks the rar write codec; no `rar` binary), so the positive rar round-trip self-skips while the `RarToolMissing` branch and the argv/re-validation wiring are in place and unit-reachable. The deploy/UAT plans run on a host or fixture that can supply a real `.rar`.

## User Setup Required

None - no external service configuration required. (`.rar` support optionally improves if the user installs `unrar` or a rar-capable `7z`, but the clear `RarToolMissing` error covers its absence by design.)

## Next Phase Readiness

- **Plan 04 (deploy)** can consume `StagedMod` read-only staging trees as the deploy source; the read-only invariant the deploy engine relies on is enforced here.
- **Plan 06 (tauri)** can wrap `install_archive` in a thin command and pair the returned `StagedMod` with a `ManagedMod` row insert.
- No blockers. One deferred non-blocking item (DEFER-01, doc-tests for `nextwist-core`).

## Threat Flags

None — no new security surface beyond the planned threat model. The two trust boundaries (untrusted archive → extractor; archive path → system rar tool) are exactly the ones in the plan's `<threat_model>`, and T-01-07 through T-01-10 are all mitigated as specified (shared validator, symlink rejection, temp-then-move, argv-not-shell + path-existence check + output re-validation).

## Known Stubs

None. All three format handlers are fully implemented and tested; the Task 1 module placeholders for `sevenz.rs`/`rar.rs` were replaced with real implementations in Task 2.

## Self-Check: PASSED

- All 10 created files verified present on disk.
- Both task commits verified in git log (`b08c095`, `35ca81c`).
- `cargo test -p nextwist-extract` green (4 zip_slip_rejected + 3 extract_formats); full workspace lib/integration suite green (53 tests); `cargo deny check bans` passes; clippy clean.

---
*Phase: 01-safe-local-round-trip*
*Completed: 2026-06-20*
