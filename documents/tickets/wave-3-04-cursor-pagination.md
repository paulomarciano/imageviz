# Wave 3.4 — Implement Cursor-Based Pagination for Media List

| Field | Value |
|-------|-------|
| **Wave** | 3 — Backend: Search, Cursor Pagination & Real-time SSE |
| **Seq** | 04 |
| **Estimate** | 2 hours |
| **Depends on** | 3.2 (Tantivy populated — or just SQLite, not dependent on search) |
| **Parallel** | No |

---

## Overview

Add the `GET /media` endpoint with cursor-based pagination. Items are returned sorted by `(file_created_at DESC, id)` — newest first. Cursor pagination uses `WHERE (created_at, id) < (?, ?)` pattern for O(log n) performance, rather than offset-based `OFFSET` which is O(n) for large datasets.

## Prerequisites

- SQLite with `idx_media_sort` index (from 1.1)
- Axum routes structure (0.4)
- MediaItem struct(s) defined

## Reference Files

- `documents/plans/development-plan.md` — §3.3 Cursor Pagination Response format, §3.4 Query Parameters, §8.2 Key Performance Decisions (cursor vs offset), §8.2 SQLite WAL mode
- `.opencode/context/development/principles/api-design.md` — cursor pagination patterns

## Deliverables

```
backend/src/routes/
└── media.rs                     # Updated: add GET /media list handler
```

## Acceptance Criteria (Pass/Fail)

- [ ] `GET /api/v1/media?limit=100` returns first page of 100 items, newest first
- [ ] Response includes `meta.next_cursor` (ISO 8601 date) and `meta.next_cursor_id` (UUID)
- [ ] `GET /api/v1/media?cursor=2025-08-05T14:32:00Z&cursor_id=uuid&limit=100` returns next page after the cursor
- [ ] `meta.has_more` is `true` when more items exist, `false` when last page
- [ ] `meta.total` returns total count of media items
- [ ] Default limit: 100. Max limit: 500 (return 400 if limit > 500)
- [ ] Empty database returns `data: [], meta.has_more: false, meta.total: 0`
- [ ] Supports `?mime_type=image/*` filter (optional — can be added later)
- [ ] Query uses the `idx_media_sort` index (verify with EXPLAIN QUERY PLAN)

## Implementation Notes

**Cursor pagination query:**
```sql
-- First page (no cursor)
SELECT * FROM media_items 
ORDER BY file_created_at DESC, id 
LIMIT ?1 + 1;

-- Subsequent pages (with cursor)
SELECT * FROM media_items 
WHERE (file_created_at, id) < (?1, ?2) 
ORDER BY file_created_at DESC, id 
LIMIT ?3 + 1;
```

The `+ 1` in the limit is to determine `has_more` — if we get N+1 rows, there are more pages.

**Handler structure:**
```rust
use axum::extract::{Query, State};
use serde::Deserialize;

#[derive(Deserialize)]
struct MediaListParams {
    #[serde(default = "default_limit")]
    limit: u32,
    cursor: Option<String>,       // ISO 8601
    cursor_id: Option<String>,    // UUID
    mime_type: Option<String>,    // Optional filter
}

async fn list_media(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MediaListParams>,
) -> Result<Json<PaginatedResponse<MediaItem>>, AppError> {
    // Validate limit
    if params.limit > 500 {
        return Err(AppError::BadRequest("Limit must not exceed 500".into()));
    }
    let limit = params.limit.min(500) as i64;
    
    let (items, has_more) = if let (Some(cursor), Some(cursor_id)) = (&params.cursor, &params.cursor_id) {
        // Cursor-based page
        state.db.query_media_after_cursor(cursor, cursor_id, limit)?
    } else {
        // First page
        state.db.query_media_first_page(limit)?
    };
    
    // Get total count (can be cached for performance)
    let total = state.db.count_media()?;
    
    let (next_cursor, next_cursor_id) = if has_more && !items.is_empty() {
        let last = items.last().unwrap();
        (Some(last.file_created_at.clone()), Some(last.id.clone()))
    } else {
        (None, None)
    };
    
    Ok(Json(PaginatedResponse {
        data: items,
        meta: PaginationMeta {
            next_cursor,
            next_cursor_id,
            has_more,
            total,
            query: None,
        },
    }))
}
```

**SQL queries module (`backend/src/db/queries.rs`):**
```rust
pub fn query_media_first_page(
    conn: &Connection,
    limit: i64,
) -> Result<(Vec<MediaItem>, bool), Error> {
    let mut stmt = conn.prepare(
        "SELECT id, filename, relative_path, mime_type, width, height, file_size,
                file_created_at, file_modified_at
         FROM media_items
         ORDER BY file_created_at DESC, id
         LIMIT ?1"
    )?;
    // ... map rows to MediaItem vec
    // has_more = items.len() > limit
}

pub fn query_media_after_cursor(
    conn: &Connection,
    cursor: &str,
    cursor_id: &str,
    limit: i64,
) -> Result<(Vec<MediaItem>, bool), Error> {
    let mut stmt = conn.prepare(
        "SELECT ... FROM media_items
         WHERE (file_created_at, id) < (?1, ?2)
         ORDER BY file_created_at DESC, id
         LIMIT ?3"
    )?;
    // ...
}
```

**Tie-breaking** — When multiple items have the same `file_created_at` (e.g., batch-generated images), the `id` (UUID) serves as a deterministic tiebreaker. The composite index `(file_created_at DESC, id)` ensures this is efficient.

**Total count** — `SELECT COUNT(*) FROM media_items` is fast on SQLite. For very large datasets, you can cache this value and invalidate on index events.

## Test Strategy

```rust
#[tokio::test]
async fn test_media_list_first_page() {
    let app = setup_test_app_with_n_items(250).await; // Index 250 items
    
    let response = app.get("/api/v1/media?limit=100").send().await;
    assert_eq!(response.status(), 200);
    
    let body: PaginatedResponse = response.json().await;
    assert_eq!(body.data.len(), 100);
    assert!(body.meta.has_more);
    assert_eq!(body.meta.total, 250);
    assert!(body.meta.next_cursor.is_some());
    assert!(body.meta.next_cursor_id.is_some());
}

#[tokio::test]
async fn test_media_list_cursor_pagination() {
    let app = setup_test_app_with_n_items(250).await;
    
    // Page 1
    let page1: PaginatedResponse = app.get("/api/v1/media?limit=100")
        .send().await.json().await;
    
    // Page 2
    let page2: PaginatedResponse = app
        .get(&format!(
            "/api/v1/media?limit=100&cursor={}&cursor_id={}",
            page1.meta.next_cursor.unwrap(),
            page1.meta.next_cursor_id.unwrap()
        ))
        .send().await.json().await;
    
    assert_eq!(page2.data.len(), 100);
    
    // Verify no overlap
    let page1_ids: HashSet<_> = page1.data.iter().map(|i| &i.id).collect();
    let page2_ids: HashSet<_> = page2.data.iter().map(|i| &i.id).collect();
    assert!(page1_ids.is_disjoint(&page2_ids));
}

#[tokio::test]
async fn test_media_list_last_page() {
    let app = setup_test_app_with_n_items(50).await;
    let response = app.get("/api/v1/media?limit=100").send().await;
    let body: PaginatedResponse = response.json().await;
    assert!(!body.meta.has_more);
    assert!(body.meta.next_cursor.is_none());
}

#[tokio::test]
async fn test_media_list_empty_database() {
    let app = setup_test_app_with_n_items(0).await;
    let response = app.get("/api/v1/media").send().await;
    let body: PaginatedResponse = response.json().await;
    assert!(body.data.is_empty());
    assert_eq!(body.meta.total, 0);
}
```
