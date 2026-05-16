# Wave 3.8 — Add Stats Endpoint

| Field | Value |
|-------|-------|
| **Wave** | 3 — Backend: Search, Cursor Pagination & Real-time SSE |
| **Seq** | 08 |
| **Estimate** | 45 minutes |
| **Depends on** | 1.8 (indexer with progress tracking) |
| **Parallel** | Can run in parallel with 3.5, 3.6, 3.7 |

---

## Overview

Add the `GET /stats` endpoint that returns index statistics: total files indexed, file count by MIME type, last indexed timestamp, and current indexing status (idle/active). This feeds the configuration panel in the frontend (Wave 6.3).

## Prerequisites

- SQLite with indexed media items (1.8)
- Indexer progress tracking (1.9)

## Reference Files

- `documents/plans/development-plan.md` — §3.2 Endpoints (GET /stats), §5 Wave 3 task 3.8
- `.opencode/context/development/principles/api-design.md`

## Deliverables

```
backend/src/routes/
└── stats.rs                     # GET /stats handler
```

## Acceptance Criteria (Pass/Fail)

- [ ] `GET /api/v1/stats` returns index statistics
- [ ] Response includes:
  - `total_files` — total number of indexed media items
  - `total_size_bytes` — sum of all file sizes
  - `by_mime_type` — count by MIME type (e.g., `{"image/png": 12000, "video/mp4": 50}`)
  - `last_indexed_at` — ISO 8601 timestamp of most recent indexing
  - `indexing_status` — current indexing progress (from 1.9)
  - `watched_folders_count` — number of configured watched folders
- [ ] Response is fast (all queries are simple aggregates on SQLite)
- [ ] Response format is consistent with other endpoints

## Implementation Notes

**Stats handler:**
```rust
use axum::{extract::State, Json};

async fn get_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<IndexStats>, AppError> {
    let db = &state.db;
    
    // Total files
    let total_files: i64 = db.query_row(
        "SELECT COUNT(*) FROM media_items", [], |r| r.get(0)
    )?;
    
    // Total size
    let total_size: i64 = db.query_row(
        "SELECT COALESCE(SUM(file_size), 0) FROM media_items", [], |r| r.get(0)
    )?;
    
    // By MIME type
    let mut by_mime_stmt = db.prepare(
        "SELECT mime_type, COUNT(*) as cnt FROM media_items GROUP BY mime_type ORDER BY cnt DESC"
    )?;
    let by_mime: HashMap<String, i64> = by_mime_stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();
    
    // Last indexed
    let last_indexed: Option<String> = db.query_row(
        "SELECT MAX(indexed_at) FROM media_items", [], |r| r.get(0)
    )?;
    
    // Current indexing progress
    let indexing_status = state.progress_tracker.current_status();
    
    // Watched folders
    let config = load_config(db)?;
    
    Ok(Json(IndexStats {
        total_files: total_files as u64,
        total_size_bytes: total_size as u64,
        by_mime_type: by_mime,
        last_indexed_at: last_indexed,
        indexing_status,
        watched_folders_count: config.watched_folders.len() as u64,
    }))
}

#[derive(Serialize)]
struct IndexStats {
    total_files: u64,
    total_size_bytes: u64,
    by_mime_type: HashMap<String, i64>,
    last_indexed_at: Option<String>,
    indexing_status: IndexProgress,
    watched_folders_count: u64,
}
```

**Performance** — All queries are simple aggregates on indexed columns. The `COUNT(*)` and `SUM()` scan the table but are O(n) only on the row count. For 100K items, this should complete in <10ms on SQLite.

## Test Strategy

```rust
#[tokio::test]
async fn test_stats_empty_database() {
    let app = setup_test_app().await;
    let response = app.get("/api/v1/stats").send().await;
    assert_eq!(response.status(), 200);
    
    let stats: IndexStats = response.json().await;
    assert_eq!(stats.total_files, 0);
    assert_eq!(stats.total_size_bytes, 0);
}

#[tokio::test]
async fn test_stats_with_indexed_data() {
    let app = setup_test_app_with_n_items(50).await;
    let response = app.get("/api/v1/stats").send().await;
    
    let stats: IndexStats = response.json().await;
    assert_eq!(stats.total_files, 50);
    assert!(stats.total_size_bytes > 0);
    assert!(stats.by_mime_type.contains_key("image/png"));
    assert!(stats.last_indexed_at.is_some());
}
```

## External Docs

This task is self-contained with no new external libraries.
