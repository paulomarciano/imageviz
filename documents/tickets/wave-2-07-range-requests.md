# Wave 2.7 — Add Range Request Support for Video Seeking

| Field | Value |
|-------|-------|
| **Wave** | 2 — Backend: Thumbnail Generation & Media Serving |
| **Seq** | 07 |
| **Estimate** | 1.5 hours |
| **Depends on** | 2.5 (file serving) |
| **Parallel** | No |

---

## Overview

Add HTTP Range request support (`Accept-Ranges`, `Content-Range`, `206 Partial Content`) to the file serving endpoint. This enables video seeking in the browser — the `<video>` element sends Range requests to fetch specific portions of the file, allowing users to skip to any timestamp without downloading the entire video.

## Prerequisites

- File serving endpoint (2.5)
- Understanding of HTTP Range headers (RFC 7233)

## Reference Files

- `documents/plans/development-plan.md` — §3.2 Endpoints (Range header supported), §8 Risk Register (large video files → stream via Range)
- `.opencode/context/development/principles/api-design.md`

## Deliverables

```
backend/src/routes/
└── media.rs                     # Updated: add Range request handling
```

## Acceptance Criteria (Pass/Fail)

- [ ] `GET /api/v1/media/{id}/file` with `Range: bytes=0-1048575` header returns 206 Partial Content
- [ ] Response includes `Content-Range: bytes 0-1048575/{total_size}` header
- [ ] Response includes `Accept-Ranges: bytes` header on all responses
- [ ] Response body contains only the requested byte range
- [ ] Handles single range (`bytes=0-1023`)
- [ ] Handles open-ended range (`bytes=1024-`)
- [ ] Handles suffix range (`bytes=-2048` — last 2048 bytes)
- [ ] Returns HTTP 200 (full file) when no Range header present
- [ ] Returns 416 Range Not Satisfiable for invalid ranges
- [ ] Works with both video and image files

## Implementation Notes

**Range request handling:**
```rust
use axum::{
    body::Body,
    http::{StatusCode, header, HeaderMap},
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

async fn get_file_range(
    abs_path: &Path,
    mime_type: &str,
    filename: &str,
    file_size: u64,
    range_header: Option<&str>,
) -> Result<impl IntoResponse, AppError> {
    let mut file = tokio::fs::File::open(&abs_path).await?;
    
    let Some(range_str) = range_header else {
        // No range — serve full file (existing behavior)
        return serve_full_file(file, mime_type, filename, file_size).await;
    };
    
    // Parse range
    let range = parse_range(range_str, file_size)?;
    
    // Seek to start position
    file.seek(std::io::SeekFrom::Start(range.start)).await?;
    
    // Read exactly the range
    let length = range.end - range.start + 1;
    let mut buffer = vec![0u8; length as usize];
    file.read_exact(&mut buffer).await?;
    
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, mime_type.parse().unwrap());
    headers.insert(
        header::CONTENT_RANGE,
        format!("bytes {}-{}/{}", range.start, range.end, file_size).parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        length.to_string().parse().unwrap(),
    );
    headers.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
    
    Ok((StatusCode::PARTIAL_CONTENT, headers, Body::from(buffer)))
}

#[derive(Debug)]
struct ByteRange {
    start: u64,
    end: u64,
}

fn parse_range(range_str: &str, file_size: u64) -> Result<ByteRange, AppError> {
    // Handle: "bytes=0-1023", "bytes=1024-", "bytes=-2048"
    if !range_str.starts_with("bytes=") {
        return Err(AppError::BadRequest("Invalid Range header".into()));
    }
    
    let range_value = &range_str[6..];
    
    if let Some(suffix) = range_value.strip_prefix('-') {
        // Suffix: bytes=-2048
        let suffix_len: u64 = suffix.parse().map_err(|_| AppError::range_not_satisfiable())?;
        let start = if suffix_len > file_size { 0 } else { file_size - suffix_len };
        let end = file_size - 1;
        Ok(ByteRange { start, end })
    } else if let Some((start_str, end_str)) = range_value.split_once('-') {
        let start: u64 = start_str.parse().map_err(|_| AppError::range_not_satisfiable())?;
        
        let end: u64 = if end_str.is_empty() {
            // Open-ended: bytes=1024-
            file_size - 1
        } else {
            end_str.parse().map_err(|_| AppError::range_not_satisfiable())?
        };
        
        if start > end || start >= file_size {
            return Err(AppError::range_not_satisfiable());
        }
        
        Ok(ByteRange { start, end: end.min(file_size - 1) })
    } else {
        Err(AppError::BadRequest("Invalid Range format".into()))
    }
}
```

**416 Range Not Satisfiable** — Return this when:
- Start > file_size
- Start > end
- Malformed range value

```rust
fn range_not_satisfiable() -> (StatusCode, HeaderMap) {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_RANGE,
        format!("bytes */{}", file_size).parse().unwrap(),
    );
    (StatusCode::RANGE_NOT_SATISFIABLE, headers)
}
```

## Test Strategy

```rust
#[tokio::test]
async fn test_range_request_first_megabyte() {
    let app = test_app_with_indexed_video().await;
    
    let response = app.get(&format!("/api/v1/media/{}/file", VIDEO_ID))
        .header("Range", "bytes=0-1048575")
        .send().await;
    
    assert_eq!(response.status(), 206);
    assert!(response.headers().get("content-range").is_some());
    assert_eq!(
        response.headers().get("content-length").unwrap().to_str().unwrap(),
        "1048576"
    );
}

#[tokio::test]
async fn test_range_request_suffix() {
    let response = app.get(...)
        .header("Range", "bytes=-1024")
        .send().await;
    assert_eq!(response.status(), 206);
    // Response body is exactly 1024 bytes
}

#[tokio::test]
async fn test_range_not_satisfiable() {
    let response = app.get(...)
        .header("Range", "bytes=999999999-")
        .send().await;
    assert_eq!(response.status(), 416);
}

#[tokio::test]
async fn test_accept_ranges_header_present() {
    let response = app.get(...).send().await; // No Range header
    assert_eq!(response.headers().get("accept-ranges").unwrap(), "bytes");
}
```
