# Phase 01 — Deferred Items (out-of-scope discoveries)

Items found during execution that are NOT part of the current plan's task scope.
Logged per the executor SCOPE BOUNDARY rule (do not fix pre-existing/unrelated failures).

## DEFER-01: `nextwist-core` doc-tests fail to compile (pre-existing, Wave 1) — ✅ RESOLVED

**RESOLVED (2026-06-20, orchestrator post-merge gate, Wave 2):** Renamed the `crates/core` `[lib] name` from `core` to `nextwist_core`, stopping the std-`::core` shadowing in the rustdoc doc-test context. Dependants reference the crate by dependency key (`store` → `core`, `steam`/`extract` → `nextwist_core`), which is independent of the lib target name, so the rename is transparent. `cargo test --workspace` now exits 0 with all doc-test suites compiling (44 passed, 13 suites). Caught by the execute-phase post-merge integration gate (the per-plan `--lib` self-checks missed it).

---



- **Discovered during:** Plan 01-03 Task 2 (`cargo test --workspace`)
- **Symptom:** `Doc-tests core` fail with `error[E0433]: cannot find 'write'/'fmt'/'option' in 'core'` originating from the `#[derive(Error)]` macro in `crates/core/src/error.rs`.
- **Root cause:** The crate's lib is named `core` (package `nextwist-core`). `thiserror`'s derive macro expands to absolute `::core::fmt` / `::core::write!` paths; in the rustdoc doc-test compilation context the extern crate named `core` shadows the std `::core` crate, so those paths fail to resolve. (Normal lib/integration-test builds are unaffected — only doc-tests.)
- **Why deferred:** Pre-existing in Wave 1 commit `e24229e`; the file (`crates/core/src/error.rs`) is not in plan 01-03's task scope. Plan 01-03 introduced the same shadowing risk in `crates/extract` and resolved it locally by aliasing the dependency as `nextwist_core` (not `core`).
- **Suggested fix (future plan):** either (a) rename the `core` lib to `nextwist_core` and update dependants, or (b) add `#![doc(test(attr(...)))]` / disable doc-tests for `nextwist-core` (`[lib] doctest = false`), or (c) add explicit `use thiserror::Error;` is already present — the real fix is renaming the lib to stop shadowing `::core`. Low blast radius (doc-tests only); does not affect the engine's correctness or the unit/integration suites (53 tests pass).
