//! `NexusClient` — the async reqwest + governor NexusMods API client.
//!
//! The hybrid surface (RESEARCH Pitfall 2): the **download link** comes from REST v1
//! `download_link.json` (v2 does NOT generate links and v1 is load-bearing), while
//! **file metadata** (version / display name) is read over GraphQL v2. Both calls go
//! through the [`RateLimiter`] — proactively gated by `until_ready().await` and
//! reactively backed off from the response's `X-RL-*` headers.
//!
//! The HTTP client mirrors `crates/loadorder`'s `real_fetch` (rustls, redirects
//! disabled via `Policy::none()`, `error_for_status()`) converted to async — these are
//! security-reviewed choices (SSRF/open-redirect guard, V9), not stylistic.
//!
//! The base URL is **injectable**: production uses the real Nexus hosts; tests pass a
//! `mockito` server URL. A failed/expired free-user key maps to [`NexusError::Redeem`]
//! (distinct from a generic `Http` error) so the UI can show "link expired" rather than
//! "download failed". A 429 maps to [`NexusError::RateLimited`] with a reset-derived
//! retry-after. SECRET DISCIPLINE (V7): no URI, token, or key is ever logged.

use std::sync::Arc;

use reqwest::StatusCode;
use serde::Deserialize;

use crate::error::NexusError;
use crate::model::{DownloadLink, ModFile};
use crate::ratelimit::RateLimiter;

/// Default NexusMods REST v1 / metadata host (production). Tests override it.
pub const NEXUS_API_BASE: &str = "https://api.nexusmods.com";

/// How the client authenticates to NexusMods.
///
/// Centralised so the request builders attach the right header in one place — an
/// API-key session uses the `apikey` header (matching Plan 01's `validate_api_key`),
/// an OAuth session uses `Authorization: Bearer`.
#[derive(Debug, Clone)]
pub enum NexusAuth {
    /// Legacy personal API key (the works-today path). Sent as the `apikey` header.
    ApiKey(String),
    /// OAuth2 access token. Sent as `Authorization: Bearer <token>`.
    Bearer(String),
}

/// The async NexusMods API client: a hardened reqwest client + a `governor` rate limiter
/// + an injectable base URL + the session auth.
///
/// WR-03: the `limiter` is an `Arc<RateLimiter>` so a single process-wide limiter can be
/// shared across every per-download client. With a per-client limiter, N concurrent
/// downloads each got a full fresh hourly bucket and an independent backoff deadline, so
/// the "never self-inflict a ban" guarantee did not hold across parallel downloads. The
/// shell constructs one limiter in `AppState` and threads it in via [`Self::with_limiter`].
pub struct NexusClient {
    http: reqwest::Client,
    base: String,
    auth: NexusAuth,
    limiter: Arc<RateLimiter>,
}

impl NexusClient {
    /// Build a client against the real NexusMods host with the given session auth and a
    /// **fresh** (un-shared) rate limiter. Prefer [`Self::with_limiter`] in the shell so
    /// the limiter is shared process-wide (WR-03); this constructor remains for tests and
    /// one-off callers that do not need cross-request coordination.
    pub fn new(auth: NexusAuth) -> Result<Self, NexusError> {
        Self::with_base(NEXUS_API_BASE, auth)
    }

    /// Build a client against an explicit base URL with a fresh limiter (mockito tests).
    ///
    /// The reqwest client disables redirect-following (open-redirect hardening) and uses
    /// the workspace's rustls-only feature set (no native-tls).
    pub fn with_base(base: &str, auth: NexusAuth) -> Result<Self, NexusError> {
        Self::with_limiter(base, auth, Arc::new(RateLimiter::new()))
    }

    /// Build a client that uses a **shared** process-wide rate limiter (WR-03).
    ///
    /// All NexusMods requests issued by clients built with the same `limiter` `Arc`
    /// coordinate one token bucket and one backoff deadline, so parallel downloads cannot
    /// each carve out a fresh hourly budget or clobber each other's 429 backoff.
    pub fn with_limiter(
        base: &str,
        auth: NexusAuth,
        limiter: Arc<RateLimiter>,
    ) -> Result<Self, NexusError> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| NexusError::Http(e.to_string()))?;
        Ok(NexusClient {
            http,
            base: base.trim_end_matches('/').to_string(),
            auth,
            limiter,
        })
    }

    /// Stream a CDN URI to `dest` using this client's hardened inner reqwest client,
    /// reporting progress through a Tauri-free callback. Delegates to
    /// [`crate::download::download_to`] so the rustls/redirect policy is applied once.
    ///
    /// The shell calls this instead of constructing its own HTTP client, keeping reqwest
    /// out of the `src-tauri` dependency set (the headless crate owns all HTTP).
    pub async fn download<F>(
        &self,
        uri: &str,
        dest: &std::path::Path,
        cancel: &crate::download::CancelFlag,
        on_progress: F,
    ) -> Result<u64, NexusError>
    where
        F: Fn(u64, Option<u64>),
    {
        crate::download::download_to(&self.http, uri, dest, cancel, on_progress).await
    }

    /// Attach the session auth header to a request builder.
    fn authed(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.auth {
            NexusAuth::ApiKey(k) => rb.header("apikey", k),
            NexusAuth::Bearer(t) => rb.bearer_auth(t),
        }
    }

    /// Generate the CDN download links for a file (REST v1 `download_link.json`).
    ///
    /// Premium users pass `key`/`expires` as `None` (no query params); free users pass
    /// the `key`+`expires` redeemed from an `nxm://` link (both `Some`). Returns the
    /// parsed `[{name, short_name, URI}]` array.
    ///
    /// # Errors
    /// * [`NexusError::RateLimited`] on HTTP 429 (retry-after from `X-RL-*-Reset`).
    /// * [`NexusError::Redeem`] on a 4xx that indicates an expired/invalid `key`+`expires`
    ///   (free-user link redemption) — distinct from a generic `Http` error.
    /// * [`NexusError::Http`] for any other transport/status failure.
    pub async fn download_link(
        &self,
        game_domain: &str,
        mod_id: u64,
        file_id: u64,
        key: Option<&str>,
        expires: Option<&str>,
    ) -> Result<Vec<DownloadLink>, NexusError> {
        // Proactive gate before issuing the request.
        self.limiter.until_ready().await;

        let mut url = format!(
            "{}/v1/games/{}/mods/{}/files/{}/download_link.json",
            self.base, game_domain, mod_id, file_id
        );

        // Free-user redemption: append key+expires ONLY when present (premium omits them).
        // Built manually (no `serde_urlencoded`) so the workspace reqwest stays on its
        // minimal rustls-only feature set. Both values are url-encoded.
        if let (Some(k), Some(e)) = (key, expires) {
            url.push_str(&format!(
                "?key={}&expires={}",
                urlencode(k),
                urlencode(e)
            ));
        }

        let rb = self.authed(self.http.get(&url));

        tracing::debug!(game_domain, mod_id, file_id, "requesting download link"); // no key/uri

        let resp = rb
            .send()
            .await
            .map_err(|e| NexusError::Http(e.to_string()))?;

        let status = resp.status();
        let headers = resp.headers().clone();
        // Reactive: feed the X-RL-* headers (and a possible 429) into the limiter.
        self.limiter.note_headers(&headers, status == StatusCode::TOO_MANY_REQUESTS);

        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(NexusError::RateLimited(RateLimiter::retry_after_secs(&headers)));
        }
        if !status.is_success() {
            // A 4xx on a keyed (free-user) request means the link could not be redeemed
            // (expired/invalid key+expires). Surface that distinctly so the UI shows
            // "link expired", not a generic download failure.
            if key.is_some() && status.is_client_error() {
                return Err(NexusError::Redeem(format!(
                    "download link could not be redeemed (HTTP {})",
                    status.as_u16()
                )));
            }
            return Err(NexusError::Http(format!("HTTP {}", status.as_u16())));
        }

        resp.json::<Vec<DownloadLink>>()
            .await
            .map_err(|e| NexusError::Http(e.to_string()))
    }

    /// Read a mod-file's metadata (version + display name) over GraphQL v2.
    ///
    /// v2 is the modern metadata read path (the download link itself still comes from
    /// REST v1 — see [`download_link`]). The base URL is centralised so a future host
    /// swap is one edit.
    pub async fn mod_file_metadata(
        &self,
        game_domain: &str,
        mod_id: u64,
        file_id: u64,
    ) -> Result<ModFile, NexusError> {
        self.limiter.until_ready().await;

        let url = format!("{}/v2/graphql", self.base);
        // A minimal GraphQL query for the two fields we persist. The server shape is
        // `{ "data": { "modFile": { "version", "name" } } }`.
        let query = serde_json::json!({
            "query": "query($gameDomain:String!,$modId:Int!,$fileId:Int!){\
                modFile(gameDomain:$gameDomain,modId:$modId,fileId:$fileId){version name}}",
            "variables": { "gameDomain": game_domain, "modId": mod_id, "fileId": file_id },
        });

        let resp = self
            .authed(self.http.post(&url))
            .json(&query)
            .send()
            .await
            .map_err(|e| NexusError::Http(e.to_string()))?;

        let status = resp.status();
        let headers = resp.headers().clone();
        self.limiter.note_headers(&headers, status == StatusCode::TOO_MANY_REQUESTS);

        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(NexusError::RateLimited(RateLimiter::retry_after_secs(&headers)));
        }
        if !status.is_success() {
            return Err(NexusError::Http(format!("HTTP {}", status.as_u16())));
        }

        let body: GraphQlResponse = resp
            .json()
            .await
            .map_err(|e| NexusError::Http(e.to_string()))?;

        let mf = body
            .data
            .and_then(|d| d.mod_file)
            .ok_or_else(|| NexusError::Http("GraphQL response missing modFile".to_string()))?;

        Ok(ModFile {
            version: mf.version,
            display_name: mf.name,
        })
    }
}

/// Minimal percent-encoding for a query-parameter value (RFC 3986 unreserved set kept
/// literal; everything else `%XX`-escaped). Avoids pulling in `serde_urlencoded`/`url`
/// so the workspace reqwest stays on its minimal rustls-only feature set. The `key` and
/// `expires` values from an `nxm://` link are short ASCII tokens; this handles any stray
/// reserved byte safely.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The minimal shape of the GraphQL v2 `modFile` response we parse.
#[derive(Debug, Deserialize)]
struct GraphQlResponse {
    data: Option<GraphQlData>,
}

#[derive(Debug, Deserialize)]
struct GraphQlData {
    #[serde(rename = "modFile")]
    mod_file: Option<GraphQlModFile>,
}

#[derive(Debug, Deserialize)]
struct GraphQlModFile {
    version: String,
    name: String,
}
