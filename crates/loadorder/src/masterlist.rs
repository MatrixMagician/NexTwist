//! Per-game LOOT masterlist fetch + cache + bundled fallback (PLUGIN-03, D-10).
//!
//! "Sort with LOOT" needs the game's masterlist (the LOOT project's CC0-1.0 sorting
//! metadata). This module resolves a cached `masterlist.yaml`, fetching it over HTTPS when
//! absent/stale and falling back to a bundled snapshot when offline.
//!
//! ## Trust boundary (T-02-10)
//!
//! The fetch is **pinned**: HTTPS only, host `raw.githubusercontent.com`, repo
//! `loot/<game-slug>`, branch [`MASTERLIST_BRANCH`] (`v0.29`, matching the libloot 0.29.x
//! major — Pitfall 5). reqwest uses the `rustls` feature (no OpenSSL — CLAUDE.md). The branch is
//! pinned to the libloot major so the masterlist schema always matches the parser. The
//! masterlist is itself parsed by libloot (a trusted CC0 source), never by hand here.
//!
//! ## Caching (D-10)
//!
//! Cache path: `<app_data>/masterlists/<appid>/masterlist.yaml`. With `refresh == false`
//! and a cache present, [`ensure_masterlist`] returns the cache WITHOUT any network call.
//! With `refresh == true` (or no cache), it fetches; on any network/TLS/HTTP failure it
//! falls back to the bundled CC0 snapshot shipped in `assets/<slug>/masterlist.yaml`,
//! seeding the cache from it so subsequent calls are offline-clean.

use std::path::{Path, PathBuf};

use crate::error::LoadOrderError;

/// Skyrim Special Edition AppID (mirrors `loot::SKYRIM_SE`).
const SKYRIM_SE: u32 = 489830;
/// Fallout 4 AppID (mirrors `loot::FALLOUT4`).
const FALLOUT4: u32 = 377160;

/// The masterlist branch pinned to the libloot 0.29.x major (Pitfall 5). Bump this in
/// lock-step with the `libloot` workspace version.
pub const MASTERLIST_BRANCH: &str = "v0.29";

/// The pinned raw-content host the masterlist is fetched from (T-02-10).
const MASTERLIST_HOST: &str = "https://raw.githubusercontent.com";

/// Bundled CC0 masterlist snapshots, compiled into the binary so the offline fallback
/// needs no filesystem layout at runtime (works inside an AppImage). CC0-1.0 — public
/// domain, legally safe to embed (RESEARCH Pattern 3).
const SKYRIMSE_SNAPSHOT: &str = include_str!("../assets/skyrimse/masterlist.yaml");
const FALLOUT4_SNAPSHOT: &str = include_str!("../assets/fallout4/masterlist.yaml");

/// The LOOT repo slug for a supported AppID (`loot/<slug>`), or `None` if unsupported.
fn game_slug(appid: u32) -> Option<&'static str> {
    match appid {
        SKYRIM_SE => Some("skyrimse"),
        FALLOUT4 => Some("fallout4"),
        _ => None,
    }
}

/// The bundled CC0 snapshot text for a supported AppID.
fn bundled_snapshot(appid: u32) -> Option<&'static str> {
    match appid {
        SKYRIM_SE => Some(SKYRIMSE_SNAPSHOT),
        FALLOUT4 => Some(FALLOUT4_SNAPSHOT),
        _ => None,
    }
}

/// The pinned HTTPS URL for a game's masterlist on the libloot-major branch.
fn masterlist_url(slug: &str) -> String {
    format!("{MASTERLIST_HOST}/loot/{slug}/{MASTERLIST_BRANCH}/masterlist.yaml")
}

/// The on-disk cache path for a game's masterlist under the app-data dir.
pub fn cache_path(app_data: &Path, appid: u32) -> PathBuf {
    app_data
        .join("masterlists")
        .join(appid.to_string())
        .join("masterlist.yaml")
}

/// Ensure a usable masterlist exists for `appid` and return its cached path.
///
/// * `refresh == false` + cache present → returns the cache, NO network.
/// * cache absent OR `refresh == true` → fetch the pinned URL over rustls HTTPS; on
///   success write the cache and return it; on ANY network/TLS/HTTP failure, fall back to
///   the bundled CC0 snapshot (seeding the cache from it) so the caller always gets a
///   parseable masterlist when one is bundled.
///
/// # Errors
/// * [`LoadOrderError::UnsupportedGame`] if `appid` is not in the allow-list.
/// * [`LoadOrderError::Io`] if the cache cannot be written.
/// * [`LoadOrderError::Network`] only if the fetch fails AND no bundled snapshot exists
///   (never happens for the two supported games, which always ship a snapshot).
pub fn ensure_masterlist(
    app_data: &Path,
    appid: u32,
    refresh: bool,
) -> Result<PathBuf, LoadOrderError> {
    ensure_masterlist_with_fetcher(app_data, appid, refresh, real_fetch)
}

/// Cache-aware masterlist resolution with an injectable fetcher (the testable core).
///
/// `fetch` returns the masterlist body as a `String` for a given URL, or an error string.
/// Real callers pass [`real_fetch`]; tests inject a stub to exercise the cache /
/// offline-fallback paths without hitting the network. Generic (not a `dyn` trait object)
/// so a borrowing test closure works without a `'static` bound.
fn ensure_masterlist_with_fetcher<F>(
    app_data: &Path,
    appid: u32,
    refresh: bool,
    fetch: F,
) -> Result<PathBuf, LoadOrderError>
where
    F: Fn(&str) -> Result<String, String>,
{
    let slug = game_slug(appid).ok_or(LoadOrderError::UnsupportedGame(appid))?;
    let cache = cache_path(app_data, appid);

    // Fresh cache + no forced refresh → return it, no network (D-10).
    if cache.is_file() && !refresh {
        return Ok(cache);
    }

    let url = masterlist_url(slug);
    match fetch(&url) {
        Ok(body) => {
            write_cache(&cache, &body)?;
            Ok(cache)
        }
        Err(fetch_err) => {
            // Offline / TLS / HTTP failure: fall back to the bundled CC0 snapshot.
            tracing::warn!(
                appid,
                url = %url,
                error = %fetch_err,
                "masterlist fetch failed; falling back to bundled CC0 snapshot"
            );
            // If a cache already exists (stale but present), prefer it over re-seeding.
            if cache.is_file() {
                return Ok(cache);
            }
            match bundled_snapshot(appid) {
                Some(snapshot) => {
                    write_cache(&cache, snapshot)?;
                    Ok(cache)
                }
                None => Err(LoadOrderError::Network(fetch_err)),
            }
        }
    }
}

/// Write the masterlist body to the cache path, creating parent dirs.
fn write_cache(cache: &Path, body: &str) -> Result<(), LoadOrderError> {
    if let Some(parent) = cache.parent() {
        std::fs::create_dir_all(parent).map_err(|e| LoadOrderError::io(parent, e))?;
    }
    std::fs::write(cache, body).map_err(|e| LoadOrderError::io(cache, e))?;
    Ok(())
}

/// The real HTTPS fetch: a one-shot blocking rustls request to the pinned URL.
fn real_fetch(url: &str) -> Result<String, String> {
    let resp = reqwest::blocking::get(url).map_err(|e| e.to_string())?;
    let resp = resp.error_for_status().map_err(|e| e.to_string())?;
    resp.text().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use tempfile::TempDir;

    #[test]
    fn cache_path_is_namespaced_by_appid() {
        let p = cache_path(Path::new("/data"), SKYRIM_SE);
        assert_eq!(
            p,
            Path::new("/data/masterlists/489830/masterlist.yaml")
        );
    }

    #[test]
    fn url_is_pinned_to_host_repo_and_branch() {
        let url = masterlist_url("skyrimse");
        assert_eq!(
            url,
            "https://raw.githubusercontent.com/loot/skyrimse/v0.29/masterlist.yaml"
        );
        // T-02-10 / Pitfall 5: HTTPS, the pinned host, and the libloot-major branch.
        assert!(url.starts_with("https://raw.githubusercontent.com/"));
        assert!(url.contains("/v0.29/"));
    }

    #[test]
    fn unsupported_game_is_rejected_before_fetch() {
        let dir = TempDir::new().unwrap();
        let called = Cell::new(false);
        let fetch = |_: &str| -> Result<String, String> {
            called.set(true);
            Ok(String::new())
        };
        let err =
            ensure_masterlist_with_fetcher(dir.path(), 220, true, fetch).unwrap_err();
        assert!(matches!(err, LoadOrderError::UnsupportedGame(220)));
        assert!(!called.get(), "no fetch is attempted for an unsupported game");
    }

    #[test]
    fn uses_cache_without_network_when_fresh() {
        let dir = TempDir::new().unwrap();
        // Seed a cache as a prior fetch would have.
        let cache = cache_path(dir.path(), SKYRIM_SE);
        std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
        std::fs::write(&cache, "seeded: true\n").unwrap();

        let called = Cell::new(false);
        let fetch = |_: &str| -> Result<String, String> {
            called.set(true);
            Ok("fetched: should-not-happen\n".into())
        };
        let got =
            ensure_masterlist_with_fetcher(dir.path(), SKYRIM_SE, false, fetch).unwrap();
        assert_eq!(got, cache);
        assert!(!called.get(), "refresh=false + fresh cache must NOT hit the network");
        assert_eq!(std::fs::read_to_string(&got).unwrap(), "seeded: true\n");
    }

    #[test]
    fn refresh_true_fetches_and_writes_cache() {
        let dir = TempDir::new().unwrap();
        let fetch = |url: &str| -> Result<String, String> {
            assert!(url.contains("/loot/skyrimse/v0.29/"));
            Ok("fetched: yes\n".into())
        };
        let got =
            ensure_masterlist_with_fetcher(dir.path(), SKYRIM_SE, true, fetch).unwrap();
        assert_eq!(std::fs::read_to_string(&got).unwrap(), "fetched: yes\n");
    }

    #[test]
    fn falls_back_to_bundled_snapshot_when_offline() {
        let dir = TempDir::new().unwrap();
        // Fetcher simulates an unreachable host.
        let fetch = |_: &str| -> Result<String, String> {
            Err("dns error: failed to lookup unreachable.invalid".into())
        };
        let got =
            ensure_masterlist_with_fetcher(dir.path(), SKYRIM_SE, true, fetch).unwrap();
        // The cache now holds the bundled CC0 snapshot (non-empty, valid YAML-ish).
        let body = std::fs::read_to_string(&got).unwrap();
        assert!(!body.is_empty(), "bundled snapshot seeds the cache offline");
        assert_eq!(body, SKYRIMSE_SNAPSHOT);
    }

    #[test]
    fn offline_with_existing_stale_cache_keeps_the_cache() {
        let dir = TempDir::new().unwrap();
        let cache = cache_path(dir.path(), FALLOUT4);
        std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
        std::fs::write(&cache, "stale-but-present\n").unwrap();
        // refresh=true forces a fetch attempt, which fails; the stale cache is preferred
        // over re-seeding from the bundled snapshot.
        let fetch = |_: &str| -> Result<String, String> { Err("offline".into()) };
        let got =
            ensure_masterlist_with_fetcher(dir.path(), FALLOUT4, true, fetch).unwrap();
        assert_eq!(std::fs::read_to_string(&got).unwrap(), "stale-but-present\n");
    }
}
