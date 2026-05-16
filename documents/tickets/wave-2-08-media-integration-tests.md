# Wave 2.8 — Write Integration Tests for Media Endpoints

| Field | Value |
|-------|-------|
| **Wave** | 2 — Backend: Thumbnail Generation & Media Serving |
| **Seq** | 08 |
| **Estimate** | 1.5 hours |
| **Depends on** | 2.4–2.7 (all media endpoints) |
| **Parallel** | No (verifies entire Wave 2 pipeline) |

---

## Overview

Write comprehensive integration tests for all media endpoints using `reqwest` (or Axum test utilities) against a real test server with indexed media files. Tests cover thumbnail generation, caching headers, file streaming, Range requests, and error cases.

## Prerequisites

- All Wave 2 endpoints implemented (2.4, 2.5, 2.6, 2.7)
- Media items indexed in test DB (from Wave 1 test helpers)
- `reqwest` in Cargo.toml dev-dependencies

## Reference Files

- `documents/plans/development-plan.md` — §7.2 Backend Testing (integration tests with real SQLite + temp files)
- `.opencode/context/core/standards/test-coverage.md`
- `backend/tests/indexer_test.rs` — test helper patterns from Wave 1.10

## Deliverables

```
backend/tests/
├── common/
│   └── mod.rs                   # Updated: add AppState test helpers
└── media_test.rs                # Integration tests for Wave 2
```

## Acceptance Criteria (Pass/Fail)

- [ ] Test: `thumbnail_returns_webp_image` — GET thumbnail, verify Content-Type, verify response body is valid WebP
- [ ] Test: `thumbnail_returns_404_for_invalid_id`
- [ ] Test: `thumbnail_caching_304` — first request → 200, second with ETag → 304
- [ ] Test: `thumbnail_custom_width` — `?width=400` returns different thumbnail than `?width=200`
- [ ] Test: `file_streams_with_correct_content_type` — GET file, verify Content-Type matches
- [ ] Test: `file_returns_404_for_invalid_id`
- [ ] Test: `file_range_request_206` — Range: bytes=0-1023 → 206 Partial Content
- [ ] Test: `file_range_not_satisfiable_416` — invalid range → 416
- [ ] Test: `file_accept_ranges_header` — response includes Accept-Ranges: bytes
- [ ] Test: `file_content_disposition_inline` — Content-Disposition: inline
- [ ] Test: `pagination_returns_cursor` — (preview of Wave 3.4, optional)
- [ ] All tests pass with `cargo test --test media_test`

## Implementation Notes

**Test app setup helper:**
```rust
// tests/common/mod.rs
pub async fn setup_test_app() -> (Router, TempDir, Connection) {
    let dir = TempDir::new().unwrap();
    let db = create_test_db();
    let cache_dir = dir.path().join("thumbnails");
    
    // Create test file
    let test_file = dir.path().join("test.png");
    create_test_image(&test_file, 400, 300);
    
    // Index it
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: Some("test".into()),
        }],
    };
    save_config(&db, &config).unwrap();
    let (tracker, _rx) = ProgressTracker::new();
    full_index(&db, &config, &tracker).await.unwrap();
    
    let state = Arc::new(AppState {
        db,
        config,
        thumbnail_cache: ThumbnailCache::new(cache_dir),
        // ... other state
    });
    
    let app = build_router(state);
    (app, dir, db) // Return dir to keep temp dir alive
}

// Helper to create a test PNG
fn create_test_image(path: &Path, width: u32, height: u32) {
    let img = image::DynamicImage::new_rgba8(width, height);
    img.save(path).unwrap();
}
```

**Integration test example:**
```rust
#[tokio::test]
async fn test_thumbnail_returns_webp() {
    let (app, _dir, _db) = setup_test_app().await;
    
    // Find an indexed item's ID
    let items: Vec<MediaItem> = app.get("/api/v1/media?limit=1").send().await.json().await;
    let id = &items[0].id;
    
    let response = app
        .get(&format!("/api/v1/media/{}/thumbnail", id))
        .send()
        .await;
    
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "image/webp"
    );
}

#[tokio::test]
async fn test_file_range_request() {
    let (app, _dir, _db) = setup_test_app().await;
    let id = get_first_media_id(&app).await;
    
    let response = app
        .get(&format!("/api/v1/media/{}/file", id))
        .header("Range", "bytes=0-99")
        .send()
        .await;
    
    assert_eq!(response.status(), 206);
    let body = response.bytes().await;
    assert_eq!(body.len(), 100);
}
```

## Test Strategy

- Each test is independent (creates its own temp dir + DB, or uses a shared setup)
- Avoid tests depending on test order (no shared mutable state between tests)
- Use `#[ignore]` for tests requiring ffmpeg
- Verify both success paths and error paths
- `cargo test --test media_test` must pass
