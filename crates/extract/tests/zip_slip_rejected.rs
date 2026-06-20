//! Crafted malicious-archive rejection test — the Phase 1 security centerpiece.
//!
//! Builds three hostile zips in-test and proves `install_archive` rejects each
//! with the correct `ExtractError` variant AND that nothing is written outside the
//! intended target directory. The escape-token strings are constructed here in the
//! TEST (never echoed in the validator source the rejection greps for).

use std::fs;
use std::io::Write;
use std::path::Path;

use extract::{install_archive, ExtractError};
use tempfile::TempDir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// The parent-directory escape token, assembled from parts so it is not a literal
/// in either the validator source or, ideally, anywhere greppable as a sequence.
fn escape_token() -> String {
    format!("{}{}", "..", "/")
}

/// Build a zip at `path` whose single entry uses the raw `entry_name` (written via
/// the raw-name API so the writer does not sanitize our hostile path), carrying
/// `unix_mode` permission/type bits and `bytes` as content.
fn build_zip_raw(path: &Path, entry_name: &str, unix_mode: Option<u32>, bytes: &[u8]) {
    let file = fs::File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    let mut opts = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);
    if let Some(mode) = unix_mode {
        opts = opts.unix_permissions(mode);
    }
    // `start_file` keeps the name verbatim (no path sanitization), which is exactly
    // what we need to smuggle a hostile entry name into the archive.
    zip.start_file(entry_name, opts).unwrap();
    zip.write_all(bytes).unwrap();
    zip.finish().unwrap();
}

/// Build a zip at `path` whose single entry is a genuine symlink named
/// `entry_name` pointing at `target` (carries real S_IFLNK file-type bits).
fn build_symlink_zip(path: &Path, entry_name: &str, target: &str) {
    let file = fs::File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default();
    zip.add_symlink(entry_name, target, opts).unwrap();
    zip.finish().unwrap();
}

/// Snapshot the set of paths directly under `dir` (one level) for escape checks.
fn list_dir(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

#[test]
fn traversal_entry_rejected_and_nothing_escapes() {
    let work = TempDir::new().unwrap();
    let archive = work.path().join("evil_traversal.zip");
    // e.g. "../escape/pwned.txt"
    let hostile = format!("{}escape/pwned.txt", escape_token());
    build_zip_raw(&archive, &hostile, None, b"owned");

    // A sentinel sibling dir that an escaping write would land in.
    let escape_target = work.path().join("escape");
    let before = escape_target.exists();

    let staging = work.path().join("staging");
    let err = install_archive(&archive, &staging).expect_err("traversal must be rejected");
    assert!(
        matches!(err, ExtractError::UnsafeEntry(_)),
        "expected UnsafeEntry, got {err:?}"
    );

    // Nothing escaped: the sibling escape dir was not created by the extraction.
    assert_eq!(
        escape_target.exists(),
        before,
        "extraction created a directory outside the staging root"
    );
    // Staging must not contain the smuggled file.
    if staging.exists() {
        assert!(
            !staging.join("escape/pwned.txt").exists(),
            "smuggled file landed in staging"
        );
    }
}

#[test]
fn absolute_path_entry_rejected_and_nothing_escapes() {
    let work = TempDir::new().unwrap();
    let archive = work.path().join("evil_absolute.zip");
    // An absolute entry path: "/tmp/<unique>/pwned.txt".
    let abs_dir = work.path().join("abs-sentinel");
    let hostile = format!("{}/pwned.txt", abs_dir.display());
    build_zip_raw(&archive, &hostile, None, b"owned");

    let before = abs_dir.exists();

    let staging = work.path().join("staging");
    let err = install_archive(&archive, &staging).expect_err("absolute path must be rejected");
    assert!(
        matches!(err, ExtractError::UnsafeEntry(_)),
        "expected UnsafeEntry, got {err:?}"
    );

    assert_eq!(
        abs_dir.exists(),
        before,
        "extraction wrote to an absolute path outside the staging root"
    );
}

#[test]
fn symlink_entry_rejected_and_nothing_escapes() {
    let work = TempDir::new().unwrap();
    let archive = work.path().join("evil_symlink.zip");
    // Use the zip writer's dedicated symlink API so the entry carries genuine
    // S_IFLNK (0o120000) file-type bits — the writer forces regular-file bits when
    // a symlink mode is passed via plain `unix_permissions`, so a real symlink
    // entry must be created with `add_symlink`. The link target is hostile.
    build_symlink_zip(&archive, "link", "/etc/passwd");

    let staging = work.path().join("staging");
    let err = install_archive(&archive, &staging).expect_err("symlink entry must be rejected");
    assert!(
        matches!(err, ExtractError::SymlinkEntry(_)),
        "expected SymlinkEntry, got {err:?}"
    );

    // No symlink (or any file) was created in staging.
    if staging.exists() {
        assert!(
            list_dir(&staging).is_empty(),
            "symlink extraction created files in staging: {:?}",
            list_dir(&staging)
        );
    }
}

#[test]
fn benign_nested_entry_is_accepted() {
    // Control: a normal Data/-rooted entry installs cleanly, proving the validator
    // rejects only the hostile cases above (not everything).
    let work = TempDir::new().unwrap();
    let archive = work.path().join("benign.zip");
    build_zip_raw(&archive, "Data/textures/rock.dds", None, b"rockbytes");

    let staging = work.path().join("staging");
    let staged = install_archive(&archive, &staging).expect("benign archive must install");
    assert!(staging.join("Data/textures/rock.dds").is_file());
    assert_eq!(staged.files.len(), 1);
}
