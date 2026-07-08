# Phase 8: Reversible StarfieldCustom.ini Activation - Context

**Gathered:** 2026-07-08
**Status:** Ready for planning
**Mode:** Smart discuss (autonomous) — grey-area recommendations auto-accepted; safety mechanics are requirement-locked (SFINI-01..05), not discretionary.

<domain>
## Phase Boundary

Deliver reversible loose-file activation for Starfield: NexTwist writes/merges the two owned `[Archive]` keys (`bInvalidateOlderFiles=1`, `sResourceDataDirsFinal=`) into `StarfieldCustom.ini` at the CE2 prefix path (`Documents/My Games/Starfield`, resolved in Phase 6), under the **same non-destructive, byte-for-byte reversible, journaled guarantee** as file deployment. The edit is wired at the single engine choke point (end of `deploy`/`deploy_winners`/`purge` + `recover_on_launch`), gated on `is_starfield`, and rides its own journal sentinel key **outside** the `Data/` deploy root. Includes UI surfacing of the exact reversible change and any conflict.

**In scope:** `StarfieldCustom.ini` only; the two owned keys; provenance-driven restore (bytes-or-absence); surgical merge preserving byte fidelity; journaled idempotency + crash replay; verify/repair participation; a testkit reversibility suite; UI change-preview + conflict surfacing. Warrants its own SECURITY.md.

**Out of scope:** `StarfieldPrefs.ini` (user domain); `.ccc` authoring; `.ba2` v3 inspection; the deployed-vs-actually-loaded-in-game distinction (SFVER-02 → Phase 9).

</domain>

<decisions>
## Implementation Decisions

### Activation Trigger & Lifecycle
- INI activation is driven automatically at the existing deploy choke point (`deploy_winners`/`purge`/`recover_on_launch`), gated on `is_starfield` — not a separate manual command. Other games are never touched.
- Ensure-present on deploy; restore-to-prior-state on purge. Symmetric with the deployment lifecycle.
- A profile switch runs purge + redeploy, which re-runs the choke points, so INI state stays consistent with the active profile's deployed loose files — no separate profile-switch codepath.
- **No silent edit (SFINI-01):** the exact reversible change (lines to add + provenance: will-create-file vs will-edit-existing) is surfaced to the user before it is written, reusing the v1.0 dry-run/preview discipline.

### Merge, Conflict & Byte-Fidelity
- Surgical merge **into** an existing `[Archive]` section; never rewrite the whole file. All other sections, keys, comments, and ordering are preserved (SFINI-03).
- An existing **non-empty** user `sResourceDataDirsFinal` value surfaces as a **conflict** and is **not** overwritten — activation blocks pending an explicit user choice (keep-mine vs use-NexTwist's). Only an empty/absent value is auto-written.
- `bInvalidateOlderFiles` is NexTwist-owned: set to `1`, recording any prior value/state so restore is exact.
- Byte fidelity: detect and preserve the file's existing EOL style (CRLF) and BOM; a newly-created file uses CRLF + no BOM (CE2/Windows convention).

### Reversibility Mechanics (provenance + journal)
- Record `PreExisting` vs `CreatedByNexTwist` provenance at first touch, reusing the vanilla-backup ledger seam (`backup::backup_vanilla_if_absent` captures original bytes; absence is recorded when the file did not exist).
- Restore (SFINI-02): `restore_vanilla` byte-for-byte for `PreExisting`; delete-file **and prune any now-empty NexTwist-created parent dirs** for `CreatedByNexTwist` (restore-absence).
- The INI op rides the operation journal (intent-before-act `pending`→`done`, idempotent replay) on its **own sentinel key outside the `Data/` deploy root** — leave the deploy-root path guard untouched (SFINI-04).
- Crash recovery: `recover_on_launch` replays the INI op idempotently → exactly one `[Archive]` section, identical bytes. Verify/repair treats the INI like deployed files (SFINI-05).
- Regression-locked by a testkit reversibility suite extending `DIR_SENTINEL`, covering **both** provenance branches (pre-existing byte-restore + created-then-absence).

### UI Surfacing
- INI activation state, the change-preview confirmation, and any conflict live on the Starfield game view, near the existing first-launch / version-drift / masterlist-age notices (Phase 6/7 pattern).
- Conflict UI (existing user `sResourceDataDirsFinal`): a clear message showing the user's current value + an explicit keep-mine / use-NexTwist choice; no silent clobber.
- Deployed-vs-loaded (SFVER-02) is **not** surfaced here — Phase 8 shows INI activation state only; the in-game-loaded distinction is Phase 9.

### Claude's Discretion
- Exact module/type names (`gameconfig.rs`, provenance enum, journal sentinel key naming), the precise INI editor approach (`rust-ini` vs a minimal hand-rolled surgical editor that better preserves byte fidelity — the planner/researcher decides which upholds CRLF/BOM fidelity best), and the Tauri command + Svelte component shapes, following existing engine/adapter conventions.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets (verbatim reuse — roadmap-mandated)
- `crates/deploy/src/backup.rs`: `backup_vanilla_if_absent(store, game, target, target_rel) -> Result<bool>` and `restore_vanilla(store, game, target, target_rel) -> Result<bool>` — the content-addressed (blake3) vanilla ledger; the provenance + byte-for-byte restore substrate.
- `crates/deploy/src/journal.rs`: operation journal + `replay_purge(store, game, row)` — intent-before-act + idempotent replay to extend for the INI op.
- `crates/deploy/src/engine.rs`: `deploy_winners:219`, `redeploy_winners:307`, `recover_on_launch:511` — the single choke point set to wire the INI activation into.
- `crates/steam/src/ce2.rs`: `StarfieldStatus` / `starfield_status` + the Phase 6 `my_games_path` resolver (CE2 `Documents/My Games/Starfield`, case-folded, first-launch-aware, lexically `drive_c`-contained) — the resolved INI target path.
- `crates/testkit`: `DIR_SENTINEL` blake3 pristine-tree assertion — extend for the reversibility suite.
- `is_starfield` gating already threaded through loadorder (`medium_master_classifies_true_only_for_starfield`) and steam.

### Established Patterns
- Headless `crates/*` engine, **zero Tauri deps**; `src-tauri/` command adapters stay thin (3–5 lines). Errors: `thiserror` in engine, `anyhow` at the boundary.
- Crash-safety = journaled intent-before-act + idempotent file ops (not WAL alone). Purge/verify operate from recorded state, never a blind disk scan.
- Manifest/ledger-derived reversibility bounded to the deploy root; the INI is the first sanctioned write **outside** `Data/`, so it needs its own sentinel, not a relaxed deploy-root guard.
- Phase 6 hardened this exact INI path against a `user.reg` path-traversal escape (lexical `drive_c` containment) — build the writes on that hardened resolver.

### Integration Points
- Engine: new `crates/deploy/src/gameconfig.rs` (~120 LOC) called at the end of `deploy_winners`/`purge` + inside `recover_on_launch`, gated on `is_starfield`.
- Tauri: a thin command to preview/confirm/apply the activation + report conflict state.
- Frontend: Starfield game-view surfacing (change-preview + conflict), consistent with Phase 6/7 notices.

</code_context>

<specifics>
## Specific Ideas

- The two owned keys are exactly `[Archive] bInvalidateOlderFiles=1` and `sResourceDataDirsFinal=` (empty value on our write). These are the loose-file-loading enablers for CE2; treat the recipe as **validated-against-a-build** (Phase 9 closes the MEDIUM-confidence on real hardware), not a permanent constant.
- Provenance is the crux: purge must restore **absence** (file deleted + emptied NexTwist-created dirs) when NexTwist created the INI, and **original bytes** when it pre-existed — driven by recorded provenance, never inferred from the current disk state.
- Idempotency target: deploy-twice / profile-switch / crash-replay all converge to one `[Archive]` section with identical bytes.

</specifics>

<deferred>
## Deferred Ideas

- **SFVER-02** deployed-vs-actually-loaded-in-game distinction → Phase 9 (owns the on-hardware validation).
- `.ba2` v3 archive inspection (SFBA2-01, future); `StarfieldPrefs.ini` management (explicitly out of scope); SFSE management (SFSE-01, future).

</deferred>
