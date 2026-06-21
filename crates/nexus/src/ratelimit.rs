//! Client-side rate limiting (NEXUS-05).
//!
//! STUB (Plan 01): the `governor` direct token-bucket limiter sized to the NexusMods
//! budget, plus reactive `X-RL-*` header backoff, land in Plan 02. Declared now so the
//! module layout is fixed.
