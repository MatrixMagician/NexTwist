//! mockito-backed tests for the Collection parser + availability resolver (COLL-01/02).
//!
//! The resolver is the resolve-before-download HARD GATE: it classifies every pinned mod's
//! availability from metadata reads ONLY, before any download. These tests drive the REST v1
//! file-info responses with a local `mockito` server (NO live NexusMods account) and prove:
//! * a real `collection.json` fixture parses into the typed `Collection`;
//! * `nexus` sources classify Available / Archived / Unavailable from the file-info category;
//! * `bundle` ⇒ Available and `direct`/`browse`/`manual` ⇒ Manual (no request issued);
//! * the resolver issues ZERO `download_link`/CDN requests — only `.../files/{id}.json` reads;
//! * a 429 on a metadata read arms the SHARED limiter (until_ready-first proven);
//! * a stale modRule reference matching no resolved mod is skipped, not fatal.

use std::sync::Arc;

use nexus::client::{NexusAuth, NexusClient};
use nexus::collection::{Collection, ModRuleType, SourceType};
use nexus::resolve::{resolve_collection, ModStatus};
use nexus::RateLimiter;

const FIXTURE: &str = include_str!("fixtures/collection.json");

/// COLL-01: the real fixture parses into the typed Collection with every Manifest-Reference
/// field, including the IChoices FOMOD replay and the source identities.
#[test]
fn fixture_parses_into_typed_collection() {
    let c = Collection::parse(FIXTURE).expect("fixture must parse");
    assert_eq!(c.info.name, "Skyrim Essentials");
    assert_eq!(c.info.domain_name, "skyrimspecialedition");
    assert_eq!(c.mods.len(), 7);

    let skyui = &c.mods[0];
    assert_eq!(skyui.source.kind, SourceType::Nexus);
    assert_eq!(skyui.source.mod_id, Some(12604));
    assert_eq!(skyui.source.file_id, Some(120063));
    let choices = skyui.choices.as_ref().expect("SkyUI pins FOMOD choices");
    assert_eq!(choices.kind, "fomod");
    assert_eq!(choices.options[0].groups[0].choices[0].name, "Full UI");

    // before/after/conflicts + a phantom rule are all present.
    assert_eq!(c.mod_rules.len(), 4);
    assert_eq!(c.mod_rules[0].kind, ModRuleType::After);
}

/// COLL-02: resolve classifies nexus (Available/Archived/Unavailable), bundle (Available),
/// and off-Nexus direct/browse (Manual) correctly — driven by mocked v1 file-info responses.
/// Critically, NO download_link or CDN mock is registered; if the resolver tried to download,
/// the request would 501 (mockito's default for an unmatched route) and the assertions below
/// would not hold — so this also proves the zero-download gate.
#[tokio::test]
async fn resolve_classifies_every_source_type_with_zero_downloads() {
    let mut server = mockito::Server::new_async().await;
    let collection = Collection::parse(FIXTURE).unwrap();
    let domain = "skyrimspecialedition";

    // SkyUI (12604/120063): a normal MAIN file ⇒ Available.
    let m_skyui = server
        .mock("GET", "/v1/games/skyrimspecialedition/mods/12604/files/120063.json")
        .match_header("apikey", "premium-key")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"file_id":120063,"name":"SkyUI","version":"5.2.0","category_name":"MAIN"}"#)
        .create_async()
        .await;

    // USSEP (266/99999): a normal file ⇒ Available.
    let m_ussep = server
        .mock("GET", "/v1/games/skyrimspecialedition/mods/266/files/99999.json")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"file_id":99999,"name":"USSEP","version":"4.3.1","category_name":"MAIN"}"#)
        .create_async()
        .await;

    // Archived Texture Pack (5000/50001): category ARCHIVED ⇒ Archived.
    let m_archived = server
        .mock("GET", "/v1/games/skyrimspecialedition/mods/5000/files/50001.json")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"file_id":50001,"name":"Textures","version":"1.0.0","category_name":"ARCHIVED"}"#)
        .create_async()
        .await;

    // Removed Mod (6000/60001): 404 ⇒ Unavailable.
    let m_removed = server
        .mock("GET", "/v1/games/skyrimspecialedition/mods/6000/files/60001.json")
        .with_status(404)
        .with_body(r#"{"message":"Not found"}"#)
        .create_async()
        .await;

    // A guard mock: if the resolver EVER hits a download_link route, this records it and we
    // assert it was NOT called (zero downloads before the report).
    let m_download = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r"/download_link\.json$".into()),
        )
        .with_status(200)
        .with_body("[]")
        .expect(0)
        .create_async()
        .await;

    let client =
        NexusClient::with_base(&server.url(), NexusAuth::ApiKey("premium-key".into())).unwrap();
    let report = resolve_collection(&client, domain, &collection)
        .await
        .expect("resolve must succeed");

    // 7 mods classified, in manifest order.
    assert_eq!(report.mods.len(), 7);
    assert_eq!(report.mods[0].name, "SkyUI");
    assert_eq!(report.mods[0].status, ModStatus::Available);
    assert_eq!(report.mods[1].status, ModStatus::Available); // USSEP
    assert_eq!(report.mods[2].status, ModStatus::Archived); // Archived Texture Pack
    assert_eq!(report.mods[3].status, ModStatus::Unavailable); // Removed Mod

    // bundle ⇒ Available (no request).
    let bundle = report.mods.iter().find(|m| m.name == "Collection Config Patch").unwrap();
    assert_eq!(bundle.status, ModStatus::Available);
    assert_eq!(bundle.source, SourceType::Bundle);

    // direct + browse ⇒ Manual (off-Nexus, never fetched).
    let skse = report.mods.iter().find(|m| m.name == "SKSE64").unwrap();
    assert_eq!(skse.status, ModStatus::Manual);
    assert_eq!(skse.source, SourceType::Direct);
    let browse = report.mods.iter().find(|m| m.name == "Browse-Only Dependency").unwrap();
    assert_eq!(browse.status, ModStatus::Manual);

    // Helpers reflect the mixed report.
    assert!(!report.all_available(), "archived/unavailable/manual entries exist");
    assert_eq!(report.manual_steps().count(), 2, "SKSE + Browse-only are manual");

    // Each nexus metadata read happened exactly as mocked; NO download was issued.
    m_skyui.assert_async().await;
    m_ussep.assert_async().await;
    m_archived.assert_async().await;
    m_removed.assert_async().await;
    m_download.assert_async().await; // expect(0): the zero-download gate holds.
}

/// COLL-02 / T-04-10: a metadata read goes through the rate limiter (until_ready FIRST). A 429
/// on a SHARED limiter arms a backoff visible to other clients — proving the resolver's
/// per-mod read is gated by the same proactive limiter the download path uses.
#[tokio::test]
async fn resolve_metadata_read_goes_through_shared_limiter() {
    let mut server = mockito::Server::new_async().await;
    // One nexus mod whose file-info returns 429.
    let _m = server
        .mock("GET", "/v1/games/skyrimspecialedition/mods/12604/files/120063.json")
        .with_status(429)
        .with_header("x-rl-hourly-remaining", "0")
        .with_header("x-rl-hourly-reset", "120")
        .create_async()
        .await;

    let manifest = r#"{
        "info": { "name": "One", "domainName": "skyrimspecialedition" },
        "mods": [ { "name": "SkyUI", "version": "5.2.0",
            "source": { "type": "nexus", "modId": 12604, "fileId": 120063 } } ]
    }"#;
    let collection = Collection::parse(manifest).unwrap();

    let limiter = Arc::new(RateLimiter::new());
    assert!(!limiter.is_backing_off(), "fresh limiter is not backing off");

    let client = NexusClient::with_limiter(
        &server.url(),
        NexusAuth::ApiKey("k".into()),
        limiter.clone(),
    )
    .unwrap();

    let err = resolve_collection(&client, "skyrimspecialedition", &collection)
        .await
        .expect_err("a 429 on the metadata read must surface");
    assert!(
        matches!(err, nexus::NexusError::RateLimited(_)),
        "metadata 429 must map to RateLimited, got {err:?}"
    );
    assert!(
        limiter.is_backing_off(),
        "the resolver's metadata read must feed the SHARED limiter (until_ready-first gate)"
    );
}

/// Pitfall 4 / T-04-09: a stale modRule whose source/reference matches no resolved mod is
/// skipped, not fatal. Here we resolve a manifest containing only off-Nexus + bundle mods
/// (no network calls) plus a phantom rule, and confirm resolve still succeeds with the full
/// per-mod report — the rule simply has no resolved target.
#[tokio::test]
async fn stale_mod_rule_reference_is_not_fatal() {
    let server = mockito::Server::new_async().await; // no mocks needed (no nexus sources)

    let manifest = r#"{
        "info": { "name": "Off-Nexus Only", "domainName": "skyrimspecialedition" },
        "mods": [
            { "name": "Bundled", "version": "1.0", "source": { "type": "bundle" } },
            { "name": "ManualDep", "version": "1.0",
              "source": { "type": "manual", "url": "https://example.com/x" } }
        ],
        "modRules": [
            { "type": "after",
              "source": { "logicalFileName": "Ghost A" },
              "reference": { "logicalFileName": "Ghost B" } }
        ]
    }"#;
    let collection = Collection::parse(manifest).unwrap();
    assert_eq!(collection.mod_rules.len(), 1, "the phantom rule parsed");

    let client =
        NexusClient::with_base(&server.url(), NexusAuth::ApiKey("k".into())).unwrap();
    let report = resolve_collection(&client, "skyrimspecialedition", &collection)
        .await
        .expect("resolve must not error on a stale rule");

    assert_eq!(report.mods.len(), 2);
    assert_eq!(report.mods[0].status, ModStatus::Available); // bundle
    assert_eq!(report.mods[1].status, ModStatus::Manual); // manual
}
