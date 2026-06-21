//! OAuth2 Authorization-Code + PKCE (S256) and API-key auth — headless.
//!
//! STUB (Plan 01 / Task 1 RED state): the public surface is declared so the crate
//! compiles and `crates/nexus/tests/auth_mock.rs` can be written against it, but the
//! bodies are `todo!()` — the failing-test (RED) half of the TDD cycle. Task 2 fills
//! these in following RESEARCH Pattern 1 (oauth2 5.0 PKCE) verbatim.

use crate::error::NexusError;
use crate::model::{OAuthTokens, UserInfo};

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

/// Build an OAuth2+PKCE authorize URL for the NexusMods public client.
///
/// STUB (Task 1 RED): implemented in Task 2.
pub fn build_authorize_url(_client_id: &str, _redirect: &str) -> Result<AuthorizeRequest, NexusError> {
    todo!("Task 2: oauth2 5.0 PKCE authorize-URL construction")
}

/// Exchange an authorization code for tokens, validating CSRF first.
///
/// STUB (Task 1 RED): implemented in Task 2.
pub async fn exchange_code(
    _client_id: &str,
    _redirect: &str,
    _token_base: &str,
    _code: &str,
    _csrf_returned: &str,
    _csrf_expected: &str,
    _pkce_verifier: &str,
) -> Result<OAuthTokens, NexusError> {
    todo!("Task 2: oauth2 5.0 code exchange")
}

/// Validate a NexusMods personal API key, returning the authenticated user.
///
/// STUB (Task 1 RED): implemented in Task 2.
pub async fn validate_api_key(_base: &str, _key: &str) -> Result<UserInfo, NexusError> {
    todo!("Task 2: REST v1 /v1/users/validate.json")
}
