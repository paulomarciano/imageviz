use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _};
use tokio::sync::Mutex;

use crate::thumbnails::cache::CacheError;

/// Shared application state for media endpoints.
pub struct MediaState {
    pub db: Arc<Mutex<rusqlite::Connection>>,
    pub thumbnail_cache_dir: PathBuf,
}

pub fn routes() -> Router<Arc<MediaState>> {
    Router::new()
        .route("/media/{id}/file", get(serve_file))
        .route("/media/{id}/thumbnail", get(serve_thumbnail))
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

    // Resolve the relative path to an absolute path using watched folders
    // from the config table.
    let config_str: String = db
        .query_row(
            "SELECT value FROM config WHERE key = 'watched_folders'",
            [],
            |row| row.get(0),
        )
        .map_err(|_| {
            (StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"})))
        })?;

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
            if full.exists() {
                Some(full)
            } else {
                None
            }
        })
        .next()
        .ok_or_else(|| {
            (StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"})))
        })?;

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
async fn serve_file(
    State(state): State<Arc<MediaState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, Json<Value>)> {
    // Resolve file path
    let db = state.db.lock().await;
    let (file_path, mime_type, filename) = resolve_media_path(&db, &id)?;
    drop(db);

    // Get file metadata for size and Last-Modified
    let metadata = tokio::fs::metadata(&file_path).await.map_err(|e| {
        tracing::error!(error = %e, path = %file_path.display(), "Failed to stat file");
        (StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"})))
    })?;
    let file_size = metadata.len();

    // Check for Range header
    if let Some(range_header) = headers.get(header::RANGE) {
        if let Ok(range_str) = range_header.to_str() {
            if range_str.starts_with("bytes=") {
                if let Some(range) = parse_range_header(range_str, file_size) {
                    return serve_file_range(&file_path, range, file_size, &mime_type, &filename)
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
        }
    }

    // No Range header → serve full file
    serve_full_file(&file_path, &mime_type, &filename)
        .await
        .map(IntoResponse::into_response)
}

/// Serve the full file (200 OK).
async fn serve_full_file(
    path: &std::path::Path,
    mime_type: &str,
    filename: &str,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    let file = tokio::fs::File::open(path).await.map_err(|e| {
        tracing::error!(error = %e, path = %path.display(), "Failed to open file");
        (StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"})))
    })?;

    let stream = tokio_util::io::ReaderStream::new(file);
    let body = axum::body::Body::from_stream(stream);

    Ok((
        [
            (header::CONTENT_TYPE, mime_type.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"{}\"", filename),
            ),
            (header::ACCEPT_RANGES, "bytes".to_string()),
        ],
        body,
    ))
}

/// Serve a byte range of the file (206 Partial Content).
async fn serve_file_range(
    path: &std::path::Path,
    range: RangeInclusive<u64>,
    file_size: u64,
    mime_type: &str,
    filename: &str,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
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

    Ok((
        StatusCode::PARTIAL_CONTENT,
        [
            (header::CONTENT_TYPE, mime_type.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"{}\"", filename),
            ),
            (
                header::CONTENT_RANGE,
                format!("bytes {}-{}/{}", start, end, file_size),
            ),
            (header::CONTENT_LENGTH, length.to_string()),
            (header::ACCEPT_RANGES, "bytes".to_string()),
        ],
        body,
    ))
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
    // Parse optional width parameter (default 200, range 100-500)
    let width: u32 = params
        .get("width")
        .and_then(|w| w.parse().ok())
        .unwrap_or(200);

    if width < 100 || width > 500 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("Width must be between 100 and 500, got {}", width)})),
        ));
    }

    // Look up media item, resolve file path, and get checksum (single lock)
    let db = state.db.lock().await;
    let (file_path, _, _) = resolve_media_path(&db, &id)?;

    let checksum: String = db
        .query_row(
            "SELECT COALESCE(checksum, '') FROM media_items WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .unwrap_or_default();
    drop(db);

    // Generate or retrieve cached thumbnail
    let thumbnail = crate::thumbnails::get_or_generate_thumbnail(
        &file_path,
        &checksum,
        width,
        &state.thumbnail_cache_dir,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to generate thumbnail");
        match e {
            CacheError::SourceNotFound(_) => {
                (StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"})))
            }
            _ => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to generate thumbnail"})))
            }
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
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    /// Build a test `MediaState` with an in-memory SQLite database and a
    /// temporary cache directory.  The database is pre-populated with the
    /// minimum schema needed to exercise routes.
    fn test_state() -> Arc<MediaState> {
        let conn = rusqlite::Connection::open_in_memory()
            .expect("Failed to create in-memory database");
        conn.execute_batch(
            "CREATE TABLE media_items (
                id TEXT PRIMARY KEY NOT NULL,
                filename TEXT NOT NULL,
                relative_path TEXT NOT NULL UNIQUE,
                mime_type TEXT NOT NULL,
                file_size INTEGER NOT NULL DEFAULT 0,
                file_created_at TEXT NOT NULL DEFAULT '',
                file_modified_at TEXT NOT NULL DEFAULT '',
                checksum TEXT
            );
            CREATE TABLE config (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );",
        )
        .expect("Failed to create test tables");
        Arc::new(MediaState {
            db: Arc::new(Mutex::new(conn)),
            #[allow(deprecated)]
            thumbnail_cache_dir: tempfile::tempdir().unwrap().into_path(),
        })
    }

    /// Seed the database with a watched-folder config pointing at `folder_path`.
    async fn seed_config(state: &Arc<MediaState>, folder_path: &std::path::Path) {
        let db = state.db.lock().await;
        let config = json!({
            "watched_folders": [
                {"path": folder_path.to_str().unwrap()}
            ]
        });
        db.execute(
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
        let db = state.db.lock().await;
        db.execute(
            "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at, checksum)
             VALUES (?1, ?2, ?3, ?4, 1024, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', ?5)",
            rusqlite::params![id, filename, relative_path, mime_type, checksum],
        )
        .expect("Failed to seed media item");
    }

    /// Create a small solid-colour PNG file for testing.
    fn create_test_png(path: &std::path::Path) {
        let img = image::RgbaImage::new(100, 100);
        img.save_with_format(path, image::ImageFormat::Png)
            .expect("failed to create test PNG");
    }

    // -----------------------------------------------------------------------
    // Thumbnail endpoint tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_thumbnail_invalid_width_below_min_returns_400() {
        let state = test_state();
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
        let state = test_state();
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
        let state = test_state();
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
        let state = test_state();
        let watched = tempfile::tempdir().unwrap();

        seed_config(&state, watched.path()).await;
        seed_media_item(
            &state,
            "00000000-0000-0000-0000-000000000001",
            "missing.png",
            "missing.png",
            "image/png",
            "abc123",
        ).await;

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
        let state = test_state();
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
        ).await;

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
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap();
        assert_eq!(content_type, "image/webp");

        let cache_control = response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
            .unwrap();
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
        let content_type2 = response2
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap();
        assert_eq!(content_type2, "image/webp");
    }

    #[tokio::test]
    async fn test_thumbnail_cache_hit_returns_200() {
        let state = test_state();
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
        ).await;

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
        let state = test_state();
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
        let state = test_state();
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
        ).await;

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

        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap();
        assert_eq!(content_type, "image/png");

        let content_disposition = response
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .unwrap();
        assert!(content_disposition.contains("test.png"));
    }
}
