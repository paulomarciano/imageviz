use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use r2d2::Pool;

use imageviz_backend::db::SqliteConnectionManager;
use std::sync::Arc;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

use imageviz_backend::routes::media::{MediaState, routes};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Build a test app with an in-memory connection pool and a temporary cache directory.
///
/// The returned router has:
/// - `GET /api/v1/health` (from the app factory)
/// - `GET /api/v1/media/{id}/file`
/// - `GET /api/v1/media/{id}/thumbnail`
///
/// The database contains only the schema — no seeded data. Callers must
/// call `seed_config` and `seed_media_item` to populate test data.
fn create_media_test_app() -> (Router, Arc<MediaState>, tempfile::TempDir) {
    create_media_test_app_with_permits(16)
}

/// Variant of [`create_media_test_app`] whose thumbnail limiter issues only
/// `max_permits` permits — used by tests that assert on semaphore behavior.
fn create_media_test_app_with_permits(
    max_permits: usize,
) -> (Router, Arc<MediaState>, tempfile::TempDir) {
    let pool: Pool<SqliteConnectionManager> = imageviz_backend::db::pool::create_in_memory_pool();
    {
        let mut conn = pool.get().expect("Failed to get connection for migrations");
        imageviz_backend::db::migrations::run_migrations(&mut conn)
            .expect("Failed to run migrations");
    }

    let cache_dir = tempfile::tempdir().expect("Failed to create cache directory");
    let media_state = Arc::new(MediaState {
        db: pool,
        thumbnail_cache_dir: cache_dir.path().to_path_buf(),
        thumbnail_limiter: Arc::new(imageviz_backend::thumbnails::limiter::ThumbnailLimiter::new(
            max_permits,
        )),
        total_count_cache: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
    });

    let app = imageviz_backend::health_router()
        .nest("/api/v1", routes().with_state(Arc::clone(&media_state)))
        .layer(CorsLayer::permissive());

    (app, media_state, cache_dir)
}

/// Stable watched-folder id used by the seed helpers.
const SEED_FOLDER_ID: &str = "fid-test";

/// Seed a single watched folder (in the `watched_folders` table — the single
/// source of truth) pointing at `folder_path`.
async fn seed_config(state: &Arc<MediaState>, folder_path: &std::path::Path) {
    let conn = state.db.get().expect("Failed to get DB connection");
    conn.execute(
        "INSERT OR IGNORE INTO watched_folders (id, path) VALUES (?1, ?2)",
        rusqlite::params![SEED_FOLDER_ID, folder_path.to_str().unwrap()],
    )
    .expect("Failed to seed config");
}

/// Seed a single media item in the database, assigned to the seeded folder.
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
        "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at, checksum, folder_id)
         VALUES (?1, ?2, ?3, ?4, 1024, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', ?5, ?6)",
        rusqlite::params![id, filename, relative_path, mime_type, checksum, SEED_FOLDER_ID],
    )
    .expect("Failed to seed media item");
}

/// Create a small solid-colour PNG file at the given path (100 × 100 pixels).
fn create_test_png(path: &std::path::Path) {
    let img = image::RgbaImage::new(100, 100);
    img.save_with_format(path, image::ImageFormat::Png).expect("failed to create test PNG");
}

// ---------------------------------------------------------------------------
// Thumbnail endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_thumbnail_returns_webp_image() {
    let (app, state, _cache_dir) = create_media_test_app();
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

    let content_type =
        response.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap();
    assert_eq!(content_type, "image/webp");

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body_bytes.len() > 12, "WebP file too small");
    assert_eq!(&body_bytes[0..4], b"RIFF", "WebP must start with RIFF header");
    assert_eq!(&body_bytes[8..12], b"WEBP", "WebP must contain WEBP identifier at bytes 8-11");
}

#[tokio::test]
async fn test_thumbnail_returns_404_for_invalid_id() {
    let (app, _state, _cache_dir) = create_media_test_app();

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
    let (app, state, _cache_dir) = create_media_test_app();
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

    let content_type =
        response.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap();
    assert_eq!(content_type, "image/webp");
}

// ---------------------------------------------------------------------------
// Thumbnail limiter-scope tests (wave 8.9 / review P5)
// ---------------------------------------------------------------------------

/// A cache hit must be served without consuming a generation permit.
///
/// The limiter's only permit is held by the test itself; a cached thumbnail
/// must still be served. (The pre-8.9 handler acquired a permit *before* the
/// cache lookup, so this request would block until the 120 s limiter timeout.)
#[tokio::test]
async fn test_thumbnail_cache_hit_bypasses_generation_limiter() {
    let (app, state, cache_dir) = create_media_test_app_with_permits(1);
    let watched = tempfile::tempdir().unwrap();
    let source_path = watched.path().join("test.png");
    create_test_png(&source_path);

    let checksum = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
    seed_config(&state, watched.path()).await;
    seed_media_item(
        &state,
        "00000000-0000-0000-0000-000000000003",
        "test.png",
        "test.png",
        "image/png",
        checksum,
    )
    .await;

    // Pre-populate the cache so the request is a guaranteed hit.
    let cached_path = cache_dir.path().join(format!("{}_200.webp", &checksum[..16]));
    std::fs::write(&cached_path, b"RIFF0000WEBP").unwrap();

    // Hold the only permit — the request must not need it.
    let permit = state.thumbnail_limiter.acquire().await.unwrap();
    assert_eq!(state.thumbnail_limiter.available_permits(), 0);

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        app.oneshot(
            Request::builder()
                .uri("/api/v1/media/00000000-0000-0000-0000-000000000003/thumbnail")
                .body(Body::empty())
                .unwrap(),
        ),
    )
    .await
    .expect("cache hit must not block on the generation limiter")
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Header contract must be identical on the hit path (wave 8.9).
    let content_type =
        response.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap();
    assert_eq!(content_type, "image/webp");
    let cache_control =
        response.headers().get(header::CACHE_CONTROL).and_then(|v| v.to_str().ok()).unwrap();
    assert_eq!(cache_control, "public, max-age=31536000, immutable");

    // The permit was held throughout: the hit cannot have consumed one.
    assert_eq!(
        state.thumbnail_limiter.available_permits(),
        0,
        "cache hit must not consume or release a generation permit"
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body, &b"RIFF0000WEBP"[..], "cached file must be served verbatim");

    drop(permit);
}

/// A cache miss must remain bounded by the limiter: while the only permit is
/// held, the request stays pending; once released, it completes and the
/// permit is returned.
#[tokio::test]
async fn test_thumbnail_cache_miss_waits_for_generation_permit() {
    let (app, state, _cache_dir) = create_media_test_app_with_permits(1);
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
        "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
    )
    .await;

    let permit = state.thumbnail_limiter.acquire().await.unwrap();

    let request_task = tokio::spawn(
        app.oneshot(
            Request::builder()
                .uri("/api/v1/media/00000000-0000-0000-0000-000000000004/thumbnail")
                .body(Body::empty())
                .unwrap(),
        ),
    );

    // Give the handler time to reach the (blocked) permit acquisition.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(
        !request_task.is_finished(),
        "cache miss must wait for a generation permit while none is available"
    );

    drop(permit);

    let response = tokio::time::timeout(std::time::Duration::from_secs(10), request_task)
        .await
        .expect("request must complete after the permit is released")
        .expect("request task must not panic")
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        state.thumbnail_limiter.available_permits(),
        1,
        "permit must be returned once generation finishes"
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[0..4], b"RIFF", "generated thumbnail must be valid WebP");
}

// ---------------------------------------------------------------------------
// File serving endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_file_streams_with_correct_content_type() {
    let (app, state, _cache_dir) = create_media_test_app();
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

    let content_type =
        response.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap();
    assert_eq!(content_type, "image/png");
}

#[tokio::test]
async fn test_file_returns_404_for_invalid_id() {
    let (app, _state, _cache_dir) = create_media_test_app();

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
    let (app, state, _cache_dir) = create_media_test_app();
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
    let content_range =
        response.headers().get(header::CONTENT_RANGE).and_then(|v| v.to_str().ok()).unwrap();
    assert!(content_range.starts_with("bytes "), "Content-Range should start with 'bytes '");
    assert!(content_range.contains('/'), "Content-Range should contain a slash");
}

#[tokio::test]
async fn test_file_range_not_satisfiable_416() {
    let (app, state, _cache_dir) = create_media_test_app();
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
    let (app, state, _cache_dir) = create_media_test_app();
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

    let accept_ranges =
        response.headers().get(header::ACCEPT_RANGES).and_then(|v| v.to_str().ok()).unwrap();
    assert_eq!(accept_ranges, "bytes");
}

#[tokio::test]
async fn test_file_etag_header() {
    let (app, state, _cache_dir) = create_media_test_app();
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
    let etag = response.headers().get(header::ETAG).and_then(|v| v.to_str().ok()).unwrap();
    assert_eq!(etag, "\"etagchecksum\"");
}

#[tokio::test]
async fn test_file_304_not_modified() {
    let (app, state, _cache_dir) = create_media_test_app();
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

    let etag =
        response.headers().get(header::ETAG).and_then(|v| v.to_str().ok()).unwrap().to_string();

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
