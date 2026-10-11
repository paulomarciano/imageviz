pub mod handler;
pub mod stages;

pub use handler::SseEvent;

use notify::event::ModifyKind;
use notify::{EventKind, RecursiveMode, Watcher};
use notify_debouncer_mini::{Config, DebounceEventResult, Debouncer, new_debouncer_opt};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;

/// Result type returned by [`FileWatcher::new`].
type WatcherResult =
    Result<(FileWatcher, mpsc::Receiver<Vec<FileEvent>>), Box<dyn std::error::Error>>;

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

/// Return `true` if a notify event should be forwarded to the debouncer.
///
/// Two kinds of events are dropped:
///
/// - **`Access`** — notify's inotify backend watches `IN_OPEN`, so *opening a
///   watched file for reading* emits `EventKind::Access(AccessKind::Open(_))`.
///   ImageViz reads every watched media file while indexing (hashing in
///   `scanner::hasher`, metadata and ffprobe detection), so these events feed
///   straight back into the handler — a self-sustaining busy loop that burns a
///   CPU core indefinitely. This is the loop that must not re-enter the
///   debouncer.
/// - **`Modify(Metadata)`** — defensive. Metadata-only changes (`IN_ATTRIB` on
///   inotify: chmod/chown/xattr/explicit `utimes`; `NOTE_ATTRIB` on kqueue)
///   never alter content, and ImageViz derives `file_modified_at` from the
///   filesystem at index time — never from watcher events — so dropping them
///   is safe. This cannot mask a content change: every notify backend reports
///   content writes as `Create`/`Modify(Data)`/`Modify(Name)`.
///
/// Everything else — `Create`, `Modify(Data)`, `Modify(Name)`, `Remove`,
/// `Any`, `Other` — is forwarded unchanged.
fn should_forward(kind: &EventKind) -> bool {
    !matches!(kind, EventKind::Access(_) | EventKind::Modify(ModifyKind::Metadata(_)))
}

/// A [`notify::Watcher`] adaptor that drops self-inflicted events before they
/// reach the debouncer.
///
/// Without it, reading a watched media file re-triggers the handler, which
/// reads the file again — a self-sustaining busy loop that burns a CPU core
/// indefinitely (observed spinning for 27 days). The filter is defined by
/// [`should_forward`].
///
/// This wrapper forwards every genuine change event and any `Err` unchanged,
/// preserving the existing debounce semantics while breaking the loop.
struct AccessFilteringWatcher {
    inner: notify::RecommendedWatcher,
}

impl Watcher for AccessFilteringWatcher {
    fn new<F: notify::EventHandler>(
        mut event_handler: F,
        config: notify::Config,
    ) -> notify::Result<Self>
    where
        Self: Sized,
    {
        let inner = notify::RecommendedWatcher::new(
            move |result: notify::Result<notify::Event>| match result {
                Ok(event) => {
                    // Drop self-inflicted events — see `should_forward`.
                    if should_forward(&event.kind) {
                        event_handler.handle_event(Ok(event));
                    }
                }
                // Errors are forwarded unchanged.
                Err(error) => event_handler.handle_event(Err(error)),
            },
            config,
        )?;

        Ok(Self { inner })
    }

    fn watch(&mut self, path: &Path, recursive_mode: RecursiveMode) -> notify::Result<()> {
        self.inner.watch(path, recursive_mode)
    }

    fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        self.inner.unwatch(path)
    }

    fn configure(&mut self, option: notify::Config) -> notify::Result<bool> {
        self.inner.configure(option)
    }

    fn kind() -> notify::WatcherKind {
        <notify::RecommendedWatcher as Watcher>::kind()
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
    debouncer: Debouncer<AccessFilteringWatcher>,
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
    pub fn new(watched_paths: &[PathBuf]) -> WatcherResult {
        let (tx, rx) = mpsc::channel(256);
        let tx_clone = tx.clone();

        // Explicit 500ms window (also `Config::default()`) while swapping in
        // `AccessFilteringWatcher`, which drops access events from our own
        // reads and metadata-only churn before they reach the debouncer — see
        // `should_forward`.
        let config = Config::default().with_timeout(Duration::from_millis(500));
        let mut debouncer = new_debouncer_opt::<_, AccessFilteringWatcher>(
            config,
            move |result: DebounceEventResult| {
                match result {
                    Ok(debounced_events) => {
                        let file_events: Vec<FileEvent> = debounced_events
                            .iter()
                            .filter_map(|de| {
                                // Skip non-media and hidden files
                                if !is_supported_extension(&de.path) {
                                    return None;
                                }
                                if is_hidden_path(&de.path) {
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
            debouncer.watcher().watch(path, RecursiveMode::Recursive)?;
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
        self.debouncer.watcher().watch(path, RecursiveMode::Recursive)?;
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

use crate::media_types::{is_hidden_path, is_supported_extension};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "watcher_test.rs"]
mod tests;
