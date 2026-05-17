//! Integration tests for the stats endpoint (`GET /api/v1/stats`).
//!
//! These tests exercise the full pipeline: route mounting → SQLite queries →
//! ProgressTracker snapshot. The co-located unit tests in `routes/stats.rs`
//! cover handler-level edge cases; this suite validates end-to-end behaviour
//! through the production router.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Empty database
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_stats_empty_database_returns_zeros() {
    let app = common::create_test_app_with_search();

    let response = app
        .router
        .oneshot(Request::builder().uri("/api/v1/stats").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(body["total"], 0, "empty DB should have total=0");
    assert!(
        body["by_mime_type"].as_object().unwrap().is_empty(),
        "empty DB should have empty mime histogram"
    );
    assert_eq!(body["total_file_size"], 0, "empty DB should have total_file_size=0");
    assert!(body["last_indexed_at"].is_null(), "empty DB should have no indexed timestamp");

    // ProgressTracker defaults to Idle
    assert_eq!(body["indexing"]["status"], "Idle");
    assert_eq!(body["indexing"]["total"], 0);
    assert_eq!(body["indexing"]["processed"], 0);
    assert!(
        body["indexing"]["errors"].as_array().unwrap().is_empty(),
        "empty DB should have no indexing errors"
    );
}

// ---------------------------------------------------------------------------
// Database with seeded data
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_stats_with_seeded_data() {
    let app = common::create_test_app_with_search();

    // Seed some media items directly into the DB
    {
        let db = app.db.lock().await;
        // Two PNGs, one JPEG
        db.execute(
            "INSERT INTO media_items \
             (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at) \
             VALUES ('a', 'a.png', 'a.png', 'image/png', 500, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();
        db.execute(
            "INSERT INTO media_items \
             (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at) \
             VALUES ('b', 'b.png', 'b.png', 'image/png', 300, '2025-01-02T00:00:00Z', '2025-01-02T00:00:00Z')",
            [],
        ).unwrap();
        db.execute(
            "INSERT INTO media_items \
             (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at) \
             VALUES ('c', 'c.jpg', 'c.jpg', 'image/jpeg', 2000, '2025-01-03T00:00:00Z', '2025-01-03T00:00:00Z')",
            [],
        ).unwrap();
    }

    let response = app
        .router
        .oneshot(Request::builder().uri("/api/v1/stats").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(body["total"], 3, "should count 3 media items");
    assert_eq!(body["total_file_size"], 2800, "should sum file sizes (500+300+2000)");

    let by_mime = body["by_mime_type"].as_object().unwrap();
    assert_eq!(by_mime.len(), 2, "should have 2 MIME types");
    assert_eq!(by_mime.get("image/png").and_then(|v| v.as_u64()), Some(2), "should have 2 PNGs");
    assert_eq!(by_mime.get("image/jpeg").and_then(|v| v.as_u64()), Some(1), "should have 1 JPEG");

    // last_indexed_at should be set because indexed_at gets DEFAULT datetime('now')
    assert!(
        body["last_indexed_at"].is_string(),
        "last_indexed_at should be set: {:?}",
        body["last_indexed_at"]
    );

    // ProgressTracker should still report idle defaults (no indexing running)
    assert_eq!(body["indexing"]["status"], "Idle");
    assert_eq!(body["indexing"]["total"], 0);
    assert_eq!(body["indexing"]["processed"], 0);
    assert!(
        body["indexing"]["errors"].as_array().unwrap().is_empty(),
        "no indexing errors expected"
    );
}
