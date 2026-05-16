# Wave 3.5 — Implement File System Watcher (notify + Debouncer)

| Field | Value |
|-------|-------|
| **Wave** | 3 — Backend: Search, Cursor Pagination & Real-time SSE |
| **Seq** | 05 |
| **Estimate** | 2.5 hours |
| **Depends on** | None (independent — needs notify crate) |
| **Parallel** | No (but can start before 3.4) |

---

## Overview

Set up a file system watcher using the `notify` crate with debouncing. When files are added, modified, or deleted in watched folders, the watcher emits events. These events are debounced (500ms window) to batch rapid changes into a single processing cycle, preventing index thrashing during bulk operations.

## Prerequisites

- `notify = { version = "8", features = ["macos_kqueue"] }` in Cargo.toml
- `notify-debouncer-mini = "0.7"` in Cargo.toml
- Watched folder configuration (1.2)

## Reference Files

- `documents/plans/development-plan.md` — §2 Tech Stack (notify + debouncer), §5 Wave 3 task 3.5, §8.2 Key Performance Decisions (debounced watcher), §9 Risk Register (cross-platform inconsistencies)
- `.opencode/context/development/principles/clean-code.md`

## Deliverables

```
backend/src/watcher/
├── mod.rs                       # Public interface (FileWatcher struct)
└── ... (handler.rs — separate task 3.6)
```

## Acceptance Criteria (Pass/Fail)

- [ ] `FileWatcher` watches configured folders for file events
- [ ] Events emitted: file created, file modified, file deleted
- [ ] Events are **debounced** — rapid file changes within 500ms are batched
- [ ] Only watched file types trigger events (`.png`, `.jpg`, `.webp`, `.gif`, `.mp4`, `.webm`)
- [ ] Hidden files/directories are ignored
- [ ] Events include file path (relative and absolute)
- [ ] Watcher can be started and stopped gracefully (using a cancellation token)
- [ ] Multiple watched folders are supported
- [ ] Watcher outputs to a `tokio::sync::mpsc` channel (consumed by handler in 3.6)
- [ ] Unit test: create a file in watched folder → event received within 1 second

## Implementation Notes

**FileWatcher setup with notify:**
```rust
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use tokio::sync::mpsc;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum FileEvent {
    Created { path: PathBuf },
    Modified { path: PathBuf },
    Deleted { path: PathBuf },
}

pub struct FileWatcher {
    tx: mpsc::Sender<Vec<FileEvent>>,
    debouncer: Debouncer<RecommendedWatcher, notify_debouncer_mini::notify::INotifyWatcher>,
}

impl FileWatcher {
    pub fn new(
        watched_paths: &[PathBuf],
    ) -> Result<(Self, mpsc::Receiver<Vec<FileEvent>>), Error> {
        let (tx, rx) = mpsc::channel(256);
        let tx_clone = tx.clone();
        
        let mut debouncer = new_debouncer(
            Duration::from_millis(500),
            None,
            move |result: DebounceEventResult| {
                match result {
                    Ok(events) => {
                        let file_events: Vec<FileEvent> = events
                            .iter()
                            .filter_map(|e| convert_event(e))
                            .filter(|e| is_supported_media(&e.path()))
                            .collect();
                        
                        if !file_events.is_empty() {
                            let _ = tx_clone.try_send(file_events);
                        }
                    }
                    Err(errors) => {
                        for e in errors {
                            eprintln!("Watch error: {:?}", e);
                        }
                    }
                }
            },
        )?;
        
        // Watch each folder
        for path in watched_paths {
            debouncer.watcher().watch(path, RecursiveMode::Recursive)?;
        }
        
        Ok((Self { tx, debouncer }, rx))
    }
    
    pub fn watch(&mut self, path: &Path) -> Result<(), Error> {
        self.debouncer.watcher().watch(path, RecursiveMode::Recursive)?;
        Ok(())
    }
    
    pub fn unwatch(&mut self, path: &Path) -> Result<(), Error> {
        self.debouncer.watcher().unwatch(path)?;
        Ok(())
    }
}

fn convert_event(event: &Event) -> Option<FileEvent> {
    match event.kind {
        EventKind::Create(_) => Some(FileEvent::Created {
            path: event.paths.first()?.clone(),
        }),
        EventKind::Modify(_) => Some(FileEvent::Modified {
            path: event.paths.first()?.clone(),
        }),
        EventKind::Remove(_) => Some(FileEvent::Deleted {
            path: event.paths.first()?.clone(),
        }),
        _ => None,
    }
}
```

**Debouncing** — The `notify-debouncer-mini` crate batches events within a time window (500ms). This prevents the indexer from being invoked 1000 times when 1000 files are copied at once.

**Cross-platform notes:**
- Linux: Uses `inotify` — fast, reliable
- macOS: Uses `FSEvents` (requires `macos_kqueue` feature flag)
- Windows: Uses `ReadDirectoryChangesW`

**Watcher lifecycle:**
- Start from `main.rs` after loading config
- Stop during graceful shutdown (Wave 7.1)
- Reconfigure when watched folders change (PUT /config)

## Test Strategy

```rust
#[tokio::test]
async fn test_watcher_detects_new_file() {
    let dir = tempfile::tempdir().unwrap();
    let (watcher, mut rx) = FileWatcher::new(&[dir.path().to_path_buf()]).unwrap();
    
    // Create a new file
    std::fs::write(dir.path().join("new_image.png"), b"test data").unwrap();
    
    // Wait for debounce
    tokio::time::sleep(Duration::from_millis(800)).await;
    
    let events = rx.try_recv().unwrap_or_default();
    let created = events.iter().any(|e| matches!(e, FileEvent::Created { .. }));
    assert!(created, "Watcher should detect created file");
}

#[tokio::test]
async fn test_watcher_ignores_hidden_files() {
    let dir = tempfile::tempdir().unwrap();
    let (watcher, mut rx) = FileWatcher::new(&[dir.path().to_path_buf()]).unwrap();
    
    std::fs::create_dir(dir.path().join(".hidden")).unwrap();
    std::fs::write(dir.path().join(".hidden").join("test.png"), b"data").unwrap();
    
    tokio::time::sleep(Duration::from_millis(800)).await;
    
    let events = rx.try_recv().unwrap_or_default();
    assert!(events.is_empty(), "Hidden files should be ignored");
}

#[tokio::test]
async fn test_watcher_detects_deletion() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("to_delete.png");
    std::fs::write(&file_path, b"data").unwrap();
    
    let (watcher, mut rx) = FileWatcher::new(&[dir.path().to_path_buf()]).unwrap();
    
    std::fs::remove_file(&file_path).unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;
    
    let events = rx.try_recv().unwrap_or_default();
    let deleted = events.iter().any(|e| matches!(e, FileEvent::Deleted { .. }));
    assert!(deleted, "Watcher should detect deleted file");
}
```

## External Docs

Use **ExternalScout** to fetch current docs for:
- `notify` 8.x — `RecommendedWatcher`, `Event`, `EventKind`, `RecursiveMode`
- `notify-debouncer-mini` 0.7 — debouncer API, `DebounceEventResult`

**Note:** notify 8.x API differs significantly from 6.x and 7.x. Always refer to current docs.
