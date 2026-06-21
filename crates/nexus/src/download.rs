//! Streaming download with progress (NEXUS-03/04/06).
//!
//! [`download_to`] streams a CDN response body **chunk-by-chunk** to a destination file
//! via `reqwest::Response::bytes_stream()` + `futures_util::StreamExt`, writing each
//! chunk to a `tokio::fs::File` and reporting progress through a plain
//! `Fn(u64, Option<u64>)` callback. The callback carries **no Tauri type** — the shell
//! wraps it into `window.emit("download://progress", …)`.
//!
//! CRITICAL anti-pattern (RESEARCH T-03-09 / criterion #4): the whole body is NEVER
//! buffered into memory (the full-body whole-buffer read is forbidden — a multi-GB
//! texture pack would OOM the process). Only the chunked byte-stream path is used.
//!
//! A [`CancelFlag`] is checked once per chunk so the shell's "Cancel" affordance can
//! abort an in-flight download promptly. SECRET DISCIPLINE (V7): no URI is ever logged.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::error::NexusError;

/// A cooperative cancellation flag for an in-flight download.
///
/// The shell holds a clone in `AppState` keyed by the download id; `cancel_download`
/// trips it, and the [`download_to`] loop checks it once per chunk and aborts with
/// [`NexusError::Http`] (a cancelled download is surfaced distinctly by the shell, not a
/// real transport error — the partial file is removed).
#[derive(Debug, Clone, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    /// A fresh, un-cancelled flag.
    pub fn new() -> Self {
        CancelFlag(Arc::new(AtomicBool::new(false)))
    }

    /// Request cancellation. The next chunk boundary in [`download_to`] aborts.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// True once [`cancel`](Self::cancel) has been called.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Stream `uri` to `dest`, reporting progress, never buffering the whole body.
///
/// `on_progress(downloaded, total)` is called after each chunk: `downloaded` is the
/// running byte count, `total` is the `Content-Length` if the server provided one
/// (`None` otherwise). `cancel` is checked once per chunk; if tripped, the partial file
/// is removed and an error is returned.
///
/// The `client` is supplied by the caller (the shell's `NexusClient` exposes its inner
/// reqwest client, or a fresh hardened client is passed) so the same rustls/redirect
/// policy applies to the CDN GET.
///
/// # Errors
/// * [`NexusError::Http`] on a transport/status failure or on cancellation.
/// * [`NexusError::Io`] if the destination file cannot be created or written.
pub async fn download_to<F>(
    client: &reqwest::Client,
    uri: &str,
    dest: &Path,
    cancel: &CancelFlag,
    on_progress: F,
) -> Result<u64, NexusError>
where
    F: Fn(u64, Option<u64>),
{
    let resp = client
        .get(uri)
        .send()
        .await
        .map_err(|e| NexusError::Http(e.to_string()))?;
    let resp = resp
        .error_for_status()
        .map_err(|e| NexusError::Http(e.to_string()))?;

    let total = resp.content_length();
    tracing::info!(total = ?total, "starting streaming download"); // no uri logged

    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| NexusError::io(dest, e))?;

    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();

    // Chunk-by-chunk: the ONLY permitted body-consumption path (no full-buffer
    // whole-body read, which would OOM on a multi-GB pack).
    while let Some(chunk) = stream.next().await {
        if cancel.is_cancelled() {
            // Drop the partial file; a cancelled download must not look "done".
            drop(file);
            let _ = tokio::fs::remove_file(dest).await;
            return Err(NexusError::Http("download cancelled".to_string()));
        }
        let chunk = chunk.map_err(|e| NexusError::Http(e.to_string()))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| NexusError::io(dest, e))?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total);
    }

    file.flush().await.map_err(|e| NexusError::io(dest, e))?;
    Ok(downloaded)
}
