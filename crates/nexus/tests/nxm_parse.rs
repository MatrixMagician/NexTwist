//! Strict `nxm://` parser tests (NXM-01 / NEXUS-04).
//!
//! The parser is a **security boundary**: the input is an untrusted URL handed to the app
//! by the OS deep-link handler. These tests pin the four contract behaviours from the plan
//! (valid free link, premium link, oauth-callback, rejected malformed inputs) plus extra
//! spoofing/edge cases, and assert that every malformed input is a typed `Err` — never a
//! panic, never a `Download` carrying bogus ids.

use nexus::{NexusError, NxmLink, NxmLinkKind};

/// Test 1: a full free-user link parses into a Download with numeric ids + opaque
/// key/expires (and user_id), the host becoming the game domain.
#[test]
fn parses_full_free_user_download_link() {
    let url =
        "nxm://skyrimspecialedition/mods/12345/files/67890?key=ABC&expires=1700000000&user_id=42";
    match NxmLink::parse(url).expect("valid free link must parse") {
        NxmLinkKind::Download(link) => {
            assert_eq!(link.game_domain, "skyrimspecialedition");
            assert_eq!(link.mod_id, 12345);
            assert_eq!(link.file_id, 67890);
            assert_eq!(link.key.as_deref(), Some("ABC"));
            assert_eq!(link.expires.as_deref(), Some("1700000000"));
            assert_eq!(link.user_id.as_deref(), Some("42"));
        }
        other => panic!("expected Download, got {other:?}"),
    }
}

/// Test 2: a premium-style link (no key/expires query) parses into a Download with
/// key=None / expires=None.
#[test]
fn parses_premium_link_without_key_or_expires() {
    let url = "nxm://skyrimspecialedition/mods/1/files/2";
    match NxmLink::parse(url).expect("valid premium link must parse") {
        NxmLinkKind::Download(link) => {
            assert_eq!(link.game_domain, "skyrimspecialedition");
            assert_eq!(link.mod_id, 1);
            assert_eq!(link.file_id, 2);
            assert_eq!(link.key, None);
            assert_eq!(link.expires, None);
            assert_eq!(link.user_id, None);
        }
        other => panic!("expected Download, got {other:?}"),
    }
}

/// Test 3: the oauth-callback variant is discriminated from a download and yields the
/// code+state for the Plan-01 code-exchange.
#[test]
fn discriminates_oauth_callback() {
    let url = "nxm://oauth/callback?code=XYZ&state=S";
    match NxmLink::parse(url).expect("valid oauth callback must parse") {
        NxmLinkKind::OAuthCallback { code, state } => {
            assert_eq!(code, "XYZ");
            assert_eq!(state, "S");
        }
        other => panic!("expected OAuthCallback, got {other:?}"),
    }
}

/// Test 4 (rejection): a battery of malformed/spoofed inputs each return `Err` — never a
/// panic, never a partial Download with bogus ids.
#[test]
fn rejects_malformed_and_spoofed_inputs() {
    // Non-nxm scheme (a spoofed http link must never be accepted as a download).
    assert!(matches!(
        NxmLink::parse("https://evil.example/mods/1/files/2"),
        Err(NexusError::Redeem(_))
    ));

    // Missing the /mods/<id>/files/<id> path entirely.
    assert!(matches!(
        NxmLink::parse("nxm://skyrimspecialedition/"),
        Err(NexusError::Redeem(_))
    ));

    // Non-numeric mod id.
    assert!(matches!(
        NxmLink::parse("nxm://skyrimspecialedition/mods/abc/files/2"),
        Err(NexusError::Redeem(_))
    ));

    // Non-numeric file id.
    assert!(matches!(
        NxmLink::parse("nxm://skyrimspecialedition/mods/1/files/xyz"),
        Err(NexusError::Redeem(_))
    ));

    // Wrong path keywords (not /mods/.../files/...).
    assert!(matches!(
        NxmLink::parse("nxm://skyrimspecialedition/collections/1/files/2"),
        Err(NexusError::Redeem(_))
    ));

    // Extra trailing path segment (must be EXACTLY 4 segments).
    assert!(matches!(
        NxmLink::parse("nxm://skyrimspecialedition/mods/1/files/2/extra"),
        Err(NexusError::Redeem(_))
    ));

    // Trailing slash → 5th empty segment → rejected.
    assert!(matches!(
        NxmLink::parse("nxm://skyrimspecialedition/mods/1/files/2/"),
        Err(NexusError::Redeem(_))
    ));

    // Empty / garbage strings.
    assert!(matches!(NxmLink::parse(""), Err(NexusError::Redeem(_))));
    assert!(matches!(NxmLink::parse("garbage"), Err(NexusError::Redeem(_))));
    assert!(matches!(
        NxmLink::parse("nxm://"),
        Err(NexusError::Redeem(_))
    ));

    // A mod id that overflows u64 must be rejected (parse error, not truncation).
    assert!(matches!(
        NxmLink::parse("nxm://game/mods/99999999999999999999999999/files/2"),
        Err(NexusError::Redeem(_))
    ));

    // A malformed oauth-callback (missing state / wrong path) is an Auth error, not Redeem.
    assert!(matches!(
        NxmLink::parse("nxm://oauth/callback?code=XYZ"),
        Err(NexusError::Auth(_))
    ));
    assert!(matches!(
        NxmLink::parse("nxm://oauth/callback?state=S"),
        Err(NexusError::Auth(_))
    ));
    assert!(matches!(
        NxmLink::parse("nxm://oauth/wrongpath?code=XYZ&state=S"),
        Err(NexusError::Auth(_))
    ));
    // Empty code value in the oauth callback is rejected.
    assert!(matches!(
        NxmLink::parse("nxm://oauth/callback?code=&state=S"),
        Err(NexusError::Auth(_))
    ));
}

/// The scheme is matched case-insensitively (RFC 3986 §3.1) but everything else stays
/// strict — a `NXM://` link with a good shape still parses.
#[test]
fn scheme_is_case_insensitive() {
    assert!(matches!(
        NxmLink::parse("NXM://skyrimspecialedition/mods/1/files/2"),
        Ok(NxmLinkKind::Download(_))
    ));
}

/// Percent-encoded query values are decoded into opaque strings (the key/expires are
/// passed verbatim to the redemption; we only undo transport encoding).
#[test]
fn percent_decodes_opaque_query_values() {
    let url = "nxm://skyrimspecialedition/mods/1/files/2?key=a%2Bb%20c";
    match NxmLink::parse(url).expect("must parse") {
        NxmLinkKind::Download(link) => assert_eq!(link.key.as_deref(), Some("a+b c")),
        other => panic!("expected Download, got {other:?}"),
    }
}
