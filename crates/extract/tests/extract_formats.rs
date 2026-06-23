//! Multi-format extraction test: `.zip` and `.7z` extract to a read-only staging
//! tree; `.rar` is handled via a system tool (or a clear no-tool error). Fixtures
//! are built programmatically in-test (Data/-rooted) — no opaque binaries checked in.

use std::fs;
use std::io::{Cursor, Write};
use std::path::Path;

use extract::{install_archive, ExtractError};
use sevenz_rust2::{ArchiveEntry, ArchiveWriter};
use tempfile::TempDir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// The Data/-rooted fixture contents shared by the zip and 7z cases.
const FIXTURE: &[(&str, &[u8])] = &[
    ("Data/Mod.esp", b"plugin-bytes"),
    ("Data/textures/rock.dds", b"rock-texture-bytes"),
    ("Data/readme.txt", b"hello modder"),
];

fn build_zip(path: &Path) {
    build_zip_from(path, FIXTURE);
}

fn build_7z(path: &Path) {
    let mut w = ArchiveWriter::create(path).unwrap();
    for (name, bytes) in FIXTURE {
        let entry = ArchiveEntry::new_file(name);
        w.push_archive_entry(entry, Some(Cursor::new(bytes.to_vec())))
            .unwrap();
    }
    w.finish().unwrap();
}

/// A wrapper-folder layout: the mod's game content (`Wrapper/Data/Plugin.esp`) plus
/// non-game wrapper junk (`Wrapper/Info.txt`, `Wrapper/Screenshot/shot.png`). The
/// regression target: stage `Data/Plugin.esp` only, dropping the junk so it never leaks
/// into the game `Data/` at deploy time.
const WRAPPER_FIXTURE: &[(&str, &[u8])] = &[
    ("Super Cheat Legendary Weapon Fountain/Data/Plugin.esp", b"plugin-bytes"),
    ("Super Cheat Legendary Weapon Fountain/Info.txt", b"author notes"),
    ("Super Cheat Legendary Weapon Fountain/Screenshot/shot.png", b"png-bytes"),
];

fn build_zip_from(path: &Path, entries: &[(&str, &[u8])]) {
    let f = fs::File::create(path).unwrap();
    let mut z = ZipWriter::new(f);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in entries {
        z.start_file(*name, opts).unwrap();
        z.write_all(bytes).unwrap();
    }
    z.finish().unwrap();
}

/// True if any system rar tool is available (matches rar.rs detection order).
fn rar_tool_present() -> bool {
    which("unrar") || which("7z")
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let p = dir.join(program);
                p.is_file()
            })
        })
        .unwrap_or(false)
}

fn assert_staged_correctly(staged: &extract::StagedMod, staging: &Path) {
    // Expected relpaths present.
    for (name, bytes) in FIXTURE {
        let p = staging.join(name);
        assert!(p.is_file(), "missing staged file: {name}");
        assert_eq!(&fs::read(&p).unwrap(), bytes, "content mismatch for {name}");
        // Read-only bit set (staging-integrity invariant).
        let perms = fs::metadata(&p).unwrap().permissions();
        assert!(perms.readonly(), "staged file is not read-only: {name}");
    }
    assert_eq!(staged.files.len(), FIXTURE.len());
    assert_eq!(staged.staging_root, staging);
}

#[test]
fn zip_extracts_readonly_to_staging() {
    let work = TempDir::new().unwrap();
    let archive = work.path().join("mod.zip");
    build_zip(&archive);

    let staging = work.path().join("staging-zip");
    let staged = install_archive(&archive, &staging).expect("zip should install");
    assert_staged_correctly(&staged, &staging);
}

#[test]
fn sevenz_extracts_readonly_to_staging() {
    let work = TempDir::new().unwrap();
    let archive = work.path().join("mod.7z");
    build_7z(&archive);

    let staging = work.path().join("staging-7z");
    let staged = install_archive(&archive, &staging).expect("7z should install");
    assert_staged_correctly(&staged, &staging);
}

#[test]
fn wrapper_folder_stages_data_root_and_excludes_non_game_files() {
    // Regression for install-archive-root-detection: a wrapper-folder archive
    // (`Wrapper/Data/Plugin.esp` + non-game `Wrapper/Info.txt` + `Wrapper/Screenshot/`)
    // must stage so the plugin lands at `Data/Plugin.esp` (NOT `Wrapper/Data/Plugin.esp`,
    // and NOT double-nested) AND the non-game wrapper junk is EXCLUDED — so it never leaks
    // into the game `Data/` directory when deploy re-roots non-`Data/`-prefixed relpaths.
    let work = TempDir::new().unwrap();
    let archive = work.path().join("wrapper-mod.zip");
    build_zip_from(&archive, WRAPPER_FIXTURE);

    let staging = work.path().join("staging-wrapper");
    let staged = install_archive(&archive, &staging).expect("wrapper mod should install");

    // The plugin is staged Data/-rooted (one cosmetic level stripped, no double-nesting).
    let plugin = staging.join("Data/Plugin.esp");
    assert!(plugin.is_file(), "plugin must be staged at Data/Plugin.esp");
    assert_eq!(&fs::read(&plugin).unwrap(), b"plugin-bytes");

    // Non-game wrapper siblings are EXCLUDED from staging entirely.
    assert!(
        !staging.join("Info.txt").exists(),
        "non-game Info.txt must be excluded from staging"
    );
    assert!(
        !staging.join("Screenshot").exists(),
        "non-game Screenshot/ must be excluded from staging"
    );

    // The manifest reflects exactly one staged file: Data/Plugin.esp.
    assert_eq!(
        staged.files,
        vec![Path::new("Data/Plugin.esp").to_path_buf()],
        "only the Data/-rooted game file should be staged"
    );

    // And it is read-only like every staged file (staging-integrity invariant).
    assert!(
        fs::metadata(&plugin).unwrap().permissions().readonly(),
        "staged plugin must be read-only"
    );
}

#[test]
fn rar_uses_system_tool_or_reports_missing() {
    let work = TempDir::new().unwrap();
    // Build a real .rar by repacking the fixture with the system 7z if present;
    // otherwise just assert the no-tool error path on a placeholder file.
    let staging = work.path().join("staging-rar");

    if rar_tool_present() {
        // Create a genuine .rar using the system tool so the round-trip is real.
        let src = work.path().join("rarsrc");
        for (name, bytes) in FIXTURE {
            let p = src.join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, bytes).unwrap();
        }
        let archive = work.path().join("mod.rar");
        let made = make_rar(&archive, &src);
        if !made {
            // The available tool cannot CREATE rar (7z needs the rar plugin to
            // write, which is often absent). Skip the positive path but assert the
            // detection wiring by extracting a tool-made zip-as-rar is not valid —
            // so we simply return: the no-tool branch is covered by CI hosts
            // without any tool, and detection is unit-covered elsewhere.
            eprintln!("note: system tool present but cannot author .rar; skipping rar round-trip");
            return;
        }
        let staged = install_archive(&archive, &staging).expect("rar should install via system tool");
        assert_staged_correctly(&staged, &staging);
    } else {
        // No tool: a .rar (by magic or extension) must yield RarToolMissing.
        let archive = work.path().join("mod.rar");
        // Minimal RAR5 signature so format detection routes to the rar handler.
        fs::write(&archive, b"Rar!\x1A\x07\x01\x00rest").unwrap();
        let err = install_archive(&archive, &staging).expect_err("no rar tool => error");
        assert!(
            matches!(err, ExtractError::RarToolMissing),
            "expected RarToolMissing, got {err:?}"
        );
    }
}

/// Try to author a `.rar` at `archive` from directory `src` using a system tool.
/// Returns false if no tool can write the rar format (common: 7z lacks the rar
/// codec for compression).
fn make_rar(archive: &Path, src: &Path) -> bool {
    use std::process::Command;
    // `rar` is the only common tool that writes .rar; 7z cannot compress to rar.
    if which("rar") {
        let status = Command::new("rar")
            .arg("a")
            .arg("-r")
            .arg("--")
            .arg(archive)
            .arg(".")
            .current_dir(src)
            .status();
        return matches!(status, Ok(s) if s.success()) && archive.is_file();
    }
    false
}
