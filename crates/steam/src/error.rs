//! Error type for the steam crate.
//!
//! Per the locked error-design decision (CONTEXT.md): libraries use `thiserror`
//! enums; `anyhow` is reserved for the app/Tauri boundary only.

use std::path::PathBuf;

use thiserror::Error;

/// Failure modes for Steam/Proton discovery and resolution.
#[derive(Debug, Error)]
pub enum SteamError {
    /// The requested AppID is not one of the supported Bethesda games.
    #[error("appid {0} is not a supported game (only Skyrim SE 489830 and Fallout 4 377160)")]
    Unsupported(u32),

    /// A supported AppID was requested but Steam reports it is not installed.
    #[error("app {0} is not installed in any detected Steam library")]
    NotInstalled(u32),

    /// No Steam installation could be located on this machine.
    #[error("no Steam installation found (native or Flatpak)")]
    NoSteam,

    /// A manually supplied folder does not look like a supported Bethesda game.
    #[error("folder does not look like a supported game (missing {missing}): {path}")]
    InvalidGameFolder {
        /// Path the user supplied.
        path: PathBuf,
        /// Which expected marker was missing (e.g. "Data/ directory").
        missing: String,
    },

    /// The underlying steamlocate library failed (locate / parse).
    #[error("steam discovery error: {0}")]
    Locate(String),

    /// An I/O error while inspecting the filesystem.
    #[error("i/o error for {path}: {source}")]
    Io {
        /// Path involved.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}
