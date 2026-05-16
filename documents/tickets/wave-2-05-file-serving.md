# Wave 2.5 — Implement Original File Serving Endpoint

| Field | Value |
|-------|-------|
| **Wave** | 2 — Backend: Thumbnail Generation & Media Serving |
| **Seq** | 05 |
| **Estimate** | 1 hour |
| **Depends on** | None (independent endpoint, but needs DB from 1.1) |
| **Parallel** | No |

---

## Overview

Add the `GET /media/:id/file` endpoint that streams the original media file to the client. The file is streamed using `tokio::fs::File` + `ReaderStream` — never fully loaded into memory. Used by the detail viewer for full-resolution previews and by the drag-and-drop system for OS-level file transfer.

## Prerequisites

- SQLite with media_items (1.1) — to look up file paths
- Axum routes structure (0.4)
- `tokio-util` in Cargo.toml (for `ReaderStream`)
- `tokio-stream` in Cargo.toml (already in §13)

## Reference Files

- `documents/plans/development-plan.md` — §3.2 Endpoints (GET /media/:id/file), §8.2 streaming file serving, §8.3 Memory Management
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/routes/
└── media.rs                     # Updated: add GET /media/:id/file handler
```

## Acceptance Criteria (Pass/Fail)

- [ ] `GET /api/v1/media/{id}/file` streams the original file with correct Content-Type
- [ ] Returns HTTP 200 with file bytes for valid media ID
- [ ] Returns HTTP 404 if media item not found
- [ ] Returns HTTP 404 if original file doesn't exist on disk
- [ ] Content-Type header matches the file's mime_type from DB (`image/png`, `video/mp4`, etc.)
- [ ] Content-Disposition: `inline; filename="original.ext"` (so browser displays, not downloads)
- [ ] Content-Length header present (file size from DB)
- [ ] File is **streamed** using `ReaderStream` — not loaded into memory
- [ ] Handles large files (>1GB) without memory issues

## Implementation Notes

**Streaming file serving:**
```rust
use axum::{
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header, HeaderMap},
    response::IntoResponse,
};
use tokio_util::io::ReaderStream;
use std::sync::Arc;

async fn get_file(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Look up media item
    let item = state.db.get_media_by_id(&id)?
        .ok_or(AppError::NotFound("Media item not found".into()))?;
    
    let abs_path = item.absolute_path(state)?;
    
    // Check file exists
    if !abs_path.exists() {
        return Err(AppError::NotFound("Original file not found on disk".into()));
    }
    
    // Open file
    let file = tokio::fs::File::open(&abs_path).await?;
    let file_size = file.metadata().await?.len();
    
    // Determine filename for Content-Disposition
    let filename = item.filename.clone();
    
    // Stream with ReaderStream
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);
    
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        item.mime_type.parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("inline; filename=\"{}\"", filename).parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        file_size.to_string().parse().unwrap(),
    );
    
    Ok((StatusCode::OK, headers, body))
}
```

**Content-Type header** — Use the `mime_type` stored in the database (set during indexing in Wave 1.8).

**Content-Disposition** — Use `inline` (not `attachment`) so the browser displays the file. The drag-and-drop system (Wave 5.7) will download the file separately for OS drag.

**Memory:** `ReaderStream` reads the file in chunks (default 8KB) and never loads the entire file into memory. This is critical for large video files.

## Test Strategy

Tests in `backend/tests/media_test.rs` (Task 2.8):
```rust
#[tokio::test]
async fn test_get_file_valid_id() {
    let app = test_app_with_indexed_media().await;
    
    let response = app.get(&format!("/api/v1/media/{}/file", MEDIA_ID))
        .send().await;
    
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers().get("content-type").unwrap(), "image/png");
    assert!(response.headers().get("content-length").is_some());
}

#[tokio::test]
async fn test_get_file_nonexistent_id() {
    let app = test_app().await;
    let response = app.get("/api/v1/media/fake-id/file").send().await;
    assert_eq!(response.status(), 404);
}
```
