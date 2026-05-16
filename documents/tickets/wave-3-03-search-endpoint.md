# Wave 3.3 — Implement Full-Text Search Endpoint

| Field | Value |
|-------|-------|
| **Wave** | 3 — Backend: Search, Cursor Pagination & Real-time SSE |
| **Seq** | 03 |
| **Estimate** | 2 hours |
| **Depends on** | 3.2 (Tantivy populated) |
| **Parallel** | No |

---

## Overview

Add the `GET /search` endpoint that performs full-text search against the Tantivy index. Search queries are free-text (tokenized, case-insensitive) against the `metadata_json` and `filename` fields. Results are paginated using cursor-based pagination.

## Prerequisites

- Tantivy index populated (3.2)
- Axum routes structure (0.4)
- Media list endpoint or media retrieval from SQLite (to enrich search results with full MediaItem data)

## Reference Files

- `documents/plans/development-plan.md` — §3.2 Endpoints (GET /search), §3.3 Search Response, §3.4 Query Parameters, §10.Q5 (free-text search for v1)
- `.opencode/context/development/principles/api-design.md` — cursor pagination

## Deliverables

```
backend/src/routes/
└── search.rs                    # GET /search handler
```

## Acceptance Criteria (Pass/Fail)

- [ ] `GET /api/v1/search?q=sunset` returns matching media items
- [ ] Search is case-insensitive and tokenized ("beautiful sunset" matches "Beautiful Sunset.png")
- [ ] Search against `metadata_json` (full-text) finds text within ComfyUI prompt/workflow JSON
- [ ] Search against `filename` finds files by name
- [ ] Results are paginated with cursor (same cursor format as /media list — see 3.4)
- [ ] Empty query (`?q=`) returns all items (or standard media list — decide: default to empty = all)
- [ ] Missing query parameter (`/search`) returns 400 Bad Request
- [ ] Response format matches §3.3 Search Response:
  ```json
  { "data": [MediaItem[]], "meta": { "next_cursor": "...", "next_cursor_id": "...", "has_more": true, "total": 42, "query": "sunset" } }
  ```

## Implementation Notes

**Search handler:**
```rust
use axum::{
    extract::{Query, State},
    Json,
};
use tantivy::query::QueryParser;
use tantivy::collector::TopDocs;
use serde::Deserialize;

#[derive(Deserialize)]
struct SearchParams {
    q: String,
    #[serde(default = "default_limit")]
    limit: u32,
    cursor: Option<String>,
    cursor_id: Option<String>,
}

fn default_limit() -> u32 { 100 }

async fn search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, AppError> {
    let query_str = params.q.trim();
    if query_str.is_empty() {
        return Err(AppError::BadRequest("Query parameter 'q' is required".into()));
    }
    
    let limit = params.limit.min(500);
    
    let schema = state.search_index.schema();
    let reader = state.search_index.reader();
    let searcher = reader.searcher();
    
    // Build query parser for full-text fields
    let query_parser = QueryParser::for_index(
        &state.search_index.index(),
        vec![
            schema.get_field("metadata_json").unwrap(),
            schema.get_field("filename").unwrap(),
        ],
    );
    
    let query = query_parser.parse_query(query_str)
        .map_err(|e| AppError::BadRequest(format!("Invalid query: {}", e)))?;
    
    // Collect all matching doc IDs (or use TopDocs with pagination)
    let top_docs = searcher.search(&query, &TopDocs::with_limit(limit as usize + 1))
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    // Resolve Tantivy doc IDs → media item IDs → SQLite full records
    let mut media_items = Vec::new();
    let has_more = top_docs.len() > limit as usize;
    let docs = &top_docs[..top_docs.len().min(limit as usize)];
    
    for (_score, doc_address) in docs {
        let doc = searcher.doc(*doc_address)?;
        let id_field = schema.get_field("id").unwrap();
        let item_id = doc.get_first(id_field)
            .and_then(|v| v.as_str())
            .unwrap_or("");
        
        if let Some(item) = state.db.get_media_by_id(item_id)? {
            media_items.push(item);
        }
    }
    
    // Cursor: use last item's created_at + id
    let (next_cursor, next_cursor_id) = if has_more {
        let last = media_items.last().unwrap();
        (Some(last.file_created_at.clone()), Some(last.id.clone()))
    } else {
        (None, None)
    };
    
    Ok(Json(SearchResponse {
        data: media_items,
        meta: PaginationMeta {
            next_cursor,
            next_cursor_id,
            has_more,
            total: top_docs.len() as u64, // Note: this is doc count, not total hits
            query: Some(query_str.to_string()),
        },
    }))
}
```

**Query parsing** — Tantivy's `QueryParser` handles:
- Tokenization (splitting on whitespace)
- Lowercasing
- Stemming (if configured)
- Default operator: OR (documents matching ANY term are returned)
- Phrases: `"exact phrase"` for exact match

## Test Strategy

Integration test in `backend/tests/search_test.rs` (Task 3.9):
```rust
#[tokio::test]
async fn test_search_by_metadata() {
    let app = setup_test_app_with_indexed_search().await;
    
    let response = app
        .get("/api/v1/search?q=sunset")
        .send().await;
    
    assert_eq!(response.status(), 200);
    let body: SearchResponse = response.json().await;
    assert!(body.data.len() > 0);
    assert_eq!(body.meta.query.as_deref(), Some("sunset"));
}

#[tokio::test]
async fn test_search_empty_query_returns_400() {
    let app = setup_test_app().await;
    let response = app.get("/api/v1/search?q=").send().await;
    assert_eq!(response.status(), 400);
}
```
