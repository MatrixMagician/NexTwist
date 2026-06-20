//! `.rar` extraction via a system `unrar`/`7z` binary.
//!
//! NexTwist never bundles non-free RAR code (the `unrar`/`unrar_sys` crates are
//! banned by `deny.toml`). Instead it shells out to whatever the user already has:
//! it prefers `unrar`, falls back to `7z`, and returns an actionable
//! [`ExtractError::RarToolMissing`] when neither is on `PATH`.
//!
//! ## Command-injection defense
//!
//! The archive path and output directory are passed as SEPARATE argv elements via
//! [`std::process::Command`] — never concatenated into a shell string. No shell is
//! spawned, so a hostile filename cannot inject arguments or commands.
//!
//! ## Post-extraction re-validation
//!
//! A system tool may itself happily write traversal or symlink entries, so after
//! it runs the entire extracted tree is re-walked and every file is routed through
//! the shared validator (rejecting symlinks and any path that escaped `temp_root`)
//! — the same invariant the in-process zip/7z handlers enforce per entry.

use std::path::{Path, PathBuf};
use std::process::Command;

use walkdir::WalkDir;

use crate::validate::ExtractError;

/// A discovered system extraction tool and how to invoke it for a `.rar`.
enum RarTool {
    /// `unrar x -y -- <archive> <outdir>/`
    Unrar,
    /// `7z x -y -o<outdir> -- <archive>`
    SevenZip,
}

/// Extract `archive` (a `.rar`) into `temp_root` via a system tool, then re-validate
/// the resulting tree for traversal/symlink entries.
pub fn extract_rar(archive: &Path, temp_root: &Path) -> Result<(), ExtractError> {
    // Defense: ensure the archive really is an existing file before we hand its
    // path to an external process.
    if !archive.is_file() {
        return Err(ExtractError::io(
            archive,
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "rar archive path is not an existing file",
            ),
        ));
    }

    let tool = detect_tool().ok_or(ExtractError::RarToolMissing)?;
    run_tool(&tool, archive, temp_root)?;
    // The system tool wrote files directly; enforce our safety invariant on the
    // resulting tree (reject symlinks and anything that escaped the root).
    revalidate_tree(temp_root)
}

/// Prefer `unrar`, fall back to `7z`; `None` when neither is on `PATH`.
fn detect_tool() -> Option<RarTool> {
    if which("unrar").is_some() {
        return Some(RarTool::Unrar);
    }
    if which("7z").is_some() {
        return Some(RarTool::SevenZip);
    }
    None
}

/// Spawn the chosen tool with the archive path + output dir as discrete argv
/// elements. `--` terminates option parsing so a filename that starts with `-`
/// cannot be misread as a flag.
fn run_tool(tool: &RarTool, archive: &Path, temp_root: &Path) -> Result<(), ExtractError> {
    let (program, output) = match tool {
        RarTool::Unrar => {
            let mut cmd = Command::new("unrar");
            // x = extract with full paths; -y = assume yes; -- ends options.
            cmd.arg("x").arg("-y").arg("--").arg(archive).arg(temp_root);
            ("unrar", cmd)
        }
        RarTool::SevenZip => {
            let mut cmd = Command::new("7z");
            // x = extract with full paths; -y = assume yes; -o<dir> = output dir
            // (7z requires the dir glued to -o with no space); -- ends options.
            let mut odir = std::ffi::OsString::from("-o");
            odir.push(temp_root.as_os_str());
            cmd.arg("x").arg("-y").arg(odir).arg("--").arg(archive);
            ("7z", cmd)
        }
    };

    let mut cmd = output;
    let out = cmd
        .output()
        .map_err(|e| ExtractError::io(archive, e))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let message = if stderr.is_empty() { stdout } else { stderr };
        return Err(ExtractError::ToolFailed {
            tool: program.to_string(),
            message,
        });
    }
    Ok(())
}

/// Re-walk the extracted tree and reject any symlink, then confirm every regular
/// file's path validates under `temp_root` (catches a tool that wrote outside it
/// or created an escape via a path component).
fn revalidate_tree(temp_root: &Path) -> Result<(), ExtractError> {
    let canon_root = temp_root
        .canonicalize()
        .map_err(|e| ExtractError::io(temp_root, e))?;

    for entry in WalkDir::new(temp_root).follow_links(false) {
        let entry = entry.map_err(|e| ExtractError::Decode(format!("walk rar output: {e}")))?;
        let ft = entry.file_type();
        let path = entry.path();

        if ft.is_symlink() {
            return Err(ExtractError::SymlinkEntry(path.to_path_buf()));
        }
        if ft.is_file() {
            // Re-canonicalize the actual on-disk file and assert it still resides
            // under the extraction root. Combined with the symlink rejection above
            // (walkdir does not follow links, so no entry was reached *through* a
            // symlink), this enforces the same containment invariant the in-process
            // zip/7z handlers apply per entry.
            let canon = path.canonicalize().map_err(|e| ExtractError::io(path, e))?;
            if !canon.starts_with(&canon_root) {
                return Err(ExtractError::UnsafeEntry(format!(
                    "rar tool wrote a file outside the extraction root: {}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

/// Minimal `PATH` lookup for an executable, avoiding an extra crate dependency.
fn which(program: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(program);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}
