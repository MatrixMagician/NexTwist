---
phase: 05-appimage-distribution
status: secured
asvs_level: 1
block_on: high
threats_total: 8
threats_closed: 8
threats_open: 0
audited: 2026-06-22
---

# Security Audit — Phase 05: AppImage Distribution

**Date:** 2026-06-22
**ASVS Level:** 1
**Scope:** Packaging phase (CI/config/docs + one `lib.rs` self-test wiring). Threat
registers were authored at plan time; this audit VERIFIES each declared mitigation is
present in the implemented code — it does not scan for new threats.
**Block-on:** high

**Result:** SECURED — 8/8 threats closed (6 mitigate + 2 accept). No open blockers, no
unregistered flags.

---

## Threat Verification

| Threat ID | Category | Disposition | Status | Evidence |
|-----------|----------|-------------|--------|----------|
| T-05-01 | Tampering | mitigate | CLOSED | `register_all()` (durable `$APPIMAGE` Exec=) preserved at `src-tauri/src/lib.rs:110`; the `is_registered("nxm")` self-test surfaces a WARN when not default at `src-tauri/src/lib.rs:116`. Durable-Exec confirmation is a manual UAT item (`05-UAT.md`), as the plan declares. |
| T-05-02 | Denial of service | mitigate | CLOSED | `nxm_self_test` helper (`src-tauri/src/lib.rs:36-46`) matches all three `Result` arms with `tracing::info!`/`warn!` only — no `?`/`unwrap`/`expect`; returns `()`. Non-fatal contract asserted on all three arms by `src-tauri/tests/nxm_self_test.rs:27-46`. |
| T-05-03 | Information disclosure | accept | CLOSED | Self-test logs only PASS / WARN + `error = %e` (`src-tauri/src/lib.rs:40-44`) — never the `nxm://` URL. `on_open_url` forwards without logging the URL (`src-tauri/src/lib.rs:117-125`, comment V7). Accepted-risk entry recorded below. |
| T-05-04 | Information disclosure / legal | mitigate | CLOSED | `unrar` + `unrar_sys` banned in `deny.toml:22-26` (`[[bans.deny]]`); re-run in `release.yml:93-94`; `scripts/dist-audit.sh:55-63` proves UnRAR-absence in the built AppImage (binary + `usr/lib`); `DIST-AUDIT.md:85-99` records it by name. Live `cargo deny check ... bans` → **bans ok**. |
| T-05-05 | Tampering | mitigate | CLOSED | `cargo deny check advisories bans licenses sources` re-run in `release.yml:93-94`; `deny.toml:81` sets `yanked = "deny"`. Recorded in `DIST-AUDIT.md:47-49,52-53`. Live run → **advisories ok**. |
| T-05-06 | Tampering | mitigate | CLOSED | `reqwest` pinned `default-features = false, features = ["rustls", ...]` (`Cargo.toml:59`); `oauth2` uses `rustls-tls` only (`Cargo.toml:71`); client built rustls-only with redirects disabled (`crates/nexus/src/client.rs:89-92`). `ldd` step in `scripts/dist-audit.sh:47-48` confirms no app-path `libssl`/`libcrypto`; recorded `DIST-AUDIT.md:72-78`. Cargo.lock contains **no native-tls / openssl-sys**. |
| T-05-07 | Spoofing | accept | CLOSED | Code-signing deferred to v2; v1 ships over HTTPS GitHub Release. Documented accepted limitation in `DIST-AUDIT.md:143-147`. Accepted-risk entry recorded below. |
| T-05-SC | Tampering | mitigate | CLOSED | First-party `tauri-apps/tauri-action` pinned to the concrete release ref `@action-v0.6.2` (not a floating branch) at `.github/workflows/release.yml:74`. No new crates.io packages installed this phase. |

---

## Accepted Risks Log

| Threat ID | Risk | Rationale | Status |
|-----------|------|-----------|--------|
| T-05-03 | Self-test logging could leak an `nxm://` URL (may carry `key`/`expires`/`code`). | Verified the self-test logs only PASS/WARN + error cause and never the URL; `on_open_url` does not log the URL either (`src-tauri/src/lib.rs:117-125`). No new disclosure surface introduced this phase. | Accepted (no new surface) |
| T-05-07 | Unsigned release artifact (no provenance/code-signing in v1). | Code-signing/notarization explicitly deferred to v2 (CONTEXT Deferred Ideas). v1 ships over the HTTPS GitHub Release channel. Documented in `DIST-AUDIT.md`. | Accepted (v1 limitation) |

---

## Unregistered Flags

None. Neither SUMMARY contains a `## Threat Flags` section. Plan 02's SUMMARY carries a
`## Threat Coverage` section mapping every threat to its registered ID and states "No new
security surface introduced beyond the threat register." No new attack surface appeared
during implementation that lacks a threat mapping.

---

## Live Verification Performed

- `cargo deny check advisories bans licenses sources` → `advisories ok, bans ok, licenses ok, sources ok` (the `winreg` 0.55/0.56 duplicate is a `multiple-versions = "warn"`, not a failure). Confirms T-05-04 and T-05-05 gates are effective on the current dependency graph.
- `grep -i 'native-tls\|openssl-sys' Cargo.lock` → no matches. Confirms T-05-06 at the dependency-graph level (in addition to the `reqwest`/`oauth2` rustls-only feature pins).
- `test -x scripts/dist-audit.sh` → executable; `bash -n` → syntax OK.

---

*Phase: 05-appimage-distribution · Audited: 2026-06-22*
