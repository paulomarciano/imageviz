pub mod handler;

pub use handler::SseEvent;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;

/// Result type returned by [`FileWatcher::new`].
type WatcherResult = Result<(FileWatcher, mpsc::Receiver<Vec<FileEvent>>), Box<dyn std::error::Error>>;

#[derive(Debug, Clone)]
pub enum FileEvent {
    /// A new file was created in a watched folder.
    Created { path: PathBuf },
    /// An existing file was modified in a watched folder.
    Modified { path: PathBuf },
    /// A file was deleted from a watched folder.
    Deleted { path: PathBuf },
}

impl FileEvent {
    /// Return the file path associated with this event.
    pub fn path(&self) -> &Path {
        match self {
            FileEvent::Created { path } => path,
            FileEvent::Modified { path } => path,
            FileEvent::Deleted { path } => path,
        }
    }
}

/// File system watcher that monitors watched folders for changes.
///
/// Events are debounced (500ms interval) and sent in batches via a
/// `tokio::sync::mpsc` channel to decouple the file system poll loop
/// from async event consumers (e.g., indexer handler in task 3.6).
///
/// The watcher must be kept alive for the duration of monitoring;
/// dropping it stops watching all paths.
///
/// # Event Type Resolution
///
/// `notify-debouncer-mini` 0.7 collapses all native event kinds into
/// `DebouncedEventKind::Any` or `AnyContinuous`.  We infer the actual
/// event type by checking whether the file still exists on disk:
/// - File exists → `Created` or `Modified` (caller can diff checksums)
/// - File does not exist → `Deleted`
pub struct FileWatcher {
    debouncer: Debouncer<notify::RecommendedWatcher>,
}

impl FileWatcher {
    /// Create a new file watcher and start monitoring the given paths.
    ///
    /// Returns the watcher (must be held alive) and a channel receiver
    /// for debounced `FileEvent` batches.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying notify watcher fails to initialise
    /// (e.g., when a watched path does not exist).
    pub fn new(
        watched_paths: &[PathBuf],
    ) -> WatcherResult {
        let (tx, rx) = mpsc::channel(256);
        let tx_clone = tx.clone();

        let mut debouncer = new_debouncer(
            Duration::from_millis(500),
            move |result: DebounceEventResult| {
                match result {
                    Ok(debounced_events) => {
                        let file_events: Vec<FileEvent> = debounced_events
                            .iter()
                            .filter_map(|de| {
                                // Skip non-media and hidden files
                                if !is_supported_media(&de.path) {
                                    return None;
                                }
                                if is_hidden(&de.path) {
                                    return None;
                                }

                                // Infer event type from file existence
                                if de.path.exists() {
                                    // File exists → treat as modified (create or modify)
                                    Some(FileEvent::Modified { path: de.path.clone() })
                                } else {
                                    Some(FileEvent::Deleted { path: de.path.clone() })
                                }
                            })
                            .collect();

                        if !file_events.is_empty() {
                            // try_send is non-blocking and safe to call from
                            // the notify callback thread (not on the tokio runtime).
                            let _ = tx_clone.try_send(file_events);
                        }
                    }
                    Err(error) => {
                        tracing::warn!("File watcher error: {:?}", error);
                    }
                }
            },
        )?;

        for path in watched_paths {
            debouncer
                .watcher()
                .watch(path, RecursiveMode::Recursive)?;
        }

        Ok((Self { debouncer }, rx))
    }

    /// Add a path to the set of watched directories.
    ///
    /// Watches recursively — all subdirectories are included.
    ///
    /// # Errors
    ///
    /// Returns an error if the path does not exist or the OS-level
    /// watcher cannot be configured for it.
    pub fn watch(&mut self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        self.debouncer
            .watcher()
            .watch(path, RecursiveMode::Recursive)?;
        Ok(())
    }

    /// Remove a path from the set of watched directories.
    ///
    /// # Errors
    ///
    /// Returns an error if the path was not being watched.
    pub fn unwatch(&mut self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        self.debouncer.watcher().unwatch(path)?;
        Ok(())
    }
}

impl std::fmt::Debug for FileWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileWatcher").finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Return `true` if the path has a file extension that ImageViz supports.
///
/// Supported media types: PNG, JPG, JPEG, WebP, GIF, MP4, WebM, MOV.
fn is_supported_media(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).unwrap_or(""),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "mp4" | "webm" | "mov"
    )
}

/// Return `true` if any component of the path starts with a dot (`.`),
/// indicating a hidden file or directory.
fn is_hidden(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str().to_str().is_some_and(|s| s.starts_with('.')))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "watcher_test.rs"]
mod tests;
