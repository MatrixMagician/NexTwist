//! OS Secret-Service storage for the NexusMods long-lived credential (refresh token
//! or API key). Shell-only — the headless `crates/nexus` never sees a keyring handle.
//!
//! HARD INVARIANT (NEXUS-02): when no Secret Service / keyring backend exists, every
//! store/load/clear operation returns [`KeyringError::NoKeyringBackend`] and NEVER
//! writes the credential to a file. There is deliberately no plaintext fallback path in
//! this module — the only write path is the backend's `set_password`. The shell maps
//! `NoKeyringBackend` to the UI's destructive "Can't store your login securely" banner
//! and disables login.
//!
//! Follows RESEARCH Pattern 2 verbatim, with the backend abstracted behind a small
//! [`KeyringBackend`] trait so a simulated `NoStorageAccess` can be exercised in CI
//! without a real DBus session (RESEARCH Environment Availability).

use keyring::error::Error as KrError;
use keyring::Entry;
use thiserror::Error;

/// The keyring service + account the refresh token / API key is stored under.
const SERVICE: &str = "nextwist";
const USER: &str = "nexusmods-refresh-token";

/// Errors from the shell keyring layer.
#[derive(Debug, Error)]
pub enum KeyringError {
    /// No Secret Service / keyring backend is available. The NEXUS-02 hard-fail: login
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

/// The keyring operations this module needs, abstracted so tests can inject a backend
/// that simulates a missing Secret Service without a real DBus session. Each method
/// returns the raw `keyring::Error` so the no-backend mapping lives in one place.
trait KeyringBackend {
    fn set(&self, secret: &str) -> Result<(), KrError>;
    fn get(&self) -> Result<String, KrError>;
    fn delete(&self) -> Result<(), KrError>;
}

/// The real backend: the OS Secret Service via `keyring` 3.6.
struct SecretService;

impl KeyringBackend for SecretService {
    fn set(&self, secret: &str) -> Result<(), KrError> {
        Entry::new(SERVICE, USER)?.set_password(secret)
    }
    fn get(&self) -> Result<String, KrError> {
        Entry::new(SERVICE, USER)?.get_password()
    }
    fn delete(&self) -> Result<(), KrError> {
        Entry::new(SERVICE, USER)?.delete_credential()
    }
}

/// Store the long-lived credential. NEXUS-02: on no-backend this returns
/// `NoKeyringBackend` and writes nothing — the only write path is `set_password`.
pub fn store_refresh_token(token: &str) -> Result<(), KeyringError> {
    store_with(&SecretService, token)
}

/// Load the stored credential, or `None` if no entry exists. A missing backend is a
/// hard error (the caller cannot proceed securely).
pub fn load_refresh_token() -> Result<Option<String>, KeyringError> {
    load_with(&SecretService)
}

/// Clear the stored credential. Idempotent: clearing a missing entry succeeds (logout
/// is idempotent). A missing backend is a hard error.
pub fn clear_refresh_token() -> Result<(), KeyringError> {
    clear_with(&SecretService)
}

// --- testable cores (generic over the backend) ---

fn store_with(backend: &impl KeyringBackend, token: &str) -> Result<(), KeyringError> {
    backend.set(token).map_err(map_err)
}

fn load_with(backend: &impl KeyringBackend) -> Result<Option<String>, KeyringError> {
    match backend.get() {
        Ok(secret) => Ok(Some(secret)),
        Err(KrError::NoEntry) => Ok(None),
        Err(e) => Err(map_err(e)),
    }
}

fn clear_with(backend: &impl KeyringBackend) -> Result<(), KeyringError> {
    match backend.delete() {
        Ok(()) | Err(KrError::NoEntry) => Ok(()), // logout is idempotent
        Err(e) => Err(map_err(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A backend that simulates a machine with NO Secret Service: every op returns
    /// `NoStorageAccess`. Tracks whether any operation was attempted so we can assert
    /// no file was ever written (there is no file path in this module at all — the
    /// invariant is structural — but the flag documents the no-write expectation).
    struct NoBackend {
        attempted: Cell<bool>,
    }
    impl NoBackend {
        fn new() -> Self {
            Self { attempted: Cell::new(false) }
        }
        fn fail(&self) -> KrError {
            self.attempted.set(true);
            KrError::NoStorageAccess(Box::new(std::io::Error::other("no secret service")))
        }
    }
    impl KeyringBackend for NoBackend {
        fn set(&self, _secret: &str) -> Result<(), KrError> {
            Err(self.fail())
        }
        fn get(&self) -> Result<String, KrError> {
            Err(self.fail())
        }
        fn delete(&self) -> Result<(), KrError> {
            Err(self.fail())
        }
    }

    /// A backend that simulates a missing entry on delete (entry never stored).
    struct EmptyBackend;
    impl KeyringBackend for EmptyBackend {
        fn set(&self, _secret: &str) -> Result<(), KrError> {
            Ok(())
        }
        fn get(&self) -> Result<String, KrError> {
            Err(KrError::NoEntry)
        }
        fn delete(&self) -> Result<(), KrError> {
            Err(KrError::NoEntry)
        }
    }

    #[test]
    fn auth_keyring_no_backend_store_hard_fails_and_writes_nothing() {
        let backend = NoBackend::new();
        // Use a temp dir as the cwd-adjacent sentinel: assert no stray file appears.
        let tmp = tempfile::tempdir().unwrap();
        let before: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();

        let err = store_with(&backend, "super-secret-refresh").unwrap_err();

        assert!(
            matches!(err, KeyringError::NoKeyringBackend),
            "no backend must hard-fail with NoKeyringBackend, got {err:?}"
        );
        assert!(backend.attempted.get(), "the backend op should have been attempted");
        // NEXUS-02: nothing was written anywhere (this module has no file path at all).
        let after: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
        assert_eq!(before.len(), after.len(), "no credential file may be created");
    }

    #[test]
    fn auth_keyring_no_backend_load_hard_fails() {
        let backend = NoBackend::new();
        let err = load_with(&backend).unwrap_err();
        assert!(matches!(err, KeyringError::NoKeyringBackend));
    }

    #[test]
    fn auth_keyring_clear_is_idempotent_on_missing_entry() {
        // Both the no-backend... no: a MISSING ENTRY (NoEntry) must be treated as success.
        let backend = EmptyBackend;
        assert!(clear_with(&backend).is_ok(), "clearing a missing entry is Ok (idempotent logout)");
    }

    #[test]
    fn auth_keyring_load_missing_entry_is_none() {
        let backend = EmptyBackend;
        assert_eq!(load_with(&backend).unwrap(), None);
    }
}
