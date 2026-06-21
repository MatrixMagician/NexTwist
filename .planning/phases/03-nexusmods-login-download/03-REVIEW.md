---
phase: 03-nexusmods-login-download
reviewed: 2026-06-21T00:00:00Z
depth: deep
files_reviewed: 12
files_reviewed_list:
  - crates/nexus/src/auth.rs
  - crates/nexus/src/client.rs
  - crates/nexus/src/download.rs
  - crates/nexus/src/ratelimit.rs
  - crates/store/src/nexus.rs
  - crates/store/src/migrations/V4__nexus_provenance.sql
  - src-tauri/src/auth.rs
  - src-tauri/src/keyring.rs
  - src-tauri/src/commands/downloads.rs
  - src-tauri/src/commands/nexus.rs
  - src-tauri/src/lib.rs
  - frontend/src/routes/+page.svelte
findings:
  critical: 1
  warning: 7
  info: 6
  total: 14
status: issues_found
---

# Phase 3: Code Review Report

**Reviewed:** 2026-06-21
**Depth:** deep
**Files Reviewed:** 12
**Status:** issues_found

## Summary

This is a security-sensitive phase (OAuth2+PKCE, keyring token storage, untrusted
`nxm://` OS input, streaming external HTTP). The security-critical invariants are, on the
whole, implemented carefully and correctly:

- **NEXUS-02 (no-plaintext-token):** verified. The `crates/nexus` crate has no
  `keyring`/`tauri` dependency (Cargo.toml confirms), `OAuthTokens.access` is held in
  `AppState` in memory only, and the only persistence path is `keyring::store_*` →
  `Entry::set_password`. There is no file/log write of any token. The no-backend
  condition hard-fails with `NoKeyringBackend` and writes nothing. Strong.
- **OAuth2/PKCE/CSRF:** S256 challenge from `oauth2`, CSRF compared *before* any network
  call, and `complete_oauth` re-validates state inside `exchange_code`. Correct.
- **nxm:// parser:** strict, non-panicking, rejects spoofed schemes/paths/overflowing
  ids, treats `key`/`expires`/`code` as opaque, never shells out or logs them. Good.
- **Headless boundary:** clean — `crates/nexus` is Tauri-free and keyring-free; reqwest
  is rustls-only with redirects disabled.
- **Streaming:** chunk-by-chunk, no full-body buffer; cancellation removes the partial.

The defects below are mostly **robustness, resource-safety, and UX-correctness** gaps
rather than invariant breaks. The one BLOCKER is a real data-loss/leak-of-partial-file
risk on the cancel/error path that the streaming code's own contract claims to prevent.

One framing caveat for the "SSRF guard" claims throughout the module docs: disabling
redirect-following does NOT make these requests SSRF-safe — the CDN `URI` and the token
host are still fully attacker-influenceable in the download path (see WR-02). The
redirect policy only blocks *open-redirect chaining*, which the comments overstate.

---

## Critical Issues

### CR-01: A cancelled / failed download leaves the partial archive on disk in `run_download` (only the headless inner-loop cleans up)

**File:** `src-tauri/src/commands/downloads.rs:234-263`, `crates/nexus/src/download.rs:96-109`

**Issue:** The partial-file cleanup contract is split across two layers and only one of
them actually runs on most failure paths, so the untrusted partial archive leaks.

`download_to` (download.rs) only removes the temp file in **one** branch — the
cooperative-cancel branch (line 99-100). Every *other* early return leaves the partial
file on disk:

- a chunk transport error (`chunk.map_err(...)?`, line 103),
- a `write_all` I/O error (line 104-106),
- a `flush` error (line 111),
- an HTTP/`error_for_status` failure after the file was already created (lines 80-82 run
  before `File::create`, so that one is fine; but a body error mid-stream is not).

Back in `run_download` (downloads.rs), `archive_path` is only removed on the **success**
path (line 263, `remove_file` after a successful extract). If `client.download(...)`
returns `Err` (line 250, `.map_err(fail)?`), or if `extract::install_archive` fails
(line 261, `.map_err(fail)?`), the function returns early and the
`.nextwist-dl-<id>.archive` temp file is orphaned in the user's **staging dir** — the
very directory the deploy engine treats as authoritative mod content. A leftover
`.nextwist-dl-*.archive` is untrusted, partially-written bytes sitting inside staging.

This directly contradicts download.rs's own module doc ("the partial file is removed")
and the downloads.rs comment ("extract validates it before anything lands in staging").

**Fix:** Guarantee cleanup of the temp archive on *every* exit from the download/extract
window, regardless of outcome. Either remove it in `download_to` on all error returns, or
(cleaner) make the temp path RAII in the shell so it is unlinked on drop:

```rust
// in run_download, after computing archive_path:
struct TempArchive(PathBuf);
impl Drop for TempArchive {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0); // best-effort unlink on any early return
    }
}
let _guard = TempArchive(archive_path.clone());
// ... download + extract ...
// on success, the explicit remove_file is now redundant but harmless;
// on any `?` early-return the guard's Drop unlinks the partial.
```

Additionally, in `download_to`, wrap the per-chunk loop so a mid-stream transport/write
error also unlinks `dest` before returning, matching the cancel branch.

---

## Warnings

### WR-01: `rateLimited` UI state is never set — the rate-limit backoff is invisible and the bar appears frozen

**File:** `frontend/src/routes/+page.svelte:84,718-722`; `src-tauri/src/commands/downloads.rs` (no emit); `crates/nexus/src/ratelimit.rs:79-94`

**Issue:** `let rateLimited = $state(false)` is declared and gated into a `{#if rateLimited}`
notice, but **nothing ever assigns `rateLimited = true`**. The shell never emits a
rate-limit event, and `RateLimiter::until_ready` can `tokio::time::sleep` for up to the
reset window (or `DEFAULT_BACKOFF` = 60s) *before* the request is even issued. During that
sleep the download row stays in `downloading` with no progress tick and no explanation —
exactly the "frozen UI" outcome the rate-limit design was meant to avoid. The NEXUS-05
"Pausing to respect rate limits…" affordance is dead code.

**Fix:** Emit a rate-limit state from the shell and wire it. Minimal version: when
`download_link`/`mod_file_metadata` return `NexusError::RateLimited(secs)`, emit a
`download://ratelimit` (or reuse `download://progress` with a `"ratelimited"` state) and
set `rateLimited = true` in the listener, clearing it on the next non-ratelimited tick.

### WR-02: `NexusError::RateLimited` from the metadata/link calls is silently downgraded to a generic "failed" download

**File:** `src-tauri/src/commands/downloads.rs:209-222, 302-321`

**Issue:** `download_link` and `mod_file_metadata` can return `NexusError::RateLimited(n)`
(client.rs:154, 208). In `run_download` both are mapped via `.map_err(fail)`, and `fail`
→ `NexusErrorLike::from(NexusError)` only special-cases `Redeem` (downloads.rs:316-320).
A `RateLimited` therefore becomes `is_redeem:false` → a terminal `"failed"` row with the
raw "rate limited; retry after Ns" string. The user sees a hard failure for what is a
transient, automatically-recoverable condition, and the documented "downloads resume
automatically" behavior (NEXUS-05) does not happen — there is no retry.

**Fix:** Discriminate `RateLimited` in `NexusErrorLike` (add an `is_ratelimited`/retry-after
field), surface it as the rate-limit notice (WR-01) rather than a Failed row, and either
auto-retry after the backoff or leave the row in a "paused" state.

### WR-03: `note_headers` can clear an active 429 backoff on a later healthy response, and the read-modify-write across the `Mutex` is racy under concurrent downloads

**File:** `crates/nexus/src/ratelimit.rs:112-136, 79-94`

**Issue:** Two related problems with the single shared `RateLimiter`:

1. `note_headers` clears any backoff whenever a response reports healthy remaining budget
   (lines 132-135). But each `NexusClient` builds its **own** `RateLimiter` (client.rs:70),
   so cross-request coordination only exists within one client. More importantly, within a
   client that issues interleaved calls, a 429 on request A arms a deadline, and a healthy
   header on a concurrent in-flight request B (e.g. a cached/cheaper endpoint) will
   **clear** A's backoff, defeating the protection.
2. `until_ready` does a check-then-sleep-then-clear (lines 80-90) that is not atomic with
   `note_headers`'s write. Two concurrent `until_ready` callers can both read the deadline,
   both sleep, and one then sets `backoff_until = None` while a freshly-armed deadline from
   `note_headers` is racing — losing a just-recorded backoff.

Because each download constructs a fresh client+limiter (downloads.rs:206), the proactive
governor bucket and the reactive backoff are **per-download**, not process-wide. The
documented "the client never walks into a self-inflicted ban" guarantee does not hold
across parallel downloads — N concurrent downloads each get a full fresh hourly bucket.

**Fix:** Share one `RateLimiter` (or at least one governor keyed bucket + one backoff
deadline) across all NexusMods requests for the process — e.g. construct it once in
`AppState` and pass it into `NexusClient::with_*`. For the clear-while-armed case, do not
clear a backoff whose deadline is still in the future; only clear once it has elapsed.

### WR-04: `percent_decode` is used for `code`/`state`, but the query splitter does not undo `+`-as-space consistently, and a `=`-containing opaque value is truncated

**File:** `crates/nexus/src/model.rs:210-216`

**Issue:** `query_get` splits each `pair` on the **first** `=` via `split_once('=')` and
returns the remainder decoded. That is correct for a single value, but a `key`/`expires`
value that legitimately contains an encoded `=` is fine (it would be `%3D`), yet a value
containing a literal `=` (which a non-conforming minter could emit) is passed through
intact only because `split_once` keeps the rest — acceptable. The real issue is
asymmetry: `percent_decode` turns `+` into a space (form-urlencoded convention,
model.rs:227), but OAuth `code`/`state` values are **not** form fields and frequently
contain `+` in base64url-adjacent encodings. Decoding `+`→space corrupts a `code`/`state`
that contains a literal `+`, causing a spurious CSRF mismatch or a bad code exchange.

**Fix:** Use raw percent-decoding (no `+`→space) for the OAuth `code`/`state` path, or
require those values to be strictly percent-encoded and stop translating `+`. Keep the
`+`→space behavior only if a value is known-form-encoded; OAuth callback params are not.

### WR-05: Two-statement upsert in `add_nexus_source` is not atomic — a concurrent writer can interleave between the INSERT…ON CONFLICT and the SELECT

**File:** `crates/store/src/nexus.rs:20-51`

**Issue:** `add_nexus_source` does an `INSERT … ON CONFLICT … DO UPDATE` and then a
separate `SELECT id … WHERE mod_id = ?`. Between the two statements another writer could
(in principle) delete/replace the row, returning a stale or wrong id, or the row could be
gone (the `query_row` then errors). Within this single-connection desktop app the window
is small, but the function's contract ("always gets the stable nexus_source id") is not
actually guaranteed by the two-statement form. SQLite supports `RETURNING`.

**Fix:** Collapse to one statement:

```rust
let id: i64 = self.conn.query_row(
    "INSERT INTO nexus_source (mod_id, nexus_mod_id, file_id, version, display_name)
     VALUES (?1,?2,?3,?4,?5)
     ON CONFLICT (mod_id) DO UPDATE SET
       nexus_mod_id=excluded.nexus_mod_id, file_id=excluded.file_id,
       version=excluded.version, display_name=excluded.display_name
     RETURNING id",
    params![...], |r| r.get(0))?;
```

### WR-06: `start_download` resolves auth and `add_mod`/`add_nexus_source` under a held `tokio::Mutex`, but the long network/extract work is not — yet a second `start_download` with the same `id` silently overwrites the cancel flag

**File:** `src-tauri/src/commands/downloads.rs:114-128, 273-287`

**Issue:** `guard.downloads.insert(id.to_string(), cancel.clone())` (line 126) overwrites
any existing entry for the same `id`. The UI generates `crypto.randomUUID()` per start, but
the `nxm://` path derives a **deterministic** id `format!("nxm-{mod}-{file}")`
(commands/nexus.rs:167). Two `nxm://` arrivals for the same mod+file (a common
double-click / browser re-fire) produce the same id: the second insert replaces the first
cancel flag, so cancelling now only aborts the second download, and the first becomes an
unkillable orphan still streaming to the same `archive_path`
(`.nextwist-dl-nxm-<mod>-<file>.archive`) — two writers racing on one temp file.

**Fix:** Reject/deduplicate a start when `downloads` already contains the id (return early
or make the temp path id+nonce unique), and/or include a per-arrival nonce in the `nxm://`
row id so concurrent arrivals don't collide on the temp archive.

### WR-07: `account_info`/login do not restore a session from the keyring on startup — a stored credential is ignored until a fresh login

**File:** `src-tauri/src/state.rs:47-63`, `src-tauri/src/commands/nexus.rs:122-126`, `frontend/src/routes/+page.svelte:140-147,615-616`

**Issue:** `AppState::init` sets `user: None` and never calls `keyring::load_refresh_token`.
`account_info` returns the in-memory `guard.user`, which is `None` on every cold start even
though a valid API key / refresh token is sitting in the keyring. The user appears logged
out on each launch and the keyring entry is never used to re-establish the session (no
refresh-token exchange, no API-key re-validate on boot). The persisted credential is
effectively write-only until the next manual login overwrites it. This undercuts the whole
point of storing it.

**Fix:** On startup (or first `account_info`), load the keyring credential and either
re-validate the API key or refresh the OAuth token, populating `guard.user`/`access_token`
so the session survives a restart. Distinguish "no entry" (logged out) from "no backend"
(the NEXUS-02 banner).

---

## Info

### IN-01: SSRF / "redirect disabled is an SSRF guard" comments overstate the protection

**File:** `crates/nexus/src/auth.rs:5-6,115`; `crates/nexus/src/client.rs:9-11,59`; `crates/nexus/src/download.rs:58-60`

**Issue:** Multiple comments call `redirect(Policy::none())` an "SSRF guard". It is not —
it only prevents open-redirect *chaining*. The download path GETs an arbitrary CDN `URI`
returned by the server (model.rs:58, "actual HTTPS CDN URI"); if the Nexus account or a
spoofed response is compromised, that URI is fetched as-is. There is no host allow-list,
no `https`-scheme enforcement on `link.uri`, and no private-IP rejection. For v1 this is
an accepted trust-in-Nexus posture, but the comments should describe it accurately so a
future reader doesn't assume SSRF is handled.

**Fix:** Reword the comments to "redirect-following disabled (open-redirect hardening)"
and, optionally, assert `link.uri` parses as `https://` before streaming.

### IN-02: `parse_u64` reset header is treated as seconds-from-now, but Nexus `X-RL-*-Reset` is documented as a UTC timestamp in several sources

**File:** `crates/nexus/src/ratelimit.rs:120-125,140-144`

**Issue:** `note_headers` does `Instant::now() + Duration::from_secs(reset)` assuming the
header is a relative seconds value. If Nexus emits an absolute epoch timestamp (the code's
own A3/A4 `[ASSUMED]` caveats admit the format is unconfirmed), `reset_secs` becomes a
~1.7-billion-second backoff — an effectively permanent stall. The `[ASSUMED]` comments
acknowledge this, so it is INFO, but it is a latent foot-gun behind the WR-01/WR-02 paths.

**Fix:** Clamp the derived backoff to a sane ceiling (e.g. `min(reset, 3600)`) so a
mis-parsed absolute timestamp can never wedge downloads for hours.

### IN-03: `resolve_data_dir` falls back to a relative `.nextwist` dir, which depends on the process CWD

**File:** `src-tauri/src/lib.rs:21-25`

**Issue:** If `app_data_dir()` fails, the fallback is `PathBuf::from(".nextwist")` — a
CWD-relative path. The DB (and thus the credential-bearing state pointers, the deploy
manifest) would land wherever the app happened to be launched from, and a second launch
from a different CWD would open a different DB. Low likelihood, but the failure mode is
silent divergence rather than a clear error.

**Fix:** Fall back to an absolute home-anchored path (e.g. `$HOME/.nextwist`) or fail loudly.

### IN-04: `sanitize` for the staging subdir collapses distinct mod names to the same directory

**File:** `src-tauri/src/commands/downloads.rs:359-370`

**Issue:** `sanitize` maps every non-alphanumeric (except `-_ ` and space) to `_`, so
`"Mod: A"` and `"Mod_ A"` and `"Mod/ A"` all become `"Mod_ A"`. Two different Nexus mods
whose display names sanitize identically stage into the same `staging_root`, and
`install_archive` would extract the second over/into the first. The path-traversal defense
is correctly delegated to `extract`, so this is not a security hole, but it is a
collision/clobber bug for adversarial or merely-similar names.

**Fix:** Disambiguate the staging subdir with the mod/file id (e.g.
`format!("{}-{}-{}", sanitize(name), nexus_mod_id, file_id)`) so distinct downloads never
share a staging root.

### IN-05: `RateLimited` and `Redeem` `#[allow(dead_code)]` attributes are now stale

**File:** `crates/nexus/src/error.rs:49,59,69`

**Issue:** These variants/constructors are wired in this phase (client.rs constructs
`RateLimited`/`Redeem`, download.rs uses `NexusError::io`), so the `#[allow(dead_code)]`
markers labelled "wired in Plan 02" are no longer accurate and suppress real dead-code
signal going forward.

**Fix:** Remove the `#[allow(dead_code)]` attributes now that the variants are used.

### IN-06: Premium/free download gating is UI-only — a free user can trigger the premium (keyless) link path and get a confusing generic HTTP error

**File:** `src-tauri/src/commands/downloads.rs:209-216`; `crates/nexus/src/client.rs:156-166`; `frontend/src/routes/+page.svelte:706-716`

**Issue:** The frontend only *hints* that free users should use the website button; nothing
stops a `start_download` with `key=None` for a free account. `download_link` then hits the
premium endpoint without `key`/`expires`; because `key.is_none()`, a 4xx maps to generic
`NexusError::Http` (not the friendlier `Redeem` "link expired" path), surfacing as an
opaque "HTTP 403" Failed row. Not a security issue, but a poor and confusing failure for a
predictable user action.

**Fix:** Reject a keyless `start_download` for a known-free account up front with a clear
"use the website Mod Manager Download button" message, mirroring the UI hint server-side.

---

_Reviewed: 2026-06-21_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
