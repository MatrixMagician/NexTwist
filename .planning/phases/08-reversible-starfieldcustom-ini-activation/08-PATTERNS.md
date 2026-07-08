# Phase 8: Reversible StarfieldCustom.ini Activation - Pattern Map

**Mapped:** 2026-07-08
**Files analyzed:** 8 (2 new engine/test, 4 modified engine, 1 new Tauri cmd, 1 modified frontend)
**Analogs found:** 8 / 8 (every seam has an in-repo analog — this is a reuse phase, zero new logic outside the ~60-line byte editor)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/deploy/src/gameconfig.rs` (NEW) | service/util | file-I/O + transform | `crates/deploy/src/backup.rs` + `engine.rs::deploy_one_file` | role-match (byte-surgery is new, op-wrapper is exact) |
| `crates/deploy/src/journal.rs` (MODIFY) | service | event-driven (replay) | `journal.rs::replay_purge` (existing arm) | exact |
| `crates/deploy/src/engine.rs` (MODIFY) | service | request-response (choke point) | `engine.rs::deploy_winners` / `purge` tails | exact |
| `crates/deploy/src/error.rs` (MODIFY) | model | — | `DeployError::NotPristine` / `Profile` arms | exact |
| `crates/store/*` (CONFIRM, likely no change) | store | CRUD | `store::begin_op` / `OpIntent` | exact — already `kind`+`target_rel` generic |
| `crates/deploy/tests/ini_activation.rs` (NEW) | test | file-I/O | `crates/deploy/tests/vanilla_restore.rs` | exact |
| `src-tauri/src/commands/gameconfig.rs` (NEW) | controller/adapter | request-response | `src-tauri/src/commands/deploy.rs` | exact |
| `frontend/src/routes/+page.svelte` (MODIFY) | component | request-response | existing Starfield `ce2_state`/`drift` notice block | exact |

## Pattern Assignments

### `crates/deploy/src/gameconfig.rs` (NEW — service/util, file-I/O + transform)

**Analogs:** `crates/deploy/src/backup.rs` (reuse verbatim for provenance/restore) + `engine.rs::deploy_one_file:346-392` (op-sequence shape).

**Op-sequence pattern — mirror `deploy_one_file` exactly** (`engine.rs:358-387`): durable intent BEFORE syscall → backup-before-overwrite → idempotent file op → mark done. `ensure_ini_active` follows the identical ordering:
```rust
// 1. Durable intent BEFORE any syscall (journal::begin_ini — see journal.rs section)
let jid = journal::begin_ini(store, game.appid, INI_SENTINEL_REL)?;
// 2. Backup-before-overwrite = provenance capture (the bool IS the provenance — SFINI-02)
let pre_existing = backup::backup_vanilla_if_absent(store, game, &ini_target, INI_SENTINEL_REL)?;
// 3. Surgical byte merge (conflict-checked) then write ini_target
// 4. journal::finish_ini(store, jid)   (mark_done AFTER the syscall)
```

**Provenance = return bool of `backup_vanilla_if_absent`** (`backup.rs:51-84`): returns `true` iff a pre-existing non-owned regular file was captured (→ `PreExisting`, restore copies bytes back), `false` for a pure add (→ `CreatedByNexTwist`, restore = remove + prune). No new provenance column — the presence/absence of the `vanilla_backup` row IS the persisted provenance bit. Restore uses `backup::restore_vanilla` (`backup.rs:89-112`) which no-ops (returns `false`) when no row exists.

**Target-path resolution (NOT `resolve_target`)**: compute the target from the Phase-6 hardened resolver, never the `Data/`-root one:
```rust
let ini_target = steam::my_games_path(&game.prefix).join("StarfieldCustom.ini");
```
`my_games_path` (`crates/steam/src/ce2.rs:82-101`) is redirect-aware, case-folded, and enforces the load-bearing `resolved.starts_with(prefix.join("drive_c"))` containment invariant (T-06-01). The INI write MUST re-verify containment before opening (SECURITY.md V5/V12).

**Symlink-refusal discipline — copy from `backup.rs:68-72`**: use `fs::symlink_metadata`, never write through a symlink you do not own.
```rust
let meta = fs::symlink_metadata(target)...;
if meta.file_type().is_symlink() { return Ok(false); }  // do not copy through it
```

**Restore-absence empty-dir pruning — reuse `remove_emptied_dirs` discipline** (`engine.rs:462-507`): bottom-up `std::fs::remove_dir` (NOT `remove_dir_all`) which errors `DirectoryNotEmpty` on any dir still holding files — treat that error and `NotFound` as benign. Bound the prune to the INI's ancestor chain up to (excluding) the `My Games` root; a game-populated dir is refused safely.

**Surgical byte-merge (the one genuinely new algorithm, ~60 lines, std-only)**: read as `Vec<u8>`; detect+strip BOM (`EF BB BF` / `FF FE` / `FE FF`) into a saved prefix; detect dominant EOL (`\r\n` vs `\n`); locate `[Archive]` case-insensitively; merge the two owned keys in place (append a new `[Archive]` block only if absent); re-prepend BOM + re-emit with detected EOL. New file → CRLF + no BOM. Keep the two keys in ONE obvious constant (Phase 9 corrects the recipe without touching the machinery — Assumption A1). No INI crate — `rust-ini`/`ini-roundtrip` were rejected on byte-fidelity grounds (RESEARCH Alternatives table).

---

### `crates/deploy/src/journal.rs` (MODIFY — service, event-driven replay)

**Analog:** the existing `KIND_PURGE` const (`journal.rs:29`), `begin_purge` (`journal.rs:52-65`), and `replay_purge` (`journal.rs:174-181`).

**Add a `kind` token beside the existing two** (`journal.rs:28-29`):
```rust
pub const KIND_DEPLOY: &str = "deploy";
pub const KIND_PURGE: &str = "purge";
pub const KIND_INI: &str = "ini";   // NEW
```

**Add a `begin_ini` mirroring `begin_purge`** (`journal.rs:52-65`) — `method: None, source_hash: None, kind: KIND_INI`. `OpIntent` (`crates/store/src/journal.rs:26-37`) stores `target_rel` as an opaque string, so the sentinel `"StarfieldCustom.ini"` (bare, NO `Data/` prefix — Pitfall 2) can never collide with a real deployed relpath.

**Add a `KIND_INI` arm to `replay`'s match** (`journal.rs:121-133`, currently `KIND_DEPLOY`/`KIND_PURGE`/unknown-warn):
```rust
KIND_INI => replay_ini(store, game, row)?,
```
**`replay_ini` — copy `replay_purge`'s BODY (`journal.rs:174-181`) but change ONLY the target resolution** (Pitfall 1 — the copy-paste trap):
```rust
// WRONG (replay_purge): crate::resolve_target(&game.install_dir, &row.target_rel)  → Data/…
// RIGHT (replay_ini):
let target = steam::my_games_path(&game.prefix).join("StarfieldCustom.ini");
// then the SAME two lines: remove_if_present(target) + restore_vanilla(...) + mark_done,
// plus empty-dir pruning for the CreatedByNexTwist branch.
```
`recover_on_launch` (`engine.rs:511-528`) already drives `journal::replay`, so no new call there — the new arm is picked up automatically.

---

### `crates/deploy/src/engine.rs` (MODIFY — service, choke-point wiring)

**Analog:** the tail of `deploy_winners` (`engine.rs:280-282`, after the winners loop) and the tail of `purge` (`engine.rs:436-438`, after `remove_emptied_dirs`).

**Gate on `game.appid == steam::STARFIELD`** (already exported; RESEARCH Code Examples). Wire:
```rust
// TAIL of deploy_winners(), after `report.fs_warnings = seen_warnings;` (line ~281):
if game.appid == steam::STARFIELD {
    gameconfig::ensure_ini_active(store, game, /* &mut report or a dedicated report */)?;
}
// Inside purge(), after remove_emptied_dirs (line ~436):
if game.appid == steam::STARFIELD {
    gameconfig::restore_ini(store, game, ...)?;
}
```
`redeploy_winners` (`engine.rs:307-318`) is `purge` + `deploy_winners`, so a profile switch re-runs both hooks for free — NO separate codepath (Anti-Pattern). `deploy()` single-root path (`engine.rs:~200`) shares no Starfield use today but wire the same tail if that path is reachable for Starfield.

---

### `crates/deploy/src/error.rs` (MODIFY — model)

**Analog:** the existing `NotPristine(String)` (`error.rs:37-38`) and `Profile(String)` (`error.rs:49-50`) typed arms.

Add a typed conflict arm mirroring their shape (thiserror, `#[error(...)]`):
```rust
/// A pre-existing non-empty user sResourceDataDirsFinal blocks auto-activation (SFINI-03).
#[error("StarfieldCustom.ini conflict: user set sResourceDataDirsFinal to {current_value:?}")]
IniConflict { current_value: String },
```
Surface it through the report (mirror `PurgeReport.orphans` — surface, never overwrite), don't panic.

---

### `crates/store/*` (CONFIRM — likely NO change)

**Analog:** `store::begin_op` (`crates/store/src/journal.rs:60-75`), `OpIntent`/`JournalRow` (`journal.rs:26-56`), `record_vanilla`/`vanilla_for` (`crates/store/src/vanilla.rs:20,52`).

The `op_journal` schema is `(appid, target_rel TEXT, method, source_hash, kind TEXT, state)` — fully generic; `kind` is a free string and `target_rel` an opaque string. **No new store API and NO DB migration** — `KIND_INI` and the `"StarfieldCustom.ini"` sentinel key ride the existing primitives verbatim. Confirm only that no `store`-side constant is expected (the `KIND_*` consts live in `deploy::journal`, not `store`).

---

### `crates/deploy/tests/ini_activation.rs` (NEW — test, file-I/O)

**Analog:** `crates/deploy/tests/vanilla_restore.rs` (full file) + `crash_recovery.rs`.

**Copy the `fixture()` helper shape** (`vanilla_restore.rs:19-34`): install/staging/originals under one tempdir (same `st_dev` so links are viable in CI), `Store::open`, build a `Game`, `add_managed_game`. For Starfield tests set `appid: 1716740` and add a `prefix` with a `My Games/Starfield` tree via `testkit::fake_my_games_prefix` (`crates/testkit/src/lib.rs:136`).

**Copy the reversibility-assertion pattern** (`vanilla_restore.rs:46-88`): `snapshot_tree(pristine)` → activate → `purge` → `snapshot_tree(after)` → `assert_trees_identical(&pristine, &after)` (`testkit::assert_trees_identical`, `DIR_SENTINEL`-aware, `lib.rs:262`). Assert `store.pending_ops().unwrap().is_empty()` after a clean op (`vanilla_restore.rs:77`) — the intent-before-act resolution check.

**Test rows to cover** (RESEARCH Validation table): `ini_preview` (SFINI-01), `ini_restore_preexisting` (02a byte-restore), `ini_restore_absence` (02b delete+prune), `ini_merge_nonclobber` (03a), `ini_crlf_bom` (03b — a CRLF+BOM fixture that adds nothing must be byte-identical), `ini_conflict` (03c), `ini_idempotent` (04 — ensure×2 + replay → one `[Archive]`), `ini_crash_recovery` (05), `ini_kind_path` (unit — replayed path is under prefix, never `Data/`).

---

### `src-tauri/src/commands/gameconfig.rs` (NEW — controller/adapter, request-response)

**Analog:** `src-tauri/src/commands/deploy.rs` (full file — the canonical thin adapter).

Keep it 3–5 lines per command: `require_game`, lock `state.store`, forward one engine call, `.map_err(boundary_err)` (`deploy.rs:16-25`). Register in `src-tauri/src/commands/mod.rs` and the `invoke_handler`. Add a `preview_ini_activation` returning `{will_create|will_edit, diff_lines, conflict?}` (RESEARCH Open Q2 — synchronous preview; write proceeds only on explicit confirm, SFINI-01 no-silent-edit). Errors cross via `anyhow`/`boundary_err` at this boundary only — engine stays `thiserror`.

---

### `frontend/src/routes/+page.svelte` (MODIFY — component, request-response)

**Analog:** the existing Starfield CE2 notice block in the same file: `starfield = $state<StarfieldStatus | null>(null)` (`+page.svelte:157`), `isStarfield = $derived(...)` (`:158`), `starfieldPending`/`ce2_state` handling (`:160-169`), `loadStarfieldStatus()` (`:209-212`), and the drift/first-launch/masterlist-age notices this phase must sit beside (CONTEXT UI Surfacing).

Add an INI-activation state + change-preview confirmation + conflict message near those notices, gated by the existing `isStarfield` derived. Reuse existing notice CSS classes (per 08-UI-SPEC.md — do not invent styles). Add the `api.*` binding in `frontend/src/lib/api.ts` mirroring `api.starfieldStatus`.
> **Tooling note:** `+page.svelte` contains non-UTF-8 bytes — plain `grep`/`git diff` treat it as binary. Edit via exact-string replacement, verify with `grep -a`, and expect `git diff --stat` to report `Bin X -> Y bytes` (not a corruption signal).

---

## Shared Patterns

### Intent-before-act journaling (crash safety)
**Source:** `crates/deploy/src/journal.rs:1-16` (protocol doc) + `engine.rs:358-387` (`deploy_one_file` sequence).
**Apply to:** `gameconfig::ensure_ini_active` / `restore_ini`.
Durable `pending` row BEFORE the syscall (`store` opens `synchronous=FULL`), idempotent file op, flip to `done` AFTER. A crash leaves a `pending` row whose disk effect is absent-or-complete → replayed idempotently by `recover_on_launch`.

### Content-addressed provenance + byte restore
**Source:** `crates/deploy/src/backup.rs:51-112`.
**Apply to:** the INI capture/restore. `backup_vanilla_if_absent` (bool = provenance) + `restore_vanilla` (no-op when no row). Do NOT add a provenance column; the `vanilla_backup` row's presence IS the bit. Never infer provenance from current disk state (Anti-Pattern).

### `Data/`-root guard stays untouched (SFINI-04 safety seam)
**Source:** `engine.rs::deploy_one_file:353-354` (`guard_within_root(data_dir, &target)`) — the guard the INI op must NOT relax.
**Apply to:** the INI op resolves its target independently via `steam::my_games_path` and rides `KIND_INI`; it never calls `resolve_target`/`guard_within_root`. The INI's own containment is the `drive_c` check inside `my_games_path` (`ce2.rs:93`), re-verified at write time.

### Thin Tauri adapter + boundary error mapping
**Source:** `src-tauri/src/commands/deploy.rs:16-24`.
**Apply to:** the new gameconfig command(s). `require_game` + one engine call + `boundary_err`. No safety logic in the adapter.

## No Analog Found

None. Every seam maps to an existing, tested in-repo pattern. The only genuinely new *code* (not a reused seam) is the ~60-line std-only surgical byte editor inside `gameconfig.rs` — and even it has a structural analog in `backup.rs`'s `fs::read`-then-slice byte handling. Planner should NOT reach for RESEARCH-only generic patterns; every file above has a concrete codebase analog with line numbers.

## Metadata

**Analog search scope:** `crates/deploy/src`, `crates/deploy/tests`, `crates/store/src`, `crates/steam/src`, `crates/testkit/src`, `src-tauri/src/commands`, `frontend/src/routes`.
**Files scanned:** journal.rs (both), backup.rs, engine.rs, error.rs, ce2.rs, vanilla_restore.rs, testkit/lib.rs (grep), deploy.rs (cmd), +page.svelte (grep -a).
**Pattern extraction date:** 2026-07-08
</content>
</invoke>
