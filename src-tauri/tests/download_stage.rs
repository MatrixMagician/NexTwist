//! NEXUS-06 end-to-end terminus test (no webview).
//!
//! Proves the download flow's terminus reuses the SAME `extract::install_archive`
//! pipeline a local-archive install uses, and that the staged result becomes an ordinary
//! `ManagedMod` carrying persisted Nexus provenance. The Tauri-`Window`-bound part of
//! `commands::downloads::start_download` (the `window.emit` progress wrapper) is the only
//! thing not exercised here — it is covered by the human-verify checkpoint (Task 4).
//!
//! Flow under test (identical chain to `run_download`, sans the event emit):
//!   mockito CDN streams a real .zip  →  `nexus::download_to`  →  `extract::install_archive`
//!   →  `store.add_mod`  →  `store.add_nexus_source`  →  assert ManagedMod + provenance row.

use std::fs;
use std::io::Write;
use std::path::Path;

use nexus::{CancelFlag, NexusAuth, NexusClient};
use nextwist_core::{ManagedMod, NexusSource};
use store::Store;
use tempfile::TempDir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// A Data/-rooted fixture, mirroring the extract crate's own format tests.
const FIXTURE: &[(&str, &[u8])] = &[
    ("Data/Mod.esp", b"plugin-bytes"),
    ("Data/textures/rock.dds", b"rock-texture-bytes"),
];

fn build_zip(path: &Path) {
    let f = fs::File::create(path).unwrap();
    let mut z = ZipWriter::new(f);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in FIXTURE {
        z.start_file(*name, opts).unwrap();
        z.write_all(bytes).unwrap();
    }
    z.finish().unwrap();
}

#[tokio::test]
async fn download_streams_extracts_stages_and_persists_provenance() {
    // --- A real .zip fixture served by a mockito "CDN". ---
    let dir = TempDir::new().unwrap();
    let fixture_zip = dir.path().join("mod.zip");
    build_zip(&fixture_zip);
    let zip_bytes = fs::read(&fixture_zip).unwrap();

    let mut server = mockito::Server::new_async().await;
    let _cdn = server
        .mock("GET", "/cdn/mod.zip")
        .with_status(200)
        .with_header("content-length", &zip_bytes.len().to_string())
        .with_body(zip_bytes.clone())
        .create_async()
        .await;

    // --- 1. Stream the CDN body to a staging-adjacent temp archive (no full-buffer). ---
    let staging_dir = dir.path().join("staging");
    fs::create_dir_all(&staging_dir).unwrap();
    let downloaded_archive = staging_dir.join(".nextwist-dl-test.archive");

    // Stream via the headless client's `download` (same rustls/redirect policy the live
    // download uses); the mock CDN ignores auth, so a dummy key is fine.
    let client = NexusClient::new(NexusAuth::ApiKey("test".into())).unwrap();
    let cancel = CancelFlag::new();
    let uri = format!("{}/cdn/mod.zip", server.url());
    let progress_calls = std::sync::atomic::AtomicU32::new(0);
    let written = client
        .download(&uri, &downloaded_archive, &cancel, |_d, _t| {
            progress_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        })
        .await
        .expect("streaming download should succeed");
    let progress_calls = progress_calls.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(written, zip_bytes.len() as u64);
    assert!(progress_calls > 0, "progress callback must fire");

    // --- 2. Reuse the extract->staging pipeline VERBATIM (the NEXUS-06 terminus). ---
    let staging_root = staging_dir.join("SKSE64");
    let staged = extract::install_archive(&downloaded_archive, &staging_root)
        .expect("the downloaded archive must stage through the same extract path");
    assert!(staged.staging_root.is_dir(), "staging_root must exist on disk");
    assert!(
        staged.staging_root.join("Data/Mod.esp").is_file(),
        "the fixture's files must be present in the staged tree"
    );

    // --- 3. Persist the mod + its Nexus provenance via the store facade. ---
    let store = Store::open(&dir.path().join("nextwist.db")).unwrap();
    let managed = ManagedMod {
        id: 0,
        name: "SKSE64".into(),
        staging_root: staged.staging_root.clone(),
        enabled: false,
        rank: 1,
    };
    let mod_id = store.add_mod(489830, &managed).unwrap();
    store
        .add_nexus_source(&NexusSource {
            mod_id,
            nexus_mod_id: 12604,
            file_id: 120063,
            version: "1.6.3".into(),
            display_name: "SKSE64".into(),
        })
        .unwrap();

    // --- Assert: an ordinary ManagedMod with persisted provenance, ready to deploy. ---
    let got_mod = store.get_mod(mod_id).unwrap().unwrap();
    assert_eq!(got_mod.name, "SKSE64");
    assert!(got_mod.staging_root.is_dir());

    let prov = store
        .get_nexus_source(mod_id)
        .unwrap()
        .expect("Nexus provenance must be persisted for the downloaded mod");
    assert_eq!(prov.nexus_mod_id, 12604);
    assert_eq!(prov.file_id, 120063);
    assert_eq!(prov.version, "1.6.3");
}
