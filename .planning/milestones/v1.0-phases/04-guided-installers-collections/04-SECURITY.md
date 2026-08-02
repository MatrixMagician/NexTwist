---
phase: 04-guided-installers-collections
status: secured
asvs_level: 2
block_on: high
threats_total: 17
threats_closed: 17
threats_open: 0
audited: 2026-06-23
mode: verify-mitigations (register authored at plan time)
---

# SECURITY — Phase 04: Guided Installers & Collections

Retroactive threat-mitigation verification for the FOMOD scripted-installer engine
(`crates/fomod`), the NexusMods Collections browse/resolve/download/deploy/uninstall
lifecycle (`crates/nexus` + `src-tauri/src/commands/{fomod,collections}.rs`), and the
shared validated-extract/deploy primitives they reuse (`crates/extract`,
`crates/deploy`). Each declared mitigation in the phase threat register was verified to
**exist in the implemented code** with file:line evidence — documentation and intent
were not accepted as proof. Implementation files were not modified.

All 17 threats resolve to **CLOSED** (15 mitigate + 1 accept + 1 supply-chain). No open
blockers. No unregistered flags.

## Verification Result: SECURED

| Threat ID | Category | Disposition | Status | Evidence (file:line) |
|-----------|----------|-------------|--------|----------------------|
| T-04-01 | Tampering (FOMOD path traversal) | mitigate | CLOSED | `resolve` emits relative `dest_rel`/`src` only and never writes (`crates/fomod/src/resolve.rs:125-191`, `file_install` L224-239). `resolve_source_path` resolves component-by-component against real `read_dir` entries case-insensitively, dropping `.` and never matching `..` (which `read_dir` never yields) → `FomodError::MissingSource` on any escape (`crates/fomod/src/parse.rs:66-89`). Destination containment re-asserted at deploy by `guard_within_root` (`crates/deploy/src/path_guard.rs:16-24`, called `engine.rs:354`, `conflict.rs:121`). |
| T-04-02 | DoS (XML billion-laughs / entity expansion) | mitigate | CLOSED | `quick-xml = { version = "0.40", features = ["serialize"] }` (`Cargo.toml:88`) — only the `serialize` feature; no DTD/external-entity expansion is enabled. Parse failures map to `FomodError::Xml(String)` and return, never hang (`crates/fomod/src/parse.rs:28`, error `crates/fomod/src/error.rs:43-44`). Config-sized input. |
| T-04-03 | Tampering (malformed XML silently mis-installing) | mitigate | CLOSED | Unsupported construct (plugin with no `<typeDescriptor>`) returns `FomodError::MalformedSchema` (`crates/fomod/src/resolve.rs:161-165`); any broken/non-FOMOD doc returns `FomodError::Xml` (`parse.rs:28`). `resolve` is pure (module docs `resolve.rs:1-7`); the plan is surfaced before any write. No silent-OK path. |
| T-04-04 | Tampering (wrapper-folder mis-detection) | mitigate | CLOSED | `detect_archive_root` uses the explicit small token list `RECOGNIZED_ROOT_ITEMS` (`crates/extract/src/staging.rs:116`); a multi-entry top level is never flattened (`staging.rs:163-166`), an already-`Data/`-rooted tree is kept whole (L170-172), and an unwrapped wrapper stages ONLY its recognized-root children, dropping non-game siblings (the 2fa9821 fix; `WrapperChildren` L131, `recognized_root_children` L209-220). Unit-tested: `multi_folder_mod_is_never_flattened` (L410), `wrapper_non_game_siblings_are_excluded` (L370), `nested_wrapper_unwraps_only_one_level` (L434), `loose_file_mod_is_kept_verbatim` (L422). |
| T-04-05 | Tampering (apply writes outside Data/) | mitigate | CLOSED | `apply_fomod` re-resolves + rejects a blocking selection before any write (`src-tauri/src/commands/fomod.rs:311-317`), then routes every byte through `extract::install_archive` (`fomod.rs:323`) — same zip-slip/symlink/`..` defenses (`crates/extract/src/validate.rs:94-160`) and root-detection unchanged. Adapter adds no write primitive; the staging subdir name is sanitised (`fomod.rs:511-522`, tested L666). Deploy-time containment by `guard_within_root` (`deploy/src/conflict.rs:121`, `engine.rs:354`). |
| T-04-06 | EoP (business logic leaking into adapter) | mitigate | CLOSED | `commands/fomod.rs` commands only `require_game` + `extract_to_temp` + call headless `parse_module_config`/`validate_selection`/`resolve`/`install_archive`/`store.add_mod`, mapping errors via `boundary_err` (`fomod.rs:240-344`). Projection/classification helpers are pure mappers (L392-506). No FOMOD logic inline. |
| T-04-07 | InfoDisclosure (silent mis-install of malformed FOMOD) | mitigate | CLOSED | A malformed `ModuleConfig.xml` returns the verbatim `FomodError` string via `boundary_err` so the UI offers the plain-mod fallback (`fomod.rs:250-252`, test `malformed_fixture_returns_err_string_for_fallback` L606-619). A blocking selection is rejected server-side in `apply_fomod` (`fomod.rs:313-317`). No silent install path. |
| T-04-08 | Tampering/SSRF (off-Nexus auto-fetch in resolve) | mitigate | CLOSED | `resolve_collection` classifies `Direct`/`Browse`/`Manual` as `ModStatus::Manual` from `source.kind` ALONE and issues NO request for them (`crates/nexus/src/resolve.rs:97-101`); the only network call in the resolver is `client.file_availability` (a single metadata read, L104), so the off-Nexus `url` is never contacted. `is_off_nexus` (`crates/nexus/src/collection.rs:160-163`). |
| T-04-09 | Spoofing (stale/spoofed modRule reference) | mitigate | CLOSED | Rules only adjust rank within the user's own collection: `after` ⇒ `rank_delta += 1`, `before` ⇒ `-= 1` (`crates/nexus/src/replay.rs:169-185`). A rule whose `source` or `reference` resolves to no mod is SKIPPED, never fatal (`replay.rs:163-167`; `reference_to_index` returns `None` L216-238). Tested: `rule_referencing_an_unresolved_mod_is_skipped_not_fatal` (L434), `compute_ranks_skips_rule_with_unresolved_endpoint` (L552). |
| T-04-10 | InfoDisclosure (resolve gate bypass / download before report accepted) | mitigate | CLOSED | `resolve_collection` has NO download path — it only calls `file_availability` (metadata) per nexus mod (`crates/nexus/src/resolve.rs:87-124`, module contract L4-8). The download CTA is a separate command (`download_collection`) gated behind the accepted report. `file_availability` is computed from metadata only, before any download (`crates/nexus/src/client.rs:279-286`). |
| T-04-11 | DoS (unbounded collection.json) | accept | CLOSED | `Collection::parse` uses `serde_json::from_str` (allocation-bounded) and flattens any error to `NexusError::Http` — no panic on untrusted input (`crates/nexus/src/collection.rs:274-277`, tests `malformed_manifest_errors_not_panics` L351). Per-mod metadata reads are rate-limited through the shared `governor` limiter (`resolve.rs:6-8`; client `with_limiter` `commands/collections.rs:442`). Config-sized input. Accepted-risk entry recorded below. |
| T-04-12 | Tampering/SSRF (off-Nexus auto-fetch in download) | mitigate | CLOSED | The download loop partitions on `m.source.kind`: ONLY `SourceType::Nexus` with both `(mod_id, file_id)` enters `fetchable` (`src-tauri/src/commands/collections.rs:185-197`); `Direct`/`Browse`/`Manual` → `manual_steps`, never fetched (L199-208); `Bundle` is in-archive, no fetch (L211). `is_auto_fetchable` excludes all off-Nexus (`crates/nexus/src/replay.rs:206-208`). Tested: `off_nexus_sources_are_never_auto_fetchable` (`collections.rs:540`). |
| T-04-13 | Tampering (downloaded archive zip-slip/symlink) | mitigate | CLOSED | Each downloaded mod routes through `run_download_to_window` → `extract::install_archive` (`commands/collections.rs:225-231`) — the SAME per-entry validator: symlink entries rejected first (`crates/extract/src/validate.rs:99-102`), absolute/`..`/root components rejected (`validate.rs:138-160`), re-canonicalised-parent-under-root containment (L119-129), plus Plan-01 root-detection. No new file primitive. |
| T-04-14 | Tampering (partial-deploy / non-pristine uninstall) | mitigate | CLOSED | Deploy via `deploy::switch_profile` (`commands/collections.rs:333`) — the journaled purge→deploy_winners→load-order→set-active path (`crates/deploy/src/profile.rs:78`). Uninstall via `deploy::purge` (`collections.rs:370`) — manifest-driven, intent-journaled (`begin_purge`/`finish_purge` `crates/deploy/src/engine.rs:401-426`). `collection_round_trip.rs:257` asserts `assert_trees_identical(&pristine, &after)` byte-for-byte after uninstall (`purged.orphans.is_empty()` L232). |
| T-04-15 | Spoofing (pinned modId/fileId attacker-swapped file, hash mismatch) | mitigate | CLOSED | Manifest carries per-file `md5` integrity hint, parsed and persisted verbatim (`crates/nexus/src/collection.rs:116-117`; persisted `commands/collections.rs:507`). User accepts the resolve report before any download (T-04-10). Download routes through the rustls-only client with `error_for_status()` (`crates/nexus/src/download.rs:81`, `client.rs:74`). Archived/unavailable files are surfaced as `ModStatus::Archived`/`Unavailable`, never silently substituted (`resolve.rs:105-111`); identity is resolved honestly (no `0/0` sentinel for a real nexus pin, `collections.rs:489-501`). |
| T-04-16 | EoP (premium gate bypass) | mitigate | CLOSED | `download_collection` reads `UserInfo.is_premium` and calls `premium_gate(is_premium)` BEFORE the fetchable partition or any download begins (`src-tauri/src/commands/collections.rs:152-157`); a non-Premium session returns the Premium-required notice and starts nothing — no `nxm://` fallback (`premium_gate` L415-423). Tested: `premium_gate_blocks_free_account` (L528). |
| T-04-SC | Tampering (package installs) | mitigate | CLOSED | The two new deps this phase consumes — `quick-xml = "0.40", features=["serialize"]` (`Cargo.toml:88`) and `futures-util = "0.3"` (`Cargo.toml:73`) — are both pinned in `[workspace.dependencies]` and CONTEXT-audited; `serde_json` was already a nexus dep. `unrar`/`unrar_sys` remain banned (`deny.toml:23,26`) and are ABSENT from `Cargo.lock` (count 0). `cargo deny check advisories bans licenses sources` green per all four plan SUMMARYs. |

## Accepted Risks Log

- **T-04-11 (unbounded `collection.json` DoS) — ACCEPTED.** Verified valid: the manifest
  is parsed by `serde_json::from_str` (allocation-bounded, no recursion-bomb XML-style
  expansion) and any malformed input flattens to `NexusError::Http` rather than panicking
  (`crates/nexus/src/collection.rs:274-277`). Per-mod metadata reads during resolve are
  rate-limited through the single process-wide `governor` token bucket (the client is
  built `with_limiter` from the shared `AppState.rate_limiter`, `commands/collections.rs:428-442`).
  The manifest is config-sized. Accept rationale holds.

## Scope / Location Note (informational — not a gap)

The threat register, authored at plan time, cited some mitigations against earlier-assumed
file locations. The implementations exist and are verified, just relocated to dedicated
crates that did not exist when the register was written:

- The FOMOD parser/resolver cited as `extract::fomod` lives in the dedicated
  **`crates/fomod`** crate (`parse.rs`, `resolve.rs`, `condition.rs`, `error.rs`). The
  shared validated extractor it reuses (`install_archive`, `detect_archive_root`,
  `validate_entry`) is in **`crates/extract`** as cited.
- The Collection SSRF/resolve/rules/premium logic cited as `crates/store/src/collections.rs`
  lives in **`crates/nexus`** (`collection.rs` parser, `resolve.rs` resolve-before-download
  gate, `replay.rs` rule→rank + off-Nexus partitioning) and the thin
  **`src-tauri/src/commands/collections.rs`** adapter (premium gate, download loop, deploy,
  uninstall). `crates/store` holds the V5 persistence (`migrations/V5__collections.sql`,
  `store/src/collections.rs` facade), which carries no SSRF/fetch logic.

Every cited mitigation was located and verified in its actual implementation site; the
relocation does not weaken or omit any control.

## Unregistered Flags

None. Plans 04-01 / 04-02 carry a `## Threat Mitigations Applied` section and plans
04-03 / 04-04 carry a `## Threat Model Adherence` section; each maps every introduced
mitigation to a registered T-04-* ID. No `## Threat Flags` section is present and no new
attack surface appeared during implementation that lacks a threat mapping.

## Live Verification Performed

- `grep` of `crates/nexus/src/*.rs` confirms the HTTP surface is rustls-only with
  `error_for_status()` and redirects disabled (`auth.rs:115,142`, `client.rs:74`,
  `download.rs:81`, `lib.rs:13-14`) — underpinning T-04-13/T-04-15.
- `quick-xml` is declared with only `features = ["serialize"]` (`Cargo.toml:88`); no DTD
  feature is enabled — confirms T-04-02 at the dependency-feature level.
- `unrar`/`unrar_sys` count in `Cargo.lock` = 0; `deny.toml` ban intact — confirms the
  supply-chain posture (T-04-SC) holds on the current graph.
- `crates/deploy/tests/collection_round_trip.rs` present and asserting byte-for-byte
  pristine (`assert_trees_identical`, L257) — confirms T-04-14.

---

*Phase: 04-guided-installers-collections · Audited: 2026-06-23 · mode: verify-mitigations*
