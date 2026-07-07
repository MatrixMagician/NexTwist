# Phase 3: NexusMods Login & Download - Research

**Researched:** 2026-06-21
**Domain:** OAuth2/PKCE desktop auth, NexusMods REST v1 / GraphQL v2 API, Linux `nxm://` deep-linking, streaming HTTP download, system keyring, client-side rate limiting — all in Rust + Tauri 2.11
**Confidence:** MEDIUM-HIGH (crate versions HIGH/registry-verified; API endpoint paths MEDIUM/cross-checked against Nexus's own clients; OAuth client-registration process LOW — needs a real registration to confirm)

## Summary

Phase 3 adds NexusMods account integration to the proven safe-deployment engine. The work splits cleanly along the existing architecture boundary: a new **headless `crates/nexus`** crate owns the async HTTP API client (auth token exchange, mod/file metadata, download-link generation, streaming download, the `governor` rate limiter, and Nexus model types — zero Tauri deps, `core` types in/out), while the **Tauri shell owns the OS-integration**: keyring storage, `nxm://` deep-link registration + capture, single-instance forwarding, the OAuth2+PKCE browser round-trip, and the `commands/{nexus,downloads}` adapters. The downloaded archive terminates at the existing `extract::install_archive` → staging pipeline unchanged, so a Nexus mod becomes an ordinary `ManagedMod`.

The three genuine unknowns flagged in STATE/CONTEXT resolve as follows. **(1) OAuth2:** NexusMods runs a standard Authorization-Code + PKCE (S256) flow at `https://users.nexusmods.com/oauth/authorize` + `/oauth/token`, discoverable via `/.well-known/openid-configuration`, with `nxm://oauth/callback` as a valid redirect URI — the `oauth2` 5.0 crate with the `reqwest` feature implements this directly. **(2) API surface:** download-link generation is **still REST v1** (`GET /v1/games/{domain}/mods/{mod_id}/files/{file_id}/download_link.json`); GraphQL v2 is the modern read path for mod/file metadata but does **not** cover download-link generation, so the hybrid client is correct and v1 remains load-bearing. **(3) nxm:// free-user flow:** the website "Mod Manager Download" button emits `nxm://<domain>/mods/<id>/files/<id>?key=<k>&expires=<ts>&user_id=<u>`; a **non-Premium** user passes that `key`+`expires` to the same `download_link.json` endpoint as query params (Premium users omit them). This still requires real non-Premium-account manual UAT.

**Primary recommendation:** Build `crates/nexus` as an **async** reqwest client (rustls, `json`+`stream` features) speaking REST v1 for download links + GraphQL v2 for metadata, fronted by a `governor` direct rate limiter that also honors `X-RL-Hourly-Remaining`/`X-RL-Daily-Remaining` reactively. In the shell, register `tauri-plugin-single-instance` **first** (with its `deep-link` feature) then `tauri-plugin-deep-link`, call `app.deep_link().register_all()` in `setup()` for dev-mode `nxm://` registration, store the OAuth refresh token / API key via `keyring` **3.6.x** (NOT 4.x — see Pitfall 1), and hard-fail with no plaintext fallback when no Secret Service backend exists. Add `mockito` as a dev-dependency so every API path is unit-tested against a local mock server, reserving real-account runs for manual UAT.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Authentication (NEXUS-01, NEXUS-02)**
- Primary login is **OAuth2 + Authorization-Code + PKCE (S256)** via the `nxm://oauth/callback` redirect captured by `tauri-plugin-deep-link` (`oauth2` crate 5.x).
- **Manual API-key paste is shipped as a last-resort v1 fallback** (covers the works-today path while OAuth-client registration is pending). Websocket SSO deferred.
- **Tokens live in the system keyring** (Secret Service via `keyring`): long-lived **refresh token / API key in the keyring**, short-lived OAuth **access token in memory only**. **No keyring backend → hard-fail with a clear error, NEVER write plaintext** (NEXUS-02 hard invariant).
- **Logout clears the keyring entry and the in-memory token.**
- The NexusMods API client lives in a **new headless `crates/nexus` crate** (async reqwest, `core` types, **zero Tauri deps**). The **Tauri shell owns OS-integration**: keyring, deep-link registration, OAuth-redirect capture, single-instance forwarding — and passes tokens into the headless client.

**Download Flow & nxm:// Handoff (NEXUS-03, NEXUS-04, NXM-01)**
- **Premium direct download**: call the Nexus API to generate a download link, then **stream the file with reqwest** emitting progress events to the UI.
- **Free-user flow is the `nxm://` handoff**: register the `nxm://` MIME handler; the website "Mod Manager Download" button hands the app a keyed link which the app **redeems** for the actual download.
- **Routing uses `tauri-plugin-deep-link` + `tauri-plugin-single-instance`** — a second `nxm://` invocation while the app is open is **forwarded to the live instance** (never spawns a duplicate).
- **Download-manager scope for v1 is intentionally minimal**: small concurrency cap, **per-item progress, no pause/resume** (queue/pause/resume/bandwidth → NEXV2-02, deferred).

**API Surface & Rate Limiting (NEXUS-05)**
- **Hybrid API behind one client module**: prefer **GraphQL v2** where available, fall back to **REST v1** where v2 lacks coverage (download-link generation + file metadata historically v1). Verify per-endpoint.
- **Rate limiting uses a `governor` token-bucket limiter** and honors `X-RL-*` headers with backoff. Nexus quota: ~300 req (600 premium), +1/sec recovery.
- The app **ships its public OAuth client ID** (PKCE → no client secret); registering under the Nexus Acceptable Use Policy is a **release task**, not per-user.
- The Nexus client is **async reqwest (`rustls` only, never native-tls)**. `crates/loadorder`'s existing **blocking** reqwest client stays as-is — the two coexist.

**Staging Integration & UX (NEXUS-06)**
- A downloaded archive flows through the exact same **`extract`→staging** pipeline (the `commands/mods.rs` install path). A Nexus mod becomes an ordinary staged `ManagedMod`.
- **Nexus provenance is persisted** (a refinery migration / new columns or table): NexusMods **mod id, file id, version, display name** recorded against the managed mod.
- **Login UI is a minimal account panel**; **Download UX is a simple downloads list with per-item progress bars** (async progress events, never freeze).

### Claude's Discretion
- Exact crate/module split inside `crates/nexus` (auth, client, download, model), the precise migration schema shape, the specific GraphQL queries vs REST endpoints per datum, and progress-event payload shapes — settle by plan-phase research against the live API.

### Deferred Ideas (OUT OF SCOPE)
- Websocket SSO login path (wss://sso.nexusmods.com).
- Advanced download manager — queue, pause/resume, bandwidth limits (NEXV2-02).
- Mod-update notifications / version tracking (NEXV2-01).
- Encrypted-file token fallback for systems without a Secret Service (v1 hard-fails).
- Collections / FOMOD consumption of the Nexus client (Phase 4).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| NEXUS-01 | User can log into their NexusMods account via OAuth2 | OAuth2+PKCE endpoints confirmed (`users.nexusmods.com/oauth/authorize`+`/oauth/token`, `/.well-known/openid-configuration`); `oauth2` 5.0 crate `AuthCodeFlow` + `set_pkce_challenge`/`set_pkce_verifier`/`request_async`; `nxm://oauth/callback` redirect captured by deep-link plugin. See *Architecture Pattern 1*. |
| NEXUS-02 | NexTwist stores auth tokens securely in the system keyring | `keyring` 3.6.x `sync-secret-service` (Secret Service / GNOME Keyring / KWallet); refresh token in keyring, access token in memory; **no-backend → `Err`, never plaintext**. See *Pattern 2*, *Pitfall 1*, *Security Domain*. |
| NEXUS-03 | Premium users can download a mod directly from NexusMods | REST v1 `download_link.json` (Premium omits `key`/`expires`) → reqwest streaming download. See *Pattern 3*, *Pattern 4*. |
| NEXUS-04 | Free users can install mods via the website "Mod Manager Download" (nxm://) handoff | `nxm://<domain>/mods/<id>/files/<id>?key=&expires=&user_id=` redeemed via the same `download_link.json` endpoint **with** `key`+`expires` query params. See *Pattern 5*. |
| NEXUS-05 | NexTwist respects NexusMods API rate limits | `governor` 0.10 direct `RateLimiter` (token bucket, ~+1/sec, hourly+daily caps) + reactive read of `X-RL-Hourly-Remaining`/`X-RL-Daily-Remaining`/`X-RL-Hourly-Reset` headers. See *Pattern 6*. |
| NEXUS-06 | A downloaded mod is auto-extracted into staging ready to deploy | Reuse `extract::install_archive(&archive, &game.staging_dir)` verbatim; persist Nexus provenance via a refinery migration; mod appears as ordinary `ManagedMod`. See *Pattern 7*. |
| NXM-01 | One-click install from an nxm:// link via a Linux deep-link handler | `tauri-plugin-single-instance` (registered **first**, `deep-link` feature) + `tauri-plugin-deep-link` (`schemes:["nxm"]`, `register_all()` in `setup()`, `on_open_url` handler). See *Pattern 5*, *Pitfall 4*. |
</phase_requirements>

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| OAuth2 PKCE token exchange | API/Backend (`crates/nexus`) | Shell (browser-open + redirect capture) | The HTTP token exchange is pure client logic (testable headless); only opening the system browser and capturing the `nxm://oauth/callback` is OS-bound, so it lives in the shell. |
| Browser launch + redirect capture | Shell (`src-tauri`) | — | `tauri_plugin_opener`/`webbrowser` + `tauri-plugin-deep-link` `on_open_url` are OS-integration; forbidden in headless crates. |
| Token storage (refresh/API key) | Shell (`src-tauri`) | — | `keyring` touches the OS Secret Service (DBus) — an OS concern; the headless client receives a token *value*, never a keyring handle. |
| Mod/file metadata fetch | API/Backend (`crates/nexus`) | — | Pure async HTTP (GraphQL v2 / REST v1); no OS dependency. |
| Download-link generation | API/Backend (`crates/nexus`) | — | REST v1 call; pure HTTP. |
| Streaming download + progress | API/Backend (`crates/nexus`) | Shell (re-emit progress as Tauri events) | Byte-stream + progress *computation* is headless (callback/channel); converting a progress value into a `window.emit` Tauri event is the shell's job. |
| `nxm://` scheme registration + capture | Shell (`src-tauri`) | — | `xdg-mime`/`.desktop` registration + single-instance forwarding are OS-integration. |
| Rate limiting | API/Backend (`crates/nexus`) | — | `governor` limiter wraps the client; pure logic. |
| Archive → staging | API/Backend (`crates/extract`, unchanged) | — | Already proven; Phase 3 only feeds it. |
| Nexus provenance persistence | Database (`crates/store`) | — | New migration + query module; no `rusqlite` in public API (existing invariant). |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `oauth2` | `5.0.0` | OAuth2 Authorization-Code + PKCE (S256) client | The de-facto Rust OAuth2 crate (`ramosbugs/oauth2-rs`, ~784k weekly dl). v5 has first-class PKCE + an async `request_async` against a reqwest client. `[VERIFIED: crates.io]` |
| `keyring` | `3.6.3` | Refresh-token / API-key storage in the OS Secret Service | Cross-platform keyring; `sync-secret-service` feature targets GNOME Keyring / KWallet on Linux. **Pin 3.6, NOT 4.x** (Pitfall 1). `[VERIFIED: crates.io]` |
| `governor` | `0.10.4` | Client-side token-bucket rate limiting | Standard Rust rate limiter (~952k weekly dl); `RateLimiter::direct(Quota::…)` models Nexus's hourly+per-second budget. `[VERIFIED: crates.io]` |
| `reqwest` | `0.13` (already a workspace dep) | Async API client + streaming download | **Add `json` + `stream` features** to the existing pin (keep `rustls`, never native-tls). Already present for the LOOT masterlist. `[VERIFIED: workspace Cargo.toml]` |
| `tauri-plugin-deep-link` | `2.4.9` | Register + receive `nxm://` and `nxm://oauth/callback` | The official Tauri plugin; `schemes:["nxm"]` + `register_all()` for dev. Matches `tauri` 2.11. `[VERIFIED: crates.io]` |
| `tauri-plugin-single-instance` | `2.4.2` | Forward a 2nd `nxm://` invocation to the live instance | Official plugin; enable its **`deep-link` feature** so the forwarded URL reaches `on_open_url`. Register **first**. `[VERIFIED: crates.io]` |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `futures-util` | `0.3.32` | `StreamExt`/`TryStreamExt` for `reqwest::Response::bytes_stream()` | Consume the download byte-stream chunk-by-chunk for progress accounting. `[VERIFIED: crates.io]` |
| `serde` / `serde_json` | `1` (already workspace) | (De)serialize NexusMods JSON + provenance | Already pinned. `[VERIFIED: workspace Cargo.toml]` |
| `tracing` | `0.1` (already workspace) | Structured logging of auth/download/rate-limit events | Existing convention. `[VERIFIED: workspace Cargo.toml]` |
| `thiserror` | `2` (already workspace) | `crates/nexus` error enum (`anyhow` only at shell boundary) | Existing convention. `[VERIFIED: workspace Cargo.toml]` |
| `tokio` | `1` (already workspace, shell) | Async runtime for the Nexus client | Already present in the shell; `crates/nexus` async fns run on the shell's tokio runtime. `[VERIFIED: workspace Cargo.toml]` |
| `mockito` | `1.7.2` | **dev-dependency** — local mock HTTP server for unit-testing every Nexus API path | No HTTP mock exists in the repo yet; needed so API paths are tested without a live account. `[VERIFIED: crates.io]` |
| `tauri-plugin-opener` *(or `webbrowser`)* | 2.x / latest | Open the system browser to the OAuth authorize URL | One is needed to launch the browser for the auth step; opener is the Tauri-native choice. `[ASSUMED]` — verify exact crate at plan time. |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `keyring` 3.6 | `keyring` 4.1 + `keyring-core` | v4 is a **major architectural break** (stores in separate crates, explicit `set_default_store` at startup, `Entry` API changed) and is only ~2 months old (4.0.0 = 2026-04-26). Maintainers explicitly say "do not update v3→v4." v3.6.3 is the stable, documented target. Revisit post-v1. `[VERIFIED: crates.io versions]` |
| `mockito` (dev) | `wiremock` 0.6.5 | `wiremock` is async-first and more expressive but heavier; `mockito` is simpler and sufficient for a handful of endpoints. Either works. `[VERIFIED: crates.io]` |
| `oauth2` crate | hand-rolled PKCE | Hand-rolling S256 + state validation is exactly the "don't hand-roll" trap (Pitfall 3). `[ASSUMED]` |
| REST v1 only | GraphQL v2 only | v2 does **not** cover download-link generation; v1 is mandatory for downloads. v2 is better for mod/file metadata. Hybrid is correct. `[CITED: graphql.nexusmods.com / forums.nexusmods.com]` |
| async Nexus client | blocking (like loadorder) | Blocking would freeze the UI on large downloads (criterion #4). Async streaming is required. The two clients coexist. `[VERIFIED: locked decision]` |

**Installation (additions to root `[workspace.dependencies]` + member crates):**
```toml
# crates/nexus
oauth2 = { version = "5.0", default-features = false, features = ["reqwest", "rustls-tls"] }
governor = "0.10"
reqwest = { version = "0.13", default-features = false, features = ["rustls", "http2", "charset", "json", "stream"] }  # add json+stream to existing pin
futures-util = "0.3"

# src-tauri
keyring = { version = "3.6", default-features = false, features = ["sync-secret-service", "crypto-rust"] }
tauri-plugin-deep-link = "2.4"
tauri-plugin-single-instance = { version = "2.4", features = ["deep-link"] }

# dev-dependency (crates/nexus)
mockito = "1.7"
```
> Note: `oauth2` 5.0 features include `reqwest`, `rustls-tls`, `pkce-plain`, `reqwest-blocking`. Use **`reqwest` + `rustls-tls`**, `default-features = false` to avoid pulling native-tls. `keyring` 3.6 `sync-secret-service` pairs with `crypto-rust` (pure-Rust crypto) to avoid an OpenSSL dep, preserving the AppImage rustls-only rule. `[VERIFIED: crates.io feature list]`

**Version verification:** all six core crates + supporting libs were verified against the crates.io registry on 2026-06-21 (max-stable versions and publish dates listed above; legitimacy audit below).

## Package Legitimacy Audit

> All new external crates verified via `gsd-tools query package-legitimacy check --ecosystem crates …` on 2026-06-21.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| `oauth2` | crates | since 2014-12 | ~784k/wk | github.com/ramosbugs/oauth2-rs | OK | Approved |
| `keyring` | crates | since 2016-02 | ~423k/wk | github.com/open-source-cooperative/keyring-rs | OK | Approved (pin **3.6**, not 4.x) |
| `governor` | crates | since 2019-11 | ~952k/wk | github.com/boinkor-net/governor | OK | Approved |
| `tauri-plugin-deep-link` | crates | since 2023-02 | ~112k/wk | github.com/tauri-apps/plugins-workspace | OK | Approved |
| `tauri-plugin-single-instance` | crates | since 2023 | (official) | github.com/tauri-apps/plugins-workspace | OK | Approved |
| `futures-util` | crates | mature | very high | github.com/rust-lang/futures-rs | OK | Approved |
| `mockito` (dev) | crates | mature | high | github.com/lipanski/mockito | OK | Approved |

**Packages removed due to [SLOP] verdict:** none.
**Packages flagged as suspicious [SUS]:** none.
**Provenance note:** every crate name above was discovered from an authoritative source (CLAUDE.md stack table, official Tauri docs, or the oauth2/keyring/governor official repos) AND returned `OK` from the legitimacy seam — so they are `[VERIFIED: crates.io]`. No `checkpoint:human-verify` install gate is required, but the planner SHOULD still gate the `keyring` install on the **3.6 pin** (not the latest 4.x).

## Architecture Patterns

### System Architecture Diagram

```
                          ┌─────────────────────── Tauri shell (src-tauri) ───────────────────────┐
  NexusMods website       │                                                                       │
  "Mod Manager Download"  │   tauri-plugin-single-instance (registered FIRST, feature=deep-link)   │
        │ click           │            │ 2nd invocation argv → forwards to live instance           │
        ▼                 │            ▼                                                            │
  nxm://<domain>/mods/… ──┼──▶ tauri-plugin-deep-link  ──on_open_url(urls)──▶ commands/nexus        │
   ?key=&expires=         │            ▲                                          │                 │
                          │   register_all() in setup()                          │                 │
  System browser          │            │                                         │                 │
   (OAuth authorize) ◀────┼── opener ──┘                                         │                 │
        │ user consents   │   nxm://oauth/callback ──▶ on_open_url ──▶ OAuth code │                 │
        ▼                 │                                              │        │                 │
        └─────────────────┼────────────────────────────┐               ▼        ▼                 │
                          │   keyring (Secret Service)  │      ┌────────────────────────┐          │
                          │   ▲ refresh token / API key │      │   commands/{nexus,      │          │
                          │   │ (access token: in-mem)  │◀─────│   downloads} adapters   │          │
                          └───┼─────────────────────────┼──────│  (lock AppState, thin)  │──────────┘
                              │                          │      └───────────┬────────────┘
                              │  token value passed in   │                  │ delegate
                  ┌───────────┼──────────────────────────┼──────────────────▼─────────────────────┐
                  │  crates/nexus (HEADLESS, zero Tauri, async reqwest rustls)                     │
                  │                                                                                │
                  │  auth.rs ──exchange code (oauth2 5.0, PKCE S256)──▶ tokens                     │
                  │  client.rs ──governor RateLimiter──▶ GraphQL v2 (metadata) / REST v1           │
                  │                 │                          │                                   │
                  │                 │                          ▼                                   │
                  │                 │   GET /v1/games/{domain}/mods/{id}/files/{fid}/              │
                  │                 │        download_link.json  [?key=&expires= for free users]   │
                  │                 ▼                          │                                   │
                  │  download.rs ──reqwest bytes_stream()──▶ chunk loop ──▶ progress callback ─────┼──▶ Tauri event
                  │                 │  reads X-RL-* headers → backoff                               │    (per-item %)
                  │                 ▼ writes archive to temp/staging-adjacent path                 │
                  └─────────────────┼──────────────────────────────────────────────────────────────┘
                                    ▼
                  crates/extract::install_archive(&archive, &game.staging_dir)  (UNCHANGED)
                                    ▼
                  crates/store  ── managed_mod row + Nexus provenance (new migration)
                                    ▼
                  ManagedMod  ──▶  existing Phase-1/2 deploy engine (untouched)
```

### Recommended Project Structure
```
crates/nexus/
├── src/
│   ├── lib.rs        # public API: NexusClient + re-exports; thiserror NexusError
│   ├── error.rs      # NexusError enum (thiserror) — Auth, Http, RateLimited, Redeem, Io
│   ├── auth.rs       # OAuth2 PKCE: build authorize URL, exchange code, refresh; token types
│   ├── client.rs     # NexusClient: reqwest+governor; GraphQL v2 + REST v1 calls; X-RL header parse
│   ├── download.rs   # streaming download with progress callback; redeem nxm key+expires
│   └── model.rs      # Nexus DTOs (ModFile, DownloadLink, UserInfo, NxmLink) — speak/convert core types
│   └── ratelimit.rs  # governor wrapper + reactive header backoff
└── tests/
    └── client_mock.rs  # mockito-backed: download_link.json (premium/free), rate-limit headers, error paths

src-tauri/src/
├── auth/              # OS-side OAuth orchestration: open browser, await nxm://oauth/callback
├── keyring.rs         # store/load/clear refresh token; hard-fail-no-plaintext
└── commands/
    ├── nexus.rs       # login, logout, account_info, generate_download — thin adapters
    └── downloads.rs   # start_download, cancel_download — thin adapters; re-emit progress events

crates/store/src/migrations/
└── V4__nexus_provenance.sql   # NOTE: V4 (V3 already exists), NOT V3 as CONTEXT.md assumed
```

### Pattern 1: OAuth2 Authorization-Code + PKCE (S256) with `oauth2` 5.0
**What:** Build an authorize URL with a PKCE challenge, open the system browser, capture the `nxm://oauth/callback?code=…&state=…` redirect via the deep-link plugin, then exchange the code for tokens against an async reqwest client.
**When to use:** Primary login (NEXUS-01).
**Endpoints (cross-checked):** authorize `https://users.nexusmods.com/oauth/authorize`, token `https://users.nexusmods.com/oauth/token`, discovery `https://users.nexusmods.com/.well-known/openid-configuration`. Redirect URI `nxm://oauth/callback` (custom scheme) is supported. `[CITED: modding.wiki/en/api/oauth2-guide; CITED: github.com/Nexus-Mods/NexusMods.App/issues/19]`
```rust
// Source: docs.rs/oauth2/5.0 (PKCE auth-code flow) — pattern, adapt names at plan time
use oauth2::{
    basic::BasicClient, AuthUrl, TokenUrl, ClientId, RedirectUrl,
    PkceCodeChallenge, AuthorizationCode, CsrfToken, Scope, TokenResponse,
};
// 1. Build client (PKCE → NO client secret; public client id is shipped).
let client = BasicClient::new(ClientId::new(CLIENT_ID.into()))
    .set_auth_uri(AuthUrl::new("https://users.nexusmods.com/oauth/authorize".into())?)
    .set_token_uri(TokenUrl::new("https://users.nexusmods.com/oauth/token".into())?)
    .set_redirect_uri(RedirectUrl::new("nxm://oauth/callback".into())?);
// 2. Generate the PKCE pair; KEEP the verifier in memory across the round-trip.
let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
let (authorize_url, csrf) = client
    .authorize_url(CsrfToken::new_random)
    .add_scope(Scope::new("public".into())) // [ASSUMED] confirm exact scope at registration
    .set_pkce_challenge(pkce_challenge)
    .url();
// → open `authorize_url` in the system browser; await the nxm://oauth/callback deep link.
// 3. Validate state == csrf, then exchange the code (async, reqwest rustls).
let http = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build()?;
let token = client
    .exchange_code(AuthorizationCode::new(returned_code))
    .set_pkce_verifier(pkce_verifier)
    .request_async(&http).await?;
let access = token.access_token().secret();        // → memory only
let refresh = token.refresh_token().map(|r| r.secret().clone()); // → keyring
```
`[CITED: docs.rs/oauth2; VERIFIED: crates.io oauth2 5.0.0 feature list]`

### Pattern 2: Keyring storage with mandatory no-plaintext hard-fail (NEXUS-02)
**What:** Store the refresh token / API key in the OS Secret Service; if no backend exists, return an error — never write a file.
**When to use:** After login, on every token refresh, and on logout (delete).
```rust
// Source: docs.rs/keyring/3.6 — Entry API
use keyring::{Entry, Error as KeyringError};
const SERVICE: &str = "nextwist";
const USER: &str = "nexusmods-refresh-token";

pub fn store_refresh_token(token: &str) -> Result<(), AuthError> {
    let entry = Entry::new(SERVICE, USER)?;          // may fail if no backend
    entry.set_password(token).map_err(|e| match e {
        // NO Secret Service available → explicit hard-fail, NOT a downgrade to plaintext.
        KeyringError::NoStorageAccess(_) | KeyringError::PlatformFailure(_) =>
            AuthError::NoKeyringBackend,
        other => AuthError::Keyring(other),
    })
}
pub fn clear_refresh_token() -> Result<(), AuthError> {
    match Entry::new(SERVICE, USER)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),  // logout is idempotent
        Err(e) => Err(AuthError::Keyring(e)),
    }
}
```
The shell maps `AuthError::NoKeyringBackend` to the UI-SPEC's red "Can't store your login securely" banner and **disables login**. `[CITED: docs.rs/keyring; VERIFIED: locked decision NEXUS-02]`

### Pattern 3: REST v1 download-link generation
**What:** Generate the actual CDN download URL(s) for a file. Premium users call with no key; free users pass the `key`+`expires` from the `nxm://` link.
**When to use:** Both Premium direct download (NEXUS-03) and free-user redemption (NEXUS-04).
**Endpoint:** `GET https://api.nexusmods.com/v1/games/{game_domain_name}/mods/{mod_id}/files/{file_id}/download_link.json` — Premium: no extra params. Free: append `?key={key}&expires={expires}`. Auth via header (`apikey: <key>` for legacy API key, or `Authorization: Bearer <access_token>` for OAuth). Returns an array of `{ name, short_name, URI }` CDN links. `[CITED: api-docs.nexusmods.com; CITED: github.com/Nexus-Mods/node-nexus-api getDownloadURLs(modId, fileId, key?, expires?, gameId?)]`
```rust
// Premium: GET …/download_link.json
// Free:    GET …/download_link.json?key=<k>&expires=<ts>
// Response: [ { "name": "Nexus CDN", "short_name": "Nexus", "URI": "https://…" }, … ]
```

### Pattern 4: Streaming download with progress (no UI freeze, criterion #4)
**What:** Stream the CDN response body chunk-by-chunk, accumulating bytes and emitting a progress fraction without buffering the whole file.
**When to use:** Every download (NEXUS-03/04/06).
```rust
// Source: reqwest stream feature + futures-util StreamExt — pattern
use futures_util::StreamExt;
let resp = client.get(cdn_uri).send().await?.error_for_status()?;
let total = resp.content_length();           // Option<u64> — Content-Length
let mut downloaded: u64 = 0;
let mut stream = resp.bytes_stream();
let mut file = tokio::fs::File::create(&dest).await?;
while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;
    downloaded += chunk.len() as u64;
    on_progress(downloaded, total);          // callback → shell re-emits a Tauri event
}
```
The `on_progress` callback is a plain `Fn(u64, Option<u64>)` in `crates/nexus` (no Tauri type); the shell wraps it to `window.emit("download://progress", …)`. A `CancellationToken`/`AbortHandle` drives the UI "Cancel" affordance. `[CITED: docs.rs/reqwest stream; VERIFIED: futures-util on crates.io]`

### Pattern 5: nxm:// redemption + Linux deep-link wiring (NXM-01)
**What:** Register `nxm://`, receive the link in the running instance, parse it, and (for free users) redeem its `key`+`expires`.
**nxm:// shape (cross-checked):** `nxm://<game_domain>/mods/<mod_id>/files/<file_id>?key=<key>&expires=<unix_ts>&user_id=<id>`. The `key`+`expires` are **only present for free-user** "Download with Manager" links and are what unlock the v1 `download_link.json` call; Premium in-app downloads skip them. `[CITED: node-nexus-api getDownloadURLs; CITED: api-docs.nexusmods.com; CITED: forums.nexusmods.com]`
**Shell wiring (verified against official Tauri docs):**
```rust
// tauri.conf.json
// "plugins": { "deep-link": { "desktop": { "schemes": ["nxm"] } } }

// src-tauri: single-instance FIRST (with deep-link feature), then deep-link.
tauri::Builder::default()
  .plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {
      // with the `deep-link` feature, the forwarded URL is routed to on_open_url automatically
  }))
  .plugin(tauri_plugin_deep_link::init())
  .setup(|app| {
      #[cfg(any(windows, target_os = "linux"))]
      { use tauri_plugin_deep_link::DeepLinkExt; app.deep_link().register_all()?; } // dev-mode reg
      app.deep_link().on_open_url(|event| {
          for url in event.urls() { /* if scheme==nxm/oauth/callback → OAuth; else → start download */ }
      });
      Ok(())
  })
```
**Capabilities:** add `"deep-link:default"` (+ `"core:event:default"`) to the capability file. **Linux runtime needs `xdg-mime` + `update-desktop-database`** on PATH for `register_all()`. `[CITED: v2.tauri.app/plugin/deep-linking; CITED: plugins-workspace deep-link/single-instance READMEs]`

### Pattern 6: Rate limiting — proactive bucket + reactive headers (NEXUS-05)
**What:** A `governor` direct rate limiter sized to Nexus's budget, plus reading `X-RL-*` response headers to back off before hitting a 429.
**Headers (names cross-checked):** `X-RL-Hourly-Limit`, `X-RL-Hourly-Remaining`, `X-RL-Hourly-Reset`, `X-RL-Daily-Limit`, `X-RL-Daily-Remaining`, `X-RL-Daily-Reset`. Budget ≈ 2,500/day + 100/hour for API-key users historically; recovery ≈ +1/sec. The UI shows the Warning notice when backing off. `[CITED: node-nexus-api getRateLimits; ASSUMED exact header casing — confirm against a live response]`
```rust
// Source: docs.rs/governor — direct limiter
use governor::{Quota, RateLimiter};
use std::num::NonZeroU32;
let quota = Quota::per_hour(NonZeroU32::new(100).unwrap()); // tune to live limits
let limiter = RateLimiter::direct(quota);
limiter.until_ready().await;     // proactive gate before each request
// after each response: if X-RL-Hourly-Remaining low or 429 → sleep until X-RL-Hourly-Reset.
```

### Pattern 7: Reuse the extract→staging pipeline verbatim (NEXUS-06)
**What:** Hand the downloaded archive to the exact function the local-archive install path uses.
```rust
// Identical to src-tauri/src/commands/mods.rs install_archive:
let staged = extract::install_archive(&downloaded_archive, &game.staging_dir)?; // UNCHANGED
// then: store.add_mod(...) + persist Nexus provenance (V4 migration) → ManagedMod
```
A Nexus mod is indistinguishable from a local-archive mod once staged; the deploy/purge engine is untouched. `[VERIFIED: crates/extract/src/staging.rs install_archive signature; VERIFIED: commands/mods.rs]`

### Anti-Patterns to Avoid
- **Pulling reqwest/oauth2/keyring into `crates/nexus` *and* making it depend on Tauri.** `crates/nexus` must stay headless; keyring + deep-link live in the shell.
- **Buffering the full download into memory** (`resp.bytes().await`) — defeats progress and risks OOM on multi-GB texture packs. Stream it.
- **Reacting only to HTTP 429.** The locked decision requires *proactive* `X-RL-*` header reading + a token bucket, not just catching the ban.
- **Writing the refresh token to a config file when no keyring exists.** Hard invariant NEXUS-02: hard-fail instead.
- **Registering `deep-link` before `single-instance`.** Single-instance must be first or the forwarded URL is lost on Linux.
- **Assuming a `V3` migration.** `V3__profile_fks.sql` already exists — the Nexus migration is **V4**.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| OAuth2 PKCE (S256) + state/CSRF | Custom code_verifier/challenge + base64url + state validation | `oauth2` 5.0 | Timing-safe secrets, correct S256 encoding, CSRF state — subtle to get right and a security surface. |
| Secret storage | Encrypting a token to a dotfile | `keyring` 3.6 Secret Service | DBus Secret Service integration, KWallet/GNOME differences; the no-plaintext invariant *is* the point. |
| Rate limiting | Hand-rolled sleep/counter | `governor` direct limiter | GCRA token bucket with correct burst/recovery; battle-tested. |
| `nxm://` registration on Linux | Hand-writing `.desktop` + `xdg-mime` calls | `tauri-plugin-deep-link` `register_all()` | Handles xdg-mime + update-desktop-database + the absolute-path AppImage caveat. |
| Single-instance forwarding | A pidfile + IPC socket | `tauri-plugin-single-instance` (`deep-link` feature) | Cross-platform second-instance argv forwarding wired straight into `on_open_url`. |
| Streaming download | Manual chunked read of a socket | `reqwest` `stream` + `futures-util` | Content-Length, redirects, rustls, gzip — already solved. |
| Mocking the Nexus API in tests | A bespoke local TCP server | `mockito` | Stands up a real local HTTP server with stubbed routes per test. |

**Key insight:** Every genuine risk in this phase (PKCE correctness, secret storage, rate-limit bans, Linux MIME registration) has a mature crate. The phase's real work is *orchestration and the headless/shell boundary*, not protocol implementation.

## Common Pitfalls

### Pitfall 1: Pinning `keyring` 4.x
**What goes wrong:** `keyring` 4.0 (2026-04-26) is a major rewrite — the cross-platform API moved to a separate `keyring-core` crate, credential stores are now separate crates, and clients must explicitly `set_default_store` at startup. The maintainers say "do not update v3→v4." Picking 4.x means a different, sparsely-documented API and churn risk.
**Why it happens:** `cargo add keyring` resolves the latest (4.1.2, published the same day as this research).
**How to avoid:** Pin **`keyring = "3.6"`** in the workspace. `[VERIFIED: crates.io version history; CITED: keyring-rs README]`
**Warning signs:** Compile errors about a missing default store, or docs referencing `keyring-core`.

### Pitfall 2: NexusMods API in flux (v1 → v2)
**What goes wrong:** Assuming GraphQL v2 covers downloads, or that v1 will be removed. v2 does **not** generate download links; v1 `download_link.json` is the only path and Nexus has said v1 stays "for the foreseeable future."
**How to avoid:** Hybrid client — v1 for download links + file metadata, v2 for richer mod metadata. Centralize base URLs so a future migration is one change. `[CITED: forums.nexusmods.com GraphQL examples; CITED: graphql.nexusmods.com]`
**Warning signs:** A `download_link` GraphQL field that 403s or doesn't exist.

### Pitfall 3: OAuth client registration is a real-world blocker (STATE blocker)
**What goes wrong:** The PKCE flow needs a registered `client_id` and an allowed redirect URI (`nxm://oauth/callback`). Registration is gated by the Nexus Acceptable Use Policy and is **not** self-service in the public docs — it may require contacting Nexus.
**How to avoid:** Ship the **manual API-key paste fallback** (locked decision) so login works *today* while registration is pending; treat registration as a release task. The `client_id` is the only unverifiable piece here. `[ASSUMED — needs a real registration; CITED: github.com/Nexus-Mods/NexusMods.App/issues/19]`
**Warning signs:** `invalid_client` / `unauthorized_client` on the authorize call; redirect-URI-mismatch errors.

### Pitfall 4: `nxm://` works installed but not in `cargo tauri dev`
**What goes wrong:** Deep links register only when the app is "installed" by default; in dev the `nxm://` scheme points at nothing.
**How to avoid:** Call `app.deep_link().register_all()` in `setup()` (dev-mode registration), and ensure `xdg-mime` + `update-desktop-database` are installed. **AppImage** MIME registration is explicitly Phase 5 — this phase only proves dev/installed-runtime. `[CITED: v2.tauri.app/plugin/deep-linking]`
**Warning signs:** Browser says "no application registered for nxm"; `on_open_url` never fires.

### Pitfall 5: Free-user redemption differs from Premium and can't be unit-tested with a key
**What goes wrong:** The `key`+`expires` in an `nxm://` link are single-use and short-lived; you can't bake a real one into a CI test, and the free-user path behaves differently from Premium (which omits them).
**How to avoid:** Unit-test both shapes against `mockito` (assert the `?key=&expires=` query is/ isn't present); reserve the live free-user flow for **manual UAT on a real non-Premium account** (STATE blocker). `[VERIFIED: locked decision + STATE blocker]`
**Warning signs:** A test that depends on a hard-coded key — it will rot immediately.

### Pitfall 6: Mixing the new async reqwest with loadorder's blocking client
**What goes wrong:** `crates/loadorder` uses `reqwest::blocking`; calling a blocking client inside the tokio runtime panics. The Nexus client must be fully async.
**How to avoid:** `crates/nexus` uses `reqwest::Client` (async) only; the two clients never share a call path. The shell already runs tokio (`rt-multi-thread`). `[VERIFIED: crates/loadorder/src/masterlist.rs uses reqwest::blocking]`

## Code Examples

(See Patterns 1–7 above — each carries a verified or cited source snippet. Consolidated operations:)

### Account / user info (tier: Premium vs Free) — drives the UI tier tag
```text
REST v1:  GET https://api.nexusmods.com/v1/users/validate.json   (Authorization/apikey header)
          → { "user_id", "key", "name", "is_premium", "is_supporter", … }
GraphQL v2 alternative: a `me`/`user` query at https://api.nexusmods.com/v2/graphql
```
`is_premium` decides whether the UI offers in-app direct download or the website-button hint. `[CITED: api-docs.nexusmods.com /v1/users/validate.json; ASSUMED v2 field name]`

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Legacy API-key-only auth | OAuth2 + PKCE (key paste as fallback) | ~2023 onward | OAuth is the forward path; key paste still works and unblocks v1. |
| REST v1 for everything | GraphQL v2 for metadata, v1 for downloads | v2 GA ~2023–2025 | Hybrid client; downloads stay v1. |
| `keyring` 3.x monolith | `keyring` 4.x + `keyring-core` split | 4.0.0 = 2026-04-26 | **Stay on 3.6** for v1; 4.x is a breaking rewrite. |
| websocket SSO (MO2/Vortex) | OAuth2+PKCE | — | Deferred for NexTwist; OAuth + key paste suffice. |

**Deprecated/outdated:**
- Don't assume v1 endpoints vanish soon, but don't build *new* metadata reads on v1 if v2 covers them.

## Runtime State Inventory

> Not a rename/refactor phase — greenfield feature addition. Skipped except for one note: the new `V4__nexus_provenance.sql` migration is **additive** (new table/columns), consistent with the established additive-migration rule; it does not alter Phase-1/2 safety tables. No stored data, OS-registered state, or build artifacts carry a renamed string. **The one OS-registered state introduced** is the `nxm://` MIME handler (`.desktop` + `xdg-mime`), registered at runtime by `register_all()` and finalized for the AppImage in Phase 5.

## Common Pitfalls — quick verification checklist (for the planner's verification steps)
- [ ] `crates/nexus` has **no** `tauri`/`keyring`/`tauri-plugin-*` dependency (grep the crate's Cargo.toml).
- [ ] `reqwest` in `crates/nexus` is `default-features = false` + `rustls` (never `native-tls`/`default-tls`) — `cargo deny` would also catch OpenSSL.
- [ ] Migration file is `V4__*.sql`, not `V3`.
- [ ] No-keyring path returns `Err`, asserted by a test that simulates `NoStorageAccess`.
- [ ] `single-instance` plugin registered before `deep-link`.
- [ ] Download path streams (no `resp.bytes().await` on the file body).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Secret Service backend (GNOME Keyring / KWallet) | NEXUS-02 keyring storage | unknown on CI; user-machine-dependent | — | **None by design** — hard-fail with UI banner (locked decision). CI tests use a mock/`mock-keyring` feature or run the keyring path behind `#[ignore]`. |
| `xdg-mime` | `nxm://` `register_all()` (NXM-01) | typically present on desktop Linux | — | Deep-link registration fails loudly; dev can register the `.desktop` manually. |
| `update-desktop-database` | `nxm://` registration | typically present (`desktop-file-utils`) | — | As above. |
| WebKitGTK 4.1 dev libs | building `src-tauri` (existing) | per CI apt list | — | Headless `crates/nexus` needs none. |
| A real NexusMods account (Premium + non-Premium) | manual UAT of download paths | manual only | — | No automated substitute for the live download/redeem paths. |

**Missing dependencies with no fallback:** Secret Service is intentionally non-fallback (NEXUS-02). For CI, the keyring write/read test must be guarded (mock or `#[ignore]`) so headless CI without a DBus session does not falsely fail — the *hard-fail behavior itself* is the asserted unit test, using a simulated `NoStorageAccess`.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` / `#[tokio::test]` (workspace standard; no external runner) |
| Config file | none — `cargo test --workspace --locked` (CLAUDE.md) |
| Quick run command | `cargo test -p nextwist-nexus` |
| Full suite command | `cargo test --workspace --locked` |
| HTTP mock (new) | `mockito` 1.7 dev-dependency in `crates/nexus` (Wave 0 gap) |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| NEXUS-01 | PKCE authorize URL contains `code_challenge`+`S256`+`state`; code-exchange request shape correct | unit (mockito) | `cargo test -p nextwist-nexus auth` | ❌ Wave 0 |
| NEXUS-02 | No-backend → `Err(NoKeyringBackend)`, never writes a file; logout deletes entry idempotently | unit (simulated `NoStorageAccess`) | `cargo test -p nextwist auth_keyring` (in shell lib) | ❌ Wave 0 |
| NEXUS-03 | Premium `download_link.json` request omits `key`/`expires`; parses CDN URI array | unit (mockito) | `cargo test -p nextwist-nexus download_link_premium` | ❌ Wave 0 |
| NEXUS-04 | Free-user request **includes** `?key=&expires=`; expired-key error surfaces a redeem error (not a download-failed row) | unit (mockito) | `cargo test -p nextwist-nexus download_link_free` | ❌ Wave 0 |
| NEXUS-05 | Limiter gates before requests; low `X-RL-Hourly-Remaining` triggers backoff; 429 honored | unit (mockito header stub) | `cargo test -p nextwist-nexus ratelimit` | ❌ Wave 0 |
| NEXUS-06 | Downloaded archive flows through `extract::install_archive` and becomes a `ManagedMod` with persisted Nexus provenance | unit/integration (temp DB + temp staging, real `extract`) | `cargo test -p nextwist-nexus stage` + `cargo test -p nextwist-store nexus_provenance` | ❌ Wave 0 |
| NXM-01 | `nxm://` URL parses to `{domain, mod_id, file_id, key?, expires?}`; OAuth-callback vs download routing | unit (parser) | `cargo test -p nextwist-nexus nxm_parse` | ❌ Wave 0 |

**Manual-only (real-account UAT — no automated substitute):**
- NEXUS-01 live OAuth round-trip (real browser + `nxm://oauth/callback`).
- NEXUS-03 live Premium in-app download of a real mod.
- **NEXUS-04 / NXM-01 live free (non-Premium) account** "Mod Manager Download" → `nxm://` redemption — **STATE blocker; requires a real non-Premium account.**
- NEXUS-02 behavior on a machine with no Secret Service (and one with GNOME Keyring + one with KWallet).

### Sampling Rate
- **Per task commit:** `cargo test -p <crate-touched>` (e.g. `-p nextwist-nexus`) + `cargo clippy -p <crate> -- -D warnings`.
- **Per wave merge:** `cargo test --workspace --locked` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo deny check`.
- **Phase gate:** full suite green + the manual-UAT items above signed off before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] Add `mockito = "1.7"` as a `crates/nexus` dev-dependency (no HTTP mock exists in the repo).
- [ ] `crates/nexus/tests/client_mock.rs` — download_link (premium/free), rate-limit headers, error/expired-key paths.
- [ ] A keyring unit test in the shell lib that simulates `NoStorageAccess` and asserts the hard-fail (CI-safe, no real DBus).
- [ ] `crates/store` test for the V4 Nexus-provenance round-trip.
- [ ] An `nxm://` parser unit test (no network).

## Security Domain

> `security_enforcement: true`, ASVS Level 1. This phase introduces **auth + secret storage + external HTTP** — the highest-security surface in the project so far.

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes | OAuth2 Authorization-Code + **PKCE (S256)**; public client (no secret); state/CSRF validated; `oauth2` crate. |
| V3 Session Management | yes | Refresh token in keyring; short-lived access token in memory only; logout clears both. |
| V4 Access Control | partial | Premium vs free gates which download path is offered (not a privilege boundary in-app, but drives behavior). |
| V5 Input Validation | yes | Validate/parse `nxm://` URLs strictly (reject malformed scheme/host/path; validate numeric ids); the existing `extract` zip-slip defense covers archive contents. |
| V6 Cryptography | yes (delegated) | **Never hand-roll**: TLS via `rustls`; PKCE/JWT via `oauth2`; secret-at-rest via OS keyring. `crypto-rust` keyring feature (no OpenSSL). |
| V7 Error Handling/Logging | yes | `tracing` must **never log tokens/keys/`code_verifier`**; redact secrets; surface backend error strings verbatim to the user only as the UI-SPEC dictates. |
| V9 Communications | yes | `rustls` only, HTTPS enforced; disable redirect-following on the token/download_link calls (mirrors loadorder's `Policy::none()`), validate `error_for_status`. |

### Known Threat Patterns for Rust+Tauri+OAuth+nxm://
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Token theft from disk | Information Disclosure | Keyring-only storage; **hard-fail, no plaintext fallback** (NEXUS-02). |
| Secrets in logs | Information Disclosure | Redact `access_token`/`refresh_token`/`apikey`/`code_verifier`/`key`/`expires` from all `tracing` output. |
| Malicious / spoofed `nxm://` link | Tampering / Spoofing | Strict URL parsing; numeric-id validation; never execute or shell out on link content; download still routes through `extract` zip-slip defense. |
| OAuth CSRF / code interception | Spoofing | Validate `state == csrf`; PKCE `code_verifier` binds the code to this client; no client secret to leak. |
| Open-redirect / SSRF via CDN URI | Tampering | Only follow `download_link.json` `URI`s over HTTPS from Nexus's response; disable arbitrary redirect chaining. |
| Path traversal in downloaded archive | Tampering | Already mitigated by `crates/extract` (zip-slip/symlink defense) — reuse unchanged. |
| Rate-limit ban (availability) | Denial of Service (self-inflicted) | `governor` + `X-RL-*` backoff (NEXUS-05). |
| `client_secret` leakage | Information Disclosure | None to leak — PKCE public client ships only a `client_id`. |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | OAuth scope string is `"public"` (or similar); exact scope set unconfirmed | Pattern 1 | Authorize call rejected; adjust scope after registration. LOW-MEDIUM. |
| A2 | OAuth `client_id` registration requires contacting Nexus (not self-service) | Pitfall 3 | Could be self-service after all; either way the API-key fallback unblocks. MEDIUM (STATE blocker). |
| A3 | `X-RL-*` header exact casing/names (`X-RL-Hourly-Remaining`, etc.) | Pattern 6 | Backoff reads wrong header → no proactive backoff (429 still caught). LOW. |
| A4 | Daily/hourly quota numbers (~2500/day, ~100/hr; +1/sec) | Pattern 6 | Limiter mis-sized; reactive headers still protect. LOW. |
| A5 | `nxm://` query includes `user_id` alongside `key`+`expires` | Pattern 5 | Extra param ignored if absent; `key`+`expires` are the load-bearing ones. LOW. |
| A6 | GraphQL v2 `me`/user field name for premium status | Code Examples | Use v1 `/users/validate.json` instead (confirmed). LOW. |
| A7 | `tauri-plugin-opener` is the browser-launch crate to use | Standard Stack | Swap for `webbrowser`/`open` if needed; trivial. LOW. |
| A8 | OAuth access-token lifetime warrants a refresh path this phase | Pattern 1/2 | If tokens are long-lived, refresh logic is simpler; refresh-token-in-keyring still correct. LOW. |

**These `[ASSUMED]` items (esp. A1, A2) should be confirmed during planning or flagged as a `checkpoint:human-verify` against the live API / a real registration before the OAuth path is locked.**

## Open Questions

1. **Exact OAuth `client_id` registration path under the Acceptable Use Policy.**
   - What we know: PKCE flow needs a registered public `client_id` + allowed `nxm://oauth/callback` redirect; NexusMods.App issue #19 confirms OAuth exists.
   - What's unclear: Whether registration is self-service or requires contacting Nexus, and the exact scope set.
   - Recommendation: Treat as a release task (STATE blocker); ship API-key paste so the phase isn't blocked. Add a `checkpoint:human-verify` before locking the OAuth `client_id`.

2. **Whether any download-link generation has moved to GraphQL v2.**
   - What we know: As of this research, download links are v1 `download_link.json`; v2 is metadata-focused and some v2 endpoints need an OAuth token.
   - What's unclear: Future v2 coverage.
   - Recommendation: Build v1 for downloads now; centralize base URLs for a one-line future swap.

3. **CI strategy for the keyring path without a DBus session.**
   - Recommendation: Assert the *hard-fail* via a simulated `NoStorageAccess`; gate any real read/write test behind a feature flag or `#[ignore]`.

## Sources

### Primary (HIGH confidence)
- crates.io registry (api/v1/crates/*) — verified max-stable versions + publish dates + legitimacy: `oauth2` 5.0.0, `keyring` 3.6.3 / 4.1.2, `governor` 0.10.4, `tauri-plugin-deep-link` 2.4.9, `tauri-plugin-single-instance` 2.4.2, `futures-util` 0.3.32, `mockito` 1.7.2, `wiremock` 0.6.5 (2026-06-21).
- Local codebase — `Cargo.toml` (workspace deps), `crates/extract/src/staging.rs`, `src-tauri/src/{lib.rs,commands/mods.rs}`, `crates/store/src/migrations/` (V3 already exists → Nexus = V4), `crates/loadorder/src/masterlist.rs` (blocking reqwest precedent).
- v2.tauri.app/plugin/deep-linking + tauri-apps/plugins-workspace deep-link & single-instance READMEs — `schemes` config, `register_all()`, `on_open_url`, single-instance-first + `deep-link` feature, capabilities.

### Secondary (MEDIUM confidence)
- modding.wiki/en/api/oauth2-guide — OAuth2+PKCE endpoints (`users.nexusmods.com/oauth/authorize`,`/oauth/token`,`/.well-known/openid-configuration`), `nxm://oauth/callback`.
- github.com/Nexus-Mods/node-nexus-api (docs) — `getDownloadURLs(modId, fileId, key?, expires?, gameId?)`, `getRateLimits()`, `createWithOAuth`, `setKey`.
- api-docs.nexusmods.com — `download_link.json` path; `/v1/users/validate.json`.
- github.com/Nexus-Mods/NexusMods.App/issues/19 — OAuth2 for Nexus.
- graphql.nexusmods.com + forums.nexusmods.com/GraphQL examples — v2 metadata vs v1 downloads; v2 needs OAuth for some endpoints.
- docs.rs/oauth2/5.0, docs.rs/keyring/3.6, docs.rs/governor — crate APIs.

### Tertiary (LOW confidence)
- WebSearch summaries for `nxm://` parameter shape and `X-RL-*` header casing — cross-checked but exact strings need a live-response confirmation (A3, A5).
- keyring-rs README v4 migration note (WebSearch-relayed) — basis for the 3.6 pin (Pitfall 1).

## Metadata

**Confidence breakdown:**
- Standard stack / versions: **HIGH** — every crate registry-verified + legitimacy-audited on 2026-06-21.
- Architecture (headless/shell split, deep-link wiring, streaming): **HIGH** — verified against official Tauri docs + the existing codebase boundary.
- API endpoints (download_link.json v1, OAuth endpoints): **MEDIUM** — cross-checked against Nexus's own clients (node-nexus-api) + modding.wiki, not a live call this session.
- OAuth client registration + exact scope/header strings: **LOW** — needs a real registration / live response (A1–A6); API-key fallback de-risks this.

**Research date:** 2026-06-21
**Valid until:** ~2026-07-21 for crate versions (keyring 4.x is moving fast — re-verify the 3.6 pin); NexusMods API surface is "in flux" → re-verify download_link.json + GraphQL coverage at plan time if more than ~2 weeks elapse.
