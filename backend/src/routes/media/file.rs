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

/// Resolve a media item's absolute file path, MIME type, and filename from the
/// database by UUID.
///
/// Queries `media_items` for the relative path and looks it up against every
/// configured watched folder, returning the first matching absolute path.
pub(super) fn resolve_media_path(
    db: &rusqlite::Connection,
    id: &str,
) -> Result<(PathBuf, String, String), (StatusCode, Json<Value>)> {
    let (relative_path, mime_type, filename): (String, String, String) = db
        .query_row(
            "SELECT relative_path, mime_type, filename FROM media_items WHERE id = ?1",
            rusqlite::params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
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

    // Try to resolve via folder_id → watched_folders path first (preferred).
    let folder_id: Option<String> = db
        .query_row(
            "SELECT folder_id FROM media_items WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .ok();

    if let Some(ref fid) = folder_id {
        let folder_path: Option<String> = db
            .query_row(
                "SELECT path FROM watched_folders WHERE id = ?1",
                rusqlite::params![fid],
                |row| row.get(0),
            )
            .ok();

        if let Some(ref base) = folder_path {
            let full = std::path::Path::new(base).join(&relative_path);
            if full.exists() {
                return Ok((full, mime_type, filename));
            }
        }
    }

    // Fallback: resolve using the config JSON (backward compat).
    let config_str: String = db
        .query_row("SELECT value FROM config WHERE key = 'watched_folders'", [], |row| row.get(0))
        .map_err(|_| (StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"}))))?;

    let config: Value = serde_json::from_str(&config_str).map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Invalid configuration"})))
    })?;

    let folders = config["watched_folders"].as_array().ok_or_else(|| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Invalid configuration format"})))
    })?;

    let file_path = folders
        .iter()
        .filter_map(|folder| {
            let base = folder["path"].as_str()?;
            let full = std::path::Path::new(base).join(&relative_path);
            if full.exists() { Some(full) } else { None }
        })
        .next()
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"}))))?;

    Ok((file_path, mime_type, filename))
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

    // Resolve file path and get caching info from DB
    let conn = state.db.get().map_err(|e| {
        tracing::error!(error = %e, "Failed to acquire database connection");
        (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Service temporarily unavailable"})))
    })?;
    let (file_path, mime_type, filename) = resolve_media_path(&conn, &id)?;
    let (checksum, modified_at): (String, String) = conn
        .query_row(
            "SELECT COALESCE(checksum, ''), COALESCE(file_modified_at, '') FROM media_items WHERE id = ?1",
            rusqlite::params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap_or_default();
    drop(conn);

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
