# Pitfalls Research

**Domain:** Adding Starfield (Creation Engine 2) support — reversible `StarfieldCustom.ini` editing, Proton-prefix path resolution, and CE2 load order — to NexTwist's existing non-destructive/byte-for-byte-reversible deploy engine.
**Researched:** 2026-07-07
**Confidence:** HIGH for load-order rules (source: Ortham, the libloot/LOOT author), the loose-file INI settings, and libloot's Starfield support (registry + docs.rs verified); MEDIUM for the loose-file regression history and Proton-prefix folder-lifecycle specifics.

> Phase numbers below follow PROJECT.md ("Phase numbering continues from v1.0, starts at Phase 6"). Named phases are the likely owners; the roadmap should confirm exact numbering. The through-line: the v1.0 safety guarantee (non-destructive, byte-for-byte reversible via the journal + vanilla-backup ledger) must extend **verbatim** to the new INI write target — the INI is just another deployed artifact and must be journaled and reversible like a file op.

---

## Critical Pitfalls

### Pitfall 1: Purge leaves an INI behind when the file did not exist before (reversibility violation)

**What goes wrong:**
`StarfieldCustom.ini` does **not** ship with the game — the user (or NexTwist) must create it. If NexTwist creates the file to add `[Archive]`/`sResourceDataDirsFinal`, and then "purge/uninstall" merely blanks the two keys or writes an empty `[Archive]` section, the game state is left in a configuration that did **not** exist pre-mod. That is a direct breach of the v1.0 core guarantee ("byte-for-byte pristine, restore ABSENCE"). Worse, an orphaned empty `StarfieldCustom.ini` can itself change game behaviour (it overrides defaults for any key it contains).

**Why it happens:**
The v1.0 engine's reversibility model is "backup-before-overwrite → restore original bytes." That model implicitly assumes the target file **existed** and had original bytes to restore. An INI that was *absent* has no bytes to back up, so a naïve port records "no backup needed" and purge has nothing to remove — leaving the file NexTwist created. Developers think "restore = write the backup" and forget "restore = delete the file we created."

**How to avoid:**
Treat "file absent at first touch" as a first-class ledger state, not a missing backup. Before the first INI edit, record provenance in the store: `PreExisting{ backup_hash }` (blake3 of original bytes, exactly like the vanilla-backup ledger) or `CreatedByNexTwist`. Purge dispatches on that: `PreExisting` → restore backed-up bytes byte-for-byte; `CreatedByNexTwist` → **delete the file** (restore absence), and remove the `My Games/Starfield` directory only if NexTwist created it and it is now empty (mirror the v1.0 `DIR_SENTINEL` empty-dir pristine assertion). Same intent-before-act journal: write `pending{restore_absence}` before unlink, flip to `done` after.

**Warning signs:**
- The INI ledger schema has a nullable "original backup" column but no explicit "existed?" boolean.
- A round-trip test (`deploy → purge`) asserts INI *content* equals baseline but never asserts the file's *existence* equals baseline.
- Purge code path for the INI has no `remove_file` branch — only writes.

**Phase to address:**
Phase 8 — Reversible loose-file activation / INI management. Regression-lock with a testkit assertion (extend `DIR_SENTINEL`) covering both provenance branches.

---

### Pitfall 2: Clobbering an existing user `StarfieldCustom.ini` (whole-file rewrite instead of surgical merge)

**What goes wrong:**
Many Starfield users already have a hand-tuned `StarfieldCustom.ini` (FOV, mouse accel, ultrawide, `[Display]`, `[Controls]`, an existing `[Archive]` with extra keys). If NexTwist writes a canonical "known-good" INI wholesale to enable loose files, it silently deletes the user's other settings — and "restoring" a file NexTwist template-overwrote is meaningless. This is the INI analogue of "overwriting into the real game dir without a ledger" (already a v1.0 anti-pattern).

**Why it happens:**
The community's canonical fix ("just paste this 3-line INI") is a *whole-file* recipe. Copying it into the tool is the path of least resistance and works in the demo (clean prefix) but destroys real users' configs.

**How to avoid:**
Never write a whole file. Parse → mutate only the `[Archive]` keys NexTwist owns (`sResourceDataDirsFinal=`, `bInvalidateOlderFiles=1`) → re-serialize preserving everything else: unrelated sections/keys, comments, key order, and the user's pre-existing values for keys NexTwist did not set. If `[Archive]` already exists, edit in place; if `sResourceDataDirsFinal` already has a non-empty user value, surface it as a conflict, don't silently overwrite. Back up full original bytes first (Pitfall 1) so even the surgical edit is reversible.

**Warning signs:**
- Code builds the INI from a `const TEMPLATE: &str`.
- No INI parser; the code uses `fs::write` with a literal string.
- Tests only ever start from a non-existent or empty INI, never a populated one.

**Phase to address:**
Phase 8 — INI management. Include a fixture INI with unrelated `[Display]`/`[Controls]` keys and assert they survive a deploy→purge cycle unchanged.

---

### Pitfall 3: CRLF / BOM / encoding drift makes the "reversible" edit not byte-for-byte

**What goes wrong:**
Bethesda INIs on Windows are CRLF, no BOM, ASCII/Windows-1252. A user's existing file may be CRLF (Notepad) or carry a UTF-8 BOM. If NexTwist's parser normalizes line endings to `\n`, strips/adds a BOM, or re-encodes, then even a "no semantic change" round-trip produces different bytes than the backup — the byte-for-byte guarantee fails its own hash check, and verify/repair may flag a file it just wrote as corrupt. An INI written LF-only can also be mis-parsed by some Bethesda tooling.

**Why it happens:**
Rust string/line APIs and most INI crates normalize line endings and assume UTF-8. Linux devs default to LF and never see CRLF until a real user's file arrives.

**How to avoid:**
Preserve the original file's line-ending style and encoding on the surgical edit; when NexTwist *creates* the file fresh, write CRLF, no BOM (Bethesda convention). Reversibility must not depend on re-parsing — Pitfall 1's byte-for-byte backup is the source of truth for restore, so restore writes the exact stored bytes, never a re-serialized version. Keep parse/edit purely for the *forward* mutation.

**Warning signs:**
- Backup hash mismatch on a file NexTwist itself round-tripped with no intended change.
- INI crate chosen without checking it round-trips CRLF and preserves unknown bytes.
- verify/repair reports the INI as modified immediately after a clean deploy.

**Phase to address:**
Phase 8 — INI management. Add a CRLF-and-BOM fixture to the round-trip regression test.

---

### Pitfall 4: Non-idempotent INI deploy (duplicate `[Archive]` sections / stacked keys on re-run)

**What goes wrong:**
Deploy can run more than once (re-deploy, profile switch, crash-replay of the journal — the v1.0 engine explicitly replays interrupted ops). A naïve "append `[Archive]\nsResourceDataDirsFinal=\nbInvalidateOlderFiles=1`" produces duplicate `[Archive]` headers or repeated keys on the second run. Bethesda INIs take the *last* value for a duplicated key, so behaviour becomes order-dependent, and the "restore original" backup may capture a NexTwist-mangled version if the backup was taken on the second pass.

**Why it happens:**
The v1.0 journal guarantees safe replay only if the op is **idempotent** (a stated invariant of the crash-safety model). An append-based INI edit is not idempotent, and it's easy to forget the journal will legitimately re-run this op.

**How to avoid:**
Make the mutation a set-key operation (find-or-create section, set-or-replace key), never append. Running it N times must yield an identical file. Take the backup exactly once, gated on the provenance record (Pitfall 1), so replay never re-snapshots a NexTwist-modified file as pristine. Fold the INI edit into the existing intent-before-act journal so replay semantics match file ops.

**Warning signs:**
- A deployed INI shows two `[Archive]` lines.
- The backup is captured inside the same function that mutates, with no "already backed up?" guard.
- No run-twice assertion in the INI op's test matrix.

**Phase to address:**
Phase 8 — INI management, wired into the journal/replay path. Add a run-twice idempotency assertion and a crash-replay test analogous to v1.0's `crash_recovery` suite.

---

### Pitfall 5: Wrong Proton-prefix INI path — folder is inside the Wine prefix and may not exist yet

**What goes wrong:**
`StarfieldCustom.ini` does not live at a Linux `~/Documents` path. It lives **inside the Proton prefix**:
`STEAM/steamapps/compatdata/1716740/pfx/drive_c/users/steamuser/Documents/My Games/Starfield/StarfieldCustom.ini`.
Three traps: (a) the `My Games/Starfield` directory (and often `Documents`) **does not exist until the game has been launched at least once**; (b) case — Wine/Proton on a case-sensitive Linux FS may materialize `My Games` vs `My games`, differing `Documents` casing, etc., and the game folds case, so a hard-coded casing misses the real dir; (c) some prefixes redirect `Documents` (a `user.reg` folder redirect or Steam's Proton default), so it isn't always under `users/steamuser/Documents`.

**Why it happens:**
Developers reuse "put it in Documents/My Games" Windows knowledge and forget the whole path is virtualized inside `compatdata/<appid>/pfx`. The folder-not-created-until-first-launch case never appears in a dev's own already-played prefix. v1.0 already solved the analogous `plugins.txt`/AppData-in-prefix problem — the trap is *not reusing* that solved seam.

**How to avoid:**
Reuse v1.0's Proton-prefix + Wine case-folding resolution (`steam` crate, `casing.rs`) rather than re-deriving the path. Resolve `Documents` by reading the prefix's `user.reg` folder mappings, falling back to the default; case-fold every segment through the existing casing logic. If `My Games/Starfield` is absent, that is the "first-launch not done" state: create it (recording provenance per Pitfall 1) or, better UX, detect and warn "launch Starfield once first." Never create the file at a Linux-side `~/Documents` path — the game will never read it there.

**Warning signs:**
- INI path built from `dirs::document_dir()` or `$HOME` instead of the prefix.
- The path constant contains a literal `steamuser/Documents/My Games` with fixed casing.
- Works on the dev's machine (game already launched) but users report "INI has no effect."

**Phase to address:**
Phase 6 — Starfield detection & CE2 AppData path resolution (owns path/casing/first-launch), consumed by Phase 8 (INI) and Phase 7 (plugins.txt), which share the `My Games/Starfield` directory.

---

### Pitfall 6: Reordering or disabling Starfield's always-active base masters (breaks the game / corrupts saves)

**What goes wrong:**
Starfield hardcodes a set of official plugins to be **implicitly always active**; the game **ignores `plugins.txt` for them** and will not deviate from its internal order. If NexTwist writes them into its managed `plugins.txt`, lets the user disable/reorder them, or treats them as normal user plugins, results range from "no effect" (game strips them) to missing-master errors and save-corruption warnings ("This save relies on content that is no longer present").

Protected, always-active base masters (current Starfield):
- `Starfield.esm`
- `Constellation.esm`
- `OldMars.esm`
- `BlueprintShips-Starfield.esm`
- `SFBGS003.esm`, `SFBGS006.esm`, `SFBGS007.esm`, `SFBGS008.esm` (the SFBGS*.esm update/patch masters — the exact set **grows with game updates**; `SFBGS004.esm` also appears via `Starfield.ccc`)

Extra subtlety (Ortham/libloot): **`BlueprintShips-*` plugins are special** — Starfield *removes all `BlueprintShips-` plugins from `plugins.txt` on startup*, and a `BlueprintShips-X.esm` auto-activates when `X` is active. A tool must never write them to load-order files or order them by hand.

**Why it happens:**
These files look like ordinary ESMs. A load-order tool naturally enumerates all plugins uniformly and offers enable/disable + drag-to-reorder. The "hardcoded, plugins.txt-ignored" behaviour is invisible unless you know CE2 specifics.

**How to avoid:**
Maintain an explicit **protected/implicitly-active set**; never write those to the managed `plugins.txt`, never expose enable/disable or reorder for them (grey them out / lock, as MO2 did after its bug #2051), never count them against user-facing limits. Delegate the determination to **libloot** — it already knows Starfield's implicitly-active list and BlueprintShips rules — rather than hand-maintaining the list (the SFBGS set changes per update, so a hard-coded array rots). Treat the SFBGS list as data from libloot's Starfield game handle/masterlist, not a constant.

**Warning signs:**
- Managed `plugins.txt` contains any `*Starfield.esm` / `*SFBGS0**.esm` / `*BlueprintShips-*` line.
- UI lets the user uncheck `Starfield.esm`.
- Load-order code enumerates plugins with a plain directory scan instead of libloot's game handle.

**Phase to address:**
Phase 7 — Starfield plugin load order. Verify: a fixture plugin dir round-trips through sort/write with no protected master appearing in `plugins.txt`.

---

### Pitfall 7: Mishandling the new medium-master tier and CE2 form-count / index limits

**What goes wrong:**
Starfield introduces a **medium master** tier (new vs Skyrim/FO4's full+ESL two-tier model): full masters (≤253 index slots), **medium masters** (up to 256, ≤65,535 forms each, FormIDs `FDxxyyyy`), and small/light masters (ESL, up to 4,096, ≤4,095 forms). A tool ported from FO4/Skyrim two-tier assumptions misclassifies plugins, counts against the wrong limit, and presents a wrong mod-limit picture. Overrides also consume extra index slots, so naive counting is wrong.

**Why it happens:**
The v1.0 plugin layer targeted Skyrim SE / FO4, which have no medium tier. Reusing that classification/counting logic silently for Starfield produces subtly wrong load-order and limit reporting.

**How to avoid:**
Don't hand-classify plugin types or count limits — let libloot/esplugin (already in the stack) report plugin type and validity per its Starfield support, which understands the medium tier and the `FDxxyyyy` scheme. If NexTwist surfaces a "mod limit," derive it from libloot's per-type counts, and treat any hard-coded limit constant as game-specific data, not shared across Bethesda games.

**Warning signs:**
- Plugin-type enum has only `{Full, Light}` with no `Medium`.
- Limit constants (`253`, `4096`) shared across all Bethesda games.
- Load-order view mislabels a medium master as full/light.

**Phase to address:**
Phase 7 — Starfield plugin load order. Verify against a fixture with one plugin of each tier.

---

### Pitfall 8: Loose-file loading silently regresses after a Bethesda game update

**What goes wrong:**
Starfield's history includes updates that changed loose-file / archive-invalidation behaviour — e.g. the Vortex Starfield extension **removed** its loose-file feature (~ext v0.6.7) because a bug introduced around game version ~1.10.31.0 changed how the `sResourceDataDirsFinal` mechanism worked; community "enable loose files" recipes have shifted across patches. Risk: NexTwist writes the "correct" INI, it works today, then a Starfield update lands and mods silently stop loading in-game while NexTwist still reports "deployed OK." The safety guarantee isn't violated (nothing corrupted), but the feature quietly breaks and NexTwist gets blamed.

**Why it happens:**
The INI recipe is treated as a permanent constant. Bethesda ships updates that touch the resource loader; a live-service Creation Engine game is a moving target. There's no in-tool signal linking "deployed" to "actually loaded in-game."

**How to avoid:**
(a) Keep the INI keys NexTwist writes in one place, easy to update, with version-notes on which game builds were validated. (b) Provide a lightweight **in-game verification affordance** — deploy a tiny sentinel loose file and give the user a documented one-step check so "deployed" can be confirmed as "loaded." (c) Detect the installed Starfield build and warn when it's newer than the last NexTwist-validated build ("Starfield updated; loose-file loading may need re-verification"). (d) Watch reference implementations (Nexus `game-starfield` Vortex extension, modding.wiki loose-file page) as the canary for recipe changes.

**Warning signs:**
- Users report "mods installed but nothing changes in-game" clustered right after a Starfield patch.
- No record of which game build the INI recipe was validated against.
- No end-to-end "is it actually loaded" check — only "files are on disk."

**Phase to address:**
Phase 9 — On-hardware verification (owns the deployed-vs-loaded gap); the version-drift warning belongs to Phase 6 (detection).

---

### Pitfall 9: Staleness / availability of libloot's Starfield masterlist and game-handle behaviour

**What goes wrong:**
libloot's Starfield support has evolved rapidly (LOOT/libloot changelogs across 0.23–0.26 changed `Starfield.ccc` handling — e.g. `SFBGS004.esm` added to the implicit set, and LOOT *stopped* writing `My Games\Starfield\Starfield.ccc`). **Verified 2026-07-07:** the pinned `libloot 0.29.5` already supports Starfield — `GameType::Starfield` exists in the crate's public API, and 0.29.5 is on the same 0.29.x line as the current max-stable 0.29.6, so **no dependency bump or MSRV change is needed to add Starfield** (crates.io registry + docs.rs cross-check). The residual risk is therefore *not* "does libloot support Starfield" (it does) but **masterlist currency**: if NexTwist ships a stale/absent Starfield masterlist, sorting can produce wrong order, miss newly-implicit masters, or fight the game over `Starfield.ccc`. Also: `Starfield.ccc` is loaded from `My Games\Starfield\Starfield.ccc` if present, else from the install dir — writing one into the prefix is another *reversible* write target with the same absence/restore concerns as the INI.

**Why it happens:**
Bethesda's update masters (SFBGS*) and CE2 quirks are a moving target that libloot tracks via masterlist + code; a bundled masterlist drifts out of date. Teams also forget the masterlist is fetched/updateable content, not compiled-in.

**How to avoid:**
Treat the masterlist as updateable data (fetch/refresh path, bundled fallback, "masterlist age" indicator) rather than assuming the compiled-in libloot is enough. **Do not** have NexTwist author/manage `Starfield.ccc` in the prefix unless deliberately needed — current libloot no longer writes it, and letting the game own it avoids an extra reversible write target. If NexTwist ever does write `.ccc`, route it through the same provenance/journal/restore-absence machinery as the INI (Pitfall 1).

**Warning signs:**
- Sort results disagree with LOOT desktop on the same load order.
- Newly-released SFBGS masters aren't recognized as implicit.
- A bundled masterlist file with a months-old date and no refresh path.

**Phase to address:**
Phase 7 — load order (masterlist currency; libloot version already confirmed sufficient); the `.ccc`-as-write-target decision is a Phase 8 concern if pursued.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Whole-file INI template written with `fs::write` | Fastest to ship; matches community copy-paste recipe | Destroys users' existing INI settings; not reversibly restorable; breaks core guarantee | **Never** for the real path; only in a throwaway spike |
| Hard-coded `SFBGS*.esm` protected-master array | No libloot round-trip to gate the UI | Rots on every Bethesda update that adds a master; wrong enable/disable gating | Only as a belt-and-suspenders *fallback* behind libloot's list, never the source of truth |
| Backup-only reversibility (no "created-by-us" provenance) | Reuses v1.0 vanilla-backup ledger unchanged | Purge can't restore ABSENCE; leaves orphan INI/dirs | Never — the headline pitfall for this milestone |
| Hard-coded prefix INI path with fixed casing | Works on the dev's already-launched prefix | Fails on case-sensitive FS, redirected Documents, first-launch-not-done | Never — reuse v1.0 casing/prefix seam |
| Ship a bundled Starfield masterlist with no refresh path | No network code needed | Drifts stale as Bethesda adds SFBGS masters; wrong sorts | MVP-only; add refresh/age indicator before wide release |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Proton prefix (`compatdata/1716740/pfx`) | Using a Linux-side `~/Documents/My Games` path | Resolve inside the prefix via v1.0's steam/casing seam; read `user.reg` for Documents redirection; handle folder-absent (first launch) |
| Wine case-folding | Hard-coded `My Games`/`Documents`/`steamuser` casing | Case-fold every segment through `casing.rs`; the on-disk casing Wine materializes isn't guaranteed |
| libloot (Starfield) | Hand-enumerating plugins and classifying types | Use libloot's `GameType::Starfield` game handle (present in pinned 0.29.5) for type, validity, implicit-active set, and BlueprintShips rules |
| `plugins.txt` (asterisk format) | Writing protected/implicit masters or `BlueprintShips-*` into it | Never write implicit masters; the game strips `BlueprintShips-*` on startup regardless |
| `Starfield.ccc` | Authoring/writing it into the prefix | Let the game own it; current libloot stopped writing it. If written, journal it as a reversible target |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Trusting a mod-supplied `StarfieldCustom.ini` and merging it wholesale | A mod could inject arbitrary INI keys (paths, loader settings) into the user's prefix | Only ever set the two keys NexTwist owns; treat any mod-provided INI as data to inspect, not apply verbatim |
| Following an INI-relative path outside `My Games/Starfield` when creating parents | Write-through outside the intended prefix location | Bound INI writes to the resolved prefix Starfield dir; reuse the spirit of v1.0's zip-slip/symlink-write-through defenses |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Enabling loose files silently, no confirmation | User's existing INI changed without consent; distrust | Show the exact `[Archive]` change; note it's reversible; record provenance |
| Reporting "deployed" with no "is it loaded in-game" signal | User thinks mods work; they don't (post-update regression) | Provide an in-game verification step (Pitfall 8) |
| Showing protected base masters as toggleable | User disables `Starfield.esm`, corrupts save, blames tool | Lock/grey implicit masters (MO2 issue #2051 pattern) |
| Silent no-op when `My Games/Starfield` doesn't exist | INI written nowhere useful; mods don't load | Detect first-launch-not-done and guide "launch the game once" |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Re-hashing the whole game/staging tree to verify INI reversibility | Slow verify on huge Bethesda load orders | Hash only the INI (and journaled targets), not a blind disk scan — reuse v1.0's manifest/journal-bounded verify | Large texture-pack load orders (tens of GB) |
| Re-running libloot full sort on every UI interaction | UI lag with many plugins | Sort on demand / cache; libloot sort is not free | Hundreds of active plugins |

## "Looks Done But Isn't" Checklist

- [ ] **Reversible INI (absence):** Purge tested from the *absent-INI* start state — asserts the file and any NexTwist-created parent dirs are gone, not blanked.
- [ ] **Reversible INI (restore):** Purge tested from a *pre-existing populated INI* — asserts unrelated `[Display]`/`[Controls]` keys and original bytes (incl. CRLF/BOM) restored exactly.
- [ ] **Idempotency:** INI deploy run twice (and crash-replayed) yields one `[Archive]` section, identical bytes.
- [ ] **Path:** INI resolves inside the Proton prefix, case-folded, Documents-redirection handled — verified on a real prefix, not just the dev's.
- [ ] **Load order:** No protected/implicit master or `BlueprintShips-*` ever appears in the managed `plugins.txt`.
- [ ] **Load order:** Medium-master tier classified correctly (not full/light).
- [ ] **libloot:** Sort matches LOOT desktop on the same inputs; masterlist currency indicated.
- [ ] **On hardware:** A real loose-file mod is confirmed *visible in-game*, not merely "on disk" (PROJECT.md's stated milestone bar).

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Orphan INI left after purge (P1) | LOW | Add provenance branch + restore-absence; ship a one-time cleanup that removes a NexTwist-signature empty INI |
| Clobbered user INI (P2) | HIGH (user data lost) | Only recoverable if a byte-for-byte backup was taken; otherwise unrecoverable — prevention is the only defense |
| Protected master disabled/reordered (P6) | MEDIUM | Rebuild `plugins.txt` from libloot's implicit set; user may need a save-integrity check |
| Loose files stopped loading post-update (P8) | LOW–MEDIUM | Detect game version drift; refresh the INI recipe; re-run in-game verification |
| Stale masterlist wrong sort (P9) | LOW | Refresh masterlist; re-sort (no libloot bump needed — 0.29.5 already supports Starfield) |

## Testing Traps (fixtures vs on-hardware)

**Unit/integration without a live Starfield install — use fixture prefixes (extend v1.0 `testkit`):**
- Build a fake Proton prefix tree: `compatdata/1716740/pfx/drive_c/users/steamuser/Documents/My Games/Starfield/` with variants — INI absent, INI present-and-populated (CRLF + unrelated sections), INI present with an existing `[Archive]`, and a case-mismatched (`My games`) variant.
- Build a fake Starfield Data dir with plugins of each tier (full/medium/small master) plus the protected base masters and a `BlueprintShips-X.esm`, to drive libloot classification and the "never write protected masters" assertion.
- Reuse the `DIR_SENTINEL` blake3 pristine-tree assertion to lock byte-for-byte reversibility across both INI provenance branches (absent→gone, present→restored) and empty-dir cleanup.
- Add an idempotency test (op run twice) and a crash-replay test (mirror the `crash_recovery` suite) for the INI op through the journal.
- All headless — no webview, no live game — consistent with the `crates/*` zero-Tauri-dep boundary.

**What must be checked on real hardware (owner has Starfield on Proton — PROJECT.md):**
- The in-prefix INI path actually resolves on a genuine Proton prefix (Documents redirection, real Wine casing).
- The `My Games/Starfield` folder-lifecycle: behaviour before vs after the game's first launch.
- Enabling loose files via the written INI makes a real loose-file mod **visible in-game** (the milestone's explicit bar), not merely present on disk.
- Sort output agrees with LOOT desktop and the game doesn't strip/re-order what NexTwist wrote.
- Post-update sanity: after a Starfield patch, re-confirm loose files still load (the regression canary).

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| 1. Purge must restore INI absence | Phase 8 (INI) | Deploy→purge from absent start asserts file+dirs gone (extend `DIR_SENTINEL`) |
| 2. Don't clobber existing user INI | Phase 8 (INI) | Populated-INI fixture: unrelated keys survive round-trip |
| 3. CRLF/BOM byte-for-byte | Phase 8 (INI) | CRLF+BOM fixture round-trips with matching backup hash |
| 4. Idempotent INI deploy | Phase 8 (INI) + journal/replay | Run-twice + crash-replay assert single `[Archive]`, identical bytes |
| 5. Proton-prefix path/casing/first-launch | Phase 6 (detection/paths) | Resolves correct in-prefix path on a real + case-sensitive fixture prefix |
| 6. Never touch protected base masters | Phase 7 (load order) | No implicit master/`BlueprintShips-*` in managed `plugins.txt` |
| 7. Medium-master tier + limits | Phase 7 (load order) | Each-tier fixture classified correctly via libloot |
| 8. Post-update loose-file regression | Phase 9 (HW verify) + Phase 6 (version drift) | Real mod visible in-game; version-drift warning fires on newer build |
| 9. Masterlist staleness; `.ccc` | Phase 7 (load order) | Sort matches LOOT desktop; masterlist age surfaced |

## Sources

- Ortham (LOOT/libloot author) — "Load order in Starfield" (2024-06-28), "part 2: blueprint plugins" (2024-07-30), "October 2024 edition" (2024-10-12), "BlueprintShips plugins" (2026-05-04). Implicit-active masters, medium-master tier, `FDxxyyyy` scheme, `Starfield.ccc` semantics, BlueprintShips stripping/auto-activation. **HIGH** (authoritative — the load-order library's author). https://blog.ortham.net/
- LOOT docs — "Changing plugin types in Starfield"; Version History 0.24.0/0.26.0 (`Starfield.ccc` handling, `SFBGS004.esm`, stopped writing `My Games\Starfield\Starfield.ccc`). **HIGH**. https://loot.github.io / loot.readthedocs.io
- crates.io registry + docs.rs — `libloot` max-stable 0.29.6 (pinned 0.29.5 same 0.29.x line); `GameType::Starfield` present in the public API → Starfield supported with zero dependency/MSRV change. **HIGH** (direct registry + generated-docs verification, 2026-07-07). https://crates.io/crates/libloot / https://docs.rs/libloot
- Nexus Mods — "Base StarfieldCustom.ini to Enable Loose File Mods" (mods/273), "Best Practices with Loose Files" (articles/578), "Howto: Archive Invalidation" (articles/116). INI `[Archive]`/`sResourceDataDirsFinal=`/`bInvalidateOlderFiles=1`, `.txt` extension trap, file location. **HIGH** (universally corroborated recipe).
- modding.wiki — "Loose File Modding" (Starfield). INI settings + fixes. **MEDIUM–HIGH**.
- AFK Mods — "Starfield ESM Modules — What are they?"; Unofficial Starfield Patch readme (master list Starfield/Constellation/OldMars/SFBGS00x/BlueprintShips). **MEDIUM** (community, cross-checked with Ortham).
- ModOrganizer2 issue #2051 — basegame ESMs disableable when they should be greyed out (protected-master UI-locking pattern). **MEDIUM**.
- Nexus-Mods/game-starfield (Vortex extension) + Nexus news 14883 — loose-file feature removed (~ext v0.6.7) after game ~1.10.31.0 broke it; regression history. **MEDIUM**.
- Bethesda Support a_id 62179 — "save relies on content no longer present" (consequence of disabled/missing masters). **MEDIUM**.
- NexTwist `the project brief` + `CLAUDE.md` — v1.0 crash-safety journal, vanilla-backup ledger, `DIR_SENTINEL` pristine assertion, casing/steam prefix seam, libloot 0.29.5, milestone scope/phase numbering. **HIGH** (project-internal).

---
*Pitfalls research for: Starfield (CE2) support + reversible StarfieldCustom.ini editing on Linux/Proton*
*Researched: 2026-07-07*
