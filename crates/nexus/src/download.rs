//! Streaming download with progress (NEXUS-03/04/06).
//!
//! STUB (Plan 01): the `reqwest::Response::bytes_stream()` chunk loop + the
//! `Fn(u64, Option<u64>)` progress callback (no Tauri type) land in Plan 02. Declared
//! now so the module layout is fixed.
