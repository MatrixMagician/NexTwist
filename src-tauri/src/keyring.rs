//! OS Secret-Service storage for the NexusMods long-lived credential (refresh token
//! or API key). Shell-only — the headless `crates/nexus` never sees a keyring handle.
//!
//! HARD INVARIANT: when no Secret Service / keyring backend exists, every
//! store/load/clear operation returns [`KeyringError::NoKeyringBackend`] and NEVER
//! writes the credential to a file. That guarantee is STRUCTURAL: this module reaches
//! for no file API at all, and the only write path is the backend's `set_password`.
//! There is deliberately no plaintext fallback. The shell maps `NoKeyringBackend` to the
//! UI's destructive "Can't store your login securely" banner and disables login.
//!
//! All the branching lives in the three pure mappers below ([`map_err`], [`map_load`],
//! [`map_clear`]), which are tested directly — a raw `keyring::Error` is constructible in
//! a test, so simulating a machine with no Secret Service needs no DBus session and no
//! injected backend.

use keyring::Entry;
use keyring::error::Error as KrError;
use thiserror::Error;

/// The keyring service + account the refresh token / API key is stored under.
const SERVICE: &str = "nextwist";
const USER: &str = "nexusmods-refresh-token";

/// Errors from the shell keyring layer.
#[derive(Debug, Error)]
pub enum KeyringError {
    /// No Secret Service / keyring backend is available. A hard-fail: login
    /// is blocked and NOTHING is written to disk. Never downgraded to a plaintext file.
    #[error("no keyring backend available — refusing to store credentials as plaintext")]
    NoKeyringBackend,
    /// Any other keyring failure (a real backend present but the operation failed).
    #[error("keyring error: {0}")]
    Keyring(String),
}

/// Map a raw keyring error to [`KeyringError`], collapsing the no-backend conditions
/// (`NoStorageAccess` / `PlatformFailure`) to the hard-fail variant.
fn map_err(e: KrError) -> KeyringError {
    match e {
        KrError::NoStorageAccess(_) | KrError::PlatformFailure(_) => KeyringError::NoKeyringBackend,
        other => KeyringError::Keyring(other.to_string()),
    }
}

/// Map a raw `get_password` result: an absent entry reads as `None`, not an error.
fn map_load(r: Result<String, KrError>) -> Result<Option<String>, KeyringError> {
    match r {
        Ok(secret) => Ok(Some(secret)),
        Err(KrError::NoEntry) => Ok(None),
        Err(e) => Err(map_err(e)),
    }
}

/// Map a raw `delete_credential` result: clearing an absent entry succeeds, so logout
/// is idempotent.
fn map_clear(r: Result<(), KrError>) -> Result<(), KeyringError> {
    match r {
        Ok(()) | Err(KrError::NoEntry) => Ok(()),
        Err(e) => Err(map_err(e)),
    }
}

/// The OS Secret Service entry this module stores the credential under.
fn entry() -> Result<Entry, KrError> {
    Entry::new(SERVICE, USER)
}

/// Store the long-lived credential. On no-backend this returns
/// `NoKeyringBackend` and writes nothing — the only write path is `set_password`.
pub fn store_refresh_token(token: &str) -> Result<(), KeyringError> {
    entry().and_then(|e| e.set_password(token)).map_err(map_err)
}

/// Load the stored credential, or `None` if no entry exists. A missing backend is a
/// hard error (the caller cannot proceed securely).
pub fn load_refresh_token() -> Result<Option<String>, KeyringError> {
    map_load(entry().and_then(|e| e.get_password()))
}

/// Clear the stored credential. Idempotent: clearing a missing entry succeeds (logout
/// is idempotent). A missing backend is a hard error.
pub fn clear_refresh_token() -> Result<(), KeyringError> {
    map_clear(entry().and_then(|e| e.delete_credential()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The error a machine with no Secret Service produces.
    fn no_storage_access() -> KrError {
        KrError::NoStorageAccess(Box::new(std::io::Error::other("no secret service")))
    }

    /// The hard invariant: with no keyring backend, storing hard-fails. Nothing can be
    /// written to disk because this module reaches for no file API — the guarantee is
    /// structural, so the error mapping is the whole of what there is to test.
    #[test]
    fn auth_keyring_no_backend_hard_fails_never_plaintext() {
        for e in [
            no_storage_access(),
            KrError::PlatformFailure(Box::new(std::io::Error::other("dbus down"))),
        ] {
            let mapped = map_err(e);
            assert!(
                matches!(mapped, KeyringError::NoKeyringBackend),
                "no backend must hard-fail with NoKeyringBackend, got {mapped:?}"
            );
        }
    }

    /// A real backend that merely failed an operation must NOT be reported as a missing
    /// backend — that would trigger the UI's destructive "can't store your login" banner.
    #[test]
    fn auth_keyring_other_failure_is_not_a_missing_backend() {
        let mapped = map_err(KrError::Invalid("attribute".into(), "bad".into()));
        assert!(matches!(mapped, KeyringError::Keyring(_)));
    }

    #[test]
    fn auth_keyring_no_backend_load_hard_fails() {
        let err = map_load(Err(no_storage_access())).unwrap_err();
        assert!(matches!(err, KeyringError::NoKeyringBackend));
    }

    #[test]
    fn auth_keyring_load_missing_entry_is_none() {
        assert_eq!(map_load(Err(KrError::NoEntry)).unwrap(), None);
        assert_eq!(
            map_load(Ok("secret".to_string())).unwrap(),
            Some("secret".to_string())
        );
    }

    #[test]
    fn auth_keyring_clear_is_idempotent_on_missing_entry() {
        assert!(
            map_clear(Err(KrError::NoEntry)).is_ok(),
            "clearing a missing entry is Ok (idempotent logout)"
        );
        assert!(map_clear(Ok(())).is_ok());
    }

    #[test]
    fn auth_keyring_no_backend_clear_hard_fails() {
        let err = map_clear(Err(no_storage_access())).unwrap_err();
        assert!(matches!(err, KeyringError::NoKeyringBackend));
    }
}
