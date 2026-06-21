//! `nextwist-nexus` — headless NexusMods API client.
//!
//! This crate owns everything that is *pure client logic* in NexusMods integration:
//! OAuth2+PKCE token exchange, API-key validation, REST v1 / GraphQL v2 metadata,
//! download-link generation, streaming download, and the `governor` rate limiter.
//!
//! Tauri-free and **keyring-free** by design (NEXUS-01/02 locked decision). The
//! src-tauri shell owns ALL OS-integration — the keyring (Secret Service), `nxm://`
//! deep-link registration + capture, single-instance forwarding, and opening the
//! system browser for the OAuth round-trip — and passes token *values* into this
//! client. The client never holds a keyring handle or a Tauri type.
//!
//! HTTP is async `reqwest` with `rustls` only (never native-tls), the redirect policy
//! disabled and `error_for_status()` enforced — the same security-hardened shape as
//! `crates/loadorder`'s masterlist fetch, converted from blocking to async (the two
//! clients never share a call path; this crate runs on the shell's tokio runtime).
//!
//! Plan 01 (this slice) lands `error`, `model`, and `auth` (OAuth2-PKCE + API-key).
//! `client`, `download`, and `ratelimit` are declared here but carry stub bodies until
//! Plan 02 layers the download flow on top of this auth spine.

pub mod auth;
pub mod client;
pub mod download;
pub mod error;
pub mod model;
pub mod ratelimit;

pub use auth::{build_authorize_url, exchange_code, validate_api_key, AuthorizeRequest};
pub use error::NexusError;
pub use model::{OAuthTokens, UserInfo};
