# Wave 3.9 — Write Integration Tests for Search + SSE

| Field | Value |
|-------|-------|
| **Wave** | 3 — Backend: Search, Cursor Pagination & Real-time SSE |
| **Seq** | 09 |
| **Estimate** | 2 hours |
| **Depends on** | 3.3 (search endpoint), 3.7 (SSE endpoint) |
| **Parallel** | No (verifies entire Wave 3 pipeline) |

---

## Overview

Write comprehensive integration tests for the search endpoint and SSE real-time events. Verify that search returns expected results, pagination works correctly, and SSE streams file events to connected clients.

## Prerequisites

- Search endpoint (3.3)
- SSE endpoint (3.7)
- Media items indexed in SQLite and Tantivy
- Test helpers from previous waves

## Reference Files

- `documents/plans/development-plan.md` — §7.2 Backend Testing, §7.5 Test Data Strategy
- `.opencode/context/core/standards/test-coverage.md`
- `backend/tests/indexer_test.rs` — patterns from Wave 1.10
- `backend/tests/media_test.rs` — patterns from Wave 2.8

## Deliverables

```
backend/tests/
├── search_test.rs               # Search integration tests
└── events_test.rs               # SSE integration tests
```

## Acceptance Criteria (Pass/Fail)

**Search tests:**
- [ ] Test: `search_by_metadata_keyword` — indexing a file with `"sunset"` in metadata → searching "sunset" returns it
- [ ] Test: `search_by_filename` — searching by filename returns matching files
- [ ] Test: `search_case_insensitive` — "SUNSET" matches "sunset"
- [ ] Test: `search_nonexistent_term` — returns empty results, not error
- [ ] Test: `search_pagination` — search with 100+ results → cursor pagination works
- [ ] Test: `search_empty_query_400` — missing `q` parameter returns 400
- [ ] Test: `search_response_format` — response matches §3.3 schema

**SSE tests:**
- [ ] Test: `sse_connection_receives_connected_event` — initial connection gets `connected` event
- [ ] Test: `sse_receives_file_created` — creating a file broadcasts `file_created`
- [ ] Test: `sse_receives_file_deleted` — deleting a file broadcasts `file_deleted`
- [ ] Test: `sse_multiple_clients_receive_events` — two clients both receive events
- [ ] Test: `sse_keep_alive` — connection stays open with keep-alive comments

## Implementation Notes

**Search test setup — need items indexed in Tantivy:**
```rust
async fn setup_search_test_app() -> Router {
    let dir = tempfile::tempdir().unwrap();
    let db = create_test_db();
    let tantivy_dir = dir.path().join("tantivy");
    let index_manager = IndexManager::open(&tantivy_dir).unwrap();
    
    // Insert media items with known metadata
    db.execute(
        "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at, indexed_at, metadata_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'), ?8)",
        params![
            "id-1", "sunset.png", "2025/sunset.png", "image/png", 1024,
            "2025-01-01T00:00:00Z", "2025-01-01T00:00:00Z",
            r#"{"prompt": "a beautiful sunset over mountains"}"#,
        ],
    ).unwrap();
    db.execute("INSERT ... (another item with 'portrait' metadata)").unwrap();
    
    // Populate Tantivy from SQLite
    full_reindex(&db, &index_manager).unwrap();
    
    build_test_router(db, index_manager)
}
```

**Search test examples:**
```rust
#[tokio::test]
async fn test_search_by_metadata_keyword() {
    let app = setup_search_test_app().await;
    
    let response: SearchResponse = app
        .get("/api/v1/search?q=sunset")
        .send().await
        .json().await;
    
    assert_eq!(response.meta.query.as_deref(), Some("sunset"));
    assert!(!response.data.is_empty());
    assert!(response.data.iter().any(|item| item.filename == "sunset.png"));
}

#[tokio::test]
async fn test_search_nonexistent_term() {
    let app = setup_search_test_app().await;
    
    let response: SearchResponse = app
        .get("/api/v1/search?q=zzzzzzzzz")
        .send().await
        .json().await;
    
    assert!(response.data.is_empty());
    assert!(!response.meta.has_more);
}
```

**SSE test examples:**
```rust
#[tokio::test]
async fn test_sse_receives_file_created_event() {
    let app = setup_test_app_with_sse().await;
    
    // Connect SSE client (using reqwest streaming)
    let response = app.get("/api/v1/events").send().await;
    assert_eq!(response.status(), 200);
    
    let mut stream = response.bytes_stream();
    
    // Read initial "connected" event
    let first = read_sse_event(&mut stream).await;
    assert_eq!(first.event_type, "connected");
    
    // Trigger a file event via broadcast
    app.state().sse_tx.send(SseEvent {
        event_type: "file_created".into(),
        data: json!({"id": "test-id", "filename": "test.png"}),
    }).unwrap();
    
    // Read the file_created event
    let second = read_sse_event(&mut stream).await;
    assert_eq!(second.event_type, "file_created");
}

// Helper to parse SSE stream
async fn read_sse_event(stream: &mut impl Stream<Item = Result<Bytes, reqwest::Error>>) -> SseEventParsed {
    // Accumulate lines until empty line
    let mut event_type = String::new();
    let mut data = String::new();
    
    while let Some(chunk) = stream.next().await {
        let text = String::from_utf8_lossy(&chunk.unwrap());
        for line in text.lines() {
            if line.is_empty() { break; }
            if let Some(field) = line.strip_prefix("event: ") {
                event_type = field.to_string();
            }
            if let Some(field) = line.strip_prefix("data: ") {
                data = field.to_string();
            }
        }
        if !event_type.is_empty() { break; }
    }
    
    SseEventParsed { event_type, data }
}
```

**Note:** SSE testing with reqwest streams requires careful handling since the response body is never "complete" (persistent connection). Tests should use timeouts to avoid hanging.

## Test Strategy

- All tests pass with `cargo test --test search_test` and `cargo test --test events_test`
- Tests are independent and can run in any order
- Use temp directories for Tantivy index
- SSE tests may need longer timeouts due to stream nature
