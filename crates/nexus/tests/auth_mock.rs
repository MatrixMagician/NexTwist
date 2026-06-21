//! Headless auth tests for `crates/nexus` (mockito-backed for the HTTP paths).
//!
//! Covers the OAuth2+PKCE authorize-URL shape, the code-exchange request shape against
//! a mock token endpoint, and API-key validation against a mock `/v1/users/validate.json`.
//! No live network and no real account — the live OAuth round-trip is the human-verify
//! checkpoint (gated on a registered client_id).

use nexus::{build_authorize_url, exchange_code, validate_api_key};

const CLIENT_ID: &str = "nextwist-public-test-client";
const REDIRECT: &str = "nxm://oauth/callback";

/// NEXUS-01: the authorize URL must carry a PKCE S256 challenge and a CSRF state, and
/// the returned verifier must be non-empty (kept in memory for the exchange).
#[test]
fn authorize_url_carries_pkce_s256_and_state() {
    let req = build_authorize_url(CLIENT_ID, REDIRECT).expect("authorize URL builds");

    assert!(
        req.authorize_url.contains("code_challenge="),
        "authorize URL missing code_challenge: {}",
        req.authorize_url
    );
    assert!(
        req.authorize_url.contains("code_challenge_method=S256"),
        "authorize URL missing S256 method: {}",
        req.authorize_url
    );
    assert!(
        req.authorize_url.contains("response_type=code"),
        "authorize URL missing response_type=code: {}",
        req.authorize_url
    );
    assert!(
        req.authorize_url.contains("state="),
        "authorize URL missing state: {}",
        req.authorize_url
    );
    assert!(!req.pkce_verifier.is_empty(), "PKCE verifier must be non-empty");
    assert!(!req.csrf_state.is_empty(), "CSRF state must be non-empty");
}

/// NEXUS-01: the code exchange POSTs to the token endpoint with grant_type, the code,
/// and the code_verifier present; a stubbed token JSON parses into OAuthTokens.
#[tokio::test]
async fn exchange_code_posts_pkce_and_returns_tokens() {
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("POST", "/oauth/token")
        .match_body(mockito::Matcher::AllOf(vec![
            mockito::Matcher::Regex("grant_type=authorization_code".into()),
            mockito::Matcher::Regex("code=the-auth-code".into()),
            mockito::Matcher::Regex("code_verifier=".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"access_token":"acc-123","refresh_token":"ref-456","token_type":"bearer","expires_in":3600}"#,
        )
        .create_async()
        .await;

    // Build a request so we have a real PKCE verifier + CSRF state to feed back.
    let req = build_authorize_url(CLIENT_ID, REDIRECT).expect("authorize URL builds");

    let tokens = exchange_code(
        CLIENT_ID,
        REDIRECT,
        &server.url(),
        "the-auth-code",
        &req.csrf_state,      // returned state == expected → CSRF ok
        &req.csrf_state,
        &req.pkce_verifier,
    )
    .await
    .expect("code exchange succeeds against the mock token endpoint");

    mock.assert_async().await;
    assert_eq!(tokens.access, "acc-123");
    assert_eq!(tokens.refresh.as_deref(), Some("ref-456"));
}

/// NEXUS-01: a CSRF mismatch is rejected as an auth error (not a panic), before any
/// network call.
#[tokio::test]
async fn exchange_code_rejects_csrf_mismatch() {
    let err = exchange_code(
        CLIENT_ID,
        REDIRECT,
        "http://127.0.0.1:1", // never contacted — CSRF check fails first
        "code",
        "returned-state",
        "expected-state-DIFFERENT",
        "verifier",
    )
    .await
    .expect_err("CSRF mismatch must error");

    assert!(
        matches!(err, nexus::NexusError::Auth(_)),
        "CSRF mismatch should map to NexusError::Auth, got {err:?}"
    );
}

/// NEXUS-01 fallback: validate_api_key hits /v1/users/validate.json with the apikey
/// header and parses {user_id,name,is_premium} into UserInfo.
#[tokio::test]
async fn validate_api_key_parses_user_info() {
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/v1/users/validate.json")
        .match_header("apikey", "my-secret-key")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"user_id":99,"name":"linux-modder","is_premium":true}"#)
        .create_async()
        .await;

    let info = validate_api_key(&server.url(), "my-secret-key")
        .await
        .expect("api-key validation succeeds against the mock");

    mock.assert_async().await;
    assert_eq!(info.user_id, 99);
    assert_eq!(info.name, "linux-modder");
    assert!(info.is_premium);
}

/// NEXUS-01 fallback: a 401 maps to NexusError::Auth (not a panic / not a generic Http).
#[tokio::test]
async fn validate_api_key_401_maps_to_auth_error() {
    let mut server = mockito::Server::new_async().await;

    let _mock = server
        .mock("GET", "/v1/users/validate.json")
        .with_status(401)
        .with_body(r#"{"message":"Please provide a valid API Key"}"#)
        .create_async()
        .await;

    let err = validate_api_key(&server.url(), "bad-key")
        .await
        .expect_err("401 must error");

    assert!(
        matches!(err, nexus::NexusError::Auth(_)),
        "401 should map to NexusError::Auth, got {err:?}"
    );
}
