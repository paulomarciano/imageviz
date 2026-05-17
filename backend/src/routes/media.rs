use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use r2d2::Pool;

use crate::db::SqliteConnectionManager;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _};

use crate::middleware::validation;
use crate::thumbnails::cache::CacheError;
use crate::thumbnails::limiter::ThumbnailLimiter;

/// Shared application state for media endpoints.
pub struct MediaState {
    pub db: Pool<SqliteConnectionManager>,
    pub thumbnail_cache_dir: PathBuf,
    pub thumbnail_limiter: Arc<ThumbnailLimiter>,
}

pub fn routes() -> Router<Arc<MediaState>> {
    Router::new()
        .route("/media", get(list_media))
        .route("/media/{id}", get(get_media_item))
        .route("/media/{id}/metadata", get(get_media_metadata))
        .route("/media/{id}/file", get(serve_file))
        .route("/media/{id}/thumbnail", get(serve_thumbnail))
}

// ---------------------------------------------------------------------------
// Media list (cursor-based pagination)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct MediaListParams {
    #[serde(default = "default_limit")]
    limit: u32,
    cursor: Option<String>,
    cursor_id: Option<String>,
    mime_type: Option<String>,
}

fn default_limit() -> u32 {
    100
}

#[derive(Serialize)]
struct MediaItemSummary {
    id: String,
    filename: String,
    path: String,
    mime_type: String,
    thumbnail_url: String,
    width: Option<i64>,
    height: Option<i64>,
    file_size: i64,
    created_at: String,
    modified_at: String,
}

/// GET /api/v1/media — list media items with cursor-based pagination.
///
/// Query parameters:
/// - `limit` (default 100, max 500): number of items per page
/// - `cursor` (ISO 8601 date): exclusive cursor from the last item's `created_at`
/// - `cursor_id` (UUID): tiebreaker for items with the same `file_created_at`
/// - `mime_type` (e.g. `image/%`): optional MIME type filter (SQL LIKE)
///
/// Returns a JSON object with `data` (array of `MediaItemSummary`) and `meta`
/// (pagination metadata: `next_cursor`, `next_cursor_id`, `has_more`, `total`).
async fn list_media(
    State(state): State<Arc<MediaState>>,
    Query(params): Query<MediaListParams>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    validation::validate_limit(params.limit)?;
    validation::validate_cursor(params.cursor.as_deref())?;
    validation::validate_cursor_id(params.cursor_id.as_deref())?;

    let limit = params.limit;
    let fetch_limit = limit + 1;
    let has_cursor = params.cursor.is_some() && params.cursor_id.is_some();
    let has_mime = params.mime_type.is_some();

    let conn = state.db.get().map_err(|e| {
        tracing::error!(error = %e, "Failed to acquire database connection");
        (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Service temporarily unavailable"})))
    })?;

    // Compute total count (fast COUNT with or without mime_type filter)
    let total: i64 = if let Some(ref mime_type) = params.mime_type {
        conn.query_row(
            "SELECT COUNT(*) FROM media_items WHERE mime_type LIKE ?1",
            rusqlite::params![mime_type],
            |row| row.get(0),
        )
        .unwrap_or(0)
    } else {
        conn.query_row("SELECT COUNT(*) FROM media_items", [], |row| row.get(0)).unwrap_or(0)
    };

    // Build SQL dynamically for cursor-based pagination
    let mut sql = String::from(
        "SELECT id, filename, relative_path, mime_type, width, height, file_size, \
         file_created_at, file_modified_at FROM media_items",
    );

    let mut where_parts: Vec<String> = Vec::new();
    let mut next_param = 1;
    if has_cursor {
        where_parts.push(format!("(file_created_at, id) < (?{}, ?{})", next_param, next_param + 1));
        next_param += 2;
    }
    if has_mime {
        where_parts.push(format!("mime_type LIKE ?{}", next_param));
        next_param += 1;
    }

    if !where_parts.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&where_parts.join(" AND "));
    }

    sql.push_str(" ORDER BY file_created_at DESC, id DESC LIMIT ?");
    sql.push_str(&next_param.to_string());

    // Collect parameter values in the same order as their placeholders
    let mut values: Vec<rusqlite::types::Value> = Vec::new();
    if let (Some(cursor), Some(cursor_id)) = (&params.cursor, &params.cursor_id) {
        values.push(rusqlite::types::Value::Text(cursor.clone()));
        values.push(rusqlite::types::Value::Text(cursor_id.clone()));
    }
    if let Some(ref mime_type) = params.mime_type {
        values.push(rusqlite::types::Value::Text(mime_type.clone()));
    }
    values.push(rusqlite::types::Value::Integer(fetch_limit as i64));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        values.iter().map(|v| v as &dyn rusqlite::types::ToSql).collect();

    let mut items: Vec<MediaItemSummary> = {
        let mut stmt = conn.prepare(&sql).map_err(|e| {
            tracing::error!(error = %e, "Failed to prepare media list query");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Internal server error"})))
        })?;

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                let id: String = row.get(0)?;
                let filename: String = row.get(1)?;
                let relative_path: String = row.get(2)?;
                let mime_type: String = row.get(3)?;
                let width: Option<i64> = row.get(4)?;
                let height: Option<i64> = row.get(5)?;
                let file_size: i64 = row.get(6)?;
                let file_created_at: String = row.get(7)?;
                let file_modified_at: String = row.get(8)?;
                Ok(MediaItemSummary {
                    thumbnail_url: format!("/api/v1/media/{}/thumbnail", id),
                    id,
                    filename,
                    path: relative_path,
                    mime_type,
                    width,
                    height,
                    file_size,
                    created_at: file_created_at,
                    modified_at: file_modified_at,
                })
            })
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to query media items");
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Internal server error"})))
            })?;

        let mut items: Vec<MediaItemSummary> = Vec::new();
        for row in rows {
            match row {
                Ok(item) => items.push(item),
                Err(e) => {
                    tracing::error!(error = %e, "Failed to read media row");
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": "Internal server error"})),
                    ));
                }
            }
        }
        items
    };

    drop(conn);

    let has_more = items.len() > limit as usize;
    items.truncate(limit as usize);

    let (next_cursor, next_cursor_id) = if has_more {
        let last = items.last().expect("items non-empty when has_more is true");
        (Some(last.created_at.clone()), Some(last.id.clone()))
    } else {
        (None, None)
    };

    Ok(Json(json!({
        "data": items,
        "meta": {
            "next_cursor": next_cursor,
            "next_cursor_id": next_cursor_id,
            "has_more": has_more,
            "total": total,
        }
    })))
}

// ---------------------------------------------------------------------------
// Media item detail (GET /media/:id)
// ---------------------------------------------------------------------------

/// Full media item response for the detail view.
#[derive(Serialize)]
struct MediaItemDetail {
    id: String,
    filename: String,
    path: String,
    mime_type: String,
    thumbnail_url: String,
    file_url: String,
    width: Option<i64>,
    height: Option<i64>,
    file_size: i64,
    created_at: String,
    modified_at: String,
    metadata: Option<Value>,
}

/// GET /api/v1/media/{id} — return a single media item with full metadata.
async fn get_media_item(
    State(state): State<Arc<MediaState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    validation::validate_media_id(&id)?;

    let conn = state.db.get().map_err(|e| {
        tracing::error!(error = %e, "Failed to acquire database connection");
        (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Service temporarily unavailable"})))
    })?;

    let row = conn
        .query_row(
            "SELECT id, filename, relative_path, mime_type, width, height, file_size,
                    file_created_at, file_modified_at, metadata_json
             FROM media_items WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                let id: String = row.get(0)?;
                let metadata_raw: Option<String> = row.get(9)?;
                Ok(MediaItemDetail {
                    id: id.clone(),
                    filename: row.get(1)?,
                    path: row.get(2)?,
                    mime_type: row.get(3)?,
                    thumbnail_url: format!("/api/v1/media/{}/thumbnail", id),
                    file_url: format!("/api/v1/media/{}/file", id),
                    width: row.get(4)?,
                    height: row.get(5)?,
                    file_size: row.get(6)?,
                    created_at: row.get(7)?,
                    modified_at: row.get(8)?,
                    metadata: metadata_raw.and_then(|s| serde_json::from_str(&s).ok()),
                })
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

    Ok(Json(json!(row)))
}

/// GET /api/v1/media/{id}/metadata — return the structured metadata for an item.
async fn get_media_metadata(
    State(state): State<Arc<MediaState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    validation::validate_media_id(&id)?;

    let conn = state.db.get().map_err(|e| {
        tracing::error!(error = %e, "Failed to acquire database connection");
        (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Service temporarily unavailable"})))
    })?;

    let metadata_json: Option<String> = conn
        .query_row(
            "SELECT metadata_json FROM media_items WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                (StatusCode::NOT_FOUND, Json(json!({"error": "Media not found"})))
            }
            _ => {
                tracing::error!(error = %e, "Database error fetching metadata");
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Internal server error"})))
            }
        })?;

    match metadata_json {
        Some(json_str) => match serde_json::from_str(&json_str) {
            Ok(val) => Ok(Json(val)),
            Err(_) => {
                // Non-JSON metadata — return empty rather than serving
                // raw strings that would confuse the frontend.
                Ok(Json(json!({})))
            }
        },
        None => Ok(Json(json!({}))),
    }
}

/// Resolve a media item's absolute file path, MIME type, and filename from the
/// database by UUID.
///
/// Queries `media_items` for the relative path and looks it up against every
/// configured watched folder, returning the first matching absolute path.
fn resolve_media_path(
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

// ---------------------------------------------------------------------------
// File serving (with optional Range support)
// ---------------------------------------------------------------------------

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
async fn serve_file(
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

// ---------------------------------------------------------------------------
// Thumbnail serving
// ---------------------------------------------------------------------------

/// GET /api/v1/media/{id}/thumbnail — serve a WebP thumbnail.
async fn serve_thumbnail(
    State(state): State<Arc<MediaState>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    validation::validate_media_id(&id)?;

    // Parse optional width parameter (default 200, range 100-500)
    let width: u32 = params.get("width").and_then(|w| w.parse().ok()).unwrap_or(200);
    validation::validate_thumbnail_width(width)?;

    // Look up media item, resolve file path, and get checksum
    let conn = state.db.get().map_err(|e| {
        tracing::error!(error = %e, "Failed to acquire database connection");
        (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Service temporarily unavailable"})))
    })?;
    let (file_path, mime_type, _) = resolve_media_path(&conn, &id)?;

    let checksum: String = conn
        .query_row(
            "SELECT COALESCE(checksum, '') FROM media_items WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .unwrap_or_default();
    drop(conn);

    // Acquire thumbnail generation permit (limits CPU contention)
    let _permit = state.thumbnail_limiter.acquire().await.map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "Too many thumbnail requests. Try again later."})),
        )
    })?;

    // Generate or retrieve cached thumbnail
    let thumbnail = crate::thumbnails::get_or_generate_thumbnail(
        &file_path,
        &checksum,
        width,
        &state.thumbnail_cache_dir,
        &mime_type,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to generate thumbnail");
        match e {
            CacheError::SourceNotFound(_) => {
                (StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"})))
            }
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to generate thumbnail"})),
            ),
        }
    })?;

    // Read the thumbnail file into memory
    let data = tokio::fs::read(&thumbnail).await.map_err(|e| {
        tracing::error!(error = %e, "Failed to read thumbnail file");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Internal server error"})))
    })?;

    Ok((
        [
            (header::CONTENT_TYPE, "image/webp".to_string()),
            (header::CONTENT_LENGTH, data.len().to_string()),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable".to_string()),
        ],
        data,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use chrono::NaiveDateTime;

    use http_body_util::BodyExt;
    use tower::ServiceExt;

    /// Build a test `MediaState` with an in-memory SQLite connection pool and
    /// a temporary cache directory (kept alive until the test finishes).
    /// The database is created via the real migration path so the schema
    /// matches production.
    fn test_state() -> (Arc<MediaState>, tempfile::TempDir) {
        let pool = crate::db::pool::create_in_memory_pool();
        {
            let mut conn = pool.get().expect("Failed to get connection for migrations");
            crate::db::migrations::run_migrations(&mut conn).expect("Failed to run migrations");
        }
        let cache_dir = tempfile::tempdir().expect("tempdir");
        let state = Arc::new(MediaState {
            db: pool,
            thumbnail_cache_dir: cache_dir.path().to_path_buf(),
            thumbnail_limiter: Arc::new(ThumbnailLimiter::new(16)), // generous for tests
        });
        (state, cache_dir)
    }

    /// Seed the database with a watched-folder config pointing at `folder_path`.
    async fn seed_config(state: &Arc<MediaState>, folder_path: &std::path::Path) {
        let conn = state.db.get().expect("Failed to get DB connection");
        let config = json!({
            "watched_folders": [
                {"path": folder_path.to_str().unwrap()}
            ]
        });
        conn.execute(
            "INSERT INTO config (key, value) VALUES ('watched_folders', ?1)",
            rusqlite::params![config.to_string()],
        )
        .expect("Failed to seed config");
    }

    /// Seed a single media item in the database.
    async fn seed_media_item(
        state: &Arc<MediaState>,
        id: &str,
        filename: &str,
        relative_path: &str,
        mime_type: &str,
        checksum: &str,
    ) {
        let conn = state.db.get().expect("Failed to get DB connection");
        conn.execute(
            "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at, checksum)
             VALUES (?1, ?2, ?3, ?4, 1024, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', ?5)",
            rusqlite::params![id, filename, relative_path, mime_type, checksum],
        )
        .expect("Failed to seed media item");
    }

    /// Seed a media item with full fields including dimensions and custom dates.
    /// Used by cursor pagination tests to create items at specific timestamps.
    #[allow(dead_code)]
    async fn seed_media_item_full(
        state: &Arc<MediaState>,
        id: &str,
        filename: &str,
        relative_path: &str,
        mime_type: &str,
        checksum: &str,
        width: Option<i64>,
        height: Option<i64>,
        file_created_at: &str,
        file_modified_at: &str,
    ) {
        let conn = state.db.get().expect("Failed to get DB connection");
        conn.execute(
            "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, width, height, file_created_at, file_modified_at, checksum)
             VALUES (?1, ?2, ?3, ?4, 1024, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![id, filename, relative_path, mime_type, width, height, file_created_at, file_modified_at, checksum],
        )
        .expect("Failed to seed media item");
    }

    /// Create a small solid-colour PNG file for testing.
    fn create_test_png(path: &std::path::Path) {
        let img = image::RgbaImage::new(100, 100);
        img.save_with_format(path, image::ImageFormat::Png).expect("failed to create test PNG");
    }

    // -----------------------------------------------------------------------
    // Thumbnail endpoint tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_thumbnail_invalid_width_below_min_returns_400() {
        let (state, _cache_dir) = test_state();
        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000000/thumbnail?width=50")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();
        assert!(body["error"].as_str().unwrap().contains("Width must be between"));
    }

    #[tokio::test]
    async fn test_thumbnail_invalid_width_above_max_returns_400() {
        let (state, _cache_dir) = test_state();
        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000000/thumbnail?width=600")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_thumbnail_media_not_found_returns_404() {
        let (state, _cache_dir) = test_state();
        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000000/thumbnail")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body["error"], "Media not found");
    }

    #[tokio::test]
    async fn test_thumbnail_file_not_found_on_disk_returns_404() {
        let (state, _cache_dir) = test_state();
        let watched = tempfile::tempdir().unwrap();

        seed_config(&state, watched.path()).await;
        seed_media_item(
            &state,
            "00000000-0000-0000-0000-000000000001",
            "missing.png",
            "missing.png",
            "image/png",
            "abc123",
        )
        .await;

        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000001/thumbnail")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body["error"], "File not found on disk");
    }

    #[tokio::test]
    async fn test_thumbnail_happy_path_generates_webp() {
        let (state, _cache_dir) = test_state();
        let watched = tempfile::tempdir().unwrap();
        let source_path = watched.path().join("test.png");
        create_test_png(&source_path);

        seed_config(&state, watched.path()).await;
        seed_media_item(
            &state,
            "00000000-0000-0000-0000-000000000002",
            "test.png",
            "test.png",
            "image/png",
            "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
        )
        .await;

        let app = routes().with_state(state);

        // Request with default width
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000002/thumbnail")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Verify response headers
        let content_type =
            response.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap();
        assert_eq!(content_type, "image/webp");

        let cache_control =
            response.headers().get(header::CACHE_CONTROL).and_then(|v| v.to_str().ok()).unwrap();
        assert_eq!(cache_control, "public, max-age=31536000, immutable");

        // Verify body is valid WebP
        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert!(body_bytes.len() > 12, "WebP file too small");
        assert_eq!(&body_bytes[0..4], b"RIFF", "WebP must start with RIFF header");
        assert_eq!(&body_bytes[8..12], b"WEBP", "WebP must contain WEBP identifier");

        // Request with custom width
        let response2 = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000002/thumbnail?width=300")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response2.status(), StatusCode::OK);
        let content_type2 =
            response2.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap();
        assert_eq!(content_type2, "image/webp");
    }

    #[tokio::test]
    async fn test_thumbnail_cache_hit_returns_200() {
        let (state, _cache_dir) = test_state();
        let watched = tempfile::tempdir().unwrap();
        let source_path = watched.path().join("test.png");
        create_test_png(&source_path);

        seed_config(&state, watched.path()).await;
        seed_media_item(
            &state,
            "00000000-0000-0000-0000-000000000003",
            "test.png",
            "test.png",
            "image/png",
            "fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321",
        )
        .await;

        let app = routes().with_state(state);

        // First call — cache miss, generates thumbnail
        let r1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000003/thumbnail")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(r1.status(), StatusCode::OK);
        let body1 = r1.into_body().collect().await.unwrap().to_bytes();

        // Second call — cache hit
        let r2 = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000003/thumbnail")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(r2.status(), StatusCode::OK);
        let body2 = r2.into_body().collect().await.unwrap().to_bytes();

        assert_eq!(body1, body2, "cached thumbnail should be identical");
    }

    // -----------------------------------------------------------------------
    // File serving endpoint tests (post-refactor)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_serve_file_media_not_found_returns_404() {
        let (state, _cache_dir) = test_state();
        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000000/file")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_serve_file_happy_path() {
        let (state, _cache_dir) = test_state();
        let watched = tempfile::tempdir().unwrap();
        let source_path = watched.path().join("test.png");
        create_test_png(&source_path);

        seed_config(&state, watched.path()).await;
        seed_media_item(
            &state,
            "00000000-0000-0000-0000-000000000004",
            "test.png",
            "test.png",
            "image/png",
            "",
        )
        .await;

        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000004/file")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let content_type =
            response.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap();
        assert_eq!(content_type, "image/png");

        let content_disposition = response
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .unwrap();
        assert!(content_disposition.contains("test.png"));
    }

    // -----------------------------------------------------------------------
    // Caching header tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_serve_file_etag_and_cache_control() {
        let (state, _cache_dir) = test_state();
        let watched = tempfile::tempdir().unwrap();
        let source_path = watched.path().join("test.png");
        create_test_png(&source_path);

        seed_config(&state, watched.path()).await;
        seed_media_item(
            &state,
            "00000000-0000-0000-0000-000000000005",
            "test.png",
            "test.png",
            "image/png",
            "abc123def456",
        )
        .await;

        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000005/file")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Verify ETag header
        let etag = response.headers().get(header::ETAG).and_then(|v| v.to_str().ok()).unwrap();
        assert_eq!(etag, "\"abc123def456\"");

        // Verify Cache-Control header
        let cache_control =
            response.headers().get(header::CACHE_CONTROL).and_then(|v| v.to_str().ok()).unwrap();
        assert_eq!(cache_control, "private, max-age=3600");
    }

    #[tokio::test]
    async fn test_serve_file_304_not_modified() {
        let (state, _cache_dir) = test_state();
        let watched = tempfile::tempdir().unwrap();
        let source_path = watched.path().join("test.png");
        create_test_png(&source_path);

        seed_config(&state, watched.path()).await;
        seed_media_item(
            &state,
            "00000000-0000-0000-0000-000000000006",
            "test.png",
            "test.png",
            "image/png",
            "xyz789",
        )
        .await;

        let app = routes().with_state(state);

        // Request with matching If-None-Match
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000006/file")
                    .header(header::IF_NONE_MATCH, "\"xyz789\"")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);

        // 304 response must have an empty body
        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert!(body_bytes.is_empty(), "304 response must have empty body");
    }

    #[tokio::test]
    async fn test_serve_file_etag_mismatch_returns_200() {
        let (state, _cache_dir) = test_state();
        let watched = tempfile::tempdir().unwrap();
        let source_path = watched.path().join("test.png");
        create_test_png(&source_path);

        seed_config(&state, watched.path()).await;
        seed_media_item(
            &state,
            "00000000-0000-0000-0000-000000000007",
            "test.png",
            "test.png",
            "image/png",
            "realchecksum",
        )
        .await;

        let app = routes().with_state(state);

        // Request with non-matching If-None-Match
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000007/file")
                    .header(header::IF_NONE_MATCH, "\"wrongchecksum\"")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Server should still return its own ETag
        let etag = response.headers().get(header::ETAG).and_then(|v| v.to_str().ok()).unwrap();
        assert_eq!(etag, "\"realchecksum\"");
    }

    #[tokio::test]
    async fn test_serve_file_no_checksum_omits_etag() {
        let (state, _cache_dir) = test_state();
        let watched = tempfile::tempdir().unwrap();
        let source_path = watched.path().join("test.png");
        create_test_png(&source_path);

        seed_config(&state, watched.path()).await;
        seed_media_item(
            &state,
            "00000000-0000-0000-0000-000000000008",
            "test.png",
            "test.png",
            "image/png",
            "", // empty checksum
        )
        .await;

        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000008/file")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // No ETag header when checksum is empty
        assert!(
            response.headers().get(header::ETAG).is_none(),
            "ETag should be absent when checksum is empty"
        );

        // Cache-Control should still be present
        let cache_control =
            response.headers().get(header::CACHE_CONTROL).and_then(|v| v.to_str().ok()).unwrap();
        assert_eq!(cache_control, "private, max-age=3600");
    }

    // -----------------------------------------------------------------------
    // Media list / cursor pagination tests
    // -----------------------------------------------------------------------

    /// Helper to seed N media items descending from a base date.
    /// Items get sequential IDs and dates spaced 1 second apart.
    async fn seed_n_items(state: &Arc<MediaState>, n: u32, base_date: &str, mime_type: &str) {
        let base = NaiveDateTime::parse_from_str(base_date, "%Y-%m-%dT%H:%M:%S")
            .expect("Invalid base date");

        for i in 0..n {
            let date = base - chrono::Duration::seconds(i as i64);
            let date_str = date.format("%Y-%m-%dT%H:%M:%S").to_string();
            let id = format!("00000000-0000-0000-0000-{:012}", i);
            seed_media_item_full(
                state,
                &id,
                &format!("file_{}.png", i),
                &format!("2025-01-01/file_{}.png", i),
                mime_type,
                "checksum",
                Some(100),
                Some(100),
                &date_str,
                &date_str,
            )
            .await;
        }
    }

    #[tokio::test]
    async fn test_media_list_empty_database() {
        let (state, _cache_dir) = test_state();
        let app = routes().with_state(state);

        let response = app
            .oneshot(Request::builder().uri("/media").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(body["data"].as_array().unwrap().len(), 0, "empty DB should return empty data");
        assert_eq!(body["meta"]["has_more"], false, "empty DB should have has_more=false");
        assert_eq!(body["meta"]["total"], 0, "empty DB should have total=0");
        assert!(body["meta"]["next_cursor"].is_null(), "empty DB should have null next_cursor");
        assert!(
            body["meta"]["next_cursor_id"].is_null(),
            "empty DB should have null next_cursor_id"
        );
    }

    #[tokio::test]
    async fn test_media_list_first_page() {
        let (state, _cache_dir) = test_state();
        seed_n_items(&state, 250, "2025-06-15T12:00:00", "image/png").await;
        let app = routes().with_state(state);

        let response = app
            .oneshot(Request::builder().uri("/media?limit=100").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();

        let data = body["data"].as_array().unwrap();
        assert_eq!(data.len(), 100, "first page should return exactly 100 items");
        assert_eq!(body["meta"]["has_more"], true, "250 items, 100 limit should have more");
        assert!(body["meta"]["next_cursor"].is_string(), "has_more=true should have next_cursor");
        assert!(
            body["meta"]["next_cursor_id"].is_string(),
            "has_more=true should have next_cursor_id"
        );

        // Items should be ordered newest first (descending date)
        if data.len() >= 2 {
            let first_created = data[0]["created_at"].as_str().unwrap();
            let second_created = data[1]["created_at"].as_str().unwrap();
            assert!(first_created >= second_created, "items should be ordered newest first");
        }
    }

    #[tokio::test]
    async fn test_media_list_cursor_pagination() {
        let (state, _cache_dir) = test_state();
        seed_n_items(&state, 250, "2025-06-15T12:00:00", "image/png").await;
        let app = routes().with_state(state);

        // First page
        let response1 = app
            .clone()
            .oneshot(Request::builder().uri("/media?limit=100").body(Body::empty()).unwrap())
            .await
            .unwrap();

        let body1: Value =
            serde_json::from_slice(&response1.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        let page1_ids: Vec<&str> = body1["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect();
        assert_eq!(page1_ids.len(), 100);
        let next_cursor = body1["meta"]["next_cursor"].as_str().unwrap().to_string();
        let next_cursor_id = body1["meta"]["next_cursor_id"].as_str().unwrap().to_string();

        // Second page using cursor
        let response2 = app
            .oneshot(
                Request::builder()
                    .uri(&format!(
                        "/media?limit=100&cursor={}&cursor_id={}",
                        next_cursor, next_cursor_id
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body2: Value =
            serde_json::from_slice(&response2.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        let page2_ids: Vec<&str> = body2["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect();

        assert!(!page2_ids.is_empty(), "second page should have items");
        assert_eq!(body2["meta"]["has_more"], true, "250 items, page 2 should still have more");

        // Verify no overlap between pages
        for id in &page2_ids {
            assert!(
                !page1_ids.contains(id),
                "cursor pagination should have no overlap: {} is in both pages",
                id
            );
        }
    }

    #[tokio::test]
    async fn test_media_list_last_page() {
        let (state, _cache_dir) = test_state();
        seed_n_items(&state, 50, "2025-06-15T12:00:00", "image/png").await;
        let app = routes().with_state(state);

        let response = app
            .oneshot(Request::builder().uri("/media?limit=100").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();

        let data = body["data"].as_array().unwrap();
        assert_eq!(data.len(), 50, "should return all 50 items when less than limit");
        assert_eq!(body["meta"]["has_more"], false, "50 items, 100 limit should have no more");
        assert_eq!(body["meta"]["total"], 50, "total should be 50");
    }

    #[tokio::test]
    async fn test_media_list_mime_type_filter() {
        let (state, _cache_dir) = test_state();
        // Insert 25 images and 25 videos
        for i in 0..25 {
            let date_str = format!("2025-06-15T12:{:02}:00", 59 - i);
            let id = format!("img-{:012}", i);
            seed_media_item_full(
                &state,
                &id,
                &format!("img_{}.png", i),
                &format!("img_{}.png", i),
                "image/png",
                "chk",
                Some(100),
                Some(100),
                &date_str,
                &date_str,
            )
            .await;

            let vid_id = format!("vid-{:012}", i);
            let vid_date = format!("2025-06-15T12:{:02}:00", 29 - i);
            seed_media_item_full(
                &state,
                &vid_id,
                &format!("vid_{}.mp4", i),
                &format!("vid_{}.mp4", i),
                "video/mp4",
                "chk",
                None,
                None,
                &vid_date,
                &vid_date,
            )
            .await;
        }

        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media?limit=100&mime_type=image/%")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();

        let data = body["data"].as_array().unwrap();
        assert_eq!(data.len(), 25, "should return only 25 images");
        assert_eq!(body["meta"]["total"], 25, "total should be 25 images");

        // Verify all returned items are images
        for item in data {
            let mime = item["mime_type"].as_str().unwrap();
            assert!(mime.starts_with("image/"), "all items should be images, got: {}", mime);
        }
    }

    #[tokio::test]
    async fn test_media_list_invalid_limit() {
        let (state, _cache_dir) = test_state();
        let app = routes().with_state(state);

        // Test limit > 500
        let response = app
            .clone()
            .oneshot(Request::builder().uri("/media?limit=1000").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();
        assert!(body["error"].as_str().unwrap().contains("500"), "error should mention 500 limit");

        // Test limit = 0
        let response2 = app
            .oneshot(Request::builder().uri("/media?limit=0").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response2.status(), StatusCode::BAD_REQUEST);
    }

    // -----------------------------------------------------------------------
    // Media item detail (GET /media/:id)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_media_item_not_found_returns_404() {
        let (state, _cache_dir) = test_state();
        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000000")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_media_item_happy_path() {
        let (state, _cache_dir) = test_state();

        {
            let conn = state.db.get().expect("Failed to get DB connection");
            conn.execute(
                "INSERT INTO media_items \
                 (id, filename, relative_path, mime_type, width, height, file_size, \
                  file_created_at, file_modified_at, metadata_json, checksum) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                rusqlite::params![
                    "00000000-0000-0000-0000-000000000100",
                    "detail.png",
                    "sub/detail.png",
                    "image/png",
                    800_i64,
                    600_i64,
                    4096_i64,
                    "2025-06-15T12:00:00Z",
                    "2025-06-15T12:00:00Z",
                    r#"{"prompt":{"text":"a test image"}}"#,
                    "checksum100",
                ],
            )
            .expect("seed media item");
        }

        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000100")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(body["id"], "00000000-0000-0000-0000-000000000100");
        assert_eq!(body["filename"], "detail.png");
        assert_eq!(body["path"], "sub/detail.png");
        assert_eq!(body["mime_type"], "image/png");
        assert_eq!(body["width"], 800);
        assert_eq!(body["height"], 600);
        assert_eq!(body["file_size"], 4096);
        assert!(
            body["thumbnail_url"]
                .as_str()
                .unwrap()
                .contains("/media/00000000-0000-0000-0000-000000000100/thumbnail")
        );
        assert!(
            body["file_url"]
                .as_str()
                .unwrap()
                .contains("/media/00000000-0000-0000-0000-000000000100/file")
        );
        assert_eq!(body["metadata"]["prompt"]["text"], "a test image");
    }

    #[tokio::test]
    async fn test_get_media_item_no_metadata_returns_null() {
        let (state, _cache_dir) = test_state();

        {
            let conn = state.db.get().expect("Failed to get DB connection");
            conn.execute(
                "INSERT INTO media_items \
                 (id, filename, relative_path, mime_type, file_size, \
                  file_created_at, file_modified_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    "00000000-0000-0000-0000-000000000101",
                    "no_meta.png",
                    "no_meta.png",
                    "image/png",
                    512_i64,
                    "2025-06-15T12:00:00Z",
                    "2025-06-15T12:00:00Z",
                ],
            )
            .expect("seed media item");
        }

        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000101")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(body["filename"], "no_meta.png");
        assert!(body["metadata"].is_null(), "metadata should be null when not present");
    }

    // -----------------------------------------------------------------------
    // Media metadata endpoint (GET /media/:id/metadata)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_media_metadata_not_found_returns_404() {
        let (state, _cache_dir) = test_state();
        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000000/metadata")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_media_metadata_happy_path() {
        let (state, _cache_dir) = test_state();

        {
            let conn = state.db.get().expect("Failed to get DB connection");
            conn.execute(
                "INSERT INTO media_items \
                 (id, filename, relative_path, mime_type, file_size, \
                  file_created_at, file_modified_at, metadata_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    "00000000-0000-0000-0000-000000000200",
                    "with_meta.png",
                    "with_meta.png",
                    "image/png",
                    2048_i64,
                    "2025-06-15T12:00:00Z",
                    "2025-06-15T12:00:00Z",
                    r#"{"seed":12345,"steps":20,"cfg":7.5}"#,
                ],
            )
            .expect("seed media item");
        }

        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000200/metadata")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(body["seed"], 12345);
        assert_eq!(body["steps"], 20);
        assert_eq!(body["cfg"], 7.5);
    }

    #[tokio::test]
    async fn test_get_media_metadata_empty_when_no_metadata() {
        let (state, _cache_dir) = test_state();

        {
            let conn = state.db.get().expect("Failed to get DB connection");
            conn.execute(
                "INSERT INTO media_items \
                 (id, filename, relative_path, mime_type, file_size, \
                  file_created_at, file_modified_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    "00000000-0000-0000-0000-000000000201",
                    "no_meta.png",
                    "no_meta.png",
                    "image/png",
                    512_i64,
                    "2025-06-15T12:00:00Z",
                    "2025-06-15T12:00:00Z",
                ],
            )
            .expect("seed media item");
        }

        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/media/00000000-0000-0000-0000-000000000201/metadata")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();

        // Should return empty JSON object when no metadata
        assert_eq!(body, json!({}));
    }
}
