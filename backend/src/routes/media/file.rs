use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _};

use crate::middleware::validation;

use super::MediaState;

// Test-only count of SQL statements executed by the media resolver. Pins the
// wave-8.17 contract: one resolution statement per request.
#[cfg(test)]
thread_local! {
    pub(super) static RESOLVE_STATEMENTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Fully resolved location and caching metadata for a media item, produced by
/// a single SQL statement (media row LEFT JOIN watched folder).
#[derive(Debug)]
pub(super) struct ResolvedMedia {
    pub(super) full_path: PathBuf,
    pub(super) mime_type: String,
    pub(super) filename: String,
    pub(super) checksum: String,
    pub(super) modified_at: String,
}

/// Query half of the resolver: exactly one statement returning the media row
/// joined with its watched folder's base path.
///
/// `COALESCE` keeps the empty-string contract the ETag/Last-Modified header
/// code relies on. A `NULL` folder path (no `folder_id`, dangling `folder_id`,
/// or deleted folder row) yields the same "missing folder" 404 as before the
/// JOIN rewrite. Disk existence is checked separately by [`verify_on_disk`].
pub(super) fn resolve_media_row(
    db: &rusqlite::Connection,
    id: &str,
) -> Result<ResolvedMedia, (StatusCode, Json<Value>)> {
    #[cfg(test)]
    RESOLVE_STATEMENTS.with(|count| count.set(count.get() + 1));

    let (relative_path, mime_type, filename, checksum, modified_at, folder_path): (
        String,
        String,
        String,
        String,
        String,
        Option<String>,
    ) = db
        .query_row(
            "SELECT m.relative_path, m.mime_type, m.filename, \
                    COALESCE(m.checksum, ''), COALESCE(m.file_modified_at, ''), w.path \
             FROM media_items m \
             LEFT JOIN watched_folders w ON w.id = m.folder_id \
             WHERE m.id = ?1",
            rusqlite::params![id],
            |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                (StatusCode::NOT_FOUND, Json(json!({"error": "Media not found"})))
            }
            _ => {
                tracing::error!(error = %e, "Database error fetching media item");
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Internal server error"})))
            }
        })?;

    // LEFT JOIN: a NULL folder path means no folder_id, a dangling folder_id,
    // or a deleted folder row — treat as missing folder (404) exactly as
    // before. Never turn NULL into an empty path (which would resolve to CWD).
    let Some(folder_path) = folder_path else {
        return Err((StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"}))));
    };

    Ok(ResolvedMedia {
        full_path: std::path::Path::new(&folder_path).join(relative_path),
        mime_type,
        filename,
        checksum,
        modified_at,
    })
}

/// Async existence check for a resolved path (`tokio::fs::try_exists`, never
/// blocking the runtime; I/O errors count as "not found" like `Path::exists`).
/// Consumes and returns the resolution so routes can destructure it after the
/// check. No live `&rusqlite::Connection` borrow may cross this await
/// (`Connection` is not `Sync`); handlers drop the pooled connection first so
/// the pool slot is not held across disk I/O.
pub(super) async fn verify_on_disk(
    resolved: ResolvedMedia,
) -> Result<ResolvedMedia, (StatusCode, Json<Value>)> {
    if !tokio::fs::try_exists(&resolved.full_path).await.unwrap_or(false) {
        return Err((StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"}))));
    }
    Ok(resolved)
}

/// Parse a single Range header value.
/// Only handles `bytes=start-end`, `bytes=start-`, and `bytes=-suffix` formats.
fn parse_range_header(range_header: &str, file_size: u64) -> Option<RangeInclusive<u64>> {
    let range_str = range_header.strip_prefix("bytes=")?;

    if let Some(suffix) = range_str.strip_prefix('-') {
        // Suffix range: -2048 → last 2048 bytes
        let suffix_len: u64 = suffix.parse().ok()?;
        if suffix_len == 0 {
            return None;
        }
        let start = file_size.saturating_sub(suffix_len);
        Some(start..=file_size - 1)
    } else if let Some((start_str, end_str)) = range_str.split_once('-') {
        // Open-ended: 1024- → from 1024 to end
        // Full range: 0-1023
        let start: u64 = start_str.parse().ok()?;
        if start >= file_size {
            return None;
        }
        let end: u64 = match end_str.parse() {
            Ok(e) if e < file_size => e,
            _ => file_size - 1,
        };
        if start > end {
            return None;
        }
        Some(start..=end)
    } else {
        None
    }
}

/// GET /api/v1/media/{id}/file — stream the original file with optional Range support.
/// Adds caching headers (ETag, Cache-Control, Last-Modified) and supports
/// conditional requests via If-None-Match (returns 304 Not Modified).
pub(super) async fn serve_file(
    State(state): State<Arc<MediaState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, Json<Value>)> {
    validation::validate_media_id(&id)?;

    // One resolution statement supplies path, MIME type, and caching metadata.
    // The connection is released before the await (see `verify_on_disk`).
    let conn = state.db.get().map_err(|e| {
        tracing::error!(error = %e, "Failed to acquire database connection");
        (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Service temporarily unavailable"})))
    })?;
    let resolved = resolve_media_row(&conn, &id)?;
    drop(conn);
    let ResolvedMedia { full_path: file_path, mime_type, filename, checksum, modified_at } =
        verify_on_disk(resolved).await?;

    // Get file metadata for size
    let metadata = tokio::fs::metadata(&file_path).await.map_err(|e| {
        tracing::error!(error = %e, path = %file_path.display(), "Failed to stat file");
        (StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"})))
    })?;
    let file_size = metadata.len();

    // Check If-None-Match (only for full-file requests, not range)
    if !checksum.is_empty()
        && let Some(val) = headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok())
    {
        let expected = format!("\"{}\"", checksum);
        if val == expected {
            let mut res = Response::new(axum::body::Body::empty());
            *res.status_mut() = StatusCode::NOT_MODIFIED;
            res.headers_mut()
                .insert(header::ETAG, HeaderValue::from_bytes(expected.as_bytes()).unwrap());
            res.headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=3600"));
            return Ok(res);
        }
    }

    // Check for Range header
    if let Some(range_header) = headers.get(header::RANGE)
        && let Ok(range_str) = range_header.to_str()
        && range_str.starts_with("bytes=")
    {
        if let Some(range) = parse_range_header(range_str, file_size) {
            return serve_file_range(
                &file_path, range, file_size, &mime_type, &filename, &checksum,
            )
            .await
            .map(IntoResponse::into_response);
        } else {
            return Err((
                StatusCode::RANGE_NOT_SATISFIABLE,
                Json(json!({
                    "error": "Range not satisfiable",
                    "content_range": format!("bytes */{}", file_size)
                })),
            ));
        }
    }

    // No Range header → serve full file with caching headers
    serve_full_file(&file_path, file_size, &mime_type, &filename, &checksum, &modified_at)
        .await
        .map(IntoResponse::into_response)
}

/// Serve the full file (200 OK) with caching headers.
///
/// Includes `Content-Length`, `ETag` (checksum), `Cache-Control: private,
/// max-age=3600`, and `Last-Modified` (from `file_modified_at`) headers.
/// Conditional requests (304) are handled upstream in `serve_file`.
async fn serve_full_file(
    path: &std::path::Path,
    file_size: u64,
    mime_type: &str,
    filename: &str,
    checksum: &str,
    modified_at: &str,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let file = tokio::fs::File::open(path).await.map_err(|e| {
        tracing::error!(error = %e, path = %path.display(), "Failed to open file");
        (StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"})))
    })?;

    let stream = tokio_util::io::ReaderStream::new(file);
    let body = axum::body::Body::from_stream(stream);

    let mut res = Response::new(body);
    res.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_bytes(mime_type.as_bytes()).unwrap());
    res.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_bytes(format!("inline; filename=\"{}\"", filename).as_bytes()).unwrap(),
    );
    res.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_bytes(file_size.to_string().as_bytes()).unwrap(),
    );
    res.headers_mut().insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=3600"));
    if !checksum.is_empty() {
        res.headers_mut().insert(
            header::ETAG,
            HeaderValue::from_bytes(format!("\"{}\"", checksum).as_bytes()).unwrap(),
        );
    }
    if !modified_at.is_empty() {
        res.headers_mut().insert(
            header::LAST_MODIFIED,
            HeaderValue::from_bytes(modified_at.as_bytes()).unwrap(),
        );
    }
    Ok(res)
}

/// Serve a byte range of the file (206 Partial Content) with caching headers.
///
/// Includes `ETag` (checksum) and `Cache-Control: private, max-age=3600`.
/// Range responses skip 304 handling since they are already partial.
async fn serve_file_range(
    path: &std::path::Path,
    range: RangeInclusive<u64>,
    file_size: u64,
    mime_type: &str,
    filename: &str,
    checksum: &str,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let start = *range.start();
    let end = *range.end();
    let length = end - start + 1;

    let file = tokio::fs::File::open(path).await.map_err(|e| {
        tracing::error!(error = %e, path = %path.display(), "Failed to open file");
        (StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"})))
    })?;

    let mut file = file;
    file.seek(std::io::SeekFrom::Start(start)).await.map_err(|e| {
        tracing::error!(error = %e, "Failed to seek in file");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Internal server error"})))
    })?;

    // Take only the requested range of bytes
    let reader = file.take(length);
    let stream = tokio_util::io::ReaderStream::new(reader);
    let body = axum::body::Body::from_stream(stream);

    let mut res = Response::new(body);
    *res.status_mut() = StatusCode::PARTIAL_CONTENT;
    res.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_bytes(mime_type.as_bytes()).unwrap());
    res.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_bytes(format!("inline; filename=\"{}\"", filename).as_bytes()).unwrap(),
    );
    res.headers_mut().insert(
        header::CONTENT_RANGE,
        HeaderValue::from_bytes(format!("bytes {}-{}/{}", start, end, file_size).as_bytes())
            .unwrap(),
    );
    res.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_bytes(length.to_string().as_bytes()).unwrap(),
    );
    res.headers_mut().insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=3600"));
    if !checksum.is_empty() {
        res.headers_mut().insert(
            header::ETAG,
            HeaderValue::from_bytes(format!("\"{}\"", checksum).as_bytes()).unwrap(),
        );
    }
    Ok(res)
}

#[cfg(test)]
#[path = "file_test.rs"]
mod file_test;
