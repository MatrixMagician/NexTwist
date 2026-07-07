---
phase: 04
plan: 01
subsystem: fomod-engine
status: complete
tags: [fomod, xml, quick-xml, staging, root-detection, dry-run, headless]
requires:
  - "crates/core (nextwist_core types + StoreError)"
  - "crates/extract (install_archive staging pipeline)"
provides:
  - "crates/fomod: headless FOMOD 5.x parse->condition->resolve engine"
  - "fomod::parse_module_config (case-insensitive locate + BOM strip + serde deserialize)"
  - "fomod::eval + fomod::plugin_type_state (composite-dependency / live type-state)"
  - "fomod::resolve (PURE dry-run file-install plan, zero disk write)"
  - "fomod::resolve_source_path (case-insensitive FOMOD source -> staged path)"
  - "extract::detect_archive_root (wrapper-folder flattening between validate and move)"
affects:
  - "crates/extract/src/staging.rs (install_archive now root-detects before move)"
  - "Cargo.toml (quick-xml 0.40 added to [workspace.dependencies])"
tech-stack:
  added:
    - "quick-xml 0.40 (serialize feature) — pure-Rust XML, MIT, cargo-deny-clean"
  patterns:
    - "Crate shape copied verbatim from crates/nexus: nextwist_core alias, headless HARD INVARIANT comment, pub mod + pub use surface, thiserror enum flattening the foreign error (quick_xml::DeError) to String"
    - "serde-derived AST against LOCAL element names, #[serde(default)] on every optional element (namespace-ignorant, Pitfall 5)"
    - "Pure dry-run resolve (no std::fs write) as the locked safety gate — purity unit-tested"
key-files:
  created:
    - "crates/fomod/Cargo.toml"
    - "crates/fomod/src/lib.rs"
    - "crates/fomod/src/error.rs"
    - "crates/fomod/src/model.rs"
    - "crates/fomod/src/parse.rs"
    - "crates/fomod/src/condition.rs"
    - "crates/fomod/src/resolve.rs"
    - "crates/fomod/tests/corpus.rs"
    - "crates/fomod/tests/fixtures/ (9 categories)"
  modified:
    - "Cargo.toml"
    - "Cargo.lock"
    - "crates/extract/src/staging.rs"
decisions:
  - "FOMOD CompositeDependency deserializes interleaved child elements into typed Vecs (file_deps/flag_deps/game_deps/fomm_deps/nested) rather than an enum-tagged list — quick-xml serde maps repeated heterogeneous children cleanly this way and the evaluator chains the arms."
  - "Version (game/fomm) dependency arms are treated as held during the pure dry-run (no live game-version oracle); the apply path can supply a real comparator later. Documented in condition::eval."
  - "A destination-less <file>/<folder> resolves to the Data root as the source's leaf name; the staging tree itself is Data-rooted after detect_archive_root."
  - "detect_archive_root unwraps at most ONE cosmetic wrapper level and only when the single top-level dir directly contains a recognized root (Data/SKSE/F4SE/OBSE/NVSE/MWSE, case-insensitive); a multi-entry top level or an already-Data-rooted tree is never flattened (T-04-04)."
metrics:
  duration: "~12 min"
  completed: 2026-06-21
  tasks: 3
  files: 13
---

# Phase 4 Plan 01: FOMOD Engine + Archive Root-Detection Summary

Built the headless `crates/fomod` engine — quick-xml+serde parse of the full FOMOD 5.x `ModuleConfig.xml` spec into a typed AST, a recursive composite-dependency / flag evaluator with live plugin type-state resolution, and a PURE dry-run resolver that emits an ordered `Vec<FileInstall>` with zero disk writes — and closed the carried Phase-2 archive-root-detection gap so wrapper-folder mods stage into a `Data/`-rooted tree.

## What Was Built

### Task 1 — Crate scaffold + failing corpus (RED) — `f1039d6`
- New workspace member `nextwist-fomod` (lib name `fomod`), copying the `crates/nexus` shape verbatim: `nextwist_core` alias (never bare `core`), the headless HARD INVARIANT comment (no tauri/reqwest/keyring), `pub mod` + `pub use` re-export surface.
- Full FOMOD 5.x typed AST in `model.rs` (serde-derived from the canonical XSD): all 5 `GroupType` variants, static `<type>` vs `<dependencyType>`, the 5-state `PluginType`, composite `And`/`Or` deps (nested), `conditionalFileInstalls`, `file`/`folder` items with `priority`/`alwaysInstall`/`installIfUsable`, the `order` enum.
- `error.rs` `FomodError` thiserror enum flattening `quick_xml::DeError` to `String`; the `MalformedSchema` variant is the locked "fail clearly, never mis-install" outcome.
- `parse`/`condition`/`resolve` stubbed (`unimplemented!()`) so the suite compiles and fails.
- `tests/fixtures/` corpus (9 categories): simple, group_types, flags, conditional, nested_deps, dependency_type, bom (real UTF-8 BOM), case_insensitive (`FOMOD/moduleconfig.xml`), malformed.
- `quick-xml = { version = "0.40", features = ["serialize"] }` added to `[workspace.dependencies]` (the only new dep this phase). 17 tests RED.

### Task 2 — Implement parse + condition + resolve (GREEN) — `3fa2653`
- `parse.rs`: locate `fomod/ModuleConfig.xml` case-insensitively via walkdir (Pitfall 3), strip a leading UTF-8 BOM (Pitfall 5), `quick_xml::de::from_str` into the AST (namespace-ignorant). `resolve_source_path` walks a FOMOD `source` onto the staged tree component-by-component, case-insensitively.
- `condition.rs`: `eval` = recursive composite-dependency walk (`And`=all / `Or`=any, empty-`And` true / empty-`Or` false, nested recursion, flag/file arms; version arms held in dry-run). `plugin_type_state` walks `dependencyType.patterns` in order, else `defaultType`.
- `resolve.rs`: PURE dry-run — `requiredInstallFiles` (unconditional) -> selected plugin files (+ `alwaysInstall`, + `installIfUsable` when not `NotUsable`) -> `conditionalFileInstalls` patterns whose deps hold; dedup by `(dest_rel, priority desc)`. A plugin missing `<typeDescriptor>` => `MalformedSchema`. No `std::fs` write (no-write marker test confirms purity).
- All 17 corpus tests pass; `cargo clippy -p nextwist-fomod` clean.

### Task 3 — Archive root-detection in staging — `8bd99c9`
- `detect_archive_root(temp_root) -> PathBuf` added to `crates/extract/src/staging.rs`, invoked AFTER extract-validate and BEFORE `move_into_staging` (the validated extract->validate->move->read-only ordering preserved).
- Heuristic: a tree is "wrapped" iff its top level is EXACTLY one directory that directly contains a recognizable game root (`Data` child case-insensitively, or a known top-level item: SKSE/F4SE/OBSE/NVSE/MWSE). Then stage from that subdir; otherwise unchanged. Unwraps at most one cosmetic level.
- 5 unit tests: wrapper / already-data-rooted / multi-folder (T-04-04 guard) / nested-wrapper / known-top-level-item. No regression; clippy clean.

## Verification

| Gate | Result |
|------|--------|
| `cargo test -p nextwist-fomod` | 17 passed |
| `cargo test -p nextwist-extract` | 12 passed (5 new root-detection + 7 existing) |
| `cargo test -p nextwist-fomod -p nextwist-extract --locked` | all green |
| `cargo clippy -p nextwist-fomod -p nextwist-extract --all-targets -- -D warnings` | clean |
| `cargo deny check advisories bans licenses sources` | advisories/bans/licenses/sources ok (quick-xml clean) |
| Headless invariant grep on `crates/fomod/Cargo.toml` | no tauri/reqwest/keyring dependency |

## Must-Haves Satisfied

- Parsing a real `ModuleConfig.xml` yields a typed AST covering all 5 group types, static + dependencyType plugins, ordered steps, conditionalFileInstalls, composite And/Or — VERIFIED by the corpus parse tests.
- `resolve(module, selection)` returns an ordered `Vec<FileInstall>` with NO disk write — VERIFIED by `resolve_performs_no_filesystem_write`.
- A malformed `ModuleConfig.xml` returns `FomodError::Xml | MalformedSchema`, never silent mis-install — VERIFIED by `malformed_fixture_returns_specific_error_not_ok`.
- A single-wrapper-folder archive stages into a `Data/`-rooted tree — VERIFIED by `wrapper_folder_is_flattened`.
- FOMOD source paths and the fomod/ folder located case-insensitively — VERIFIED by `case_insensitive_*` tests.

## Threat Mitigations Applied

| Threat ID | Mitigation in this plan |
|-----------|--------------------------|
| T-04-01 (path traversal) | `resolve` emits relative paths only and never writes; `resolve_source_path` resolves strictly inside `tree_root` (component-by-component, no `..` ascent). |
| T-04-02 (XML billion-laughs) | quick-xml does no DTD/external-entity expansion by default; any parse failure maps to a specific `FomodError::Xml`, never hangs. |
| T-04-03 (silent mis-install) | Every unsupported construct returns `MalformedSchema`/`Xml`; resolve is pure and surfaces the plan before any write. |
| T-04-04 (wrapper mis-detection) | `detect_archive_root` uses an explicit small recognized-root token list; unit-tested so a real multi-folder mod is never wrongly flattened. |

## Deviations from Plan

None — plan executed exactly as written. Three clippy lints surfaced during the GREEN/Task-3 gates (collapsible-if, repeat().take(), field-reassign-with-default, needless-borrow) and were fixed inline as part of satisfying the `-D warnings` verification gate; no behavioral change.

## Known Stubs

None. The two FOMOD `Dependency` enum + version-arm handling are documented design choices (version arms held during pure dry-run), not stubs — the AST is fully populated and every fixture deserializes.

## Self-Check: PASSED

- All 6 declared artifact files exist on disk (verified).
- All commit hashes exist: `f1039d6`, `3fa2653`, `8bd99c9` (verified in git log).
- All key-link `contains` markers present: `FomodModule`, `parse_module_config`, `eval`, `resolve`, `FomodError`, `detect_archive_root`.
