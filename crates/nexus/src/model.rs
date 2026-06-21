//! NexusMods client DTOs.
//!
//! Pure serde data types the headless client speaks. These mirror the relevant
//! fields of the NexusMods REST v1 / OAuth responses; richer mod/file metadata DTOs
//! land in Plan 02. Naming follows the `core::model` round-trip convention.
//!
//! SECURITY (NEXUS-02): [`OAuthTokens`] is an **in-memory** carrier. The short-lived
//! `access` token never touches disk; only the long-lived `refresh` string is handed
//! to the shell to store in the OS keyring. There is deliberately NO code path here
//! (or anywhere in this crate) that serialises an [`OAuthTokens`] to a file — the
//! `Serialize` impl exists only for IPC/test round-tripping, and the shell persists
//! the refresh *string*, never this struct.

use serde::{Deserialize, Serialize};

/// The authenticated NexusMods user, as returned by REST v1 `/v1/users/validate.json`.
///
/// `is_premium` drives the account-panel tier tag ("Premium" / "Free") and, later,
/// which download path the UI offers (in-app direct vs the website handoff).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInfo {
    /// Stable NexusMods user id.
    pub user_id: u64,
    /// Display name shown in the account panel.
    pub name: String,
    /// Whether the account is Premium (gates the in-app direct-download affordance).
    pub is_premium: bool,
}

/// OAuth2 tokens from a successful code exchange.
///
/// `access` is short-lived and kept **in memory only** (the shell's `AppState`); it is
/// never written to the keyring or any file. `refresh` (when the provider returns one)
/// is the long-lived credential the shell stores in the OS Secret Service. This struct
/// is never persisted to disk as a whole — see the module-level security note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthTokens {
    /// Short-lived OAuth access token — in-memory only, never persisted.
    pub access: String,
    /// Long-lived refresh token (when issued) — the only value the shell puts in the
    /// keyring. `None` when the provider issues no refresh token.
    pub refresh: Option<String>,
}

/// One CDN download link entry, as returned by REST v1 `download_link.json`.
///
/// The endpoint returns an array `[{ "name": …, "short_name": …, "URI": … }, …]`. We
/// keep the first entry's `uri` to stream from. NexusMods serialises the field as the
/// upper-case `URI`, so the serde rename is load-bearing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadLink {
    /// Human-readable CDN name (e.g. "Nexus CDN").
    pub name: String,
    /// Short CDN name (e.g. "Nexus").
    pub short_name: String,
    /// The actual HTTPS CDN URI to stream the file from. NEVER logged.
    #[serde(rename = "URI")]
    pub uri: String,
}

/// Mod-file metadata read over GraphQL v2 (version + display name).
///
/// v2 is the modern read path for metadata (RESEARCH Pitfall 2); the download link
/// itself still comes from REST v1. These two fields are what the provenance record
/// and the downloads-list row label need.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModFile {
    /// The file's version string (e.g. "1.6.3").
    pub version: String,
    /// The file's display name shown in the downloads list.
    pub display_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_link_parses_nexus_uri_field() {
        // NexusMods serialises the URI as the upper-case `URI`; the rename must catch it.
        let json = r#"[{"name":"Nexus CDN","short_name":"Nexus","URI":"https://cdn.example/file.zip"}]"#;
        let links: Vec<DownloadLink> = serde_json::from_str(json).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].uri, "https://cdn.example/file.zip");
        assert_eq!(links[0].short_name, "Nexus");
    }

    #[test]
    fn mod_file_serde_round_trips() {
        let f = ModFile {
            version: "1.6.3".into(),
            display_name: "Skyrim Script Extender".into(),
        };
        let json = serde_json::to_string(&f).unwrap();
        let back: ModFile = serde_json::from_str(&json).unwrap();
        assert_eq!(f, back);
    }

    #[test]
    fn user_info_serde_round_trips() {
        let u = UserInfo {
            user_id: 42,
            name: "modder".into(),
            is_premium: true,
        };
        let json = serde_json::to_string(&u).unwrap();
        let back: UserInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(u, back);
    }

    #[test]
    fn oauth_tokens_serde_round_trips_and_preserves_fields() {
        let t = OAuthTokens {
            access: "access-xyz".into(),
            refresh: Some("refresh-abc".into()),
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: OAuthTokens = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
        assert_eq!(back.access, "access-xyz");
        assert_eq!(back.refresh.as_deref(), Some("refresh-abc"));

        // A token with no refresh round-trips too (provider may omit it).
        let no_refresh = OAuthTokens {
            access: "a".into(),
            refresh: None,
        };
        let json = serde_json::to_string(&no_refresh).unwrap();
        let back: OAuthTokens = serde_json::from_str(&json).unwrap();
        assert_eq!(no_refresh, back);
    }
}
