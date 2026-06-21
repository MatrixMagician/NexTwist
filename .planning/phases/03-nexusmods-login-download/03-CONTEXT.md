# Phase 3: NexusMods Login & Download - Context

**Gathered:** 2026-06-21
**Status:** Ready for planning
**Mode:** Smart discuss (autonomous) — 16 decisions across 4 areas, all recommended answers accepted

<domain>
## Phase Boundary

This phase adds **NexusMods account integration and mod acquisition** on top of the proven Phase-1/2 safe deployment engine. A user can:

1. **Log into their NexusMods account via OAuth2 + PKCE**, with the long-lived credential stored only in the system keyring (never plaintext) — NEXUS-01, NEXUS-02.
2. **Download mods straight into NexTwist's staging store**: Premium users via in-app direct download (API-generated link, streamed with progress), free users via the website **"Mod Manager Download"** `nxm://` handoff — NEXUS-03, NEXUS-04.
3. **One-click install from an `nxm://` link** on nexusmods.com, routed to the running app by a registered Linux deep-link handler, with a second invocation forwarded to the live instance — NXM-01.
4. **Have a downloaded mod auto-extracted into staging**, ready to deploy through the existing safe engine, while NexTwist **respects NexusMods API rate limits** with no UI freeze on large downloads — NEXUS-05, NEXUS-06.

**Requirements covered:** NEXUS-01..06, NXM-01 (7 requirements).

**Explicitly out of scope for this phase:** FOMOD guided installers + Collections (Phase 4); AppImage packaging + nxm:// MIME registration *in the distributed build* + license audit (Phase 5 — this phase registers the handler in dev/runtime, Phase 5 makes it work from the AppImage); advanced download manager — queue, pause/resume, bandwidth limits (NEXV2-02, deferred); mod-update notifications/version tracking (NEXV2-01, deferred); non-NexusMods sources (out of scope for v1).

</domain>

<decisions>
## Implementation Decisions

### Authentication (NEXUS-01, NEXUS-02)
- **Primary login is OAuth2 + Authorization-Code + PKCE (S256)** via the `nxm://oauth/callback` redirect captured by `tauri-plugin-deep-link` — matches success-criterion #1 and the CLAUDE.md forward-looking recommendation (`oauth2` crate 5.x).
- **Manual API-key paste is shipped as a last-resort v1 fallback** so a user can authenticate even while NexusMods OAuth-client registration is still pending (carries the STATE blocker: "Register app under Nexus Acceptable Use Policy early"). Websocket SSO was considered and deferred — OAuth2 + key-paste covers both the forward path and the works-today path without a third auth implementation.
- **Tokens live in the system keyring** (Secret Service via `keyring` 3.x): the long-lived **refresh token / API key is stored in the keyring**, the short-lived OAuth **access token is kept in memory only**. If no Secret Service / keyring backend is available, NexTwist **hard-fails with a clear error rather than writing plaintext** — NEXUS-02 is a hard invariant.
- **Logout clears the keyring entry and the in-memory token.**
- **The NexusMods API client lives in a new headless `crates/nexus` crate** (async reqwest, speaks `core` types, **zero Tauri deps**, consistent with the engine boundary). The **Tauri shell owns the OS-integration bits**: keyring storage, deep-link registration, OAuth-redirect capture, single-instance forwarding — and passes tokens into the headless client.

### Download Flow & nxm:// Handoff (NEXUS-03, NEXUS-04, NXM-01)
- **Premium direct download**: call the Nexus API to generate a download link, then **stream the file with reqwest** emitting progress events to the UI (success-criterion #2 / #4).
- **Free-user flow is the `nxm://` handoff**: NexTwist registers the `nxm://` MIME handler; the website **"Mod Manager Download"** button hands the app a keyed link (`nxm://<game>/mods/<id>/files/<id>?key=...&expires=...`) which the app **redeems** for the actual download (NEXUS-04, NXM-01).
- **Routing a browser click to the app uses `tauri-plugin-deep-link` + `tauri-plugin-single-instance`** — a second `nxm://` invocation while the app is open is **forwarded to the live instance** (never spawns a duplicate).
- **Download-manager scope for v1 is intentionally minimal**: a small concurrency cap, **per-item progress, no pause/resume** (queue/pause/resume/bandwidth-limits are explicitly NEXV2-02, deferred).

### API Surface & Rate Limiting (NEXUS-05)
- **Hybrid API behind one client module**: prefer **GraphQL v2** where available, fall back to **REST v1** where v2 lacks coverage (download-link generation and file metadata are historically v1). Plan-phase research **verifies per-endpoint** against api.nexusmods.com / graphql.nexusmods.com (carries the STATE blocker on the v1→v2 migration).
- **Rate limiting uses a `governor` token-bucket limiter and honors the `X-RL-*` response headers** (`X-RL-Hourly-Remaining`, `X-RL-Daily-Remaining`, reset) with backoff so NexTwist never gets throttled or banned (NEXUS-05). Nexus quota: ~300 req (600 premium), +1/sec recovery.
- **The app ships its public OAuth client ID** (PKCE flow → no client secret to protect); registering the app under the Nexus Acceptable Use Policy is a **release task**, not per-user.
- **The Nexus client is async reqwest** (`rustls` only, never native-tls) so downloads stream off the UI thread (success-criterion #4 "no UI freeze"). `crates/loadorder`'s existing **blocking** reqwest client stays as-is — the two coexist; the new async usage lives in `crates/nexus` / the tokio-backed shell.

### Staging Integration & UX Surface (NEXUS-06)
- **A downloaded archive flows through the exact same Phase-1 `extract`→staging pipeline** already proven for local archives (the `commands/mods.rs` install path). A Nexus mod becomes an ordinary staged `ManagedMod`, immediately deployable by the safe engine — NEXUS-06, and reuse over a parallel Nexus-only path.
- **Nexus provenance is persisted** (a **V3 refinery migration** / new columns or table): NexusMods **mod id, file id, version, display name** are recorded against the managed mod, so mods are traceable and Phase-4 Collections + future update-checks (NEXV2-01) can rely on it.
- **Login UI is a minimal account panel**: logged-in state + login / logout button, **functional-minimal Svelte 5** consistent with the Phase-1/2 UI altitude. (Detailed visual contract comes from the UI-SPEC step.)
- **Download UX is a simple downloads list with per-item progress bars**, driven by async progress events so the UI never freezes (success-criterion #4).

### Claude's Discretion
- Exact crate/module split inside `crates/nexus` (auth, client, download, model), the precise V3 schema shape, the specific GraphQL queries vs REST endpoints per datum, and progress-event payload shapes are at Claude's discretion, to be settled by plan-phase research against the live API.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`crates/extract/`** — the untrusted-archive → validated staging-tree transform (zip/7z/shell-RAR, zip-slip/symlink defense). Phase 3 routes every downloaded archive through this unchanged (NEXUS-06).
- **`src-tauri/src/commands/mods.rs`** — the existing "install a local archive into staging" command path. The Nexus download path terminates here: download → hand the archive to the same extract→stage flow.
- **`crates/store/`** — rusqlite (bundled, pinned **0.39**) + **refinery** versioned migrations (`src/migrations/V*.sql`, currently through V2). Phase 3 adds a **V3 migration** for Nexus source metadata + (optionally) a downloads table. Hard invariant preserved: **no `rusqlite` type in the public API**.
- **`crates/core/src/model.rs`** — stable vocabulary (`ManagedMod { id, name, staging_root, enabled }`, `Game`, etc.). Phase 3 adds Nexus source metadata to the `core` types (additive — shapes are a contract).
- **`reqwest` is already a workspace dependency** (`0.13`, `rustls`/`http2`/`charset`/**`blocking`**), added in Phase 2 for the LOOT masterlist fetch. Phase 3 **adds the `json` + `stream` features** for the async API client and streaming downloads (keep `rustls` only — never native-tls, per CLAUDE.md AppImage rule).
- **Tauri command pattern** — thin 3–10 line adapters in `src-tauri/src/commands/{games,mods,deploy,conflicts,plugins,profiles}.rs` that lock `AppState` and delegate to engine crates. Phase 3 adds `commands/{nexus,downloads}.rs`-style adapters; **no real logic in adapters**.

### Established Patterns
- **Headless safety/engine core in `crates/*` with ZERO Tauri deps**; `src-tauri` is a thin adapter. `crates/nexus` follows this — async API client, no Tauri; OS-integration (keyring, deep-link, OAuth redirect, single-instance) lives in the shell.
- **`thiserror` enums in engine crates, `anyhow` only at the app boundary**; `tracing` for logs. `crates/nexus` gets its own `error.rs` enum.
- **`reqwest` uses `rustls` only** — load-bearing for a self-contained AppImage; `cargo-deny` gates licenses/sources.
- **Frontend is SvelteKit (Svelte 5) static SPA** (`frontend/src/`, currently a single `+page.svelte` + `lib/api.ts`), embedded via `frontendDist`. Phase 3 adds the account panel + downloads list views and `nxm://` event wiring. (UI hint: yes → a UI-SPEC is generated before planning.)
- **New Tauri plugins on the matching 2.x line** — `tauri-plugin-deep-link` 2.4.x and `tauri-plugin-single-instance` 2.x (keep majors aligned with `tauri` 2.11 to preserve the IPC/permissions contract).

### Integration Points
- New crate: **`crates/nexus`** (auth token exchange, API client, download/stream, rate limiter, Nexus model types) — workspace alias, `core` types in / out.
- New persistence: **`crates/store/src/migrations/V3__*.sql`** + a query module for Nexus source metadata; new `core` model fields.
- New shell wiring: `src-tauri` gains **keyring**, **`tauri-plugin-deep-link`** (`nxm://` + `nxm://oauth/callback`), **`tauri-plugin-single-instance`**, the OAuth2+PKCE flow, and `commands/{nexus,downloads}` adapters; `lib.rs` registers the new plugins + deep-link handler **before the UI is served** (alongside the existing `recover_on_launch`).
- New frontend: account/login panel + downloads list in `frontend/src/`, listening for download-progress + nxm:// deep-link events.

</code_context>

<specifics>
## Specific Ideas

- **The safe-engine boundary is untouched** — Phase 3 only *feeds* the staging store; deploy/purge-to-pristine stays exactly as proven in Phases 1-2. A Nexus mod and a local-archive mod are indistinguishable once staged.
- **NEXUS-02 (no plaintext tokens) is a hard, testable invariant** — keyring-only storage; absence of a keyring backend is an error state, not a downgrade-to-file. This is a key security-review item for the phase.
- **The free-user `nxm://` flow MUST be confirmed against a real non-Premium account** (STATE blocker) — the keyed website-button handoff differs from the Premium API-link path and is a manual-UAT item analogous to prior in-game UAT items.
- **API surface is in flux (v1 REST → GraphQL v2)** — do not assume v1 endpoints are permanent; verify per-endpoint at plan time. Download-link generation in particular is historically v1.
- **Rate-limit headers (`X-RL-*`) must be read and respected**, not just reacted-to on 429 — proactive token-bucket + header-driven backoff (NEXUS-05) to avoid bans during Collection-scale downloads in Phase 4.
- **`nxm://` from the AppImage is a Phase-5 concern** — this phase proves the handler in dev/installed-runtime; the AppImage MIME `.desktop` registration is finalized during distribution (DIST/Phase 5).

</specifics>

<deferred>
## Deferred Ideas

- **Websocket SSO login path** (the wss://sso.nexusmods.com flow MO2/Vortex use) — not built for v1; OAuth2+PKCE + API-key paste cover the need. Revisit if OAuth registration proves blocking.
- **Advanced download manager** — queue, pause/resume, bandwidth limits (NEXV2-02). v1 is minimal per-item progress.
- **Mod-update notifications / version tracking** (NEXV2-01) — the V3 metadata makes this possible later, but it is not built now.
- **Encrypted-file token fallback** for systems without a Secret Service — deferred; v1 hard-fails to protect the no-plaintext invariant.
- **Collections / FOMOD consumption of the Nexus client** — Phase 4 builds on this phase's client + metadata; not in scope here.

</deferred>
</content>
</invoke>
