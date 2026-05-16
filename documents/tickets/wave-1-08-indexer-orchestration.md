# Wave 1.8 — Implement Indexer Orchestration (Scan → Extract → Store)

| Field | Value |
|-------|-------|
| **Wave** | 1 — Backend: File System Scanner & Metadata Extraction |
| **Seq** | 08 |
| **Estimate** | 2.5 hours |
| **Depends on** | 1.1 (SQLite), 1.3 (scanner), 1.6 (detection), 1.7 (hashing) |
| **Parallel** | No (orchestrates all Wave 1 modules) |

---

## Overview

Wire together all Wave 1 modules into an indexing orchestrator. Performs the full pipeline: scan watched folders → detect file type → extract metadata → compute hash → store in SQLite. This is the core "index" operation.

## Prerequisites

- SQLite DB with schema (1.1)
- Config management for watched folders (1.2)
- Scanner (1.3)
- File type detection (1.6)
- Hash computation (1.7)

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 1 overview, §8.1 Performance Targets (scan >500 files/sec)
- `.opencode/context/core/standards/code-quality.md` — functional composition, modular design

## Deliverables

```
backend/src/indexer/
├── mod.rs                       # Orchestrator: full_index, incremental_index
└── (progress.rs — separate task 1.9)
```

## Acceptance Criteria (Pass/Fail)

- [ ] `full_index(db, config)` scans all watched folders and stores all media items in SQLite
- [ ] Each stored item has: id (UUID), filename, relative_path, mime_type, width, height, file_size, created_at, modified_at, checksum, metadata_json
- [ ] Items are inserted with `INSERT OR REPLACE` (idempotent — re-indexing same file updates, not duplicates)
- [ ] UUID v4 generated for each new media item
- [ ] `incremental_index(db, config)` only processes new/modified files (detected by checksum comparison)
- [ ] Indexer skips files that haven't changed (same checksum → same relative_path → skip)
- [ ] Indexer can be called multiple times without data loss
- [ ] Handles deleted files: items in DB but not on disk can be flagged/deleted (or left — watcher handles deletes in Wave 3)
- [ ] Integration test: index 100 test files, verify all entries in DB

## Implementation Notes

**Indexer flow:**
```
1. Load config → get watched_folders
2. For each folder:
   a. Scanner: walk directory → Vec<FileEntry>
   b. For each file:
      - Generate UUID (if new)
      - Detect: MediaInfo (mime_type, dimensions, size)
      - Hash: compute SHA-256
      - Metadata: parse PNG chunks or video ffprobe
      - Store: INSERT OR REPLACE into SQLite
```

**UUID generation:**
```rust
use uuid::Uuid;

// For existing files, look up UUID by relative_path
// For new files, generate:
let id = Uuid::new_v4().to_string();
```

**INSERT OR REPLACE with checksum change detection:**
```rust
// Check if file already indexed with same hash
let existing: Option<(String, String)> = db.query_row(
    "SELECT id, checksum FROM media_items WHERE relative_path = ?1",
    params![entry.relative_path],
    |r| Ok((r.get(0)?, r.get(1)?)),
).optional()?;

if let Some((existing_id, existing_hash)) = existing {
    if existing_hash == new_hash {
        // File unchanged — skip
        continue;
    }
    // File changed — update with existing UUID
    id = existing_id;
}

// Upsert
db.execute(
    "INSERT OR REPLACE INTO media_items (id, filename, relative_path, mime_type, width, height, file_size, thumbnail_path, file_created_at, file_modified_at, indexed_at, metadata_json, checksum) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
    params![id, ...],
)?;
```

**Performance considerations:**
- Use prepared statements for batch inserts
- Process files sequentially (not parallel) — SQLite writes are serialized
- Chunk scan results (e.g., process in batches of 100)
- For >1000 files, consider transaction batching (100 files per transaction)

**Concurrency:** Use a single database connection for indexing (no connection pool needed yet). Read operations can happen on a separate read-only connection.

## Test Strategy

Integration test in `backend/tests/indexer_test.rs` (Task 1.10 will create this file):
```rust
#[tokio::test]
async fn test_full_index() {
    let dir = tempfile::tempdir().unwrap();
    // Create test files: 5 PNGs, 3 WEBMs
    // Run indexer
    let db = open_in_memory_db();
    run_migrations(&db);
    save_config(&db, &AppConfig { watched_folders: vec![dir.path()] });
    
    full_index(&db, &load_config(&db)).await.unwrap();
    
    let count: i32 = db.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 5); // Only 5 PNGs (WEBMs skipped if ffmpeg not available)
}
```

**Unit test** for incremental indexing:
```rust
#[test]
fn test_incremental_index_skips_unchanged() {
    // Index once → 5 entries
    // Index again (no files changed) → 5 entries, no upserts executed
    // Modify one file → index again → that file updated
}
```
