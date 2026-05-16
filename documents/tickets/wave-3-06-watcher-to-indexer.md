# Wave 3.6 — Wire Watcher Events → Indexer → Broadcast Channel

| Field | Value |
|-------|-------|
| **Wave** | 3 — Backend: Search, Cursor Pagination & Real-time SSE |
| **Seq** | 06 |
| **Estimate** | 1.5 hours |
| **Depends on** | 3.5 (file watcher), 1.8 (indexer) |
| **Parallel** | No |

---

## Overview

Connect the file system watcher to the indexing pipeline. When files are created/modified/deleted in watched folders, the handler: (1) updates SQLite, (2) updates the Tantivy index, and (3) broadcasts the event to SSE subscribers via a `tokio::sync::broadcast` channel.

## Prerequisites

- File watcher (3.5) — emits events to an mpsc channel
- Indexer (1.8) — can index individual files
- Tantivy index (3.2) — can add/delete documents

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 3 task 3.6, §3.3 SSE Event Format (file_created, file_deleted, file_modified, indexing_complete)
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/watcher/
├── mod.rs                       # Updated
└── handler.rs                   # Event handler: watcher events → indexer → broadcast
```

## Acceptance Criteria (Pass/Fail)

- [ ] Watcher events (from 3.5's mpsc channel) are consumed and processed
- [ ] `FileCreated` → new file scanned, metadata extracted, stored in SQLite, indexed in Tantivy, broadcast to SSE
- [ ] `FileModified` → file re-scanned, SQLite updated (via INSERT OR REPLACE), Tantivy updated (delete + insert), broadcast to SSE
- [ ] `FileDeleted` → SQLite row deleted (or flagged as deleted), Tantivy document deleted, broadcast to SSE
- [ ] Unknown/new event kinds are logged and ignored (don't crash)
- [ ] Handler runs in a background Tokio task (doesn't block the main loop)
- [ ] Events broadcast to `tokio::sync::broadcast` channel for SSE subscribers (Wave 3.7)

## Implementation Notes

**Handler task:**
```rust
use tokio::sync::{mpsc, broadcast};

pub struct SseEvent {
    pub event_type: String,
    pub data: serde_json::Value,
}

pub async fn run_event_handler(
    mut file_events_rx: mpsc::Receiver<Vec<FileEvent>>,
    db: Connection,                 // Or connection pool
    search_index: Arc<IndexManager>,
    sse_tx: broadcast::Sender<SseEvent>,
    progress_tracker: ProgressTracker,
) {
    while let Some(events) = file_events_rx.recv().await {
        for event in events {
            if let Err(e) = handle_single_event(
                &event,
                &db,
                &search_index,
                &sse_tx,
                &progress_tracker,
            ).await {
                eprintln!("Error handling file event for {:?}: {}", event.path(), e);
            }
        }
        
        // Optional: commit Tantivy after batch
        let _ = search_index.commit();
    }
}

async fn handle_single_event(
    event: &FileEvent,
    db: &Connection,
    search_index: &IndexManager,
    sse_tx: &broadcast::Sender<SseEvent>,
    progress: &ProgressTracker,
) -> Result<(), Error> {
    match event {
        FileEvent::Created { path } => {
            // Scan single file
            let info = detect_media(path).await?;
            let checksum = compute_file_hash(path).await?;
            let metadata = extract_metadata(path).await?;
            let id = Uuid::new_v4().to_string();
            
            // Store in SQLite
            db.store_media_item(&MediaItem { id: id.clone(), ... })?;
            
            // Index in Tantivy
            search_index.add_document(IndexableDoc::from(&item))?;
            
            // Broadcast to SSE
            let _ = sse_tx.send(SseEvent {
                event_type: "file_created".into(),
                data: serde_json::to_value(&item)?,
            });
        }
        
        FileEvent::Modified { path } => {
            // Look up existing item by path
            // Re-extract metadata
            // Update SQLite + Tantivy
            // Broadcast "file_modified"
        }
        
        FileEvent::Deleted { path } => {
            // Look up item by path
            // Delete from SQLite
            // Delete from Tantivy
            // Broadcast "file_deleted"
        }
    }
    
    Ok(())
}
```

**Broadcast channel setup:**
```rust
use tokio::sync::broadcast;

// 256 is the capacity of the channel (bounded)
let (sse_tx, _) = broadcast::channel::<SseEvent>(256);
```

The broadcast channel is cloned for each SSE subscriber. Slow subscribers that can't keep up will miss older messages (lagged receiver behavior).

**Path matching — finding the media item for a watcher event:**
```rust
// Given an absolute path from the watcher event,
// compute the relative_path (relative to the watched folder root)
// Then query SQLite: SELECT * FROM media_items WHERE relative_path = ?

fn resolve_relative_path(absolute_path: &Path, watched_folders: &[WatchedFolder]) -> Option<String> {
    for folder in watched_folders {
        if let Ok(relative) = absolute_path.strip_prefix(&folder.path) {
            return Some(relative.to_string_lossy().to_string());
        }
    }
    None
}
```

## Test Strategy

```rust
#[tokio::test]
async fn test_file_created_event_triggers_indexing() {
    let dir = tempfile::tempdir().unwrap();
    let db = create_test_db();
    let (sse_tx, mut sse_rx) = broadcast::channel(256);
    let (file_tx, file_rx) = mpsc::channel(256);
    
    // Spawn handler
    tokio::spawn(run_event_handler(
        file_rx,
        db.clone(),
        search_index.clone(),
        sse_tx,
        progress_tracker,
    ));
    
    // Create a file
    std::fs::write(dir.path().join("test.png"), b"fake png data").unwrap();
    
    // Send event to handler
    file_tx.send(vec![FileEvent::Created {
        path: dir.path().join("test.png"),
    }]).await.unwrap();
    
    // Wait for processing
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    // Verify file in DB
    let count = db.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get::<_, i32>(0)).unwrap();
    assert_eq!(count, 1);
    
    // Verify SSE event broadcast
    let sse_event = sse_rx.try_recv().unwrap();
    assert_eq!(sse_event.event_type, "file_created");
}
```
