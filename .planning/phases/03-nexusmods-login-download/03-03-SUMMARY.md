---
phase: 03-nexusmods-login-download
plan: 03
subsystem: nxm-handoff
tags: [nexusmods, nxm, deep-link, single-instance, tauri-plugin, oauth-callback, free-user, security-parser, svelte]

# Dependency graph
requires:
  - phase: 03-nexusmods-login-download
    plan: 01
    provides: "headless crates/nexus spine (NexusError Redeem/Auth), shell auth::complete_oauth (CSRF-validated OAuth code-exchange), AppState pending_oauth/oauth_client_id/access_token, OAUTH_REDIRECT, declared (un-wired) deep-link/single-instance plugins"
  - phase: 03-nexusmods-login-download
    plan: 02
    provides: "run_download flow (stream->extract::install_archive verbatim->add_mod->add_nexus_source), download://progress window.emit, downloads-list UI + expired-link Warning + free-user hint scaffolding"
provides:
  - "nexus::NxmLink DTO {game_domain, mod_id:u64, file_id:u64, key/expires/user_id: Option<String>} + NxmLinkKind enum (OAuthCallback{code,state} | Download(NxmLink))"
  - "nexus::NxmLink::parse — strict security-boundary parser (scheme=nxm, /mods/<id>/files/<id> u64 ids, oauth-callback discrimination, opaque percent-decoded key/expires; dependency-free; never panics/shells-out/logs-secrets)"
  - "tauri.conf.json deep-link desktop scheme [nxm] + capabilities/default.json deep-link:default + core:event:default"
  - "lib.rs single-instance(FIRST)+deep-link plugin registration + register_all() + on_open_url routing (cfg linux/windows)"
  - "commands::nexus::handle_nxm_url — thin shell router: NxmLink::parse -> complete_oauth (closes Plan-01 loop) | run_download_to_window (free-user key+expires redemption, NEXUS-04)"
  - "commands::downloads::run_download_to_window — shared download core reused by both the IPC start_download command AND the nxm:// router (single download path, no fork)"
  - "secret-free nxm://arrival + nxm://expired events; frontend arrival toast + Premium hint + expired Warning"
affects: [05-packaging-appimage, download-flow]

# Tech tracking
tech-stack:
  added:
    - "tauri-plugin-deep-link 2.4 + tauri-plugin-single-instance 2.4 (deep-link feature) — declared in Plan 01, WIRED INTO THE BUILDER here (no new crate added; both cargo-deny-approved)"
  patterns:
    - "Strict headless untrusted-input parser as a security boundary: dependency-free hand-split (no url/serde_urlencoded dep — consistent with Plan-02's minimal-reqwest decision), exact-shape validation, typed Err per arm (Redeem for download, Auth for oauth-callback), never panics"
    - "single-instance-FIRST then deep-link plugin order (load-bearing on Linux); register_all() in setup() for dev/installed-runtime; AppImage .desktop MIME deferred to Phase 5"
    - "Thin on_open_url router: ALL parsing in the headless crate, ALL download logic in the shared Plan-02 core, ALL OAuth in Plan-01 complete_oauth — the shell closure only forwards"
    - "Shared run_download_to_window core so the free-user nxm:// redemption reuses the EXACT Plan-02 stream->extract->stage path (no parallel download flow)"
    - "Secret-free deep-link telemetry: url/key/expires/code never logged or placed in an emit payload (NxmArrival=id only, NxmExpired=reason only)"

key-files:
  created:
    - crates/nexus/tests/nxm_parse.rs
  modified:
    - crates/nexus/src/model.rs
    - crates/nexus/src/lib.rs
    - src-tauri/tauri.conf.json
    - src-tauri/capabilities/default.json
    - src-tauri/src/lib.rs
    - src-tauri/src/commands/nexus.rs
    - src-tauri/src/commands/downloads.rs
    - frontend/src/lib/api.ts
    - frontend/src/routes/+page.svelte

key-decisions:
  - "nxm:// parser is a dependency-free hand-split (not the `url` crate). `url` 2.5.8 is only transitively present; adding it as a direct dep of crates/nexus would re-add the dependency Plan-02 deliberately avoided to keep reqwest minimal + cargo-deny clean. The plan's action explicitly permits hand-splitting; for a fixed, well-known scheme grammar a strict hand-split is safer (no custom-scheme authority quirks) and adds zero supply-chain surface."
  - "domain->appid mapping (skyrimspecialedition->489830, fallout4->377160) lives in the SHELL (commands/nexus.rs), not the headless crate: it is a registry/allow-list concern, mirrors the frontend SUPPORTED list, and an unknown domain is rejected (Warning) rather than guessed. core::Game carries no nexus_domain field, so the link's own host is the domain source for download_link/metadata."
  - "Extracted run_download_to_window as the shared download core; start_download (IPC) and the nxm:// router both delegate to it. This guarantees the free-user redemption is the SAME flow as the in-app path — the key+expires are threaded straight through, never re-implemented."
  - "on_open_url runs the work via tauri::async_runtime::spawn so the synchronous callback returns immediately; the arrival toast is emitted synchronously (before the spawn) so the UI confirms instantly, then the row streams via the existing download://progress events."
  - "A malformed/expired/unredeemable link emits nxm://expired (UI Warning), never a stuck Failed row (UI-SPEC §C.3). The parser Err is NOT logged (it could echo link content — V7)."

patterns-established:
  - "The deep-link + single-instance wiring is the OS-integration spine Phase 5's AppImage MIME registration finalizes; register_all() proves dev/installed-runtime now."
  - "Untrusted-URL parsing as an explicit, unit-tested security boundary in the headless crate; the shell never sees a raw segment, only a validated NxmLinkKind."

requirements-completed: []
requirements-deferred-live-uat: [NEXUS-04, NXM-01]

# Metrics
duration: ~9min
completed: 2026-06-21
status: complete
---

# Phase 3 Plan 03: nxm:// Free-User Handoff + Deep-Link Wiring Summary

**A user clicks "Mod Manager Download" on nexusmods.com; the OS routes the keyed `nxm://` link to the already-running NexTwist (a second invocation is forwarded to the live instance, never a duplicate window); a strict headless parser validates the untrusted URL into `{game_domain, mod_id, file_id, key?, expires?}`, discriminating the `nxm://oauth/callback` variant (→ the Plan-01 CSRF-validated code-exchange) from a download (→ the EXACT Plan-02 stream→extract→stage path, free-user `key`+`expires` threaded straight through); and a non-blocking "Download started from NexusMods" toast plus a new downloads-list row confirm the handoff — with an expired/malformed link surfacing a Warning instead of a broken row.**

## Performance
- **Duration:** ~9 min
- **Completed:** 2026-06-21
- **Tasks:** 3 of 4 (Task 4 is the live free-user nxm:// human-verify checkpoint — deferred, see below)
- **Files created/modified:** 9

## Accomplishments
- **Headless strict `nxm://` parser** (`crates/nexus/src/model.rs`): `NxmLink` DTO + `NxmLinkKind` enum + `NxmLink::parse`. A security boundary for untrusted OS deep-link input (threat T-03-12): scheme must be exactly `nxm` (case-insensitive), the download path must be exactly `/mods/<id>/files/<id>` with BOTH ids parsing as `u64`, the `oauth/callback` authority is discriminated to `OAuthCallback{code,state}`, and `key`/`expires`/`user_id` are parsed as **opaque** percent-decoded strings never interpreted/logged/shelled. Dependency-free (a tiny query reader + percent-decoder; no `url`/`serde_urlencoded` dep), never panics, every malformed input a typed `Err` (`Redeem` for a download link, `Auth` for a bad oauth-callback).
- **6 parser tests** (`crates/nexus/tests/nxm_parse.rs`) + a serde round-trip: full free-user link, premium link (no key), oauth-callback discrimination, a **rejection battery** (non-nxm scheme / missing path / non-numeric ids / wrong keywords / extra+trailing segments / empty+garbage / u64-overflow / malformed oauth-callbacks), case-insensitive scheme, percent-decoded opaque values.
- **Deep-link + single-instance wiring** (`tauri.conf.json`, `capabilities/default.json`, `lib.rs`): `deep-link` desktop scheme `["nxm"]`; `deep-link:default` + `core:event:default` capabilities; `tauri_plugin_single_instance::init` registered **FIRST** (raises/focuses the live window on a second invocation), **then** `tauri_plugin_deep_link::init` (the load-bearing order — RESEARCH Anti-Pattern); `register_all()` + `on_open_url` in `setup()` (cfg `linux`/`windows`), registration failures non-fatal.
- **Thin nxm router** (`commands/nexus.rs::handle_nxm_url`): parses via the headless `NxmLink::parse`, then dispatches — `OAuthCallback` → `auth::complete_oauth` (validates `state == csrf` before exchange, stores the refresh token in the keyring; **closes the Plan-01 OAuth loop**); `Download` → `run_download_to_window` with the free-user `key`+`expires` (NEXUS-04). A `domain→appid` Bethesda allow-list resolves the managed game; an unknown domain → the Warning, not a guess. Emits secret-free `nxm://arrival` + `nxm://expired` events.
- **Shared download core** (`commands/downloads.rs::run_download_to_window`): extracted so the IPC `start_download` command and the `nxm://` router run the **same** Plan-02 stream→extract→stage flow — the free-user redemption is not a parallel path.
- **Frontend** (`api.ts`, `+page.svelte`): `onNxmArrival`/`onNxmExpired` listen bridges; a §C.1 non-blocking Success toast "Download started from NexusMods" (auto-dismiss 4s) that also mirrors a new downloads-list row (reusing the Plan-02 list, no second surface); the Premium-tier hint branch added alongside the existing Free-account hint; the expired event maps to the existing §C.3 Warning notice.

## Task Commits
1. **Task 1: Headless strict nxm:// parser (NxmLink) + tests** — `596b17a` (feat / TDD)
2. **Task 2: Deep-link + single-instance wiring + on_open_url routing** — `9d9803c` (feat)
3. **Task 3: nxm:// arrival toast + Premium hint + expired-link Warning UI** — `35df340` (feat / TDD)
4. **Task 4: Live free-user nxm:// human-verify** — DEFERRED (see "Deferred — Pending live-account UAT")

_TDD note: Tasks 1 & 3 are `tdd="true"`. Task 1's `nxm_parse.rs` rejection/round-trip suite is the executable contract and landed with the parser in one atomic commit (the tests + the code that satisfies them). Task 3's behaviour is asserted via `npm run check` (0 errors) plus exact-copy branch-presence greps (arrival toast / free + Premium hints / expired Warning / auto-dismiss timer)._

## Verification Evidence
- `cargo test --workspace --locked` — all green (0 failures): the 6 new `nxm_parse` tests + the `nxm_link_serde_round_trips` unit test, alongside the full Phase-1/2/3 suite (store 35, nexus client/ratelimit/auth, deploy/extract/loadorder, src-tauri download_stage NEXUS-06).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo deny check advisories bans licenses sources` — advisories/bans/licenses/sources all OK. `tauri-plugin-single-instance v2.4.2` + `tauri-plugin-deep-link` are now active in the tree and clear the supply-chain gate (both official tauri-apps/plugins-workspace crates, approved in the RESEARCH Package Legitimacy Audit — T-03-SC).
- `npm --prefix frontend run check` — 0 errors, 0 warnings (142 files).
- **Grep gates:** `"nxm"` in `tauri.conf.json` (1); `deep-link:default` in the capability (1); single-instance `init` (line 64) appears **before** deep-link `init` (line 72) in `lib.rs`; `handle_nxm_url` routes via `NxmLink::parse` → `run_download_to_window` + `complete_oauth` (call sites confirmed); no `process::Command`/shell-out in `model.rs` or the handler (0); no `tracing::` in `model.rs` (0); no `tracing!`/`println!` takes a url/key/expires/code/state arg in the handler (0); the emit payloads carry no secret (`NxmArrival`=id only, `NxmExpired`=reason only); `crates/nexus/Cargo.toml` still has no tauri/keyring/tauri-plugin dependency.

## Deviations from Plan

### Auto-fixed / reasoned adjustments

**1. [Rule 3 - Blocking] nxm:// parser is a dependency-free hand-split, not the `url` crate**
- **Found during:** Task 1.
- **Issue:** The plan's action allowed "the `url` crate if already transitively available … else hand-split defensively." `url` 2.5.8 is present only *transitively* (via reqwest/oauth2); adding it as a **direct** dep of `crates/nexus` would re-introduce the dependency Plan-02 deliberately avoided (it built its own percent-encoder to keep reqwest on its minimal rustls-only feature set and cargo-deny clean). `url`'s custom-scheme authority handling is also quirky for `nxm://`.
- **Fix:** A strict dependency-free hand-split parser (a small `query_get` + RFC-3986 `percent_decode`), validating the exact `nxm://host/mods/<u64>/files/<u64>` grammar and discriminating `oauth/callback`. Zero new supply-chain surface; the fixed grammar is safer hand-rolled than via a general URL parser.
- **Files:** crates/nexus/src/model.rs · **Commit:** `596b17a`

**2. [Rule 2 - Missing critical functionality] domain→appid resolution for the download arm**
- **Found during:** Task 2.
- **Issue:** The `nxm://` link carries the game *domain* (e.g. `skyrimspecialedition`), but `run_download_to_window` needs the Steam *AppID* (via `require_game`) to resolve the managed game's staging dir. `core::Game` has no `nexus_domain` field and the store holds no domain→appid map.
- **Fix:** A small fixed v1 Bethesda allow-list in the shell (`commands/nexus.rs::appid_for_domain`: `skyrimspecialedition`→489830, `fallout4`→377160), mirroring the frontend `SUPPORTED` list. An unknown domain is **rejected** with the §C.3 Warning rather than guessed. Kept in the shell (registry concern), not the headless crate.
- **Files:** src-tauri/src/commands/nexus.rs · **Commit:** `9d9803c`

**3. [Rule 3 - Blocking] shared download core so the nxm router can't call the IPC command directly**
- **Found during:** Task 2.
- **Issue:** `on_open_url` hands the handler an `AppHandle`, but `start_download` is a `#[tauri::command]` whose `State<'_,…>`/`Window` lifetimes are bound to an IPC invocation — it can't be invoked directly from the deep-link closure.
- **Fix:** Extracted `run_download_to_window(state, window, …)` as the shared core (auth resolution + cancel-flag registration + the Plan-02 `run_download` + terminal progress emit). `start_download` now delegates to it; the nxm router obtains the `main` window + state from the `AppHandle` and calls the same core — guaranteeing one download path.
- **Files:** src-tauri/src/commands/downloads.rs, src-tauri/src/commands/nexus.rs · **Commit:** `9d9803c`/`35df340`

**Total:** 3 reasoned adjustments (1 missing-critical, 2 blocking). No security invariant weakened: the parser is strict + dependency-free, no link content is logged/shelled/interpolated (V5/V7), the OAuth arm still goes through `complete_oauth`'s CSRF/PKCE validation (T-03-13), the download arm reuses the unchanged `extract::install_archive` zip-slip defense + the Plan-02 `Policy::none()`/rustls client (T-03-16), and the headless crate still has zero Tauri/keyring deps.

## Threat surface
No new trust boundary beyond the plan's `<threat_model>`. All assigned `mitigate` dispositions are implemented:
- **T-03-12** (spoofed nxm://): strict headless parse (scheme=nxm, numeric u64 ids, opaque key/expires); no shell-out / no command interpolation (grep-gated); the downloaded file still flows through `extract`'s zip-slip defense.
- **T-03-13** (OAuth CSRF): the `oauth/callback` arm calls `auth::complete_oauth`, which validates `state == csrf` (PKCE-bound) before the exchange; a malformed callback is `Err(Auth)`.
- **T-03-14** (duplicate-instance/link-flood): single-instance forwarding yields ONE row per link (no duplicate window); each download is still governor-rate-limited by the Plan-02 limiter.
- **T-03-15** (key/expires/code in logs): no `tracing`/emit in the parser or the handler takes key/expires/code/url (grep-gated); the parser `Err` is intentionally not logged.
- **T-03-16** (open-redirect): redemption reuses the Plan-02 client (`Policy::none()` + rustls + status-checked) verbatim.
- **T-03-SC** (plugin supply chain): both plugins are official tauri-apps crates, declared/pinned in Plan 01, and re-cleared `cargo deny` here.

## Known Stubs
None that block the plan goal. The autonomous, mockable surface is complete and verified: the parser is unit-tested against the rejection battery; the wiring compiles + clippy-clean with the load-bearing register order; the download arm reuses the Plan-02 flow that is already integration-tested end-to-end (`download_stage.rs`, NEXUS-06). The ONLY unexercised piece is the LIVE OS routing of a real single-use `nxm://` link from a non-Premium browser session — that is the deferred checkpoint below, an intentional manual-UAT boundary (a single-use, short-lived key cannot be baked into a test), not a code stub.

## Deferred — Pending live-account UAT

**Task 4 (`checkpoint:human-verify`, `gate="blocking-human"`) is DEFERRED, not performed.** The LIVE free-user `nxm://` handoff cannot be exercised in this autonomous session: it requires a real **NON-Premium NexusMods account** clicking the website "Mod Manager Download" button (the `key`+`expires` it mints are **single-use and short-lived** — they cannot be captured into a test), the OS routing the `nxm://` scheme to the running app, and `xdg-mime` + `update-desktop-database` on PATH for `register_all()`. **No live `nxm://` pass is claimed, fabricated, or simulated.** Every parser/routing path is unit-tested; the redemption→extract→stage terminus it reuses is integration-tested (Plan 02, NEXUS-06).

**NEXUS-04 / NXM-01 status:** the free-user redemption code path + the deep-link/single-instance wiring + the parser are implemented and unit-verified; their **live** verification is `deferred-pending-non-premium-account` (and, for the OAuth-callback arm, also `deferred-pending-oauth-client-registration` from Plan 01).

**Manual UAT steps (mirrors the plan's how-to-verify):**
1. Confirm `xdg-mime` + `update-desktop-database` are installed. Run `cargo tauri dev` (the app must be running so single-instance forwarding applies).
2. (NXM-01) Log in as a FREE (non-Premium) NexusMods account in a browser, open a mod page for a managed game (Skyrim SE / Fallout 4), and click "Mod Manager Download". Confirm the OS routes the `nxm://` link to the already-running NexTwist (NOT a second window), the "Download started from NexusMods" toast appears, and a new row begins downloading.
3. (NEXUS-04) Confirm the free-user download completes, extracts into staging, and appears as an ordinary deployable `ManagedMod` — then deploy + purge to confirm the round-trip-to-pristine guarantee holds for an `nxm:`-sourced mod.
4. With the app already open, trigger a second `nxm://` link and confirm it produces ONE new row (forwarded to the live instance), never a duplicate window.
5. (Optional, if an OAuth `client_id` is registered) Confirm the `nxm://oauth/callback` path completes a real OAuth login and stores the refresh token in the keyring.
6. Open an expired/old `nxm://` link and confirm the "This download link has expired" Warning appears (not a stuck Failed row).

## Next Phase Readiness
- The `nxm://` MIME handler is registered at dev/installed-runtime via `register_all()`; **Phase 5 (AppImage)** finalizes the `.desktop` MIME registration + the absolute-path AppImage caveat (the one OS-registered state this phase introduces).
- **Blockers carried forward:** live free-user `nxm://` redemption is gated on a non-Premium account (UAT); the live OAuth-callback arm additionally awaits OAuth client registration (carried from Plan 01). All mockable behaviour is unblocked and green.

## Self-Check: PASSED
- Created file `crates/nexus/tests/nxm_parse.rs` verified on disk.
- All three task commits (`596b17a`, `9d9803c`, `35df340`) verified in git history.

---
*Phase: 03-nexusmods-login-download*
*Completed: 2026-06-21*
