# Phase 8: Reversible StarfieldCustom.ini Activation - Research

**Researched:** 2026-07-08
**Domain:** Reversible, journaled filesystem mutation of a single Windows INI file inside a Proton prefix (Rust, headless `crates/deploy` engine)
**Confidence:** HIGH (the entire safety substrate is existing, in-repo, and read directly this session; the only external question — the INI editor library — resolves to "add no dependency")

## Summary

Phase 8 is **not** a new-technology phase. Every hard part — content-addressed byte backup, intent-before-act journaling, idempotent crash replay, empty-dir pruning, byte-for-byte pristine assertion — already exists and is battle-tested in `crates/deploy` and `crates/testkit`. The phase is an **exercise in reuse discipline**: wire one new ~120-LOC module (`gameconfig.rs`) into the existing choke points and reuse `backup.rs` + `journal.rs` verbatim, changing only *where the target path resolves* (the Proton-prefix `My Games` dir instead of `<install>/Data`).

The single genuine research question — "does `rust-ini` preserve byte fidelity (CRLF, BOM, comments, order)?" — resolves clearly: **it does not, and no Rust INI crate does full round-trip byte-fidelity**. The correct (and laziest) answer is to **add no dependency at all** and hand-roll a ~60-line surgical byte editor over `std` that touches only the two owned `[Archive]` keys. This satisfies SFINI-03 byte-fidelity better than any parse-and-reserialize library could, and keeps `cargo-deny` and the AppImage footprint untouched. The roadmap already flagged `rust-ini` as "optional" — drop it.

The one true novelty is that this is **the first sanctioned write outside the `Data/` deploy root**. The reversibility substrate is bounded to `Data/` by `resolve_target` + `guard_within_root`. The INI op must NOT relax that guard; instead it rides a **distinct journal `kind` token** with a **sentinel `target_rel`** that a new `replay` match arm resolves via `steam::my_games_path(&game.prefix)` — leaving the `Data/`-root guard completely untouched (SFINI-04).

**Primary recommendation:** Add zero crates. New `crates/deploy/src/gameconfig.rs` = a std-only surgical INI editor + a thin op wrapper reusing `backup::{backup_vanilla_if_absent, restore_vanilla}` and a new `journal` `KIND_INI` arm. Gate on `game.appid == steam::STARFIELD`. Wire at the tail of `deploy_winners`, inside `purge`, and add a `KIND_INI` arm to `journal::replay` (which `recover_on_launch` already drives). `redeploy_winners` (profile switch) needs no new code — it is purge + deploy_winners.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| INI path resolution (My Games/Starfield) | `crates/steam` (`ce2::my_games_path`) | — | Phase-6 hardened, case-folded, `drive_c`-contained resolver already owns this |
| Byte-fidelity surgical merge | `crates/deploy/src/gameconfig.rs` (NEW, std-only) | — | Pure byte transform, no I/O framework; belongs beside `backup`/`method` |
| Provenance capture + byte restore | `crates/deploy/src/backup.rs` (REUSE) | `crates/store/vanilla.rs` | Content-addressed vanilla ledger is exactly "capture original bytes / detect absence" |
| Journaled idempotency + crash replay | `crates/deploy/src/journal.rs` (EXTEND) | `crates/store/journal.rs` | Add a `KIND_INI` arm; store row schema is already generic |
| Choke-point wiring + gating | `crates/deploy/src/engine.rs` (EXTEND) | — | `deploy_winners`/`purge`/`recover_on_launch` are the sanctioned choke points |
| Conflict detection (non-empty user value) | `crates/deploy/src/gameconfig.rs` + `DeployError` | — | Typed `thiserror` arm surfaced through the report, mirroring `orphans` |
| Preview / confirm / conflict UI | `src-tauri/commands/*` (thin) → SvelteKit | — | Adapter stays 3–5 lines; Starfield game view hosts the notice (Phase 6/7 pattern) |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `std::fs` / `std::io` | (toolchain ≥1.89) | Read/write INI bytes, detect BOM+EOL, prune empty dirs | Already the substrate for `backup.rs`; a surgical INI edit is byte slicing, not parsing `[VERIFIED: crates/deploy/src/backup.rs read this session]` |
| `nextwist-steam` (`steam`) | path dep | `my_games_path(&prefix)`, `STARFIELD` const, `resolve_ce2_config` | Phase-6 resolver is the hardened, case-folded, `drive_c`-contained INI-path source `[VERIFIED: crates/steam/src/ce2.rs:82, lib.rs:23-31]` |
| `store` (journal + vanilla facades) | path dep | `begin_op`/`mark_done`/`pending_ops`, `record_vanilla`/`vanilla_for` | Generic row primitives already carry `kind` + arbitrary `target_rel` string `[VERIFIED: crates/store/src/journal.rs, vanilla.rs]` |
| `blake3` | workspace pin | Hash INI bytes for content-addressed backup + verify | Same primitive the whole engine uses `[VERIFIED: crates/deploy/Cargo.toml]` |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `nextwist-testkit` (`testkit`) | dev-dep | `snapshot_tree`, `assert_trees_identical`, `DIR_SENTINEL`, `fake_my_games_prefix` | The reversibility suite extends these `[VERIFIED: crates/testkit/src/lib.rs:44-262]` |
| `thiserror` | workspace pin | Typed `DeployError::IniConflict` arm | Engine error convention `[VERIFIED: CLAUDE.md]` |
| `serde` | workspace pin | Serialize INI activation state / conflict for IPC | Reports already cross the Tauri boundary via serde `[VERIFIED: crates/deploy/src/engine.rs:21]` |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| std-only surgical byte editor | `rust-ini` 0.21.3 (imported as `ini`) | **Rejected.** Parse-then-reserialize model: normalizes `key=value` spacing, does not round-trip comments/blank-lines/order reliably, emits LF (not CRLF), and has no BOM handling — a direct SFINI-03 byte-fidelity violation. Adds a `cargo-deny`-audited dep for a job `std` does in 60 lines. `[CITED: lib.rs/crates/ini-roundtrip, crates.io rust-ini 0.21.3 updated 2025-08-30]` |
| std-only surgical byte editor | `ini-roundtrip` | **Rejected.** Closest to fidelity via raw-line attributes, but its own docs state "Newlines are not saved… a mix of CR/CRLF/LF is supported on loading but not on saving" — the caller must reconstruct EOLs anyway, so you write the byte logic regardless. `[CITED: lib.rs/crates/ini-roundtrip]` |
| new journal `kind` token | relax `guard_within_root` to allow the prefix | **Rejected — safety regression.** The `Data/`-root guard is load-bearing; widening it to admit an absolute prefix path is exactly what CONTEXT.md forbids (SFINI-04). A distinct `kind` + independent path resolution keeps the guard pristine. |

**Installation:**
```bash
# NONE. Phase 8 adds zero external crates.
# gameconfig.rs is std-only; all other pieces are existing path/workspace deps.
```

**Version verification:** `rust-ini` max-stable `0.21.3`, updated 2025-08-30, 107M downloads `[VERIFIED: crates.io API this session]` — verified only to document the *rejection*; it is NOT being installed.

## Package Legitimacy Audit

> This phase installs **no external packages**. The audit is therefore a negative result.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| `rust-ini` | crates.io | mature (0.21.3, 2025-08-30) | 107M total | github.com/zonyitoo/rust-ini | OK (legit) | **NOT USED** — rejected on byte-fidelity grounds, not legitimacy |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none
**New crates added by this phase:** none — no `cargo-deny` surface change, no MSRV change, no AppImage size change.

## Architecture Patterns

### System Architecture Diagram

```
                         deploy_winners(store, game, winners)          purge(store, game)
                                     │  (existing loop)                    │  (existing loop)
                                     ▼                                     ▼
                        [per-file journaled deploy]              [per-file journaled purge + restore_vanilla]
                                     │                                     │
   is_starfield(game)?  ────────────┤                                     ├──────────── is_starfield(game)?
        (game.appid == STARFIELD)   │                                     │
                                    YES                                   YES
                                     ▼                                     ▼
                    ┌───────────────────────────────┐        ┌───────────────────────────────┐
                    │ gameconfig::ensure_ini_active  │        │ gameconfig::restore_ini        │
                    └───────────────────────────────┘        └───────────────────────────────┘
                                     │                                     │
        ini_target = steam::my_games_path(&game.prefix).join("StarfieldCustom.ini")
                                     │                                     │
        1. journal::begin_ini(store, appid, SENTINEL="StarfieldCustom.ini")  (pending, kind=KIND_INI)
        2. backup_vanilla_if_absent(store, game, ini_target, SENTINEL_REL)
             ├─ returns true  → PreExisting  (original bytes now in vanilla store)
             └─ returns false → CreatedByNexTwist (file did not pre-exist)
        3. surgical_merge(bytes) → detect BOM+EOL → find/insert [Archive] +
             bInvalidateOlderFiles=1 / sResourceDataDirsFinal=   (conflict-checked)
        4. write ini_target ; journal::finish (done)                RESTORE branch:
                                                              remove_if_present(ini_target)
                                                              restore_vanilla(...) → true copies bytes back,
                                                                false = no-op → file stays absent
                                                              prune NexTwist-created now-empty dirs ; done
                                     │                                     │
                                     └──────────────┬──────────────────────┘
                                                    ▼
                       recover_on_launch(store, game)  →  journal::replay
                                                    │
                              match row.kind { KIND_DEPLOY | KIND_PURGE | KIND_INI(new arm) }
                              KIND_INI arm resolves via my_games_path, NOT resolve_target,
                              and replays the same idempotent ensure/restore → one [Archive], identical bytes
```

File-to-implementation mapping is in Component Responsibilities below; the diagram is data flow only.

### Recommended Project Structure
```
crates/deploy/src/
├── gameconfig.rs      # NEW (~120 LOC): surgical INI editor + ensure_ini_active/restore_ini op wrappers
├── engine.rs          # EXTEND: call gameconfig hooks at tail of deploy_winners + inside purge (is_starfield gate)
├── journal.rs         # EXTEND: KIND_INI const + replay match arm (resolves via my_games_path)
├── backup.rs          # REUSE VERBATIM: backup_vanilla_if_absent / restore_vanilla
└── error.rs           # EXTEND: DeployError::IniConflict typed arm

crates/deploy/tests/
└── ini_activation.rs  # NEW reversibility suite (both provenance branches, idempotency, conflict, CRLF/BOM)

(security review)   # NEW (first write outside Data/ — path-containment + INI-injection surface)
src-tauri/src/commands/*.rs              # thin preview/confirm/conflict adapter
frontend/…                               # Starfield game-view surfacing (Phase 6/7 notice pattern)
```

### Pattern 1: Distinct journal `kind` for a non-`Data/` op (the SFINI-04 seam)
**What:** The `op_journal` row schema is fully generic — `(appid, target_rel TEXT, method, source_hash, kind TEXT, state)`. `journal::replay` dispatches on `row.kind`; unknown kinds are currently `warn!`-and-marked-done. Add `KIND_INI` and a new arm.
**When to use:** Any op whose on-disk target is not `Data/`-rooted.
**Example:**
```rust
// Source: crates/deploy/src/journal.rs (existing replay dispatch, VERIFIED this session)
pub const KIND_INI: &str = "ini";
// In journal::replay's match:
//   KIND_INI => replay_ini(store, game, row)?,   // resolves via my_games_path, NOT resolve_target
// replay_ini reuses the SAME remove_if_present + restore_vanilla body as replay_purge,
// but computes `target` from steam::my_games_path(&game.prefix) and does NOT call
// resolve_target / guard_within_root (that guard stays bounded to Data/).
```
**Why it works:** `begin_op` stores `target_rel` as an opaque string; the sentinel `"StarfieldCustom.ini"` (no `Data/` prefix) can never collide with a real deployed relpath (all of which are `Data/`-rooted) and is never in the deploy manifest, so `is_ours()` and `remove_deployed_file()` are harmless no-ops for it.

### Pattern 2: Provenance = the return bool of `backup_vanilla_if_absent` (the SFINI-02 seam)
**What:** `backup_vanilla_if_absent` returns `true` iff a pre-existing non-owned regular file was captured, `false` for a pure add. That bool **is** the provenance, and it is *persisted implicitly* by the presence/absence of a `vanilla_backup` row.
**When to use:** Restore path (`purge`/`replay`) reads it back with zero extra schema.
**Example:**
```rust
// Source: crates/deploy/src/backup.rs:51-112 + engine.rs deploy_one_file:362 (VERIFIED)
let pre_existing = backup::backup_vanilla_if_absent(store, game, &ini_target, sentinel_rel)?;
// PreExisting        => vanilla_for(appid, sentinel) is Some → restore_vanilla copies bytes back
// CreatedByNexTwist  => vanilla_for(appid, sentinel) is None → restore_vanilla no-ops;
//                       caller removes the file (already done) → restore-ABSENCE
```
**Why it works:** This is *exactly* how `replay_purge` already distinguishes a vanilla-overwrite from a pure-add: `remove_if_present(target)` then `restore_vanilla` (no-op when no row). The INI restore is the same two lines + empty-dir pruning.

### Pattern 3: Surgical byte merge preserving BOM/EOL (the SFINI-03 seam)
**What:** Never parse the whole file. Detect the leading BOM (UTF-8 `EF BB BF` / UTF-16 `FF FE`/`FE FF`), detect the dominant EOL (`\r\n` vs `\n`), locate `[Archive]` (case-insensitive section header) and the two keys, insert/update only those, re-emit every other byte untouched.
**Example:**
```rust
// std-only sketch — the actual editor lives in gameconfig.rs
fn detect_eol(s: &str) -> &'static str { if s.contains("\r\n") { "\r\n" } else { "\n" } }
// - New file: CRLF + no BOM (CE2/Windows convention, per CONTEXT.md).
// - Existing file: preserve its detected BOM + EOL exactly.
// - Merge into an existing [Archive] section in place; append a new [Archive] block
//   (with the file's EOL) only if the section is absent.
```

### Anti-Patterns to Avoid
- **Parse-and-reserialize the INI.** Any full-model library discards the byte-level details SFINI-03 requires. Byte-surgery only.
- **Relaxing `guard_within_root` / `resolve_target` to reach the prefix.** Breaks the `Data/`-root containment invariant. Use a distinct `kind` + independent resolution.
- **Overwriting a non-empty user `sResourceDataDirsFinal`.** Must surface as a typed conflict and block (CONTEXT.md, SFINI-03).
- **Inferring provenance from current disk state at restore time.** Always drive restore from the recorded ledger row, never "is the file there now?".
- **A separate profile-switch codepath.** `redeploy_winners` = `purge` + `deploy_winners`; wiring the two choke points covers profile switch for free `[VERIFIED: engine.rs:307-318]`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Original-byte capture + content-addressed store | A new backup table/dir | `backup::backup_vanilla_if_absent` | blake3-keyed, dedup'd, idempotent, already tested `[VERIFIED: backup.rs]` |
| Byte-for-byte restore | A copy-back helper | `backup::restore_vanilla` | Handles missing-blob → `NotPristine`, removes prior placement, idempotent `[VERIFIED: backup.rs:89]` |
| Crash-safe intent record | A new sentinel file / lock | `store::begin_op` + `mark_done` (`kind=KIND_INI`) | `synchronous=FULL` durable-before-syscall, replayed on launch `[VERIFIED: store/journal.rs]` |
| Empty-dir pruning after restore-absence | A bespoke walker | `engine::remove_emptied_dirs` pattern | Bottom-up `remove_dir` refuses non-empty dirs — the safety net already exists `[VERIFIED: engine.rs:462]` |
| Proton-prefix INI path | A new path builder | `steam::my_games_path(&game.prefix)` | Case-folded, redirect-aware, `drive_c`-contained (Phase-6 hardened) `[VERIFIED: ce2.rs:82]` |
| Byte-for-byte pristine assertion in tests | A new comparison | `testkit::assert_trees_identical` + `DIR_SENTINEL` | The established reversibility oracle `[VERIFIED: testkit/lib.rs:262]` |

**Key insight:** Phase 8 writes almost no new *logic*. The one genuinely new algorithm is the ~60-line surgical INI editor; everything else is calling existing seams with a different target path and a new `kind` token.

## Runtime State Inventory

> This is an additive greenfield module (new file, new journal kind, new INI target), **not** a rename/refactor of existing state. The inventory is therefore mostly "nothing to migrate", but two items matter:

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | `op_journal` gains rows with `kind='ini'`; `vanilla_backup` gains a row keyed `(1716740, "StarfieldCustom.ini")` | None — additive; schema already generic, **no DB migration** (store is appid/kind-generic) `[VERIFIED: store schema read this session]` |
| Live service config | The real `StarfieldCustom.ini` inside the user's Proton prefix (`compatdata/1716740/pfx/.../My Games/Starfield/`) — lives on disk, not in git | This IS the managed target; reversibility ledger tracks it. No export needed |
| OS-registered state | None — no Task Scheduler / systemd / registry writes | None ("None — verified: engine writes only files + SQLite rows") |
| Secrets/env vars | None | None |
| Build artifacts | New `crates/deploy/tests/ini_activation.rs` compiled into the deploy test binary; new `gameconfig` module | Standard `cargo test --workspace --locked` picks them up |

**Canonical question — after all repo files are updated, what runtime state persists?** The INI file on the user's prefix + its `op_journal`/`vanilla_backup` rows. Both are governed by the reversibility engine; a purge restores the INI to its recorded provenance and the rows to `done`/removed. **Nothing is left un-tracked.**

## Common Pitfalls

### Pitfall 1: The `KIND_INI` replay arm accidentally routing through `resolve_target`
**What goes wrong:** Copy-pasting `replay_purge` verbatim resolves the target as `<install>/Data/StarfieldCustom.ini` — the wrong file, outside the prefix.
**Why it happens:** `replay_purge` calls `crate::resolve_target(&game.install_dir, &row.target_rel)`.
**How to avoid:** `replay_ini` must compute `target = steam::my_games_path(&game.prefix).join("StarfieldCustom.ini")`. Add a test asserting the replayed path is under the prefix, not under `Data/`.
**Warning signs:** A recovery test leaves a stray `Data/StarfieldCustom.ini`.

### Pitfall 2: Sentinel `target_rel` colliding with a real deployed relpath
**What goes wrong:** If the sentinel were `Data/StarfieldCustom.ini`, `is_ours` could match a manifest row and `remove_deployed_file` could drop a real row.
**Why it happens:** All real relpaths are `Data/`-rooted.
**How to avoid:** Use a sentinel with **no** `Data/` prefix (e.g. bare `"StarfieldCustom.ini"`), guaranteeing non-collision. Document it as a reserved key.
**Warning signs:** A deploy manifest row disappears after an INI purge.

### Pitfall 3: BOM/EOL loss on merge (SFINI-03 failure)
**What goes wrong:** Reading with `read_to_string` and writing with `writeln!` silently converts CRLF→LF and drops the BOM.
**Why it happens:** Rust string I/O normalizes nothing but *also* preserves nothing you don't explicitly re-emit.
**How to avoid:** Read as `Vec<u8>`; detect+strip BOM into a saved prefix; detect EOL; operate on the body; re-prepend BOM + re-emit with the detected EOL. A round-trip test on a CRLF+BOM fixture that adds *nothing* must be byte-identical.
**Warning signs:** `assert_trees_identical` fails on a no-op re-activation of a pre-existing file.

### Pitfall 4: Restore-absence not pruning a NexTwist-created config dir
**What goes wrong:** For a pre-first-launch prefix, NexTwist may create `My Games/Starfield/` to write the INI; restore-absence must remove the file **and** any dir NexTwist created, but must never remove a dir the game created.
**Why it happens:** `FirstLaunchPending` means the dir may not have pre-existed.
**How to avoid:** Reuse the `remove_emptied_dirs` discipline (bottom-up `remove_dir`, which refuses non-empty dirs) bounded to the created chain; or record the created-dir set the same way deploy does. Belt-and-suspenders: `remove_dir` on a game-populated dir errors `DirectoryNotEmpty` and is treated benign.
**Warning signs:** A purge on a created-by-NexTwist INI leaves an empty `My Games/Starfield/`.

### Pitfall 5: Idempotency drift → two `[Archive]` sections
**What goes wrong:** A second ensure-active appends a fresh `[Archive]` block instead of merging into the existing one.
**Why it happens:** Insert-always instead of find-then-insert.
**How to avoid:** The editor must locate an existing `[Archive]` (case-insensitively) and merge in place; only append when truly absent. Test: `ensure ×2` + `recover_on_launch` all converge to exactly one `[Archive]` with identical bytes.

## Code Examples

### Choke-point wiring (engine.rs, gated on Starfield)
```rust
// Source: crates/deploy/src/engine.rs deploy_winners:219 / purge:401 (VERIFIED this session)
// At the TAIL of deploy_winners(), after the winners loop:
if game.appid == steam::STARFIELD {
    gameconfig::ensure_ini_active(store, game, &mut report)?;   // journaled, conflict-checked
}
// Inside purge(), after the manifest loop + remove_emptied_dirs:
if game.appid == steam::STARFIELD {
    gameconfig::restore_ini(store, game, &mut report)?;         // provenance-driven restore
}
// recover_on_launch already drives journal::replay, which gets the new KIND_INI arm — no extra call.
```

### The `is_starfield` predicate
```rust
// No new predicate module needed. steam::STARFIELD (=1716740) is already exported.
// Source: crates/steam/src/lib.rs:30, resolve.rs:23 (VERIFIED)
game.appid == steam::STARFIELD
// (Optionally add `pub fn is_starfield(g: &Game) -> bool` in steam for readability — 1 line.)
```

### Conflict as a typed error surfaced through the report
```rust
// Mirror PurgeReport.orphans: surface, do not overwrite. thiserror arm in error.rs.
pub enum DeployError { /* … */ IniConflict { current_value: String } }
// gameconfig detects a non-empty user sResourceDataDirsFinal → returns/records IniConflict;
// activation blocks pending an explicit keep-mine / use-NexTwist choice (CONTEXT.md).
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Deploy strictly bounded to `Data/` | First sanctioned write outside `Data/` (the prefix INI) via a distinct journal kind | Phase 8 | Guard stays bounded; new kind carries the out-of-root op |
| INI editing via a parser crate (roadmap's tentative `rust-ini`) | std-only surgical byte editor | This research | Zero new deps; strictly better byte fidelity |
| Provenance as an explicit new column | Reuse `vanilla_backup` presence/absence as the provenance bit | This research | No schema change |

**Deprecated/outdated:**
- `rust-ini` for this use: not deprecated as a crate, but wrong tool for byte-fidelity — do not add it.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The two owned keys are exactly `[Archive] bInvalidateOlderFiles=1` and `sResourceDataDirsFinal=` (empty) | Merge logic | MEDIUM — CE2 loose-file recipe is version-sensitive; **Phase 9 validates on hardware**. Treat as validated-against-a-build, not a constant (CONTEXT.md/STATE.md already flag this) |
| A2 | New file convention = CRLF + no BOM | Byte fidelity | LOW — CE2/Windows convention; a wrong choice is cosmetic and still reversible |
| A3 | The game creates `My Games/Starfield/`; NexTwist usually creates only the file | Restore-absence pruning | LOW — handled either way by the empty-dir safety net |
| A4 | `rust-ini`/`ini-roundtrip` do not round-trip CRLF+BOM+comments+order faithfully | Alternatives | LOW — corroborated by crate docs; and the recommendation (std editor) is correct regardless |

**Note:** A1 is the milestone's known MEDIUM-confidence item, deliberately deferred to the Phase 9 on-hardware gate. Phase 8 must keep the recipe in one obvious constant so Phase 9 can correct it without touching the reversibility machinery.

## Open Questions

1. **Where is the created-dir set for restore-absence recorded?**
   - What we know: deploy records created dirs implicitly via manifest relpaths; the INI has no `Data/` relpath.
   - What's unclear: whether to (a) record the created-dir chain explicitly, or (b) rely purely on bottom-up `remove_dir` refusing non-empty dirs.
   - Recommendation: (b) is sufficient and simplest — prune the ancestor chain of the INI up to (but excluding) the `My Games` root, `remove_dir` bottom-up; non-empty (game-populated) dirs are refused benignly. Add an explicit test for the first-launch created case.

2. **Preview/confirm gating (SFINI-01 "no silent edit") — synchronous or two-step?**
   - What we know: CONTEXT.md wants the exact change + provenance shown before writing, reusing the v1.0 dry-run/preview discipline.
   - Recommendation: a `preview_ini_activation` command returns `{will_create|will_edit, diff_lines, conflict?}`; the write only proceeds on explicit confirm. Mirror the existing deploy dry-run shape.

## Environment Availability

> This phase is code + local-filesystem only. No new external tools/services.

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain (`~/.cargo/bin/cargo`) | build/test | ✓ (project pinned ≥1.89) | rust-toolchain.toml | — |
| No new crates | — | n/a | — | — |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** none.

## Validation Architecture

> `nyquist_validation: true` — section required.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + integration tests under `crates/deploy/tests/` |
| Config file | none (cargo convention) |
| Quick run command | `~/.cargo/bin/cargo test -p nextwist-deploy ini` |
| Full suite command | `~/.cargo/bin/cargo test --workspace --locked` |

### Observable Truths → Test Map
| Req ID | Behavior (observable truth) | Test Type | Automated Command | File Exists? |
|--------|-----------------------------|-----------|-------------------|-------------|
| SFINI-01 | Preview reports will-create vs will-edit + exact lines before any write | integration | `cargo test -p nextwist-deploy ini_preview` | ❌ Wave 0 |
| SFINI-02a | **PreExisting** INI → purge restores **original bytes** byte-for-byte | integration | `cargo test -p nextwist-deploy ini_restore_preexisting` | ❌ Wave 0 |
| SFINI-02b | **CreatedByNexTwist** INI → purge deletes file **and** prunes NexTwist-created empty dirs (restore-ABSENCE) | integration | `cargo test -p nextwist-deploy ini_restore_absence` | ❌ Wave 0 |
| SFINI-03a | Surgical merge into existing `[Archive]` preserves all other keys/sections/comments/order | integration | `cargo test -p nextwist-deploy ini_merge_nonclobber` | ❌ Wave 0 |
| SFINI-03b | CRLF + BOM of a pre-existing file preserved byte-for-byte on activation | integration | `cargo test -p nextwist-deploy ini_crlf_bom` | ❌ Wave 0 |
| SFINI-03c | Non-empty user `sResourceDataDirsFinal` surfaces a typed conflict, is NOT overwritten | integration | `cargo test -p nextwist-deploy ini_conflict` | ❌ Wave 0 |
| SFINI-04 | deploy×2 / profile-switch(redeploy) / crash-replay all converge to **one** `[Archive]`, identical bytes | integration | `cargo test -p nextwist-deploy ini_idempotent` | ❌ Wave 0 |
| SFINI-05 | `recover_on_launch` replays a mid-INI-op crash to a consistent state; verify/repair treats INI like deployed files | integration | `cargo test -p nextwist-deploy ini_crash_recovery` | ❌ Wave 0 |
| SFINI-04 | Journal `KIND_INI` path resolves under the **prefix**, never under `Data/`; `Data/`-root guard untouched | unit | `cargo test -p nextwist-deploy ini_kind_path` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `~/.cargo/bin/cargo test -p nextwist-deploy ini` + `cargo clippy -p nextwist-deploy --all-targets -- -D warnings`
- **Per wave merge:** `~/.cargo/bin/cargo test --workspace --locked`
- **Phase gate:** full workspace green + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo deny check`

### Wave 0 Gaps
- [ ] `crates/deploy/tests/ini_activation.rs` — the reversibility suite (all rows above), extending `testkit::{snapshot_tree, assert_trees_identical, DIR_SENTINEL, fake_my_games_prefix}`.
- [ ] Test fixtures: a CRLF+BOM `StarfieldCustom.ini` with unrelated `[Display]`/comments, an empty prefix (created-by-us), a non-empty-`sResourceDataDirsFinal` conflict fixture.
- [ ] Framework install: none — existing cargo test infra covers it.

## Security Domain

> `security_enforcement: true`, ASVS level 1. This is the phase's **first write outside `Data/`** — it warrants its own SECURITY.md.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V5 Input Validation | yes | The INI path is derived from a Proton prefix + a `user.reg` `Personal` redirect (attacker-influenceable). Reuse Phase-6's lexical `drive_c` containment; the INI write must **re-verify** the final target is under `<prefix>/drive_c` before opening it. `[VERIFIED: ce2.rs:93 containment check]` |
| V5 Input Validation | yes | INI-content injection: NexTwist writes fixed keys/values (no user-supplied bytes flow into the two owned keys). When reading an existing file for merge, treat it as untrusted bytes — never `eval`/execute, only slice. |
| V6 Cryptography | no (reuse) | blake3 is a content hash for integrity, not a security primitive here — no hand-rolled crypto. |
| V12 File & Resources | yes | Writes outside the deploy root: bound the write to the resolved prefix path; never follow a symlink out (mirror `backup.rs`'s `symlink_metadata` discipline); refuse to write if the resolved path escapes `drive_c`. |

### Known Threat Patterns for {Rust engine writing into a Wine prefix}

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| `user.reg` `Personal` redirect with mixed-separator / absolute `..` escaping `drive_c` | Tampering / Elevation | Phase-6 guard already rejects `/`-embedded, absolute, and backslash `..` forms; the INI write re-checks lexical `drive_c` containment before opening `[VERIFIED: ce2.rs traversal tests; also caught previously as a HIGH path-traversal finding]` |
| Symlink at the INI target pointing outside the prefix | Tampering | Use `symlink_metadata`, refuse to write through a symlink we do not own (same rule `backup_vanilla_if_absent` applies) |
| Crash mid-write leaving a half-written INI | DoS / Integrity | Intent-before-act journal + idempotent replay → `recover_on_launch` reconstructs exactly one correct `[Archive]` |
| Purge deleting a user file it did not create | Tampering | Provenance-driven restore (ledger row), never inferred from disk; restore-absence only for `CreatedByNexTwist` |

> **SECURITY.md focus:** (1) the `drive_c` containment re-check at write time, (2) symlink-refusal at the INI target, (3) the argument that the new `KIND_INI` op does not widen the `Data/`-root guard, (4) restore-absence never removes game-created dirs.

## Sources

### Primary (HIGH confidence)
- `crates/deploy/src/{backup.rs, journal.rs, engine.rs, lib.rs}` — read in full this session (reuse seams, choke points, guards, empty-dir pruning).
- `crates/store/src/{journal.rs, vanilla.rs}` — row schema (`kind`, generic `target_rel`), `begin_op`/`mark_done`/`pending_ops`, `record_vanilla`/`vanilla_for`.
- `crates/steam/src/ce2.rs` + `lib.rs` — `my_games_path`, `STARFIELD`, `resolve_ce2_config`, `drive_c` containment + traversal tests.
- `crates/core/src/model.rs` — `Game { appid, name, install_dir, prefix, staging_dir }` (carries `prefix`).
- `crates/testkit/src/lib.rs` — `snapshot_tree`, `assert_trees_identical`, `DIR_SENTINEL`, `fake_my_games_prefix`.
- the project configuration — `nyquist_validation: true`, `security_enforcement: true`, ASVS 1.
- crates.io API — `rust-ini` 0.21.3 (2025-08-30) version/metadata.

### Secondary (MEDIUM confidence)
- [lib.rs/crates/ini-roundtrip](https://lib.rs/crates/ini-roundtrip) — confirms no Rust INI crate round-trips newlines/BOM on save.
- [docs.rs/ini](https://docs.rs/ini) — `rust-ini` parse-and-reserialize model.

### Tertiary (LOW confidence)
- CE2 loose-file INI recipe (A1) — MEDIUM per milestone; deferred to the Phase 9 on-hardware gate.

## Metadata

**Confidence breakdown:**
- Standard stack / reuse seams: HIGH — every seam read directly from source this session.
- Architecture / wiring: HIGH — choke points, journal dispatch, and guards verified in code.
- INI editor decision: HIGH — "add no dependency, hand-roll std byte editor" is both the correct fidelity answer and corroborated by crate docs.
- CE2 INI-key recipe (A1): MEDIUM — intentionally validated on hardware in Phase 9.

**Research date:** 2026-07-08
**Valid until:** 2026-08-07 (stable in-repo substrate; the only external fact, `rust-ini`'s version, is not load-bearing since it is rejected)
