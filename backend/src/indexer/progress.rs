use std::sync::Arc;
use tokio::sync::watch;

/// Current status of the indexing process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexStatus {
    Idle,
    Scanning,
    Indexing,
    Complete,
}

/// Progress information for the current indexing run.
#[derive(Debug, Clone)]
pub struct IndexProgress {
    pub status: IndexStatus,
    pub total: usize,
    pub processed: usize,
    pub errors: Vec<String>,
}

/// Thread-safe progress tracker with watch channel for real-time updates.
///
/// The watch channel enables SSE-based progress reporting (used by `/events` endpoint).
/// `status_rx` is exposed for consumers to subscribe to status changes.
#[derive(Debug)]
pub struct ProgressTracker {
    inner: Arc<std::sync::Mutex<IndexProgress>>,
    status_tx: watch::Sender<IndexStatus>,
    /// Receiver for status changes (for SSE or UI updates).
    pub status_rx: watch::Receiver<IndexStatus>,
}

impl ProgressTracker {
    /// Create a new progress tracker in `Idle` state.
    pub fn new() -> Self {
        let (status_tx, status_rx) = watch::channel(IndexStatus::Idle);
        Self {
            inner: Arc::new(std::sync::Mutex::new(IndexProgress {
                status: IndexStatus::Idle,
                total: 0,
                processed: 0,
                errors: Vec::new(),
            })),
            status_tx,
            status_rx,
        }
    }

    /// Set the current indexing status.
    pub fn set_status(&self, status: IndexStatus) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.status = status;
        }
        let _ = self.status_tx.send(status);
    }

    /// Set the total number of files to process.
    pub fn set_total(&self, total: usize) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.total = total;
        }
    }

    /// Increment the processed file count.
    pub fn increment_processed(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.processed += 1;
        }
    }

    /// Record an error message encountered during indexing.
    /// Capped at 1000 errors to prevent unbounded memory growth.
    pub fn add_error(&self, msg: String) {
        if let Ok(mut inner) = self.inner.lock() {
            if inner.errors.len() >= 1000 {
                return;
            }
            inner.errors.push(msg);
        }
    }

    /// Take a snapshot of the current progress state.
    pub fn snapshot(&self) -> IndexProgress {
        self.inner
            .lock()
            .map(|inner| IndexProgress {
                status: inner.status,
                total: inner.total,
                processed: inner.processed,
                errors: inner.errors.clone(),
            })
            .unwrap_or(IndexProgress {
                status: IndexStatus::Idle,
                total: 0,
                processed: 0,
                errors: Vec::new(),
            })
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "progress_test.rs"]
mod tests;
