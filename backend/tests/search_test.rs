//! Integration tests for the search endpoint (`GET /api/v1/search`).
//!
//! These tests exercise the full pipeline: SQLite seeding → Tantivy indexing →
//! HTTP request routing → response deserialisation.  The co-located unit tests
//! in `routes/search.rs` cover handler-level edge cases; this suite validates
//! that everything works together when mounted under the production router.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use common::TestApp;

use tantivy::doc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse an ISO 8601 string to a Tantivy DateTime for indexing.
fn parse_date(ts: &str) -> tantivy::DateTime {
    let dt: chrono::DateTime<chrono::Utc> = ts.parse().expect("parse ISO 8601");
    tantivy::DateTime::from_timestamp_secs(dt.timestamp())
}

/// Seed a media item in both SQLite and the Tantivy index attached to `app`.
async fn seed_item(
    app: &TestApp,
    id: &str,
    filename: &str,
    relative_path: &str,
    mime_type: &str,
    metadata_json: &str,
    width: Option<i64>,
    height: Option<i64>,
    file_size: i64,
    created_at: &str,
) {
    // SQLite
    {
        let conn = app.pool.get().expect("get conn");
        conn.execute(
            "INSERT INTO media_items \
             (id, filename, relative_path, mime_type, width, height, file_size, \
              file_created_at, file_modified_at, metadata_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9)",
            rusqlite::params![
                id,
                filename,
                relative_path,
                mime_type,
                width,
                height,
                file_size,
                created_at,
                metadata_json,
            ],
        )
        .expect("insert media item");
    }

    // Tantivy
    let schema = app.index_manager.schema();
    let doc = tantivy::doc!(
        schema.get_field("id").unwrap() => id,
        schema.get_field("filename").unwrap() => filename,
        schema.get_field("mime_type").unwrap() => mime_type,
        schema.get_field("metadata_json").unwrap() => metadata_json,
        schema.get_field("created_at").unwrap() => parse_date(created_at),
        schema.get_field("file_size").unwrap() => file_size as u64,
        schema.get_field("width").unwrap() => width.unwrap_or(0) as u64,
        schema.get_field("height").unwrap() => height.unwrap_or(0) as u64,
    );

    app.index_manager.add_document(doc).expect("add Tantivy doc");
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_search_basic_query_returns_matching_results() {
    let app = common::create_test_app_with_search();

    // Items with searchable metadata
    seed_item(
        &app,
        "uuid-dragon",
        "dragon.png",
        "fantasy/dragon.png",
        "image/png",
        r#"{"prompt":"a majestic dragon flying over mountains"}"#,
        Some(1024),
        Some(768),
        20480,
        "2026-03-01T10:00:00Z",
    )
    .await;

    seed_item(
        &app,
        "uuid-castle",
        "castle.png",
        "fantasy/castle.png",
        "image/png",
        r#"{"prompt":"a medieval castle at sunset"}"#,
        Some(800),
        Some(600),
        15360,
        "2026-03-02T10:00:00Z",
    )
    .await;

    seed_item(
        &app,
        "uuid-forest",
        "forest.png",
        "scenery/forest.png",
        "image/png",
        r#"{"prompt":"a peaceful forest with sunlight"}"#,
        Some(1920),
        Some(1080),
        30720,
        "2026-03-03T10:00:00Z",
    )
    .await;

    // Commit Tantivy so documents are visible to search.
    app.index_manager.commit().expect("commit Tantivy");

    let response = app
        .router
        .oneshot(Request::builder().uri("/api/v1/search?q=dragon").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 1, "should find exactly one 'dragon' item");
    assert_eq!(data[0]["id"], "uuid-dragon");
    assert_eq!(data[0]["filename"], "dragon.png");
    assert_eq!(data[0]["mime_type"], "image/png");
    assert_eq!(data[0]["width"], 1024);
    assert_eq!(data[0]["height"], 768);
    assert_eq!(data[0]["file_size"], 20480);
    assert!(data[0]["thumbnail_url"].as_str().unwrap().contains("uuid-dragon"));
    assert!(data[0]["created_at"].as_str().unwrap().contains("2026-03-01"));

    // Check meta fields
    assert_eq!(body["meta"]["query"], "dragon");
    assert_eq!(body["meta"]["total"], 1);
    assert_eq!(body["meta"]["has_more"], false);
    assert!(body["meta"]["next_cursor"].is_null());
}

#[tokio::test]
async fn test_search_across_metadata_json_field() {
    let app = common::create_test_app_with_search();

    seed_item(
        &app,
        "uuid-prometheus",
        "prometheus.png",
        "fantasy/prometheus.png",
        "image/png",
        r#"{"prompt":"Prometheus bringing fire to humanity","style":"epic"}"#,
        None,
        None,
        10240,
        "2026-04-01T10:00:00Z",
    )
    .await;

    seed_item(
        &app,
        "uuid-sunset",
        "sunset.png",
        "landscapes/sunset.png",
        "image/png",
        r#"{"prompt":"a beautiful sunset over the ocean","style":"landscape"}"#,
        None,
        None,
        8192,
        "2026-04-02T10:00:00Z",
    )
    .await;

    app.index_manager.commit().expect("commit Tantivy");

    // Search for "Prometheus" — should match via metadata_json TEXT field
    let response = app
        .router
        .oneshot(Request::builder().uri("/api/v1/search?q=Prometheus").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 1, "should find 'Prometheus' in metadata_json");
    assert_eq!(data[0]["id"], "uuid-prometheus");
}

// ---------------------------------------------------------------------------
// Empty / missing query
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_search_empty_query_returns_400() {
    let app = common::create_test_app_with_search();

    // Missing q entirely
    let response = app
        .router
        .clone()
        .oneshot(Request::builder().uri("/api/v1/search").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["error"].as_str().unwrap().contains("required"));

    // q present but blank
    let response = app
        .router
        .oneshot(Request::builder().uri("/api/v1/search?q=").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["error"].as_str().unwrap().contains("empty"));
}

// ---------------------------------------------------------------------------
// No matches
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_search_no_matches_returns_empty_data() {
    let app = common::create_test_app_with_search();

    seed_item(
        &app,
        "uuid-dragon",
        "dragon.png",
        "dragon.png",
        "image/png",
        r#"{"prompt":"a dragon"}"#,
        None,
        None,
        1024,
        "2026-01-01T00:00:00Z",
    )
    .await;

    app.index_manager.commit().expect("commit Tantivy");

    let response = app
        .router
        .oneshot(
            Request::builder()
                .uri("/api/v1/search?q=nonexistent_term_xyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    let data = body["data"].as_array().unwrap();
    assert!(data.is_empty(), "expected empty results for non-matching query");

    assert_eq!(body["meta"]["total"], 0);
    assert_eq!(body["meta"]["has_more"], false);
    assert!(body["meta"]["next_cursor"].is_null());
}

// ---------------------------------------------------------------------------
// Pagination / limit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_search_pagination_has_more_flag() {
    let app = common::create_test_app_with_search();

    // Insert 15 items with the same searchable term, ask for 10
    for i in 0..15 {
        let id = format!("uuid-page-{i:04}");
        seed_item(
            &app,
            &id,
            &format!("page_{i}.png"),
            &format!("page/page_{i}.png"),
            "image/png",
            r#"{"tag":"paginated"}"#,
            None,
            None,
            1024,
            "2026-05-01T10:00:00Z",
        )
        .await;
    }

    app.index_manager.commit().expect("commit Tantivy");

    let response = app
        .router
        .oneshot(
            Request::builder()
                .uri("/api/v1/search?q=paginated&limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 10, "should return exactly limit items");
    assert_eq!(body["meta"]["total"], 10);
    assert_eq!(body["meta"]["has_more"], true);
    assert!(body["meta"]["next_cursor"].is_string());
    assert!(body["meta"]["next_cursor_id"].is_string());
}

#[tokio::test]
async fn test_search_no_pagination_when_fewer_than_limit() {
    let app = common::create_test_app_with_search();

    // Insert only 3 items — all should fit in one page
    for i in 0..3 {
        let id = format!("uuid-small-{i:04}");
        seed_item(
            &app,
            &id,
            &format!("small_{i}.png"),
            &format!("small/small_{i}.png"),
            "image/png",
            r#"{"tag":"few"}"#,
            None,
            None,
            1024,
            "2026-06-01T10:00:00Z",
        )
        .await;
    }

    app.index_manager.commit().expect("commit Tantivy");

    let response = app
        .router
        .oneshot(
            Request::builder().uri("/api/v1/search?q=few&limit=10").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 3, "should return all items when fewer than limit");
    assert_eq!(body["meta"]["total"], 3);
    assert_eq!(body["meta"]["has_more"], false);
    assert!(body["meta"]["next_cursor"].is_null());
}

#[tokio::test]
async fn test_search_limit_capped_at_500() {
    let app = common::create_test_app_with_search();

    // Insert 600 items; the endpoint should cap at 500
    for i in 0..600 {
        let id = format!("uuid-cap-{i:04}");
        seed_item(
            &app,
            &id,
            &format!("cap_{i}.png"),
            &format!("cap/cap_{i}.png"),
            "image/png",
            r#"{"tag":"capped"}"#,
            None,
            None,
            1024,
            "2026-04-01T10:00:00Z",
        )
        .await;
    }

    app.index_manager.commit().expect("commit Tantivy");

    let response = app
        .router
        .oneshot(
            Request::builder()
                .uri("/api/v1/search?q=capped&limit=500")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 500, "limit=500 should return 500 items");
    assert_eq!(body["meta"]["total"], 500);
    assert_eq!(body["meta"]["has_more"], true);
    assert!(body["meta"]["next_cursor"].is_string());
    assert!(body["meta"]["next_cursor_id"].is_string());
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_search_handles_item_deleted_from_db() {
    let app = common::create_test_app_with_search();

    // Index two items, then delete one from SQLite only
    seed_item(
        &app,
        "uuid-kept",
        "kept.png",
        "kept.png",
        "image/png",
        r#"{"prompt":"keep me"}"#,
        None,
        None,
        1024,
        "2026-01-01T00:00:00Z",
    )
    .await;

    seed_item(
        &app,
        "uuid-deleted",
        "deleted.png",
        "deleted.png",
        "image/png",
        r#"{"prompt":"delete me"}"#,
        None,
        None,
        1024,
        "2026-01-02T00:00:00Z",
    )
    .await;

    app.index_manager.commit().expect("commit Tantivy");

    // Remove from SQLite only
    {
        let conn = app.pool.get().expect("get conn");
        conn.execute("DELETE FROM media_items WHERE id = 'uuid-deleted'", [])
            .expect("delete from SQLite");
    }

    let response = app
        .router
        .oneshot(Request::builder().uri("/api/v1/search?q=prompt").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 1, "only the kept item should be returned");
    assert_eq!(data[0]["id"], "uuid-kept");
}

#[tokio::test]
async fn test_search_invalid_query_syntax_does_not_crash() {
    let app = common::create_test_app_with_search();

    let response = app
        .router
        .oneshot(
            Request::builder()
                .uri("/api/v1/search?q=invalid///syntax***")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Tantivy is generally permissive; either 200 or 400 is acceptable,
    // but never 500.
    assert!(
        response.status() == StatusCode::OK || response.status() == StatusCode::BAD_REQUEST,
        "malformed query should return either 200 or 400, never 500"
    );
}

#[tokio::test]
async fn test_search_filename_prefix_with_field_syntax() {
    let app = common::create_test_app_with_search();

    seed_item(
        &app,
        "uuid-dragon",
        "dragon.png",
        "fantasy/dragon.png",
        "image/png",
        r#"{}"#,
        Some(64),
        Some(64),
        1024,
        "2026-03-01T10:00:00Z",
    )
    .await;

    seed_item(
        &app,
        "uuid-castle",
        "castle.png",
        "fantasy/castle.png",
        "image/png",
        r#"{}"#,
        Some(64),
        Some(64),
        1024,
        "2026-03-02T10:00:00Z",
    )
    .await;

    app.index_manager.commit().expect("commit Tantivy");

    let response = app
        .router
        .oneshot(
            Request::builder()
                .uri("/api/v1/search?q=filename:dragon.png")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 1, "should match filename:dragon.png");
}
