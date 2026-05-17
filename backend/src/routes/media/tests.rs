use super::*;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::NaiveDateTime;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::thumbnails::limiter::ThumbnailLimiter;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

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
async fn test_thumbnail_populates_thumbnail_path() {
    let (state, _cache_dir) = test_state();
    let watched = tempfile::tempdir().unwrap();
    let source_path = watched.path().join("test.png");
    create_test_png(&source_path);

    seed_config(&state, watched.path()).await;
    seed_media_item(
        &state,
        "00000000-0000-0000-0000-000000000010",
        "test.png",
        "test.png",
        "image/png",
        "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
    )
    .await;

    let app = routes().with_state(state.clone());

    // Request thumbnail generation
    let response = app
        .oneshot(
            Request::builder()
                .uri("/media/00000000-0000-0000-0000-000000000010/thumbnail")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify thumbnail_path was populated in the database
    let conn = state.db.get().unwrap();
    let thumb_path: Option<String> = conn
        .query_row(
            "SELECT thumbnail_path FROM media_items WHERE id = '00000000-0000-0000-0000-000000000010'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(
        thumb_path.is_some(),
        "thumbnail_path should be populated after generation"
    );
    let path = thumb_path.unwrap();
    assert!(
        !path.is_empty(),
        "thumbnail_path should be a non-empty string"
    );
    assert!(
        path.contains("abcdef1234567890_200.webp"),
        "thumbnail_path should point to the content-addressed cache file"
    );
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

    let content_disposition =
        response.headers().get(header::CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()).unwrap();
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
    let base =
        NaiveDateTime::parse_from_str(base_date, "%Y-%m-%dT%H:%M:%S").expect("Invalid base date");

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

    let response =
        app.oneshot(Request::builder().uri("/media").body(Body::empty()).unwrap()).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(body["data"].as_array().unwrap().len(), 0, "empty DB should return empty data");
    assert_eq!(body["meta"]["has_more"], false, "empty DB should have has_more=false");
    assert_eq!(body["meta"]["total"], 0, "empty DB should have total=0");
    assert!(body["meta"]["next_cursor"].is_null(), "empty DB should have null next_cursor");
    assert!(body["meta"]["next_cursor_id"].is_null(), "empty DB should have null next_cursor_id");
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
    assert!(body["meta"]["next_cursor_id"].is_string(), "has_more=true should have next_cursor_id");

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
        serde_json::from_slice(&response1.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let page1_ids: Vec<&str> =
        body1["data"].as_array().unwrap().iter().map(|item| item["id"].as_str().unwrap()).collect();
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
        serde_json::from_slice(&response2.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let page2_ids: Vec<&str> =
        body2["data"].as_array().unwrap().iter().map(|item| item["id"].as_str().unwrap()).collect();

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
