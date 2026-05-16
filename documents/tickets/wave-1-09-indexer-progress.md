# Wave 1.9 — Add Indexer Progress Reporting

| Field | Value |
|-------|-------|
| **Wave** | 1 — Backend: File System Scanner & Metadata Extraction |
| **Seq** | 09 |
| **Estimate** | 1 hour |
| **Depends on** | 1.8 (indexer orchestrator) |
| **Parallel** | No |

---

## Overview

Add progress reporting to the indexer so the frontend (and API consumers) can track indexing status. Reports total files to process, files processed so far, current file being indexed, and completion status. This uses a `tokio::sync::watch` channel for broadcasting progress to SSE (which will be implemented in Wave 3.7).

## Prerequisites

- Indexer orchestrator (1.8)
- `tokio::sync` in Cargo.toml (already present via `tokio = { features = ["full"] }`)

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 1 task 1.9, §3.3 SSE Event Format (`indexing_complete` event)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/indexer/
├── mod.rs                       # Updated: integrate progress reporting
└── progress.rs                  # Progress tracking struct + channel
```

## Acceptance Criteria (Pass/Fail)

- [ ] `IndexProgress` struct tracks: `status` (Idle/Scanning/Indexing/Complete/Error), `total_files`, `processed_files`, `current_file`, `started_at`, `errors` count
- [ ] Progress is updated after each file is indexed (every file, not just batches)
- [ ] `watch` channel broadcasts progress to subscribers
- [ ] Progress is accessible via a shared struct (for polling) and a channel (for streaming)
- [ ] `status` transitions: Idle → Scanning → Indexing → Complete (or Error)
- [ ] Progress safe to read from multiple tasks (Send + Sync)

## Implementation Notes

**IndexProgress struct:**
```rust
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Debug, Clone, Serialize)]
pub enum IndexStatus {
    Idle,
    Scanning,
    Indexing,
    Complete,
    Error(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexProgress {
    pub status: IndexStatus,
    pub total_files: usize,
    pub processed_files: usize,
    pub current_file: Option<String>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error_count: usize,
    pub errors: Vec<String>, // Last N errors (max 10)
}
```

**Progress tracker with watch channel:**
```rust
pub struct ProgressTracker {
    progress: Arc<std::sync::Mutex<IndexProgress>>,
    tx: watch::Sender<IndexProgress>,
}

impl ProgressTracker {
    pub fn new() -> (Self, watch::Receiver<IndexProgress>) {
        let initial = IndexProgress::default();
        let (tx, rx) = watch::channel(initial.clone());
        (
            Self {
                progress: Arc::new(std::sync::Mutex::new(initial)),
                tx,
            },
            rx,
        )
    }

    pub fn set_status(&self, status: IndexStatus) {
        let mut progress = self.progress.lock().unwrap();
        progress.status = status;
        let _ = self.tx.send(progress.clone());
    }

    pub fn set_total(&self, total: usize) {
        let mut progress = self.progress.lock().unwrap();
        progress.total_files = total;
        let _ = self.tx.send(progress.clone());
    }

    pub fn increment_processed(&self, current_file: &str) {
        let mut progress = self.progress.lock().unwrap();
        progress.processed_files += 1;
        progress.current_file = Some(current_file.to_string());
        let _ = self.tx.send(progress.clone());
    }

    pub fn add_error(&self, error: String) {
        let mut progress = self.progress.lock().unwrap();
        progress.error_count += 1;
        if progress.errors.len() < 10 {
            progress.errors.push(error);
        }
        let _ = self.tx.send(progress.clone());
    }

    pub fn subscribe(&self) -> watch::Receiver<IndexProgress> {
        self.tx.subscribe()
    }
}
```

**Integration with indexer (1.8):**
```rust
pub async fn full_index(
    db: &Connection,
    config: &AppConfig,
    progress: &ProgressTracker,
) -> Result<(), Error> {
    progress.set_status(IndexStatus::Scanning);
    
    let all_files = scan_all_folders(config)?;
    progress.set_total(all_files.len());
    progress.set_status(IndexStatus::Indexing);
    
    for file in &all_files {
        progress.increment_processed(&file.relative_path);
        // ... index this file ...
    }
    
    progress.set_status(IndexStatus::Complete);
    Ok(())
}
```

**Why `std::sync::Mutex` and not `tokio::sync::Mutex`?** Because locking is trivial (updating a few fields) — no async I/O inside the lock. `std::sync::Mutex` is simpler and faster.

## Test Strategy

```rust
#[tokio::test]
async fn test_progress_tracking() {
    let (tracker, mut rx) = ProgressTracker::new();
    
    // Initial state
    assert!(matches!(rx.borrow().status, IndexStatus::Idle));
    
    tracker.set_status(IndexStatus::Scanning);
    rx.changed().await.unwrap();
    assert!(matches!(rx.borrow().status, IndexStatus::Scanning));
    
    tracker.set_total(100);
    rx.changed().await.unwrap();
    assert_eq!(rx.borrow().total_files, 100);
    
    tracker.increment_processed("test.png");
    rx.changed().await.unwrap();
    assert_eq!(rx.borrow().processed_files, 1);
    assert_eq!(rx.borrow().current_file.as_deref(), Some("test.png"));
}

#[tokio::test]
async fn test_progress_multiple_subscribers() {
    let (tracker, rx1) = ProgressTracker::new();
    let mut rx2 = tracker.subscribe();
    
    tracker.set_total(50);
    rx1.changed().await.unwrap();
    rx2.changed().await.unwrap();
    // Both see the same value
}
```
