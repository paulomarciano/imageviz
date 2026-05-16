# Wave 2.6 — Add Caching Headers (ETag, Cache-Control, Last-Modified)

| Field | Value |
|-------|-------|
| **Wave** | 2 — Backend: Thumbnail Generation & Media Serving |
| **Seq** | 06 |
| **Estimate** | 45 minutes |
| **Depends on** | 2.4 (thumbnail serving), 2.5 (file serving) |
| **Parallel** | No |

---

## Overview

Add HTTP caching headers (ETag, Cache-Control, Last-Modified) to media serving endpoints. This allows the browser to cache thumbnails and files, returning 304 Not Modified when content hasn't changed, reducing bandwidth and improving perceived performance.

## Prerequisites

- Thumbnail serving (2.4)
- File serving (2.5)
- Media items have `checksum` and `file_modified_at` fields (from Wave 1)

## Reference Files

- `documents/plans/development-plan.md` — §3.2 Endpoints, §8.2 Key Performance Decisions (caching)
- `.opencode/context/development/principles/api-design.md` — caching patterns

## Deliverables

```
backend/src/routes/
└── media.rs                     # Updated: add caching header logic
```

## Acceptance Criteria (Pass/Fail)

**Thumbnail endpoint:**
- [ ] Response includes `ETag` header (file checksum + width)
- [ ] Response includes `Cache-Control: public, max-age=31536000, immutable` (thumbnails are content-addressed, so they never change)
- [ ] If request includes `If-None-Match` matching ETag → returns 304 Not Modified (no body)

**File endpoint:**
- [ ] Response includes `ETag` header (file checksum)
- [ ] Response includes `Last-Modified` header (from `file_modified_at`)
- [ ] Response includes `Cache-Control: private, max-age=3600` (files can change)
- [ ] If request includes `If-None-Match` matching ETag → returns 304

**Both:**
- [ ] CORS headers preserved on 304 responses

## Implementation Notes

**Thumbnail caching — aggressive (immutable):**
Since thumbnails are content-addressed (checksum determines filename), they are immutable. A new checksum means a new file path, so the URL itself changes. This justifies `immutable` in Cache-Control.

```rust
// In get_thumbnail handler:
let etag = format!("\"{}\"", item.checksum);

// Check If-None-Match
if let Some(if_none_match) = req.headers().get(header::IF_NONE_MATCH) {
    if if_none_match.to_str().unwrap_or("") == etag {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }
}

let headers = [
    (header::ETAG, etag),
    (header::CACHE_CONTROL, "public, max-age=31536000, immutable".to_string()),
    (header::CONTENT_TYPE, "image/webp".to_string()),
];
```

**File serving — conservative (mutable):**
Files can be modified, so caching is time-based with ETag validation.

```rust
// In get_file handler:
let etag = format!("\"{}\"", item.checksum);
let last_modified = item.file_modified_at.clone(); // ISO 8601

// Check If-None-Match
if let Some(if_none_match) = req.headers().get(header::IF_NONE_MATCH) {
    if if_none_match.to_str().unwrap_or("") == etag {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }
}

let headers = [
    (header::ETAG, etag),
    (header::CACHE_CONTROL, "private, max-age=3600".to_string()),
    (header::LAST_MODIFIED, last_modified),
    (header::CONTENT_TYPE, item.mime_type),
];
```

**304 responses** — Must also include CORS headers (if the original response would have them). Axum's `tower-http` CORS layer handles this automatically for most cases.

## Test Strategy

```rust
#[tokio::test]
async fn test_thumbnail_etag_304() {
    let app = test_app_with_indexed_media().await;
    
    // First request — get ETag
    let resp1 = app.get(&format!("/api/v1/media/{}/thumbnail", MEDIA_ID)).send().await;
    let etag = resp1.headers().get("etag").unwrap().to_str().unwrap().to_string();
    assert_eq!(resp1.status(), 200);
    
    // Second request with If-None-Match
    let resp2 = app.get(&format!("/api/v1/media/{}/thumbnail", MEDIA_ID))
        .header("If-None-Match", &etag)
        .send().await;
    assert_eq!(resp2.status(), 304);
}

#[tokio::test]
async fn test_thumbnail_cache_control_header() {
    let app = test_app_with_indexed_media().await;
    let resp = app.get(&format!("/api/v1/media/{}/thumbnail", MEDIA_ID)).send().await;
    let cache_control = resp.headers().get("cache-control").unwrap().to_str().unwrap();
    assert!(cache_control.contains("immutable"));
}
```
