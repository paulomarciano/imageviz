//! Integration tests for the config endpoint (`GET /api/v1/config`, `PUT /api/v1/config`).
//!
//! These tests exercise the full pipeline: route mounting → request routing →
//! SQLite persistence. The co-located unit tests in `routes/config.rs` cover
//! handler-level edge cases; this suite validates end-to-end behaviour through
//! the production router.

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Happy path: empty config
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_config_empty_on_fresh_database() {
    let app = common::create_test_app_with_search();

    let response = app
        .router
        .oneshot(Request::builder().uri("/api/v1/config").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    let folders = body["watched_folders"].as_array().unwrap();
    assert!(folders.is_empty(), "fresh database should have no watched folders");
}

// ---------------------------------------------------------------------------
// Happy path: PUT then GET roundtrip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_config_put_and_get_roundtrip() {
    let app = common::create_test_app_with_search();

    let input = json!({
        "watched_folders": [
            {"path": "/tmp/media", "label": "Media folder"},
            {"path": "/home/user/images"}
        ]
    });

    // PUT config
    let put_response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/v1/config")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&input).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(put_response.status(), StatusCode::OK);

    // GET config — should match what we PUT
    let get_response = app
        .router
        .oneshot(Request::builder().uri("/api/v1/config").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(get_response.status(), StatusCode::OK);

    let body_bytes = get_response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    let folders = body["watched_folders"].as_array().unwrap();
    assert_eq!(folders.len(), 2, "should have 2 folders after PUT");
    assert_eq!(folders[0]["path"], "/tmp/media");
    assert_eq!(folders[0]["label"], "Media folder");
    assert_eq!(folders[1]["path"], "/home/user/images");
    // Second folder has no label — should be null or absent
    assert!(folders[1].get("label").is_none() || folders[1]["label"].is_null());
}

// ---------------------------------------------------------------------------
// Validation: empty path rejected
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_config_empty_path_rejected() {
    let app = common::create_test_app_with_search();

    let input = json!({
        "watched_folders": [
            {"path": "   "}
        ]
    });

    let response = app
        .router
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/v1/config")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&input).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(
        body["error"], "Invalid watched folder configuration",
        "top-level error should indicate the config is invalid",
    );
    assert_eq!(
        body["details"][0]["message"], "Path must not be empty",
        "details should indicate which field failed",
    );
}

// ---------------------------------------------------------------------------
// Idempotency: PUT same config twice yields no change
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_config_put_idempotent() {
    let app = common::create_test_app_with_search();

    let input = json!({
        "watched_folders": [
            {"path": "/persistent/path"}
        ]
    });

    let body = serde_json::to_vec(&input).unwrap();

    // First PUT
    let r1 = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/v1/config")
                .header("content-type", "application/json")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r1.status(), StatusCode::OK);

    // Second PUT with same data
    let r2 = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/v1/config")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r2.status(), StatusCode::OK);

    // GET — should still have 1 folder, not duplicated
    let get_response = app
        .router
        .oneshot(Request::builder().uri("/api/v1/config").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body_bytes = get_response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    let folders = body["watched_folders"].as_array().unwrap();
    assert_eq!(folders.len(), 1, "PUT should replace, not append");
}

// ---------------------------------------------------------------------------
// Removal: PUT removing a folder that has indexed media must succeed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_config_put_removing_folder_with_media_returns_200() {
    let app = common::create_test_app_with_search();

    // Seed a folder with media rows referencing it (FK enforcement is ON in
    // this pool — media_items.folder_id references watched_folders.id).
    {
        let conn = app.pool.get().unwrap();
        conn.execute("INSERT INTO watched_folders (id, path) VALUES ('fid-gone', '/gone')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, \
             folder_id, file_created_at, file_modified_at) \
             VALUES ('m-1', 'a.png', 'a.png', 'image/png', 1, 'fid-gone', \
             '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    }

    // PUT a config that no longer contains /gone.
    let input = json!({ "watched_folders": [{"path": "/kept"}] });
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/v1/config")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&input).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "removing a media-bearing folder must not fail the FK constraint"
    );

    // The folder and its media rows are removed together.
    let conn = app.pool.get().unwrap();
    let folders: i64 = conn
        .query_row("SELECT COUNT(*) FROM watched_folders WHERE path = '/gone'", [], |r| r.get(0))
        .unwrap();
    let items: i64 = conn
        .query_row("SELECT COUNT(*) FROM media_items WHERE folder_id = 'fid-gone'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!((folders, items), (0, 0), "folder and its media rows must be removed together");
}
