# Wave 2.4 — Implement Thumbnail Serving Endpoint

| Field | Value |
|-------|-------|
| **Wave** | 2 — Backend: Thumbnail Generation & Media Serving |
| **Seq** | 04 |
| **Estimate** | 1 hour |
| **Depends on** | 2.3 (thumbnail cache) |
| **Parallel** | No |

---

## Overview

Add the `GET /media/:id/thumbnail` endpoint that serves WebP thumbnail images. The endpoint looks up the media item by ID, generates or retrieves the cached thumbnail, and streams it to the client with appropriate Content-Type and caching headers.

## Prerequisites

- Thumbnail cache (2.3)
- SQLite database with media_items table (1.1)
- Axum routes structure (from 0.4)

## Reference Files

- `documents/plans/development-plan.md` — §3.2 Endpoints (GET /media/:id/thumbnail), §8.2 streaming file serving, §3.1 Base URL
- `.opencode/context/development/principles/api-design.md` — response headers, error codes

## Deliverables

```
backend/src/routes/
├── mod.rs                       # Updated: mount media routes
└── media.rs                     # GET /media/:id/thumbnail handler (and future handlers)
```

## Acceptance Criteria (Pass/Fail)

- [ ] `GET /api/v1/media/{id}/thumbnail` returns WebP image data with `Content-Type: image/webp`
- [ ] Returns HTTP 200 with thumbnail bytes for valid media ID
- [ ] Returns HTTP 404 if media item not found
- [ ] Returns HTTP 404 if thumbnail doesn't exist and can't be generated
- [ ] Accepts optional `?width=` query parameter (100-500px, default 200px)
- [ ] Thumbnail is generated on first request, cached for subsequent requests
- [ ] Response includes proper Content-Length header

## Implementation Notes

**Media routes structure:**
```rust
use axum::{
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
    Router,
};
use std::sync::Arc;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/media", get(list_media))           // Future: Wave 3.4
        .route("/media/{id}", get(get_media))        // Future: Wave 3.x
        .route("/media/{id}/thumbnail", get(get_thumbnail))
        .route("/media/{id}/file", get(get_file))    // Future: Wave 2.5
}

#[derive(Deserialize)]
struct ThumbnailParams {
    #[serde(default = "default_width")]
    width: u32,
}

fn default_width() -> u32 { 200 }

async fn get_thumbnail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<ThumbnailParams>,
) -> Result<impl IntoResponse, AppError> {
    // Validate width range
    if params.width < 100 || params.width > 500 {
        return Err(AppError::BadRequest("Width must be between 100 and 500".into()));
    }
    
    // Look up media item
    let item = state.db.get_media_by_id(&id)?
        .ok_or(AppError::NotFound("Media item not found".into()))?;
    
    // Get or generate thumbnail
    let thumb_path = state.thumbnail_cache
        .get_or_generate(
            &item.absolute_path(),
            &item.checksum,
            params.width,
        )
        .await?;
    
    // Read and serve the file
    let bytes = tokio::fs::read(&thumb_path).await?;
    
    let response = (StatusCode::OK, [
        (header::CONTENT_TYPE, "image/webp"),
        (header::CONTENT_LENGTH, &bytes.len().to_string()),
    ], bytes);
    
    Ok(response)
}
```

**Absolute path resolution:**
Media items store `relative_path` (relative to watched folder root). To resolve the absolute path, you need to know which watched folder this item belongs to. Either:
1. Store the watched folder root alongside the relative_path, or
2. Search through configured watched folders to find the file

Recommendation: Add a `folder_root` column or compute absolute path by joining `relative_path` with each watched folder until the file is found.

## Test Strategy

Co-locate tests with the routes or add to `backend/tests/media_test.rs` (Task 2.8):
```rust
#[tokio::test]
async fn test_get_thumbnail_valid_id() {
    let app = test_app_with_indexed_media().await;
    
    let response = app.get(&format!("/api/v1/media/{}/thumbnail", MEDIA_ID))
        .send().await;
    
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers().get("content-type").unwrap(), "image/webp");
}

#[tokio::test]
async fn test_get_thumbnail_nonexistent_id() {
    let app = test_app().await;
    
    let response = app.get("/api/v1/media/nonexistent/thumbnail")
        .send().await;
    
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_get_thumbnail_invalid_width() {
    let app = test_app_with_indexed_media().await;
    
    let response = app.get(&format!("/api/v1/media/{}/thumbnail?width=50", MEDIA_ID))
        .send().await;
    
    assert_eq!(response.status(), 400); // Width < 100
}
```
