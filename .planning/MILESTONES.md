# Milestones

## v1.0 MVP (Shipped: 2026-06-23)

**Phases completed:** 5 phases, 21 plans, 26 tasks
**Timeline:** 2026-06-20 → 2026-06-23 · ~196 commits · ~20k Rust + ~2.9k frontend LOC
**Requirements:** 40/40 v1 requirements satisfied · 4 of 5 phases carry a SECURITY.md (Phases 1/2/4/5, threats_open: 0); Phase 3's security boundaries (OAuth/keyring/nxm:///TLS) verified inline in 03-VERIFICATION + the milestone integration check — no standalone SECURITY.md
**Known deferred items at close:** 2 (see STATE.md → Deferred Items — both the accepted NexusMods Vortex-only Collection-download limitation)

**Key accomplishments:**

- **Phase 1 — Safe Local Round-Trip:** the reversible deployment engine (the crown jewel) — a `reflink → hardlink → symlink → copy` per-target method ladder with an **intent-before-act operation journal** for crash-safety, byte-for-byte **pristine purge**, and a vanilla-backup ledger; plus `crates/extract` turning untrusted `.zip`/`.7z`/`.rar` archives into validated read-only staging trees that reject zip-slip / absolute / symlink entries (CVE-2025-29787), RAR via a system tool only (no bundled non-free code).
- **Phase 2 — Multi-Mod Management:** file-level conflict resolution by mod rank, plugin load-order management via `libloot` (correct master-first `plugins.txt` written to the Proton-prefix AppData path), LOOT auto-sort, and per-game profiles with fully-reversible switching (purge-to-pristine between profiles).
- **Phase 3 — NexusMods Login & Download:** OAuth2+PKCE (CSRF-validated) + API-key-paste fallback in a headless `crates/nexus`, keyring credential storage that hard-fails rather than writing plaintext, Premium in-app download with progress, client-side rate limiting, and `nxm://` one-click handoff routed to the live single instance.
- **Phase 4 — Guided Installers & Collections:** a FOMOD scripted-installer wizard (live conditional re-evaluation + dry-run conflict preview) and the full NexusMods Collection lifecycle — FOMOD-choice replay + rule→rank mapping, Premium-gated bulk download, deploy via `switch_profile`, and byte-for-byte reversible uninstall — composed from existing primitives, with SSRF defense (off-Nexus sources never auto-fetched).
- **Phase 5 — AppImage Distribution:** a tag-triggered `release.yml` building a license-clean AppImage on `ubuntu-22.04` (DIST-01) and a reproducible bundled-binary audit (`scripts/dist-audit.sh` + `DIST-AUDIT.md`) proving no non-free UnRAR and no app-path system-OpenSSL ship (DIST-02), with a registered, self-tested `nxm://` handler.

**The guarantee that held throughout:** deployment is non-destructive (the base game is never modified in place), fully reversible (purge restores a byte-for-byte pristine game), and conflict-aware.

---
