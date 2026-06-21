---
phase: 03-nexusmods-login-download
verified: 2026-06-21T00:00:00Z
status: passed
score: 14/14 must-haves verified (11 automated + 3 live UAT passed on real hardware 2026-06-21; user sign-off "all good")
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Live OAuth2 round-trip login (NEXUS-01)"
    expected: "After a public OAuth client_id + nxm://oauth/callback redirect is registered under the Nexus Acceptable Use Policy, running cargo tauri dev and completing an OAuth login populates the account panel with the real username + tier, and the refresh token lands in the OS keyring (never a plaintext file)."
    why_human: "Requires a registered Nexus OAuth public client_id (release task, not self-service) plus a real account and live token exchange. The PKCE/CSRF/code-exchange logic is implemented and mockito-tested for request shape; only the LIVE round-trip is unverifiable autonomously. The API-key-paste fallback is the works-today login path and is fully unit-tested."
  - test: "Live Premium in-app direct download (NEXUS-03)"
    expected: "Logged in as a real Premium account, starting an in-app download of a small mod shows an advancing per-item progress bar (percent + bytes) without freezing the UI, completes to '✓ Done — added to staging', and the mod appears as an ordinary deployable ManagedMod that survives a deploy→purge round-trip to pristine."
    why_human: "Needs a real Premium NexusMods account + the live API/CDN; the single download_link/stream path is mockito-tested and the extract→stage→provenance terminus is integration-tested (download_stage.rs), but the LIVE premium fetch cannot be exercised autonomously."
  - test: "Live free-user nxm:// 'Mod Manager Download' handoff (NEXUS-04 / NXM-01)"
    expected: "Logged in as a FREE (non-Premium) account in a browser, clicking 'Mod Manager Download' on a Skyrim SE / Fallout 4 mod page routes the nxm:// link to the already-running app (one new downloads row + 'Download started from NexusMods' toast, never a second window), the keyed link redeems and extracts into staging as a deployable mod, a second link forwards to the live instance (one row, no duplicate window), and an expired link surfaces the 'link expired' Warning (not a stuck Failed row)."
    why_human: "Requires a real non-Premium account clicking the website button (mints a single-use, short-lived key+expires that cannot be baked into a test), OS nxm:// scheme routing, and xdg-mime/update-desktop-database on PATH. The strict headless parser, single-instance/deep-link wiring, and free-user redemption path are all implemented and unit-tested; only the LIVE OS handoff is unverifiable autonomously."
---

# Phase 3: NexusMods Login & Download Verification Report

**Phase Goal:** A user can log into their NexusMods account and pull mods straight into NexTwist's staging store — Premium users via in-app direct download, free users via the website "Mod Manager Download" nxm:// handoff — ready to deploy through the safe engine already proven in Phases 1-2.
**Verified:** 2026-06-21
**Status:** human_needed
**Re-verification:** No — initial verification
**Mode:** mvp (goal-backward against ROADMAP Success Criteria; non-user-story goal, verified against the 4 success criteria as the contract — mirrors Phase 2's UAT-deferral handling)

## User Flow Coverage

ROADMAP Success Criteria mapped to codebase evidence. The end-to-end *live* user flow for three of the four criteria depends on real Nexus accounts (see Human Verification); the code that delivers each step is verified present and wired.

| Step | Expected | Evidence | Status |
|------|----------|----------|--------|
| Log in | User logs into NexusMods; tokens stored in keyring, never plaintext | `crates/nexus/src/auth.rs` (build_authorize_url PKCE-S256 + exchange_code CSRF + validate_api_key); `src-tauri/src/keyring.rs` (NoKeyringBackend hard-fail, zero fs write path); account panel `+page.svelte:142,158,179` | ✓ code; live OAuth ⚠️ human |
| Premium download | Premium user downloads a mod in-app, progress bar, no freeze | `crates/nexus/src/client.rs` (download_link premium-no-key) + `download.rs` (bytes_stream, no full-buffer) + `downloads.rs:run_download_to_window` + `+page.svelte` download://progress listener | ✓ code; live premium ⚠️ human |
| Free nxm:// handoff | Free user clicks website button → routed to running app → downloads | `crates/nexus/src/model.rs:NxmLink::parse` + `lib.rs` (single-instance FIRST → deep-link → on_open_url) + `commands/nexus.rs:handle_nxm_url→route_download` | ✓ code; live handoff ⚠️ human |
| One-click deep-link | nxm:// on nexusmods.com routed via registered Linux handler | `tauri.conf.json:29 schemes ["nxm"]` + `capabilities/default.json deep-link:default` + `lib.rs register_all()/on_open_url` | ✓ code; live OS route ⚠️ human |
| Auto-extract + rate-limit | Downloaded mod auto-extracts into staging; API rate limits respected, no UI freeze | `downloads.rs:310 extract::install_archive` verbatim + `add_nexus_source`; `ratelimit.rs:RateLimiter` (governor + X-RL-* backoff) shared in AppState; `download_stage.rs` end-to-end test PASS | ✓ VERIFIED (automated) |
| Outcome | Mods land in staging deployable by the Phase-1/2 safe engine | `download_stage.rs` proves stream→extract→stage→provenance produces an ordinary ManagedMod; V4 migration additive (no Phase-1/2 table touched) | ✓ VERIFIED (automated) |

## Goal Achievement

### Observable Truths

| #   | Truth (success criterion / plan must-have) | Status     | Evidence |
| --- | ------------------------------------------ | ---------- | -------- |
| 1   | API-key-paste login shows account panel with username + Premium/Free tier | ✓ VERIFIED | `auth.rs:validate_api_key`, panel `+page.svelte:158,664`; `validate_api_key_parses_user_info` test passes |
| 2   | OAuth2 authorize URL carries PKCE S256 + CSRF state; code-exchange shaped correctly | ✓ VERIFIED | `auth_mock.rs`: `authorize_url_carries_pkce_s256_and_state`, `exchange_code_posts_pkce_and_returns_tokens`, `exchange_code_rejects_csrf_mismatch` all pass |
| 3   | On logout keyring entry + in-memory token both cleared, panel returns logged-out | ✓ VERIFIED | `keyring.rs` clear path + idempotent logout tests; `+page.svelte:179` logout wrapper |
| 4   | No keyring backend → login blocked, no credential written to any file | ✓ VERIFIED | `keyring.rs:NoKeyringBackend`; test `auth_keyring_no_backend_store_hard_fails_and_writes_nothing` asserts zero file creation |
| 5   | Premium user starts download with advancing per-item progress, no UI freeze | ⚠️ PRESENT (live) | streaming + progress callback + event-driven UI present & wired; LIVE premium fetch → human (NEXUS-03) |
| 6   | Completed download auto-extracts into staging as ManagedMod with Nexus provenance | ✓ VERIFIED | `download_stage.rs` end-to-end test `download_streams_extracts_stages_and_persists_provenance` PASS |
| 7   | Client backs off near rate limit (X-RL-* headers) + UI shows rate-limit notice | ✓ VERIFIED | `ratelimit.rs` governor + backoff (4 unit tests); WR-01/WR-02 fix wires `ratelimited` emit + `+page.svelte:201` notice |
| 8   | V4 migration additive (no Phase-1/2 table altered); store round-trips provenance, no rusqlite in public API | ✓ VERIFIED | `v4_adds_nexus_source_additively_over_v3` + `cascade_delete_removes_provenance` tests pass; grep ALTER/DROP/UPDATE == 0; public sigs use core::NexusSource |
| 9   | Free-user keyed nxm:// link redeemed via download_link.json (key+expires), extracts to staging | ⚠️ PRESENT (live) | `client.rs` free-shape tested; `route_download` threads key/expires to shared core; LIVE redemption → human (NEXUS-04) |
| 10  | nxm:// parsed strictly into {domain,mod,file,key?,expires?}; oauth-callback vs download discriminated | ✓ VERIFIED | `nxm_parse.rs`: 6 tests incl. rejection battery, oauth discrimination, case-insensitive scheme, percent-decode |
| 11  | nxm:// arrival routes to running app (no duplicate window), new row + toast | ⚠️ PRESENT (live) | single-instance FIRST + deep-link wiring + handle_nxm_url present; LIVE OS routing → human (NXM-01) |
| 12  | Expired/malformed nxm:// link surfaces Warning, not a stuck Failed row | ✓ VERIFIED | `handle_nxm_url` Err→`emit_expired`; `+page.svelte:195` expired→Warning (row removed, not Failed) |
| 13  | Download never buffers whole file (bytes_stream, no .bytes().await) | ✓ VERIFIED | grep bytes_stream==2, .bytes().await==0 in download.rs |
| 14  | nxm:// handler never shells out / interpolates link content into a command | ✓ VERIFIED | `model.rs` parser is dependency-free, no process::Command; `handle_nxm_url` only forwards validated ids + opaque key/expires |

**Score:** 11/14 truths verified (3 present + wired + unit-tested, live-account behavior deferred to human verification — NOT failures)

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `crates/nexus/src/auth.rs` | OAuth2 PKCE + exchange + api-key validate (≥60) | ✓ VERIFIED | 218 lines; build_authorize_url/exchange_code/validate_api_key present |
| `crates/nexus/src/error.rs` | NexusError enum | ✓ VERIFIED | 76 lines; pub enum NexusError (Auth/Http/Store/Io/RateLimited/Redeem) |
| `crates/nexus/src/client.rs` | hybrid REST v1 + GraphQL v2 + X-RL parse (≥60) | ✓ VERIFIED | 289 lines; download_link premium/free + metadata + status branching |
| `crates/nexus/src/download.rs` | streaming download + progress callback (≥50) | ✓ VERIFIED | 139 lines; bytes_stream, Tauri-free Fn callback, CancelFlag |
| `crates/nexus/src/ratelimit.rs` | governor + X-RL backoff | ✓ VERIFIED | 278 lines; RateLimiter present, shared in AppState (WR-03 fix) |
| `crates/nexus/src/model.rs` | NxmLink DTO + strict parser | ✓ VERIFIED | 361 lines; NxmLink + NxmLinkKind + parse |
| `crates/store/src/migrations/V4__nexus_provenance.sql` | additive nexus_source table | ✓ VERIFIED | CREATE TABLE nexus_source, FK CASCADE, no destructive stmt |
| `crates/store/src/nexus.rs` | store facade, no rusqlite in public API | ✓ VERIFIED | add/get_nexus_source take core types; rusqlite only in private helper |
| `crates/nexus/tests/auth_mock.rs` | mockito auth tests | ✓ VERIFIED | 5 tests pass |
| `crates/nexus/tests/client_mock.rs` | download_link/rate-limit/error tests | ✓ VERIFIED | 10 tests pass |
| `crates/nexus/tests/nxm_parse.rs` | parser rejection battery | ✓ VERIFIED | 6 tests pass |
| `src-tauri/src/keyring.rs` | store/load/clear, hard-fail-no-plaintext | ✓ VERIFIED | NoKeyringBackend; zero fs write path |
| `src-tauri/tauri.conf.json` | deep-link nxm scheme | ✓ VERIFIED | schemes ["nxm"] line 29 |
| `src-tauri/src/lib.rs` | single-instance (first) + deep-link + on_open_url | ✓ VERIFIED | single_instance L64 before deep_link L72; register_all + on_open_url |

### Key Link Verification

| From | To | Via | Status |
| ---- | -- | --- | ------ |
| commands/nexus.rs | nexus/auth.rs | login/logout adapters delegate to headless auth | ✓ WIRED |
| commands/nexus.rs | keyring.rs | refresh token persisted only via keyring module | ✓ WIRED |
| +page.svelte | commands/nexus.rs | account panel invokes login/logout/account_info via api.ts | ✓ WIRED |
| commands/downloads.rs | nexus/download.rs | run_download delegates to headless streaming download | ✓ WIRED |
| commands/downloads.rs | extract/staging | extract::install_archive called verbatim (NEXUS-06) | ✓ WIRED (downloads.rs:310) |
| commands/downloads.rs | store/nexus.rs | add_nexus_source after staging | ✓ WIRED (downloads.rs:327) |
| +page.svelte | commands/downloads.rs | downloads list listens download://progress, per-item bars | ✓ WIRED |
| lib.rs | nexus/model.rs | on_open_url parses via NxmLink::parse, routes | ✓ WIRED (handle_nxm_url L191) |
| lib.rs | commands/downloads.rs | nxm download → run_download_to_window → new row | ✓ WIRED (route_download L238) |
| lib.rs | auth.rs | nxm://oauth/callback → complete_oauth | ✓ WIRED (route_oauth_callback L271) |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| Phase-3 headless crate tests | `cargo test -p nextwist-nexus --locked` | 13 unit + 5 auth_mock + 10 client_mock + 6 nxm_parse pass | ✓ PASS |
| Store provenance + additive migration | `cargo test -p nextwist-store` | 35 pass incl. v4_adds_nexus_source_additively_over_v3, cascade_delete | ✓ PASS |
| NEXUS-06 download→extract→stage end-to-end | `cargo test -p nextwist --test download_stage` | `download_streams_extracts_stages_and_persists_provenance` PASS | ✓ PASS |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | clean | ✓ PASS |
| Frontend types | `npm --prefix frontend run check` | 142 files, 0 errors, 0 warnings | ✓ PASS |
| Supply-chain | `cargo deny check advisories bans licenses sources` | advisories/bans/licenses/sources OK | ✓ PASS |

### Prohibitions (negative checks)

| Prohibition | Status | Evidence |
| ----------- | ------ | -------- |
| No token/key/code_verifier written to plaintext or logged | ✓ HELD | keyring.rs has no fs write path; test asserts no file created; secret-redaction grep clean |
| crates/nexus has NO tauri/keyring/tauri-plugin dep | ✓ HELD | Cargo.toml [dependencies] block scanned — none present |
| crates/nexus reqwest is rustls-only (no native-tls) | ✓ HELD | workspace pin default-features=false, features rustls (no native-tls) |
| Download never buffers whole file | ✓ HELD | bytes_stream==2, .bytes().await==0 |
| V4 never ALTER/DROP/UPDATE a Phase-1/2 table | ✓ HELD | grep == 0; migration is CREATE TABLE + CREATE INDEX only |
| nxm:// handler never shells out / interpolates | ✓ HELD | no process::Command in model.rs/handler; only validated ids + opaque key/expires |
| single-instance registered before deep-link | ✓ HELD | lib.rs L64 (single_instance) before L72 (deep_link) |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ----------- | ----------- | ------ | -------- |
| NEXUS-01 | 03-01 | Log in via OAuth2 | ✓ code / ⚠️ live | PKCE/CSRF/exchange implemented + mockito-tested; API-key fallback fully wired; LIVE OAuth → human (needs registered client_id) |
| NEXUS-02 | 03-01 | Tokens stored securely in keyring | ✓ SATISFIED | keyring hard-fail-no-plaintext; no fs write path; test-proven |
| NEXUS-03 | 03-02 | Premium in-app direct download | ✓ code / ⚠️ live | stream/extract/provenance implemented + mockito + download_stage; LIVE premium → human |
| NEXUS-04 | 03-03 | Free-user Mod Manager Download (nxm://) | ✓ code / ⚠️ live | parser + redemption + wiring implemented + unit-tested; LIVE handoff → human |
| NEXUS-05 | 03-02 | Respect API rate limits | ✓ SATISFIED | governor + X-RL-* backoff, shared limiter; 4 unit tests; WR-01/02/03 fixes wired |
| NEXUS-06 | 03-02 | Downloaded mod auto-extracted into staging | ✓ SATISFIED | download_stage.rs end-to-end test PASS; extract::install_archive verbatim |
| NXM-01 | 03-03 | One-click install via Linux deep-link handler | ✓ code / ⚠️ live | single-instance+deep-link+on_open_url wired; tauri.conf scheme; LIVE OS route → human |

All 7 declared requirement IDs accounted for; all 7 appear in REQUIREMENTS.md mapped to Phase 3. No orphaned requirements. (REQUIREMENTS.md currently marks NEXUS-04/NXM-01 "Pending" and NEXUS-01/03 "Complete" — the live-account portions remain to be confirmed via the Human Verification items below.)

### Anti-Patterns Found

None blocking. The 1 BLOCKER (CR-01) + 7 WARNINGS from 03-REVIEW.md were all FIXED — each has a dedicated `fix(03)` commit and the corresponding code is present in the working tree (CR-01 TempArchive RAII guard downloads.rs:207; WR-01/02 ratelimited emit + UI; WR-03 shared AppState rate_limiter; WR-04 raw percent-decode; WR-05 RETURNING upsert; WR-06 unique-per-arrival nxm row id; WR-07 keyring session restore on startup at commands/nexus.rs:143-175). No TBD/FIXME/XXX debt markers in phase files.

### Human Verification Required

Three requirement verifications genuinely depend on live NexusMods accounts that cannot be exercised in an autonomous session. The implementing code is present, wired, and unit/integration-tested in every mockable dimension; only the LIVE round-trip is deferred (intentional, mirrors Phase 2's in-game UAT). See the `human_verification` frontmatter for full step-by-step UAT (sourced from 03-VALIDATION.md "Manual-Only Verifications" and each SUMMARY's "Deferred — Pending live-account UAT"):

1. **Live OAuth2 round-trip (NEXUS-01)** — needs a registered Nexus OAuth public client_id + real account.
2. **Live Premium in-app download (NEXUS-03)** — needs a real Premium account + live API/CDN.
3. **Live free-user nxm:// handoff (NEXUS-04 / NXM-01)** — needs a real non-Premium account clicking the website button (single-use key) + OS nxm:// routing.

### Gaps Summary

No code gaps. Every artifact exists, is substantive, wired, and (for data-rendering paths) has real data flow proven by the `download_stage.rs` end-to-end test. All prohibitions hold. The full automated gate is GREEN (workspace tests, clippy -D warnings, frontend check, cargo-deny), independently re-run during this verification — not taken from SUMMARY claims. All 8 code-review findings are fixed in the tree.

The phase is **not** failed: the only items not closeable here are three LIVE-account UAT verifications, which is exactly the deferral pattern Phase 2 used. Status is `human_needed` (not `passed`) solely because those three live walkthroughs require real Nexus accounts. The two fully-automatable Nexus requirements that share this phase (NEXUS-02 keyring, NEXUS-05 rate-limit, NEXUS-06 auto-extract) are all fully VERIFIED.

---

_Verified: 2026-06-21_
_Verifier: Claude (gsd-verifier)_
