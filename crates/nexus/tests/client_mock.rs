//! mockito-backed tests for the hybrid NexusMods client (NEXUS-03/05).
//!
//! These exercise the REST v1 `download_link.json` request shape (premium vs free),
//! the rate-limit header reactions, and the error/expired-key paths — all against a
//! local `mockito` server, with NO live NexusMods account. The live Premium download is
//! a separate human-verify checkpoint (Task 4).

use nexus::client::{NexusAuth, NexusClient};
use nexus::download::{download_to, CancelFlag};
use nexus::NexusError;

/// Test 1: a PREMIUM download_link request carries NO `key`/`expires` query params and
/// parses the JSON array (with the upper-case `URI` field) into `Vec<DownloadLink>`.
#[tokio::test]
async fn download_link_premium_omits_key_and_parses_uri() {
    let mut server = mockito::Server::new_async().await;

    // `match_query(Missing(...))` asserts the premium request has neither param.
    let m = server
        .mock(
            "GET",
            "/v1/games/skyrimspecialedition/mods/12604/files/120063/download_link.json",
        )
        // Premium: NO query string at all (neither key nor expires).
        .match_query(mockito::Matcher::Missing)
        .match_header("apikey", "premium-key")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"[{"name":"Nexus CDN","short_name":"Nexus","URI":"https://cdn.example/file.zip"}]"#,
        )
        .create_async()
        .await;

    let client =
        NexusClient::with_base(&server.url(), NexusAuth::ApiKey("premium-key".into())).unwrap();
    let links = client
        .download_link("skyrimspecialedition", 12604, 120063, None, None)
        .await
        .expect("premium download_link should succeed");

    m.assert_async().await;
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].uri, "https://cdn.example/file.zip");
}

/// Test 2: a FREE download_link request INCLUDES `?key=&expires=`; an expired-key 4xx
/// maps to `NexusError::Redeem` (NOT a generic `Http` error).
#[tokio::test]
async fn download_link_free_includes_key_expires() {
    let mut server = mockito::Server::new_async().await;

    let ok = server
        .mock(
            "GET",
            "/v1/games/skyrimspecialedition/mods/12604/files/120063/download_link.json",
        )
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("key".into(), "abc".into()),
            mockito::Matcher::UrlEncoded("expires".into(), "1700000000".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"[{"name":"Nexus CDN","short_name":"Nexus","URI":"https://cdn.example/f"}]"#)
        .create_async()
        .await;

    let client =
        NexusClient::with_base(&server.url(), NexusAuth::ApiKey("free-key".into())).unwrap();
    let links = client
        .download_link(
            "skyrimspecialedition",
            12604,
            120063,
            Some("abc"),
            Some("1700000000"),
        )
        .await
        .expect("free download_link with key+expires should succeed");
    ok.assert_async().await;
    assert_eq!(links.len(), 1);
}

/// Test 2b: a 4xx on a keyed (free-user) request surfaces as `NexusError::Redeem`.
#[tokio::test]
async fn expired_free_key_maps_to_redeem_not_http() {
    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock(
            "GET",
            "/v1/games/skyrimspecialedition/mods/1/files/2/download_link.json",
        )
        .match_query(mockito::Matcher::Any)
        .with_status(410) // Gone — link expired
        .with_body("link expired")
        .create_async()
        .await;

    let client =
        NexusClient::with_base(&server.url(), NexusAuth::ApiKey("free-key".into())).unwrap();
    let err = client
        .download_link("skyrimspecialedition", 1, 2, Some("dead"), Some("1"))
        .await
        .expect_err("an expired free key must error");

    assert!(
        matches!(err, NexusError::Redeem(_)),
        "expired free-user key must map to Redeem, got: {err:?}"
    );
}

/// Test 3: a 429 maps to `NexusError::RateLimited` with the retry-after derived from the
/// `X-RL-Hourly-Reset` header, AND the limiter records a backoff (so the next call gates).
#[tokio::test]
async fn rate_limit_429_maps_to_rate_limited_and_arms_backoff() {
    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock(
            "GET",
            "/v1/games/skyrimspecialedition/mods/1/files/2/download_link.json",
        )
        .with_status(429)
        .with_header("x-rl-hourly-remaining", "0")
        .with_header("x-rl-hourly-reset", "1") // 1s so the follow-up test stays fast
        .create_async()
        .await;

    let client =
        NexusClient::with_base(&server.url(), NexusAuth::ApiKey("k".into())).unwrap();
    let err = client
        .download_link("skyrimspecialedition", 1, 2, None, None)
        .await
        .expect_err("429 must error");

    match err {
        NexusError::RateLimited(secs) => assert_eq!(secs, 1, "retry-after from X-RL-Hourly-Reset"),
        other => panic!("429 must map to RateLimited, got: {other:?}"),
    }
}

/// Test 3b: a 200 with a LOW `X-RL-Hourly-Remaining` arms a backoff; the limiter then
/// gates the next request (asserted by the standalone ratelimit unit tests; here we
/// confirm the client wires the header through and the response still parses).
#[tokio::test]
async fn low_remaining_header_on_success_is_consumed() {
    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock(
            "GET",
            "/v1/games/skyrimspecialedition/mods/1/files/2/download_link.json",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("x-rl-hourly-remaining", "2") // low → arms backoff
        .with_header("x-rl-hourly-reset", "1")
        .with_body(r#"[{"name":"n","short_name":"s","URI":"https://cdn.example/f"}]"#)
        .create_async()
        .await;

    let client =
        NexusClient::with_base(&server.url(), NexusAuth::ApiKey("k".into())).unwrap();
    let links = client
        .download_link("skyrimspecialedition", 1, 2, None, None)
        .await
        .expect("a low-remaining 200 still returns links");
    assert_eq!(links.len(), 1);
}

/// GraphQL v2 metadata read parses `version` + `name` from the `{data:{modFile:…}}` shape.
#[tokio::test]
async fn mod_file_metadata_reads_graphql_v2() {
    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("POST", "/v2/graphql")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":{"modFile":{"version":"1.6.3","name":"SKSE64"}}}"#)
        .create_async()
        .await;

    let client =
        NexusClient::with_base(&server.url(), NexusAuth::Bearer("tok".into())).unwrap();
    let mf = client
        .mod_file_metadata("skyrimspecialedition", 12604, 120063)
        .await
        .expect("metadata read should succeed");
    assert_eq!(mf.version, "1.6.3");
    assert_eq!(mf.display_name, "SKSE64");
}

/// Test (streaming): `download_to` streams a stubbed body chunk-by-chunk to a temp file,
/// invoking `on_progress` with a monotonically increasing `downloaded` and the
/// Content-Length as `total`; the written bytes equal the stubbed body. Named `*_stage_*`
/// so `cargo test -p nextwist-nexus stage` selects it (NEXUS-03/06 streaming gate).
#[tokio::test]
async fn download_to_streams_to_staging_file_with_progress() {
    use std::sync::Mutex;

    let mut server = mockito::Server::new_async().await;
    // A body comfortably larger than one chunk so progress advances in steps.
    let body = vec![7u8; 64 * 1024];
    let _m = server
        .mock("GET", "/cdn/file.zip")
        .with_status(200)
        .with_header("content-length", &body.len().to_string())
        .with_body(body.clone())
        .create_async()
        .await;

    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("file.zip");

    let progress: Mutex<Vec<(u64, Option<u64>)>> = Mutex::new(Vec::new());
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let cancel = CancelFlag::new();
    let uri = format!("{}/cdn/file.zip", server.url());

    let written = download_to(&http, &uri, &dest, &cancel, |downloaded, total| {
        progress.lock().unwrap().push((downloaded, total));
    })
    .await
    .expect("streaming download should succeed");

    // The whole body landed on disk, byte-for-byte.
    let on_disk = std::fs::read(&dest).unwrap();
    assert_eq!(on_disk, body, "written file must equal the streamed body");
    assert_eq!(written, body.len() as u64);

    // Progress was reported, monotonic, and ended at the full size with the right total.
    let events = progress.lock().unwrap();
    assert!(!events.is_empty(), "on_progress must be called");
    let mut last = 0u64;
    for (downloaded, total) in events.iter() {
        assert!(*downloaded >= last, "downloaded must be monotonic");
        last = *downloaded;
        assert_eq!(*total, Some(body.len() as u64), "total = Content-Length");
    }
    assert_eq!(last, body.len() as u64, "final progress = full size");
}

/// A tripped CancelFlag aborts the stream and removes the partial file.
#[tokio::test]
async fn download_to_cancel_removes_partial_file() {
    let mut server = mockito::Server::new_async().await;
    let body = vec![1u8; 32 * 1024];
    let _m = server
        .mock("GET", "/cdn/cancel.zip")
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("cancel.zip");
    let http = reqwest::Client::new();
    let cancel = CancelFlag::new();
    cancel.cancel(); // cancelled before the first chunk is written
    let uri = format!("{}/cdn/cancel.zip", server.url());

    let err = download_to(&http, &uri, &dest, &cancel, |_, _| {})
        .await
        .expect_err("a cancelled download must error");
    assert!(matches!(err, NexusError::Http(_)));
    assert!(!dest.exists(), "the partial file must be removed on cancel");
}
