//! DIST-01 self-test wiring: prove the `nxm://` handler self-test is non-fatal.
//!
//! True end-to-end `nxm://` registration needs a real OS desktop session + a built
//! AppImage, so it is a manual UAT item. What this headless test pins is the *contract*
//! the startup `setup` hook depends on: the extracted `nxm_self_test(..)` helper that
//! wraps the plugin's `is_registered("nxm")` `Result` MUST consume every arm — PASS,
//! "not the default handler", and a query error — and return `()` WITHOUT panicking or
//! propagating. That is the locked "warn-and-continue" decision (T-05-02): a minimal
//! distro lacking `xdg-mime` must still let the app open.
//!
//! The helper is generic over the error type, so this test needs neither a live OS
//! session nor an installed `xdg-mime` to exercise all three arms.

use nextwist_lib::nxm_self_test;

/// A stand-in for the plugin's `Error` (which is not publicly constructible) — any
/// `Display` error flows through the same `Err(_)` arm the live `is_registered` uses.
#[derive(Debug)]
struct FakeQueryError;

impl std::fmt::Display for FakeQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("xdg-mime unavailable")
    }
}

#[test]
fn self_test_pass_arm_is_non_fatal() {
    // Ok(true): NexTwist IS the registered default handler.
    let result: Result<bool, FakeQueryError> = Ok(true);
    nxm_self_test(result); // returns () — must not panic.
}

#[test]
fn self_test_not_registered_arm_is_non_fatal() {
    // Ok(false): something else owns the nxm:// default — a WARN, never an abort.
    let result: Result<bool, FakeQueryError> = Ok(false);
    nxm_self_test(result);
}

#[test]
fn self_test_query_error_arm_is_non_fatal() {
    // Err(_): xdg-mime missing on a minimal distro — must still warn-and-continue.
    let result: Result<bool, FakeQueryError> = Err(FakeQueryError);
    nxm_self_test(result);
}
