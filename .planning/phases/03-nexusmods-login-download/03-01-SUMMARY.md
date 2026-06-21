---
phase: 03-nexusmods-login-download
plan: 01
subsystem: auth
tags: [oauth2, pkce, keyring, nexusmods, reqwest, rustls, mockito, svelte, tauri]

# Dependency graph
requires:
  - phase: 02-multi-mod-management
    provides: "headless-crate pattern (loadorder masterlist real_fetch: rustls + Policy::none() + error_for_status), thin Tauri command adapters (boundary_err/require_game), AppState + generate_handler! shape, core serde DTO round-trip convention, Svelte 5 $state + run() error UI"
provides:
  - "Headless crates/nexus crate (NexusError thiserror enum, UserInfo/OAuthTokens DTOs)"
  - "OAuth2 Authorization-Code + PKCE (S256) authorize-URL builder + CSRF-validated code exchange (nexus::build_authorize_url / nexus::exchange_code)"
  - "API-key validation against REST v1 /v1/users/validate.json (nexus::validate_api_key) — the works-today login fallback"
  - "Shell keyring module: store/load/clear refresh token in the OS Secret Service with the NEXUS-02 hard-fail-no-plaintext invariant (NoKeyringBackend)"
  - "Shell OAuth orchestration (open_authorize_url + complete_oauth) ready for the Plan-03 nxm://oauth/callback deep-link"
  - "commands/nexus.rs thin adapters (login_with_api_key/login_oauth_start/logout/account_info) + AppState in-memory auth state"
  - "Account panel (UI-SPEC §A): logged-out / logged-in (Premium/Free) / no-keyring banner in +page.svelte + api.ts wrappers"
affects: [03-02-premium-download, 03-03-nxm-handoff, download-flow, rate-limiting, nexus-provenance]

# Tech tracking
tech-stack:
  added: [oauth2 5.0 (rustls-tls), governor 0.10, futures-util 0.3, mockito 1.7 (dev), keyring 3.6 (sync-secret-service + crypto-rust), tauri-plugin-deep-link 2.4, tauri-plugin-single-instance 2.4, webbrowser 1, reqwest form+json+stream features, thiserror (added to src-tauri)]
  patterns: ["headless async HTTP client (rustls, Policy::none(), error_for_status) converted from loadorder's blocking real_fetch", "injectable base-URL + mockito for testable HTTP core", "keyring backend abstracted behind a trait for CI-safe NoStorageAccess simulation", "secrets never logged / never cross the IPC boundary (only UserInfo)"]

key-files:
  created:
    - crates/nexus/Cargo.toml
    - crates/nexus/src/lib.rs
    - crates/nexus/src/error.rs
    - crates/nexus/src/model.rs
    - crates/nexus/src/auth.rs
    - crates/nexus/src/client.rs (Plan-02 stub)
    - crates/nexus/src/download.rs (Plan-02 stub)
    - crates/nexus/src/ratelimit.rs (Plan-02 stub)
    - crates/nexus/tests/auth_mock.rs
    - src-tauri/src/keyring.rs
    - src-tauri/src/auth.rs
    - src-tauri/src/commands/nexus.rs
  modified:
    - Cargo.toml (workspace deps + nexus alias + reqwest features)
    - src-tauri/Cargo.toml (keyring/plugins/webbrowser/thiserror/nexus)
    - src-tauri/src/state.rs (AppState auth state)
    - src-tauri/src/commands/mod.rs (pub mod nexus)
    - src-tauri/src/lib.rs (module decls + generate_handler! commands)
    - frontend/src/lib/api.ts (UserInfo + auth wrappers)
    - frontend/src/routes/+page.svelte (account panel)

key-decisions:
  - "oauth2 5.0.0 binds its async executor to reqwest 0.12 but the workspace is on reqwest 0.13 (distinct reqwest::Client types). Resolved by issuing the token POST with the workspace's own hardened reqwest 0.13 client; oauth2 still owns S256 PKCE + CSRF. Added reqwest 'form' feature for the urlencoded body."
  - "Chose the webbrowser crate over RESEARCH A7's tauri-plugin-opener for the OAuth browser launch — it needs no Tauri plugin registration or capability-file change (plugin wiring deferred to Plan 03) and clears cargo-deny."
  - "Added thiserror to src-tauri for the two typed shell error enums (keyring + auth) the UI must distinguish; anyhow stays the boundary context."
  - "OAuth client_id is empty in AppState (no registered public client yet — RESEARCH Pitfall 3); login_oauth_start returns a clear 'use an API key instead' error until registration lands. The API-key paste path is the works-today login."
  - "No V4 migration in this plan — Nexus provenance persistence is Plan 02. V3 remains the highest migration."

patterns-established:
  - "Headless NexusMods client: async reqwest (rustls, redirect Policy::none(), error_for_status), external errors flattened to NexusError::Auth/Http String, injectable base URL for mockito — the boundary the download slices build on."
  - "Keyring hard-fail-no-plaintext (NEXUS-02): NoStorageAccess/PlatformFailure → NoKeyringBackend, no fs write path in the module at all; backend behind a trait so the hard-fail is unit-tested without a real DBus."
  - "Secret discipline: no token/key/code_verifier is logged or returned across IPC; the UI only ever sees UserInfo."

requirements-completed: [NEXUS-01, NEXUS-02]

# Metrics
duration: 20min
completed: 2026-06-21
status: complete
---

# Phase 3 Plan 01: NexusMods Auth Spine Summary

**OAuth2+PKCE authorize-URL + CSRF-validated code exchange and an API-key-paste fallback in a new headless `crates/nexus`, with shell keyring storage that hard-fails (never plaintext) when no Secret Service exists, surfaced through a logged-in/logged-out/no-keyring account panel.**

## Performance

- **Duration:** ~20 min
- **Started:** 2026-06-21T14:07:09Z
- **Completed:** 2026-06-21T14:27:xxZ
- **Tasks:** 3 of 4 (Task 4 is the live-OAuth human-verify checkpoint — deferred, see below)
- **Files modified/created:** 19

## Accomplishments
- New headless `nextwist-nexus` crate (zero Tauri/keyring deps, rustls-only) with the `NexusError` thiserror enum and `UserInfo`/`OAuthTokens` DTOs — the boundary the download slices (Plans 02/03) build on.
- OAuth2 Authorization-Code + PKCE (S256) authorize-URL construction (oauth2 5.0), CSRF/state validation, and code exchange — mockito-verified for request shape (`grant_type=authorization_code` + `code` + `code_verifier` in the POST body).
- API-key validation against REST v1 `/v1/users/validate.json` (the works-today login fallback while OAuth client registration is pending); 401 maps to `NexusError::Auth`, not a panic.
- Shell keyring with the NEXUS-02 hard-fail-no-plaintext invariant: no Secret Service → `NoKeyringBackend`, no file written; idempotent logout. CI-safe unit tests via a simulated `NoStorageAccess` (no real DBus).
- Account panel (UI-SPEC §A): logged-out CTA + API-key reveal, logged-in username + Premium/Free tag + confirm-gated logout, and the destructive no-keyring banner that blocks login. No token/key ever rendered.

## Task Commits

1. **Task 1: Scaffold crates/nexus (config + error + model) + failing auth test** — `ceac365` (feat)
2. **Task 2: Headless OAuth2-PKCE + API-key auth (mockito-tested)** — `7c1eb56` (feat / TDD GREEN)
3. **Task 3: Shell keyring + login/logout commands + AppState + account panel** — `5c6cc90` (feat)
4. **Task 4: Live-OAuth human-verify** — DEFERRED (see "Deferred — Pending live-account UAT")

_Task 1+2 form the TDD RED→GREEN cycle: Task 1 lands the failing `auth_mock.rs` (auth.rs `todo!()`); Task 2 makes it GREEN._

## Files Created/Modified
- `crates/nexus/src/error.rs` — `NexusError` (Store/Io/Http/Auth/RateLimited/Redeem + io() ctor)
- `crates/nexus/src/model.rs` — `UserInfo`, `OAuthTokens` (access in-mem only; never serialized to disk)
- `crates/nexus/src/auth.rs` — `build_authorize_url`, `exchange_code` (CSRF + manual RFC-6749 form POST), `validate_api_key`
- `crates/nexus/tests/auth_mock.rs` — 5 tests (authorize-URL shape, mockito code-exchange, api-key validate, CSRF mismatch, 401)
- `crates/nexus/src/{client,download,ratelimit}.rs` — Plan-02 stubs (module layout fixed now)
- `src-tauri/src/keyring.rs` — store/load/clear + `NoKeyringBackend` hard-fail; 4 CI-safe tests
- `src-tauri/src/auth.rs` — `open_authorize_url` (webbrowser) + `complete_oauth` (Plan-03 deep-link entry)
- `src-tauri/src/commands/nexus.rs` — thin login/logout/account adapters
- `src-tauri/src/state.rs` — in-memory `access_token`/`pending_oauth`/`user`/`oauth_client_id`
- `frontend/src/lib/api.ts`, `frontend/src/routes/+page.svelte` — `UserInfo` + wrappers + account panel
- `Cargo.toml`, `src-tauri/Cargo.toml` — workspace + shell deps (all cargo-deny-clean)

## Decisions Made
See `key-decisions` in frontmatter. Headlines: reqwest 0.12/0.13 split forced a manual (but standard) OAuth token POST with the workspace reqwest 0.13 client (oauth2 still owns PKCE/CSRF); `webbrowser` chosen over `tauri-plugin-opener` to avoid premature plugin/capability wiring; `thiserror` added to the shell for two typed error enums.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] oauth2 5.0.0 ↔ reqwest 0.13 incompatibility**
- **Found during:** Task 2 (headless OAuth2-PKCE)
- **Issue:** oauth2 5.0.0 wires its `AsyncHttpClient` to **reqwest 0.12**, but the workspace pins **reqwest 0.13** — the two `reqwest::Client` types are distinct, so oauth2's `request_async(&client)` rejects the workspace client (RESEARCH assumed oauth2+reqwest would align).
- **Fix:** Build the authorize URL + S256 PKCE challenge/verifier + CSRF with oauth2 (the security-sensitive half), but issue the token exchange as a standard RFC-6749 §4.1.3 + RFC-7636 `x-www-form-urlencoded` POST with the workspace's own hardened reqwest 0.13 client (rustls, `Policy::none()`, `error_for_status()`). Added the reqwest `form` feature to the workspace pin for the urlencoded body.
- **Files modified:** crates/nexus/src/auth.rs, Cargo.toml
- **Verification:** mockito test `exchange_code_posts_pkce_and_returns_tokens` asserts `grant_type`+`code`+`code_verifier` are in the POST body; all 5 auth tests pass; cargo-deny clean.
- **Committed in:** `7c1eb56`

**2. [Rule 3 - Blocking] thiserror not a src-tauri dependency**
- **Found during:** Task 3 (shell keyring + auth error enums)
- **Issue:** The shell uses anyhow at the boundary and had no `thiserror` dep, but the keyring + auth modules need typed enums (the UI must distinguish the NEXUS-02 `NoKeyringBackend` hard-fail) with `Display`.
- **Fix:** Added the existing-workspace `thiserror` to `src-tauri`; anyhow remains the boundary context.
- **Files modified:** src-tauri/Cargo.toml
- **Verification:** shell lib builds; clippy `-D warnings` clean.
- **Committed in:** `5c6cc90`

**3. [Rule 3 - Blocking] browser-launch crate selection (RESEARCH A7 swap)**
- **Found during:** Task 3 (OAuth orchestration `open_authorize_url`)
- **Issue:** RESEARCH A7 named `tauri-plugin-opener` `[ASSUMED]`; wiring a Tauri plugin requires builder registration + a capability-file entry, which is Plan-03 plugin-wiring scope and would add capability churn to this slice.
- **Fix:** Used the `webbrowser` crate (the RESEARCH-allowed swap) — a plain function call, no plugin/capability change. Introduced in Task 3 and cleared `cargo deny check advisories bans licenses sources` in the same task (the supply-chain gate is load-bearing).
- **Files modified:** src-tauri/Cargo.toml, src-tauri/src/auth.rs
- **Verification:** cargo-deny clean (advisories/bans/licenses/sources ok).
- **Committed in:** `5c6cc90`

---

**Total deviations:** 3 auto-fixed (all Rule 3 — blocking). **Impact:** All three were necessary to compile/ship the locked auth design without weakening any security invariant (PKCE/CSRF intact, rustls-only intact, keyring-only intact). No scope creep — no migration, no plugin wiring, no download flow added (those stay Plan 02/03).

## Note on benign acceptance-criteria grep matches
- The Task-1 grep `grep -v '^#' crates/nexus/Cargo.toml | grep -Eic 'tauri|keyring'` returns 1 — but the single match is the package **description** prose ("Tauri-free and keyring-free…"), not a dependency. There is no `tauri`/`keyring`/`tauri-plugin-*` entry in `[dependencies]` (verified by scanning the dependencies block). Intent satisfied.
- The Task-1 "no openssl in nexus tree" check flags `openssl-probe`, which is a **pure-Rust** cert-path prober pulled via `rustls-native-certs` — **not** `openssl-sys`. No OpenSSL is linked; the rustls-only invariant holds.
- The Task-2 V7 secret-redaction grep flags lines 121/188, but the matches are substrings ("code", "key") inside the human-readable log **message strings** — no secret variable is ever passed to a tracing macro (only `user_id`). Verified by manual review.

## Issues Encountered
- Locating crate sources (cargo not on PATH in the agent shell; resolved via `~/.cargo/bin`) and reading the oauth2 5.0 typestate `BasicClient` API from the registry source to get the `EndpointSet`/`EndpointNotSet` generics right. Resolved by inspecting the installed crate source directly.

## Deferred — Pending live-account UAT

**Task 4 (`checkpoint:human-verify`, `gate="blocking-human"`) is DEFERRED, not performed.** The live OAuth2 round-trip cannot be exercised in this autonomous session: it requires a **registered NexusMods OAuth public `client_id`** (gated by the Nexus Acceptable Use Policy — RESEARCH Pitfall 3 / Assumptions A1,A2) with `nxm://oauth/callback` as an allowed redirect, plus a real NexusMods account. Neither exists here. No live-OAuth pass is claimed or simulated. This mirrors how Phase 2 deferred its in-game UAT.

**NEXUS-01 status:** the OAuth2+PKCE code path is implemented and mockito-verified for request shape; its **live** verification is `deferred-pending-registration`. The **API-key-paste fallback is the works-today login path** and is fully wired + unit-tested.

**Manual UAT steps to run when an OAuth client is registered (and on real keyring backends):**
1. Confirm a NexusMods OAuth public `client_id` + `nxm://oauth/callback` redirect + the exact authorize scope are registered. If not yet: confirm the API-key-paste fallback is the accepted v1 login path (locked decision) and live-OAuth verification stays deferred. Set `AppState.oauth_client_id` from config/env once it lands.
2. With a working system keyring (GNOME Keyring or KWallet): `cargo tauri dev`, open the account panel, paste a real NexusMods personal API key, click **Save key** — confirm the panel shows your username + correct Premium/Free tier, and **Log out** returns it to logged-out (and clears the keyring entry).
3. (NEXUS-02) On a session with **no** Secret Service running, confirm login is blocked by the red "Can't store your login securely" banner and that **no credential file** is created under the app-data dir.
4. Confirm the OAuth `"public"` scope assumption (RESEARCH A1) + the client-registration path (A2) — or record them as still-pending so Plan 03's live `nxm://oauth/callback` test knows what to expect.

## Next Phase Readiness
- The headless `crates/nexus` boundary (error + model + auth, rustls-only, zero Tauri/keyring deps) is established; `client`/`download`/`ratelimit` modules are declared as stubs ready for Plan 02 to fill (governor limiter, REST v1 download-link, streaming download).
- The shell keyring + `complete_oauth` are ready for Plan 03's `nxm://oauth/callback` deep-link + single-instance wiring (plugins are declared in `src-tauri/Cargo.toml` but NOT yet registered in the builder — intentionally Plan 03).
- **Blocker carried forward:** live OAuth round-trip is gated on OAuth client registration (release task); the API-key fallback unblocks all of Phase 3 in the meantime.

## Self-Check: PASSED

All created files verified on disk (crates/nexus + src-tauri keyring/auth/commands + SUMMARY) and all three task commits (`ceac365`, `7c1eb56`, `5c6cc90`) verified in git history.

---
*Phase: 03-nexusmods-login-download*
*Completed: 2026-06-21*
