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

use crate::error::NexusError;

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

/// A parsed `nxm://` download link (NXM-01 / NEXUS-04).
///
/// Shape (RESEARCH Pattern 5):
/// `nxm://<game_domain>/mods/<mod_id>/files/<file_id>?key=<k>&expires=<ts>&user_id=<u>`.
///
/// `mod_id`/`file_id` are validated as `u64` by the parser — a [`NxmLink`] can only exist
/// with numeric ids. `key`/`expires` are present **only for free-user** "Mod Manager
/// Download" links; a Premium link omits both. They are carried as **opaque strings** and
/// are NEVER interpreted, logged, shelled out, or string-interpolated into a command — they
/// are passed straight to the Plan-02 `download_link` redemption (Security Domain V5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NxmLink {
    /// The NexusMods game domain (the URL host), e.g. `skyrimspecialedition`.
    pub game_domain: String,
    /// The numeric mod id (validated `u64`).
    pub mod_id: u64,
    /// The numeric file id (validated `u64`).
    pub file_id: u64,
    /// The opaque single-use redemption key (free-user links only). NEVER logged.
    pub key: Option<String>,
    /// The opaque link-expiry timestamp string (free-user links only). NEVER logged.
    pub expires: Option<String>,
    /// The opaque user id the link was minted for (free-user links only). NEVER logged.
    pub user_id: Option<String>,
}

/// What an `nxm://` link routes to: a download or the OAuth callback.
///
/// The shell's `on_open_url` handler matches on this to decide whether to drive the
/// Plan-01 OAuth code-exchange or a Plan-02 download — the discrimination is made HERE in
/// the headless crate so the shell stays a thin router.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NxmLinkKind {
    /// `nxm://oauth/callback?code=…&state=…` — routes to the OAuth code-exchange.
    OAuthCallback {
        /// The OAuth authorization code. Opaque; NEVER logged.
        code: String,
        /// The CSRF state to validate against the pending round-trip. NEVER logged.
        state: String,
    },
    /// A mod download link — routes to `start_download` (with `key`/`expires` if free-user).
    Download(NxmLink),
}

impl NxmLink {
    /// Strictly parse an `nxm://` URL into a [`NxmLinkKind`].
    ///
    /// This is a **security boundary**: the input is an untrusted URL handed to the app by
    /// the OS deep-link handler (a malicious/​spoofed link is the primary new attack surface,
    /// threat T-03-12). The parser therefore:
    /// - requires the scheme to be **exactly** `nxm` (case-insensitive per RFC 3986);
    /// - discriminates the `oauth/callback` authority from a download authority;
    /// - for a download, requires the path to be exactly `/mods/<mod_id>/files/<file_id>`
    ///   with BOTH ids parsing as `u64` (any missing/extra/non-numeric segment is rejected);
    /// - parses `key`/`expires`/`user_id`/`code`/`state` from the query as **opaque** strings
    ///   (percent-decoded) without trusting or interpreting them.
    ///
    /// It never panics, never shells out, never interpolates link content into a command,
    /// and never logs the key/expires/code (V5/V7). Every malformed input is a typed
    /// [`NexusError`] (`Redeem` for a bad download link, `Auth` for a bad oauth-callback).
    pub fn parse(input: &str) -> Result<NxmLinkKind, NexusError> {
        let bad = |m: &str| NexusError::Redeem(m.to_string());

        // 1. Scheme must be exactly `nxm` (case-insensitive). Split on the FIRST "://".
        let (scheme, rest) = input
            .split_once("://")
            .ok_or_else(|| bad("not an nxm:// link"))?;
        if !scheme.eq_ignore_ascii_case("nxm") {
            return Err(bad("scheme is not nxm"));
        }

        // 2. Split the remainder into "<authority><path>" and "<query>".
        let (authority_and_path, query) = match rest.split_once('?') {
            Some((ap, q)) => (ap, Some(q)),
            None => (rest, None),
        };
        // Drop any fragment defensively (nxm:// links carry none, but never trust input).
        let authority_and_path = authority_and_path.split('#').next().unwrap_or("");

        // 3. authority = up to the first '/'; the rest (with the leading '/') is the path.
        let (authority, path) = match authority_and_path.split_once('/') {
            Some((a, p)) => (a, format!("/{p}")),
            None => (authority_and_path, String::new()),
        };
        if authority.is_empty() {
            return Err(bad("missing nxm host"));
        }

        // 4. OAuth-callback discrimination: host `oauth`, path `/callback`.
        //    (Malformed oauth-callbacks are an Auth error, not a Redeem error.)
        if authority.eq_ignore_ascii_case("oauth") {
            if !path.eq_ignore_ascii_case("/callback") {
                return Err(NexusError::Auth("malformed nxm oauth callback path".into()));
            }
            let q = query.unwrap_or("");
            let code = query_get(q, "code")
                .ok_or_else(|| NexusError::Auth("oauth callback missing code".into()))?;
            let state = query_get(q, "state")
                .ok_or_else(|| NexusError::Auth("oauth callback missing state".into()))?;
            if code.is_empty() || state.is_empty() {
                return Err(NexusError::Auth("oauth callback empty code/state".into()));
            }
            return Ok(NxmLinkKind::OAuthCallback { code, state });
        }

        // 5. Download link: path must be exactly /mods/<id>/files/<id>, both ids u64.
        let segs: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        // Reject anything but the exact 4-segment shape (no trailing slash, no extras).
        if segs.len() != 4 || !segs[0].eq_ignore_ascii_case("mods") || !segs[2].eq_ignore_ascii_case("files")
        {
            return Err(bad("path is not /mods/<id>/files/<id>"));
        }
        let mod_id: u64 = segs[1].parse().map_err(|_| bad("mod id is not numeric"))?;
        let file_id: u64 = segs[3].parse().map_err(|_| bad("file id is not numeric"))?;

        let q = query.unwrap_or("");
        Ok(NxmLinkKind::Download(NxmLink {
            game_domain: authority.to_string(),
            mod_id,
            file_id,
            // Opaque — passed straight to the download_link redemption, never interpreted.
            key: query_get(q, "key").filter(|s| !s.is_empty()),
            expires: query_get(q, "expires").filter(|s| !s.is_empty()),
            user_id: query_get(q, "user_id").filter(|s| !s.is_empty()),
        }))
    }
}

/// Look up a query-string parameter by name and percent-decode its value.
///
/// A tiny dependency-free `application/x-www-form-urlencoded` reader (mirrors Plan-02's
/// local percent-encoder decision — the workspace `reqwest` stays on its minimal
/// rustls-only feature set, and no `url`/`serde_urlencoded` dep is added). Returns the
/// first match. The decoded value is treated as opaque by every caller.
fn query_get(query: &str, name: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| percent_decode(v))
}

/// Minimal RFC-3986 percent-decoder for query values (also turns `+` into a space, per the
/// form-urlencoded convention). Invalid `%XX` sequences are passed through literally rather
/// than panicking (defensive: untrusted input must never crash the handler).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                match (hi, lo) {
                    (Some(h), Some(l)) => {
                        out.push((h * 16 + l) as u8);
                        i += 3;
                    }
                    _ => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nxm_link_serde_round_trips() {
        let link = NxmLink {
            game_domain: "skyrimspecialedition".into(),
            mod_id: 12345,
            file_id: 67890,
            key: Some("ABC".into()),
            expires: Some("1700000000".into()),
            user_id: Some("42".into()),
        };
        let json = serde_json::to_string(&link).unwrap();
        let back: NxmLink = serde_json::from_str(&json).unwrap();
        assert_eq!(link, back);
    }

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
