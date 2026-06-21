//! OAuth2 Authorization-Code + PKCE (S256) and API-key auth — headless.
//!
//! Follows RESEARCH Pattern 1 (oauth2 5.0 PKCE) verbatim. The HTTP shape mirrors
//! `crates/loadorder`'s `real_fetch` (rustls, `redirect(Policy::none())`,
//! `error_for_status()`) converted from blocking to async — these are security-reviewed
//! choices (V9 / SSRF guard), not stylistic.
//!
//! SECRET DISCIPLINE (V7): no `tracing::` call in this module is ever passed an access
//! token, refresh token, API key, authorization code, or PKCE `code_verifier`. Logs
//! carry only non-secret facts (e.g. "exchanging code", a `user_id` after success).
//! The oauth2 / reqwest error types are flattened to `NexusError::Auth(String)` so they
//! never cross the crate boundary.

use oauth2::basic::BasicClient;
use oauth2::{AuthUrl, ClientId, CsrfToken, PkceCodeChallenge, RedirectUrl, Scope};
use serde::Deserialize;

use crate::error::NexusError;
use crate::model::{OAuthTokens, UserInfo};

/// NexusMods OAuth2 authorize endpoint.
const AUTHORIZE_URL: &str = "https://users.nexusmods.com/oauth/authorize";
/// NexusMods OAuth2 token-endpoint **origin** — the shell passes this as
/// `exchange_code`'s `token_base` in production (tests pass a mockito URL). The
/// `/oauth/token` path is appended inside `exchange_code`.
pub const TOKEN_BASE: &str = "https://users.nexusmods.com";
/// Default NexusMods REST v1 API base — the shell passes this as `validate_api_key`'s
/// `base` in production (tests pass a mockito URL).
pub const API_BASE: &str = "https://api.nexusmods.com";

/// The OAuth scope requested at authorize time.
///
/// `[ASSUMED]` `"public"` per RESEARCH A1 — the exact scope set is unconfirmed until the
/// OAuth client is registered under the Nexus Acceptable Use Policy. Centralised here so
/// the registration step changes it in exactly one place.
const OAUTH_SCOPE: &str = "public";

/// The result of building an OAuth2 authorize URL: the URL to open in the system
/// browser, the CSRF `state` to validate on callback, and the PKCE verifier to keep in
/// memory for the code exchange. The verifier is a secret and is NEVER logged.
#[derive(Debug)]
pub struct AuthorizeRequest {
    /// The full authorize URL (carries `code_challenge`, `code_challenge_method=S256`,
    /// `response_type=code`, `state`, and the scope).
    pub authorize_url: String,
    /// The CSRF `state` value; the callback's `state` must equal this.
    pub csrf_state: String,
    /// The PKCE `code_verifier` — in-memory only, fed to [`exchange_code`].
    pub pkce_verifier: String,
}

/// Build an OAuth2+PKCE authorize URL for the NexusMods public client (no client secret).
///
/// Generates an S256 PKCE challenge/verifier pair and a random CSRF state. The caller
/// opens `authorize_url` in the system browser, keeps `pkce_verifier` + `csrf_state` in
/// memory, and feeds them back to [`exchange_code`] when the `nxm://oauth/callback`
/// redirect arrives.
pub fn build_authorize_url(client_id: &str, redirect: &str) -> Result<AuthorizeRequest, NexusError> {
    let client = oauth_client(client_id, redirect)?;

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let (authorize_url, csrf) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new(OAUTH_SCOPE.to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    tracing::debug!("built OAuth2 authorize URL"); // no secret logged

    Ok(AuthorizeRequest {
        authorize_url: authorize_url.to_string(),
        csrf_state: csrf.secret().clone(),
        pkce_verifier: pkce_verifier.into_secret(),
    })
}

/// The relevant fields of an OAuth2 token-endpoint JSON response.
///
/// We parse this directly (rather than via oauth2's `request_async`) because oauth2
/// 5.0.0 wires its `AsyncHttpClient` to **reqwest 0.12**, while the NexTwist workspace
/// is on reqwest 0.13 — the two `reqwest::Client` types are distinct, so oauth2's
/// async executor cannot accept our client. oauth2 still owns the security-sensitive
/// half (S256 PKCE challenge/verifier + CSRF state); the token POST itself is a plain,
/// well-specified form request we issue with the workspace's hardened reqwest 0.13
/// client (rustls, redirects disabled). See RESEARCH Pitfall — reqwest 0.12/0.13 split.
#[derive(Debug, Deserialize)]
struct TokenJson {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
}

/// Exchange an authorization code for tokens, validating CSRF first.
///
/// `csrf_returned` is the `state` from the callback; it MUST equal `csrf_expected`
/// (the value stored from [`build_authorize_url`]) — a mismatch is rejected as
/// `NexusError::Auth` BEFORE any network call (CSRF defence, V2). `token_base` is the
/// token-endpoint origin; pass [`TOKEN_URL`]'s host in production or a mockito URL in tests.
pub async fn exchange_code(
    client_id: &str,
    redirect: &str,
    token_base: &str,
    code: &str,
    csrf_returned: &str,
    csrf_expected: &str,
    pkce_verifier: &str,
) -> Result<OAuthTokens, NexusError> {
    // CSRF: validate state == csrf before touching the network.
    if csrf_returned != csrf_expected {
        return Err(NexusError::Auth("OAuth state/CSRF mismatch".to_string()));
    }

    let token_url = format!("{}/oauth/token", token_base.trim_end_matches('/'));

    // Async reqwest client, redirects disabled (SSRF/open-redirect guard, V9), rustls.
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| NexusError::Auth(e.to_string()))?;

    tracing::info!("exchanging OAuth authorization code for tokens"); // no secret logged

    // Standard RFC 6749 §4.1.3 + RFC 7636 authorization-code-with-PKCE token request.
    // The `code_verifier` (the PKCE secret oauth2 generated) binds the code to this
    // client. Sent as an x-www-form-urlencoded body; no secret is logged.
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("code_verifier", pkce_verifier),
        ("client_id", client_id),
        ("redirect_uri", redirect),
    ];

    let resp = http
        .post(&token_url)
        .form(&form)
        .send()
        .await
        .map_err(|e| NexusError::Auth(e.to_string()))?;

    let resp = resp
        .error_for_status()
        .map_err(|e| NexusError::Auth(e.to_string()))?;

    let token: TokenJson = resp
        .json()
        .await
        .map_err(|e| NexusError::Auth(e.to_string()))?;

    Ok(OAuthTokens {
        access: token.access_token,
        refresh: token.refresh_token,
    })
}

/// Validate a NexusMods personal API key against REST v1 `/v1/users/validate.json`.
///
/// `base` is the API origin ([`API_BASE`] in production, a mockito URL in tests). Sends
/// the key in the `apikey` header. A 401 (or any non-success status) maps to
/// `NexusError::Auth`; the parsed `{user_id, name, is_premium}` becomes a [`UserInfo`].
pub async fn validate_api_key(base: &str, key: &str) -> Result<UserInfo, NexusError> {
    let url = format!("{}/v1/users/validate.json", base.trim_end_matches('/'));

    // Same hardened client shape as the OAuth exchange + loadorder's real_fetch.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| NexusError::Http(e.to_string()))?;

    let resp = client
        .get(&url)
        .header("apikey", key)
        .send()
        .await
        .map_err(|e| NexusError::Http(e.to_string()))?;

    // A rejected key surfaces as an auth error, not a generic HTTP error, so the UI can
    // tell "bad key" apart from "network down".
    let resp = resp
        .error_for_status()
        .map_err(|e| NexusError::Auth(e.to_string()))?;

    let info: UserInfo = resp
        .json()
        .await
        .map_err(|e| NexusError::Http(e.to_string()))?;

    tracing::info!(user_id = info.user_id, "validated NexusMods API key"); // user_id only, no key
    Ok(info)
}

/// Build the typestate `BasicClient` (public client, NO secret) configured for the
/// authorize-URL step. The token exchange does not go through this client (see
/// [`exchange_code`] for why), so only the auth + redirect URIs are set.
fn oauth_client(
    client_id: &str,
    redirect: &str,
) -> Result<
    BasicClient<
        oauth2::EndpointSet,    // has auth URL
        oauth2::EndpointNotSet, // no device-auth URL
        oauth2::EndpointNotSet, // no introspection URL
        oauth2::EndpointNotSet, // no revocation URL
        oauth2::EndpointNotSet, // no token URL (exchange is issued manually)
    >,
    NexusError,
> {
    let auth_url = AuthUrl::new(AUTHORIZE_URL.to_string())
        .map_err(|e| NexusError::Auth(format!("invalid authorize URL: {e}")))?;
    let redirect_url = RedirectUrl::new(redirect.to_string())
        .map_err(|e| NexusError::Auth(format!("invalid redirect URL: {e}")))?;

    let client = BasicClient::new(ClientId::new(client_id.to_string()))
        .set_auth_uri(auth_url)
        .set_redirect_uri(redirect_url);

    Ok(client)
}
