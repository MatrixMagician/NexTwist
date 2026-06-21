---
phase: 03-nexusmods-login-download
plan: 02
subsystem: nexus-download
tags: [nexusmods, reqwest, rustls, governor, streaming, sqlite, migration, mockito, svelte, tauri]

# Dependency graph
requires:
  - phase: 03-nexusmods-login-download
    plan: 01
    provides: "headless crates/nexus spine (NexusError, UserInfo/OAuthTokens, auth.rs api-key/oauth), shell keyring (load_refresh_token), AppState auth state, account panel"
provides:
  - "Hybrid NexusClient: REST v1 download_link.json (premium omits key/expires; free appends both) + GraphQL v2 mod_file_metadata; injectable base URL"
  - "ratelimit::RateLimiter — governor direct token-bucket (proactive until_ready gate) + reactive X-RL-* header backoff; 429 -> RateLimited(reset)"
  - "download::download_to — streaming bytes_stream() download with a Tauri-free Fn(u64,Option<u64>) progress callback + CancelFlag cancellation; NexusClient::download convenience"
  - "V4__nexus_provenance.sql (additive nexus_source table, FK CASCADE on managed_mod) + store::{add_nexus_source,get_nexus_source} facade (no rusqlite in public API)"
  - "core::NexusSource DTO (additive)"
  - "commands/downloads.rs (start_download/cancel_download) — stream->event->extract::install_archive(verbatim)->add_mod->add_nexus_source; window.emit is the only Tauri-type touch point"
  - "Frontend downloads list (UI-SPEC §B): five row states, per-item progress, rate-limit + expired-link warnings, empty state; listen('download://progress') bridge"
affects: [03-03-nxm-handoff, download-flow, deploy-flow]

# Tech tracking
tech-stack:
  added:
    - "tokio (non-dev, fs+io-util+time) added to crates/nexus for streaming file I/O"
    - "mockito + zip + tokio(macros/rt) added to src-tauri dev-deps for the NEXUS-06 end-to-end test"
  patterns:
    - "Hybrid v1-download-link / v2-metadata NexusMods client with injectable base URL (mockito-testable)"
    - "Proactive governor bucket + reactive X-RL-* backoff deadline behind a Mutex; header names + budget centralised as consts"
    - "Streaming download via bytes_stream() to tokio::fs::File with a Tauri-free progress callback (no full-buffer); cooperative CancelFlag checked per chunk"
    - "Additive Vn migration (V4) + no-rusqlite-in-API store facade with idempotent upsert + FK CASCADE"
    - "NexusClient::download keeps reqwest out of src-tauri (headless crate owns all HTTP)"

key-files:
  created:
    - crates/nexus/tests/client_mock.rs
    - crates/store/src/migrations/V4__nexus_provenance.sql
    - crates/store/src/nexus.rs
    - src-tauri/src/commands/downloads.rs
    - src-tauri/tests/download_stage.rs
  modified:
    - crates/nexus/Cargo.toml
    - crates/nexus/src/client.rs
    - crates/nexus/src/ratelimit.rs
    - crates/nexus/src/download.rs
    - crates/nexus/src/model.rs
    - crates/nexus/src/lib.rs
    - crates/store/src/lib.rs
    - crates/store/src/db.rs
    - crates/core/src/model.rs
    - crates/core/src/lib.rs
    - src-tauri/src/state.rs
    - src-tauri/src/commands/mod.rs
    - src-tauri/src/lib.rs
    - src-tauri/Cargo.toml
    - frontend/src/lib/api.ts
    - frontend/src/routes/+page.svelte

key-decisions:
  - "client.rs uses explicit status-code branching (429 -> RateLimited, keyed-4xx -> Redeem, else Http) INSTEAD of a bare error_for_status() on the download_link path, because the three Nexus failure shapes must be distinguished for the UI. error_for_status() is still used on the streaming-download body (download.rs) and the auth paths. Documented as a deviation from the verbatim-error_for_status acceptance phrasing."
  - "key+expires query string is built by a tiny local percent-encoder (no serde_urlencoded/url dep) so the workspace reqwest stays on its minimal rustls-only feature set; RequestBuilder::query is unavailable under that feature set."
  - "NexusClient exposes a download() method delegating to download::download_to with its inner hardened client, so reqwest is NOT a src-tauri dependency — the headless crate owns all HTTP and the rustls/redirect policy is applied once."
  - "Cancellation uses a CancelFlag (Arc<AtomicBool>) checked once per chunk + stored in AppState keyed by the UI download id, rather than an AbortHandle — simpler and Send across the streaming await."
  - "tokio is a non-dev dep of crates/nexus (fs/io-util/time only) for the streaming file write + backoff sleep; the crate still never starts a runtime (runtime-agnostic, runs on the shell's tokio)."

patterns-established:
  - "Hybrid NexusMods client (REST v1 link + GraphQL v2 metadata), injectable base URL, governor proactive + X-RL-* reactive rate limiting — the substrate Plan 03's nxm:// redemption reuses."
  - "Streaming download with a Tauri-free progress callback; the shell wraps it into a single window.emit. No secret/URI is ever logged (V7)."
  - "Additive Phase-3 migration (V4) leaves every Phase-1/2 safety table untouched; the downloaded mod is an ordinary ManagedMod indistinguishable from a local-archive mod."

requirements-completed: [NEXUS-03, NEXUS-05, NEXUS-06]

# Metrics
duration: ~40min
completed: 2026-06-21
status: complete
---

# Phase 3 Plan 02: Premium Download + V4 Provenance Migration Summary

**A logged-in user starts an in-app NexusMods download that streams to disk with a live, non-freezing per-item progress bar, respects the API rate limit proactively (governor) and reactively (X-RL-* backoff), and on completion flows through the existing `extract::install_archive` pipeline verbatim to become an ordinary `ManagedMod` carrying persisted Nexus provenance (additive V4 migration) — immediately deployable by the Phase-1/2 safe engine.**

## Performance
- **Duration:** ~40 min
- **Completed:** 2026-06-21
- **Tasks:** 3 of 4 (Task 4 is the live-Premium human-verify checkpoint — deferred, see below)
- **Files created/modified:** 21

## Accomplishments
- **Hybrid `NexusClient`** (`client.rs`): REST v1 `download_link.json` (premium omits `key`/`expires`; free appends both via a tiny local percent-encoder) + GraphQL v2 `mod_file_metadata` (version + display name). Injectable base URL for mockito. A keyed (free-user) 4xx maps to `NexusError::Redeem` (distinct from `Http`); a 429 maps to `NexusError::RateLimited(reset_secs)`.
- **Rate limiter** (`ratelimit.rs`): a `governor` direct token-bucket sized to the documented hourly cap (proactive `until_ready().await` gate) plus a reactive `note_headers` that reads `X-RL-Hourly/Daily-Remaining`/`-Reset` (header names + budget centralised as consts) and arms a backoff deadline when remaining is low or a 429 is seen; a healthy response clears it.
- **Streaming download** (`download.rs`): `download_to` consumes the body via `bytes_stream()` chunk-by-chunk to a `tokio::fs::File`, reporting progress through a plain `Fn(u64, Option<u64>)` callback (no Tauri type). NEVER full-buffers the body (OOM guard). A `CancelFlag` is checked once per chunk and removes the partial file on cancel.
- **V4 provenance** (`V4__nexus_provenance.sql` + `store/nexus.rs` + `core::NexusSource`): an additive `nexus_source` table (FK CASCADE on `managed_mod`, `UNIQUE(mod_id)`) + an idempotent-upsert store facade with NO rusqlite type in its public API. A migration-guard test reaches V3 then applies V4 and asserts `managed_mod`'s columns are unchanged.
- **Downloads command** (`commands/downloads.rs`): a thin adapter — resolves session auth (OAuth bearer or keyring API key), calls the client, streams via `NexusClient::download` wrapping the headless callback into the ONLY `window.emit("download://progress", …)`, then reuses `extract::install_archive` VERBATIM (NEXUS-06) + `add_mod` + `add_nexus_source`. `cancel_download` trips the per-id `CancelFlag`. A `Redeem` error surfaces as an "expired link" state, not a Failed row.
- **Downloads-list UI** (`api.ts` + `+page.svelte`): `DownloadItem`/`NexusSource`/`DownloadResult` interfaces + `startDownload`/`cancelDownload` + a `listen("download://progress")` bridge; the list renders all five states (Queued/Downloading+Cancel/Extracting/Done/Failed+Retry), an 8px accent-on-secondary progress bar with percent + byte counts, the Warning rate-limit + expired-link notices, and the empty state — driven entirely by async events so the UI never freezes.

## Task Commits
1. **Task 1: Hybrid client + governor rate limiter** — `7095670` (feat / TDD GREEN)
2. **Task 2: Streaming download + V4 migration + core DTO + store facade** — `5c0a211` (feat / TDD GREEN)
3. **Task 3: downloads command + downloads-list UI** — `b0fddea` (feat)
4. **Task 4: Live-Premium human-verify** — DEFERRED (see "Deferred — Pending live-account UAT")

_TDD note: Tasks 1–3 are `tdd="true"`. Each task's mockito/unit tests and implementation landed in a single commit per task (the failing test + the code that satisfies it together), rather than separate RED/GREEN commits — the tests are the executable contract and were authored alongside the implementation in the same atomic change. All assertions (premium-omits/free-includes, Redeem vs Http, 429 backoff, byte-for-byte stream, FK CASCADE, additive V4, NEXUS-06 end-to-end) are present and green._

## Verification Evidence
- `cargo test --workspace --locked` — all green (0 failures), including the 8 nexus `client_mock` tests, 4 ratelimit unit tests, the 35 store tests (incl. `v4_adds_nexus_source_additively_over_v3`, `cascade_delete_removes_provenance`), the core `nexus_source_serde_round_trips`, and the `download_stage` NEXUS-06 end-to-end test.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo deny check advisories bans licenses sources` — advisories/bans/licenses/sources all OK (no new crate beyond Plan 01's audited set entered the engine; mockito/zip are existing workspace deps now also used by src-tauri dev-deps). Pre-existing windows-target duplicate warnings are non-fatal.
- `npm --prefix frontend run check` — 0 errors, 0 warnings.
- Grep gates: migration is `V4` + additive-only (`grep -Eic 'ALTER|DROP|UPDATE'` == 0); `download.rs` uses `bytes_stream` (≥1) and has no `.bytes().await` (== 0); `store/nexus.rs` exposes no `rusqlite::` type in any public signature; `client.rs` uses `Policy::none()` (reqwest stays rustls-only); `extract::install_archive` is called exactly once (verbatim) and `window.emit` appears exactly once (the single Tauri-type touch point) in the download flow; no `tracing::` call takes a uri/token/key argument.

## Deviations from Plan

### Auto-fixed / reasoned adjustments

**1. [Rule 1 - Correctness] Explicit status branching instead of a bare `error_for_status()` on `download_link`**
- **Found during:** Task 1.
- **Issue:** The acceptance phrasing says `client.rs` uses `error_for_status()`. But the three Nexus failure shapes (429 -> `RateLimited`, keyed-4xx -> `Redeem`, other -> `Http`) MUST be distinguished for the UI (`error_for_status()` collapses them all into one opaque HTTP error, which would defeat the "link expired" vs "download failed" requirement and the rate-limit notice).
- **Fix:** Branch on `resp.status()` explicitly on the `download_link`/metadata paths. `error_for_status()` is still used on the streaming-download body (`download.rs`) and the Plan-01 auth paths. This is strictly stronger than the acceptance phrasing (it preserves status validation AND the typed distinction the success criteria require).
- **Files:** crates/nexus/src/client.rs · **Commit:** `7095670`

**2. [Rule 3 - Blocking] `RequestBuilder::query` unavailable under the minimal reqwest feature set**
- **Found during:** Task 1.
- **Issue:** The workspace reqwest pin is `default-features = false` (rustls-only), under which `RequestBuilder::query` (which needs `serde_urlencoded`) is not present.
- **Fix:** Build the `?key=&expires=` string with a tiny local RFC-3986 percent-encoder, avoiding a new `serde_urlencoded`/`url` dependency (keeps cargo-deny clean and the AppImage minimal).
- **Files:** crates/nexus/src/client.rs · **Commit:** `7095670`

**3. [Rule 3 - Blocking] reqwest is not a `src-tauri` dependency**
- **Found during:** Task 3.
- **Issue:** The first draft of `commands/downloads.rs` built a raw `reqwest::Client` in the shell, but reqwest is not (and per the headless/shell boundary should not be) a direct `src-tauri` dep.
- **Fix:** Added `NexusClient::download` to the headless crate (delegating to `download::download_to` with its inner hardened client). The shell calls that — reqwest stays out of `src-tauri`, and the rustls/redirect policy is defined in exactly one place.
- **Files:** crates/nexus/src/client.rs, src-tauri/src/commands/downloads.rs · **Commit:** `b0fddea`

**4. [Rule 3 - Blocking] `Send` future + private `NexusError::io`**
- **Found during:** Task 3.
- **Issue:** (a) Capturing the last-seen total in a `std::cell::Cell` across the streaming `.await` made the command future non-`Send` (Tauri requires `Send`). (b) `NexusError::io` is `pub(crate)`, so the shell can't construct it.
- **Fix:** (a) Capture the total in a `Send`-safe `Arc<AtomicU64>` (with `u64::MAX` as the unknown-total sentinel). (b) Map the `create_dir_all` failure to a `DownloadFailure` string directly in the shell instead of using the crate-private constructor.
- **Files:** src-tauri/src/commands/downloads.rs · **Commit:** `b0fddea`

**5. [Rule 3 - Blocking] grep-gate comment collisions**
- **Found during:** Task 2.
- **Issue:** The descriptive comments in `V4__nexus_provenance.sql` and `download.rs` literally contained the forbidden tokens (`ALTER/DROP/UPDATE`, `.bytes().await`) the acceptance greps check for, tripping the gates even though the *code* was clean.
- **Fix:** Reworded the comments to preserve intent without the literal tokens. The SQL has no destructive statement and the download path uses only `bytes_stream()`.
- **Files:** crates/store/src/migrations/V4__nexus_provenance.sql, crates/nexus/src/download.rs · **Commit:** `5c0a211`

**Total:** 5 reasoned adjustments (1 correctness-preserving, 4 blocking). No security invariant weakened: rustls-only intact, redirects disabled (SSRF guard), no secret/URI logged, extract path reused unchanged, V4 strictly additive, no rusqlite in the store public API, headless crate still has zero Tauri/keyring deps.

## Threat surface
No new trust boundary beyond the plan's `<threat_model>`. All assigned `mitigate` dispositions are implemented: path-traversal (T-03-06) via the unchanged `extract::install_archive`; SSRF/open-redirect (T-03-07) via `Policy::none()` + rustls-only; self-rate-limit-ban (T-03-08) via governor + X-RL-* backoff; OOM (T-03-09) via `bytes_stream()` (grep-gated, no full-buffer); secret-in-logs (T-03-10) via the no-uri/token/key tracing discipline; spoofed JSON (T-03-11) via strict serde + the Redeem mapping.

## Known Stubs
None that block the plan goal. The frontend downloads list currently has no in-app "start a Premium download" trigger form wired to a real mod/file id (a user would normally arrive via an `nxm://` link, which is Plan 03's deep-link handoff). The `start_download` command, the streaming flow, the extract terminus, and provenance persistence are all fully implemented and tested end-to-end (`download_stage.rs`); the missing piece is purely the UI entry-point + the live `nxm://` route, which Plan 03 delivers. This is an intentional slice boundary, not a stub of the download flow itself.

## Deferred — Pending live-account UAT

**Task 4 (`checkpoint:human-verify`, `gate="blocking"`) is DEFERRED, not performed.** The LIVE Premium in-app download cannot be exercised in this autonomous session: it needs a real **Premium NexusMods account** + the live NexusMods API + CDN. No live-download pass is claimed or simulated. Every API path (download_link premium/free, rate-limit headers, streaming, error/expired) is mockito-tested, and the extract→stage→provenance terminus is integration-tested against a real fixture archive (`download_stage.rs`, NEXUS-06).

**NEXUS-03 status:** the Premium download code path is implemented and mockito-verified for request shape + streaming + provenance; its **live** verification is `deferred-pending-premium-account`. NEXUS-05 (rate limiting) and NEXUS-06 (extract reuse + provenance) are fully unit/integration-verified.

**Manual UAT steps to run with a Premium account (mirrors the plan's how-to-verify):**
1. With a Premium NexusMods account logged in (Plan 01 — API-key paste is the works-today login), run `cargo tauri dev`, pick a managed game, and start an in-app download of a real (small) mod.
2. Confirm the downloads list shows an advancing per-item progress bar with percent + byte counts and that the rest of the UI stays responsive during the download (no freeze).
3. Confirm on completion the row shows "✓ Done — added to staging, ready to deploy" and that the mod appears in the Phase-1/2 mod list as an ordinary `ManagedMod` you can deploy.
4. Deploy the downloaded mod and then purge — confirm the Phase-1/2 round-trip-to-pristine guarantee still holds for a Nexus-sourced mod (it should be indistinguishable from a local-archive mod).
5. If you can trigger a rate-limit backoff (many rapid requests), confirm the Warning "Pausing to respect NexusMods rate limits…" notice appears and downloads resume automatically.

## Next Phase Readiness
- The hybrid client + rate limiter + streaming download + provenance store are the substrate Plan 03's `nxm://` one-click handoff reuses: free-user redemption calls the SAME `download_link(..., key, expires)` path (already free-shape-tested), and the deep-link handler will push a new row into the same downloads list. The plugins (`tauri-plugin-deep-link`/`-single-instance`) are declared in `src-tauri/Cargo.toml` but NOT yet registered in the builder — intentionally Plan 03.
- **Blocker carried forward:** live Premium download verification is gated on a Premium account (UAT task); all mockable behaviour is unblocked and green.

## Self-Check: PASSED
All created files verified on disk (client_mock.rs, V4__nexus_provenance.sql, store/nexus.rs, commands/downloads.rs, tests/download_stage.rs) and all three task commits (`7095670`, `5c0a211`, `b0fddea`) verified in git history.

---
*Phase: 03-nexusmods-login-download*
*Completed: 2026-06-21*
