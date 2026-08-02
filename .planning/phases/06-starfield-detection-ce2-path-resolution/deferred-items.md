
## Deferred from 06-02 (2026-07-07)

**Pre-existing cargo-deny advisory failures (out of scope — no deps added in 06-02):**
- RUSTSEC-2026-0190 (anyhow unsoundness) → `cargo update -p anyhow` to >=1.0.103
- RUSTSEC-2026-0204 (crossbeam-epoch) → `cargo update -p crossbeam-epoch` to >=0.9.20
- RUSTSEC-2026-0194 / 0195 (quick-xml DoS) → `cargo update -p quick-xml` to >=0.41.0

These are time-based advisories on already-pinned transitive deps, unrelated to the Starfield allow-list/asset work. `bans`, `licenses`, `sources` all pass. Address in a dedicated dependency-refresh task.
