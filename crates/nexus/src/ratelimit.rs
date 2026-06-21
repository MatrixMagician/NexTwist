//! Client-side rate limiting (NEXUS-05).
//!
//! Two layers, per RESEARCH Pattern 6:
//!
//! 1. **Proactive** — a `governor` direct token-bucket limiter sized to the documented
//!    NexusMods budget. [`RateLimiter::until_ready`] gates *before* each request so a
//!    burst can never exceed the bucket; the bucket recovers at the configured rate.
//! 2. **Reactive** — after each response, [`RateLimiter::note_headers`] reads the
//!    `X-RL-*` headers (and a possible 429) and, when the remaining budget is low or a
//!    429 was seen, records a backoff *deadline*. `until_ready` then also sleeps until
//!    that deadline, so the client never walks into a self-inflicted ban.
//!
//! The exact budget numbers (RESEARCH A4) and header casing (RESEARCH A3) are
//! `[ASSUMED]` until confirmed against a live response — so they are centralised here as
//! consts and the *reactive* header path is the real protection (it adapts to whatever
//! the live `X-RL-*` headers report, regardless of the proactive bucket's sizing).
//!
//! No secret/URI is ever logged from this module.

use std::num::NonZeroU32;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter as GovLimiter};
use reqwest::header::HeaderMap;

/// Documented NexusMods per-hour request cap for API-key users (RESEARCH A4, `[ASSUMED]`).
/// The reactive header path corrects for the real budget; this only sizes the proactive
/// bucket so a runaway loop is throttled even before the first response is seen.
const HOURLY_CAP: u32 = 100;

/// When `X-RL-Hourly-Remaining` (or `-Daily-Remaining`) drops to or below this, we begin
/// backing off until the corresponding `-Reset` so we glide to the limit instead of
/// slamming into a 429.
const LOW_REMAINING_THRESHOLD: u64 = 5;

/// Fallback backoff (when a header signals "low"/429 but carries no usable `-Reset`).
const DEFAULT_BACKOFF: Duration = Duration::from_secs(60);

// --- X-RL-* header names (centralised — RESEARCH A3 flags the exact casing as ASSUMED). ---
const H_HOURLY_REMAINING: &str = "x-rl-hourly-remaining";
const H_HOURLY_RESET: &str = "x-rl-hourly-reset";
const H_DAILY_REMAINING: &str = "x-rl-daily-remaining";
const H_DAILY_RESET: &str = "x-rl-daily-reset";

/// A proactive token-bucket limiter plus a reactive `X-RL-*` backoff deadline.
pub struct RateLimiter {
    /// The governor direct (un-keyed) limiter sized to [`HOURLY_CAP`].
    limiter: GovLimiter<NotKeyed, InMemoryState, DefaultClock>,
    /// The reactive backoff deadline: when set and in the future, `until_ready` sleeps
    /// until it. Behind a `Mutex` so `note_headers(&self, …)` can record it.
    backoff_until: Mutex<Option<Instant>>,
}

impl RateLimiter {
    /// Build a limiter sized to the documented NexusMods hourly budget.
    pub fn new() -> Self {
        Self::with_hourly_cap(HOURLY_CAP)
    }

    /// Build a limiter with an explicit hourly cap (used by tests for a tight quota).
    pub fn with_hourly_cap(cap: u32) -> Self {
        let cap = NonZeroU32::new(cap.max(1)).expect("cap >= 1");
        // `allow_burst(cap)` lets the bucket start full so a first request is immediate;
        // it then refills at cap-per-hour. This models Nexus's "budget + recovery".
        let quota = Quota::per_hour(cap).allow_burst(cap);
        RateLimiter {
            limiter: GovLimiter::direct(quota),
            backoff_until: Mutex::new(None),
        }
    }

    /// Proactive gate: await both the token bucket AND any reactive backoff deadline.
    ///
    /// Returns once a request may be issued. When a backoff deadline is in the future
    /// (set by [`note_headers`]), this sleeps until it first, then waits for the bucket.
    ///
    /// WR-03: with one shared limiter fronting parallel requests, the deadline may be
    /// re-armed (extended) by a concurrent [`note_headers`] while we sleep. So we loop:
    /// after each sleep we re-read the deadline and only proceed once it has elapsed,
    /// and we clear it only if no later deadline was armed meanwhile — never blowing away
    /// a freshly-recorded future backoff.
    pub async fn until_ready(&self) {
        // 1. Reactive backoff: sleep until the recorded deadline, re-checking after each
        //    sleep in case a concurrent response extended it.
        loop {
            let wait = {
                let guard = self.backoff_until.lock().expect("backoff lock");
                guard.and_then(|deadline| deadline.checked_duration_since(Instant::now()))
            };
            match wait {
                Some(d) => {
                    tracing::info!(
                        backoff_secs = d.as_secs(),
                        "rate-limit backoff before next request"
                    );
                    tokio::time::sleep(d).await;
                    // Loop: a concurrent note_headers may have pushed the deadline later.
                }
                None => {
                    // No future deadline. Clear an elapsed one (best-effort) and proceed.
                    let mut guard = self.backoff_until.lock().expect("backoff lock");
                    if matches!(*guard, Some(deadline) if deadline <= Instant::now()) {
                        *guard = None;
                    }
                    break;
                }
            }
        }

        // 2. Proactive bucket: wait for a token.
        self.limiter.until_ready().await;
    }

    /// True if a backoff deadline is currently set and still in the future. Diagnostic /
    /// test helper (drives the UI's "Pausing to respect rate limits…" notice in the shell).
    pub fn is_backing_off(&self) -> bool {
        self.backoff_until
            .lock()
            .expect("backoff lock")
            .map(|d| d > Instant::now())
            .unwrap_or(false)
    }

    /// Reactively record a backoff from a response's `X-RL-*` headers.
    ///
    /// If `status_429` is true, OR the hourly/daily remaining is at/below
    /// [`LOW_REMAINING_THRESHOLD`], schedule a backoff until the matching `-Reset`
    /// (in seconds-from-now), falling back to [`DEFAULT_BACKOFF`].
    ///
    /// WR-03: a healthy response only clears an *already-elapsed* backoff deadline — it
    /// must NEVER clear a deadline that is still in the future. Because one shared limiter
    /// fronts all parallel NexusMods requests, a healthy header on a concurrent in-flight
    /// request (e.g. a cheaper/cached endpoint) would otherwise wipe a freshly-armed 429
    /// backoff from another request, walking straight into a self-inflicted ban. We only
    /// drop a stale (past) deadline so a future backoff survives concurrent healthy ticks.
    pub fn note_headers(&self, headers: &HeaderMap, status_429: bool) {
        let hourly_remaining = parse_u64(headers, H_HOURLY_REMAINING);
        let daily_remaining = parse_u64(headers, H_DAILY_REMAINING);

        let low = matches!(hourly_remaining, Some(r) if r <= LOW_REMAINING_THRESHOLD)
            || matches!(daily_remaining, Some(r) if r <= LOW_REMAINING_THRESHOLD);

        if status_429 || low {
            // Prefer the hourly reset, then the daily reset, then the default.
            let reset_secs = parse_u64(headers, H_HOURLY_RESET)
                .or_else(|| parse_u64(headers, H_DAILY_RESET))
                .map(Duration::from_secs)
                .unwrap_or(DEFAULT_BACKOFF);
            let deadline = Instant::now() + reset_secs;
            let mut guard = self.backoff_until.lock().expect("backoff lock");
            // Extend, never shorten: keep the later of any existing future deadline and the
            // newly-derived one so a concurrent weaker signal can't cut a stronger backoff.
            *guard = Some(match *guard {
                Some(existing) if existing > deadline => existing,
                _ => deadline,
            });
            tracing::warn!(
                backoff_secs = reset_secs.as_secs(),
                status_429,
                "recording rate-limit backoff from X-RL-* headers"
            );
        } else if hourly_remaining.is_some() || daily_remaining.is_some() {
            // Budget is healthy and the server reported it. Only clear a backoff that has
            // ALREADY elapsed — a still-future deadline (armed by a concurrent 429/low
            // response) must survive (WR-03).
            let mut guard = self.backoff_until.lock().expect("backoff lock");
            if matches!(*guard, Some(deadline) if deadline <= Instant::now()) {
                *guard = None;
            }
        }
    }

    /// Derive the retry-after seconds for a 429 from the `X-RL-*-Reset` headers (for
    /// `NexusError::RateLimited`). Falls back to [`DEFAULT_BACKOFF`]'s seconds.
    pub fn retry_after_secs(headers: &HeaderMap) -> u64 {
        parse_u64(headers, H_HOURLY_RESET)
            .or_else(|| parse_u64(headers, H_DAILY_RESET))
            .unwrap_or(DEFAULT_BACKOFF.as_secs())
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a numeric `X-RL-*` header value, case-insensitively keyed.
fn parse_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue};

    fn hm(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    /// Test 4: with budget available, `until_ready` resolves immediately (not flaky).
    #[tokio::test]
    async fn until_ready_is_immediate_when_budget_available() {
        let rl = RateLimiter::with_hourly_cap(100);
        let start = Instant::now();
        rl.until_ready().await;
        assert!(start.elapsed() < Duration::from_millis(200), "first token must be immediate");
        assert!(!rl.is_backing_off());
    }

    /// Test 3a: a low `X-RL-Hourly-Remaining` records a backoff deadline.
    #[test]
    fn low_remaining_header_records_backoff() {
        let rl = RateLimiter::new();
        assert!(!rl.is_backing_off());
        rl.note_headers(&hm(&[("x-rl-hourly-remaining", "1"), ("x-rl-hourly-reset", "120")]), false);
        assert!(rl.is_backing_off(), "low remaining must arm a backoff");
    }

    /// Test 3b: a 429 arms a backoff and `retry_after_secs` reads the reset header.
    #[test]
    fn status_429_records_backoff_and_retry_after() {
        let rl = RateLimiter::new();
        let headers = hm(&[("x-rl-hourly-reset", "90")]);
        rl.note_headers(&headers, true);
        assert!(rl.is_backing_off());
        assert_eq!(RateLimiter::retry_after_secs(&headers), 90);
    }

    /// WR-03: a healthy response must NOT clear a still-FUTURE backoff deadline. With one
    /// shared limiter fronting parallel requests, a concurrent healthy header (a cheaper
    /// endpoint) would otherwise wipe a freshly-armed 429 backoff and walk into a ban.
    #[test]
    fn healthy_remaining_does_not_clear_future_backoff() {
        let rl = RateLimiter::new();
        rl.note_headers(&hm(&[("x-rl-hourly-remaining", "0"), ("x-rl-hourly-reset", "60")]), false);
        assert!(rl.is_backing_off());
        // A concurrent healthy response arrives while the 60s backoff is still in the
        // future: it must be IGNORED, not allowed to clear the armed deadline.
        rl.note_headers(&hm(&[("x-rl-hourly-remaining", "99")]), false);
        assert!(rl.is_backing_off(), "a future backoff must survive a healthy response");
    }

    /// WR-03: an ALREADY-elapsed backoff is cleared by a healthy response (a deadline of
    /// 0s is in the past by the time we re-check), so a stale deadline doesn't linger.
    #[test]
    fn healthy_remaining_clears_elapsed_backoff() {
        let rl = RateLimiter::new();
        // Reset of 0 → the deadline is effectively now/past immediately.
        rl.note_headers(&hm(&[("x-rl-hourly-remaining", "0"), ("x-rl-hourly-reset", "0")]), false);
        // is_backing_off compares strictly `> now`, so a 0s deadline already reads false.
        assert!(!rl.is_backing_off(), "a 0s deadline is already elapsed");
        rl.note_headers(&hm(&[("x-rl-hourly-remaining", "99")]), false);
        assert!(!rl.is_backing_off(), "an elapsed backoff is cleared by a healthy response");
    }

    /// WR-03: a stronger (later) backoff is never shortened by a weaker concurrent signal.
    #[test]
    fn later_backoff_is_not_shortened_by_earlier_one() {
        let rl = RateLimiter::new();
        rl.note_headers(&hm(&[("x-rl-hourly-reset", "300")]), true); // arm a long 300s backoff
        assert!(rl.is_backing_off());
        // A concurrent low-remaining response with a SHORTER reset must not cut it down.
        rl.note_headers(&hm(&[("x-rl-hourly-remaining", "1"), ("x-rl-hourly-reset", "10")]), false);
        assert!(rl.is_backing_off(), "the longer backoff must remain armed");
    }
}
