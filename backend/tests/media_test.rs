use axum::{
    body::Body,
    http::{Request, StatusCode, header},
    Router,
};
use http_body_util::BodyExt;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

use imageviz_backend::routes::media::{MediaState, routes};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Build a test app with an in-memory database and a temporary cache directory.
///
/// The returned router has:
/// - `GET /api/v1/health` (from the app factory)
/// - `GET /api/v1/media/{id}/file`
/// - `GET /api/v1/media/{id}/thumbnail`
///
/// The database contains only the schema — no seeded data. Callers must
/// call `seed_config` and `seed_media_item` to populate test data.
fn create_media_test_app() -> (Router, Arc<MediaState>) {
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

    let cache_dir = tempfile::tempdir().expect("Failed to create cache directory");
    let db = Arc::new(Mutex::new(conn));
    #[allow(deprecated)]
    let media_state = Arc::new(MediaState {
        db: Arc::clone(&db),
        thumbnail_cache_dir: cache_dir.into_path(),
    });

    let app = imageviz_backend::app()
        .nest("/api/v1", routes().with_state(Arc::clone(&media_state)))
        .layer(CorsLayer::permissive());

    (app, media_state)
}

/// Seed the config table with a single watched folder pointing at `folder_path`.
async fn seed_config(state: &Arc<MediaState>, folder_path: &std::path::Path) {
    let db = state.db.lock().await;
    let config = json!({
        "watched_folders": [{"path": folder_path.to_str().unwrap()}]
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

/// Create a small solid-colour PNG file at the given path (100 × 100 pixels).
fn create_test_png(path: &std::path::Path) {
    let img = image::RgbaImage::new(100, 100);
    img.save_with_format(path, image::ImageFormat::Png)
        .expect("failed to create test PNG");
}

// ---------------------------------------------------------------------------
// Thumbnail endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_thumbnail_returns_webp_image() {
    let (app, state) = create_media_test_app();
    let watched = tempfile::tempdir().unwrap();
    let source_path = watched.path().join("test.png");
    create_test_png(&source_path);

    seed_config(&state, watched.path()).await;
    seed_media_item(
        &state,
        "00000000-0000-0000-0000-000000000001",
        "test.png",
        "test.png",
        "image/png",
        "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/media/00000000-0000-0000-0000-000000000001/thumbnail")
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
    assert_eq!(content_type, "image/webp");

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body_bytes.len() > 12, "WebP file too small");
    assert_eq!(&body_bytes[0..4], b"RIFF", "WebP must start with RIFF header");
    assert_eq!(
        &body_bytes[8..12], b"WEBP",
        "WebP must contain WEBP identifier at bytes 8-11"
    );
}

#[tokio::test]
async fn test_thumbnail_returns_404_for_invalid_id() {
    let (app, _state) = create_media_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/media/00000000-0000-0000-0000-000000000000/thumbnail")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_thumbnail_custom_width() {
    let (app, state) = create_media_test_app();
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

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/media/00000000-0000-0000-0000-000000000002/thumbnail?width=400")
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
    assert_eq!(content_type, "image/webp");
}

// ---------------------------------------------------------------------------
// File serving endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_file_streams_with_correct_content_type() {
    let (app, state) = create_media_test_app();
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
        "somechecksum",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/media/00000000-0000-0000-0000-000000000003/file")
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
}

#[tokio::test]
async fn test_file_returns_404_for_invalid_id() {
    let (app, _state) = create_media_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/media/00000000-0000-0000-0000-000000000000/file")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_file_range_request_206() {
    let (app, state) = create_media_test_app();
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
        "checksum_range",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/media/00000000-0000-0000-0000-000000000004/file")
                .header(header::RANGE, "bytes=0-99")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);

    // Verify Content-Range header is present and well-formed
    let content_range = response
        .headers()
        .get(header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .unwrap();
    assert!(
        content_range.starts_with("bytes "),
        "Content-Range should start with 'bytes '"
    );
    assert!(
        content_range.contains('/'),
        "Content-Range should contain a slash"
    );
}

#[tokio::test]
async fn test_file_range_not_satisfiable_416() {
    let (app, state) = create_media_test_app();
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
        "checksum_416",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/media/00000000-0000-0000-0000-000000000005/file")
                .header(header::RANGE, "bytes=999999999-")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
}

// ---------------------------------------------------------------------------
// Caching header tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_file_accept_ranges_header() {
    let (app, state) = create_media_test_app();
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
        "checksum_ar",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/media/00000000-0000-0000-0000-000000000006/file")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let accept_ranges = response
        .headers()
        .get(header::ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .unwrap();
    assert_eq!(accept_ranges, "bytes");
}

#[tokio::test]
async fn test_file_etag_header() {
    let (app, state) = create_media_test_app();
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
        "etagchecksum",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/media/00000000-0000-0000-0000-000000000007/file")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // ETag is returned as a quoted string: "{checksum}"
    let etag = response
        .headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .unwrap();
    assert_eq!(etag, "\"etagchecksum\"");
}

#[tokio::test]
async fn test_file_304_not_modified() {
    let (app, state) = create_media_test_app();
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
        "etag304test",
    )
    .await;

    // First request — capture the ETag from the response
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/media/00000000-0000-0000-0000-000000000008/file")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let etag = response
        .headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .unwrap()
        .to_string();

    // Second request with matching If-None-Match — expect 304 Not Modified
    let response2 = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/media/00000000-0000-0000-0000-000000000008/file")
                .header(header::IF_NONE_MATCH, &etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response2.status(), StatusCode::NOT_MODIFIED);

    // 304 response must have an empty body per RFC 7232
    let body_bytes = response2.into_body().collect().await.unwrap().to_bytes();
    assert!(body_bytes.is_empty(), "304 response must have empty body");
}
