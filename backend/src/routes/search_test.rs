use super::*;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use tantivy::doc;
use tower::ServiceExt;

/// Build a test `SearchState` with an in-memory SQLite connection pool, a
/// temporary Tantivy index, and no seeded data.
///
/// Returns the `TempDir` guard so the on-disk Tantivy index lives as
/// long as the test.
fn test_state() -> (tempfile::TempDir, Arc<SearchState>) {
    let dir = tempfile::tempdir().expect("tempdir");

    let pool = crate::db::pool::create_in_memory_pool();
    {
        let mut conn = pool.get().expect("Failed to get connection for migrations");
        crate::db::migrations::run_migrations(&mut conn).expect("Failed to run migrations");
    }

    let index_manager = IndexManager::open_or_create(&dir.path().join("tantivy"), 50_000_000)
        .expect("IndexManager");

    let state = Arc::new(SearchState { index_manager: Arc::new(index_manager), db: pool });

    (dir, state)
}

/// Parse an ISO 8601 string to a Tantivy DateTime for indexing in tests.
fn parse_date(ts: &str) -> tantivy::DateTime {
    let dt: chrono::DateTime<chrono::Utc> = ts.parse().expect("parse ISO 8601");
    tantivy::DateTime::from_timestamp_secs(dt.timestamp())
}

/// Seed a media item in both SQLite and the Tantivy index.
async fn seed_item(
    state: &Arc<SearchState>,
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
        let conn = state.db.get().expect("Failed to get DB connection");
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
    let schema = state.index_manager.schema();
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

    state.index_manager.add_document(doc).expect("add doc");
    state.index_manager.commit().expect("commit");
}

// -----------------------------------------------------------------------
// Happy path
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_search_basic_query() {
    let (_dir, state) = test_state();

    // Items with searchable metadata
    seed_item(
        &state,
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
        &state,
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
        &state,
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

    let app = routes().with_state(state);

    // Search for "dragon" — should match via metadata_json
    let response = app
        .oneshot(Request::builder().uri("/search?q=dragon").body(Body::empty()).unwrap())
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

    // Check meta
    assert_eq!(body["meta"]["query"], "dragon");
    assert_eq!(body["meta"]["total"], 1);
    assert_eq!(body["meta"]["has_more"], false);
    assert!(body["meta"]["next_cursor"].is_null());
}

#[tokio::test]
async fn test_search_filename_prefix() {
    let (_dir, state) = test_state();

    seed_item(
        &state,
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
        &state,
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

    let app = routes().with_state(state);

    // Search using field-scoped syntax to match the exact filename STRING term
    let response = app
        .oneshot(
            Request::builder().uri("/search?q=filename:dragon.png").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 1, "should match filename:dragon.png");
}

// -----------------------------------------------------------------------
// Empty / missing query
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_search_empty_query_returns_400() {
    let (_dir, state) = test_state();
    let app = routes().with_state(state);

    // Missing q entirely
    let response = app
        .clone()
        .oneshot(Request::builder().uri("/search").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["error"].as_str().unwrap().contains("required"));

    // q present but blank
    let response = app
        .oneshot(Request::builder().uri("/search?q=").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["error"].as_str().unwrap().contains("empty"));
}

// -----------------------------------------------------------------------
// No matches
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_search_no_matches() {
    let (_dir, state) = test_state();

    seed_item(
        &state,
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

    let app = routes().with_state(state);

    let response = app
        .oneshot(
            Request::builder().uri("/search?q=nonexistent_term_xyz").body(Body::empty()).unwrap(),
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

// -----------------------------------------------------------------------
// Limit & default
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_search_default_limit() {
    let (_dir, state) = test_state();

    // Insert many items with the same metadata term so all match
    for i in 0..50 {
        let id = format!("uuid-item-{i:04}");
        seed_item(
            &state,
            &id,
            &format!("item_{i}.png"),
            &format!("items/item_{i}.png"),
            "image/png",
            r#"{"tag":"common"}"#,
            Some(100),
            Some(100),
            1024,
            "2026-03-01T10:00:00Z",
        )
        .await;
    }

    let app = routes().with_state(state);

    // No limit param — should default to 100, returning all 50 items
    let response = app
        .oneshot(Request::builder().uri("/search?q=common").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 50, "default limit should be 100, so all 50 fit");
    assert_eq!(body["meta"]["total"], 50);
    assert_eq!(body["meta"]["has_more"], false);
}

#[tokio::test]
async fn test_search_limit_capped_at_500() {
    let (_dir, state) = test_state();

    // Insert 600 items with matching metadata
    for i in 0..600 {
        let id = format!("uuid-cap-{i:04}");
        seed_item(
            &state,
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

    let app = routes().with_state(state);

    // Request limit=500 — max valid limit, exercises +1 has_more check
    let response = app
        .oneshot(Request::builder().uri("/search?q=capped&limit=500").body(Body::empty()).unwrap())
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

// -----------------------------------------------------------------------
// Pagination cursor
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_search_pagination_has_more() {
    let (_dir, state) = test_state();

    // Insert 15 items, search with limit=10
    for i in 0..15 {
        let id = format!("uuid-page-{i:04}");
        seed_item(
            &state,
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

    let app = routes().with_state(state);

    let response = app
        .oneshot(
            Request::builder().uri("/search?q=paginated&limit=10").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 10);
    assert_eq!(body["meta"]["total"], 10);
    assert_eq!(body["meta"]["has_more"], true);
    assert!(body["meta"]["next_cursor"].is_string());
    assert!(body["meta"]["next_cursor_id"].is_string());
}

// -----------------------------------------------------------------------
// Edge cases
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_search_item_deleted_from_db_after_index() {
    let (_dir, state) = test_state();

    // Index two items, then delete one from SQLite only
    seed_item(
        &state,
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
        &state,
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

    // Remove the "deleted" item from SQLite only
    {
        let conn = state.db.get().expect("Failed to get DB connection");
        conn.execute("DELETE FROM media_items WHERE id = 'uuid-deleted'", []).expect("delete");
    }

    let app = routes().with_state(state);

    // Search should still work — deleted item is silently skipped
    let response = app
        .oneshot(Request::builder().uri("/search?q=prompt").body(Body::empty()).unwrap())
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
async fn test_search_invalid_query_syntax() {
    let (_dir, state) = test_state();
    let app = routes().with_state(state);

    // Tantivy QueryParser may reject malformed queries with special chars
    let response = app
        .oneshot(
            Request::builder().uri("/search?q=invalid///syntax***").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();

    // Tantivy is generally permissive, but this should not produce a 500
    assert!(
        response.status() == StatusCode::OK || response.status() == StatusCode::BAD_REQUEST,
        "malformed query should return either 200 or 400, never 500"
    );
}

// -----------------------------------------------------------------------
// MIME type filter
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_search_filter_by_image_mime_type() {
    let (_dir, state) = test_state();

    seed_item(
        &state,
        "img-0001",
        "dragon.png",
        "dragon.png",
        "image/png",
        r#"{"tag":"creature"}"#,
        Some(1024),
        Some(768),
        20480,
        "2026-03-01T10:00:00Z",
    )
    .await;

    seed_item(
        &state,
        "vid-0001",
        "video.mp4",
        "video.mp4",
        "video/mp4",
        r#"{"tag":"creature"}"#,
        None,
        None,
        51200,
        "2026-03-02T10:00:00Z",
    )
    .await;

    let app = routes().with_state(state);

    // Search for "creature" filtered to images only
    let response = app
        .oneshot(
            Request::builder()
                .uri("/search?q=creature&mime_type=image/%")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 1, "should return only the image item");
    assert_eq!(data[0]["id"], "img-0001");
    assert_eq!(data[0]["mime_type"], "image/png");
}

#[tokio::test]
async fn test_search_filter_by_video_mime_type() {
    let (_dir, state) = test_state();

    seed_item(
        &state,
        "img-0002",
        "castle.png",
        "castle.png",
        "image/png",
        r#"{"tag":"building"}"#,
        Some(800),
        Some(600),
        15360,
        "2026-03-01T10:00:00Z",
    )
    .await;

    seed_item(
        &state,
        "vid-0002",
        "movie.webm",
        "movie.webm",
        "video/webm",
        r#"{"tag":"building"}"#,
        None,
        None,
        102400,
        "2026-03-02T10:00:00Z",
    )
    .await;

    let app = routes().with_state(state);

    // Search for "building" filtered to videos only
    let response = app
        .oneshot(
            Request::builder()
                .uri("/search?q=building&mime_type=video/%")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 1, "should return only the video item");
    assert_eq!(data[0]["id"], "vid-0002");
    assert_eq!(data[0]["mime_type"], "video/webm");
}

#[tokio::test]
async fn test_search_mime_type_filter_no_match() {
    let (_dir, state) = test_state();

    seed_item(
        &state,
        "img-0003",
        "forest.png",
        "forest.png",
        "image/png",
        r#"{"tag":"nature"}"#,
        Some(1920),
        Some(1080),
        30720,
        "2026-03-01T10:00:00Z",
    )
    .await;

    let app = routes().with_state(state);

    // Search tagged "nature" but filter to videos — no results expected
    let response = app
        .oneshot(
            Request::builder()
                .uri("/search?q=nature&mime_type=video/%")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 0, "no videos match the 'nature' tag");
}

#[tokio::test]
async fn test_search_mime_type_filter_all_types() {
    let (_dir, state) = test_state();

    seed_item(
        &state,
        "img-0004",
        "lake.png",
        "lake.png",
        "image/png",
        r#"{"tag":"water"}"#,
        Some(640),
        Some(480),
        8192,
        "2026-03-01T10:00:00Z",
    )
    .await;

    seed_item(
        &state,
        "vid-0004",
        "river.mp4",
        "river.mp4",
        "video/mp4",
        r#"{"tag":"water"}"#,
        None,
        None,
        20480,
        "2026-03-02T10:00:00Z",
    )
    .await;

    let app = routes().with_state(state);

    // No mime_type filter = both results returned
    let response = app
        .oneshot(Request::builder().uri("/search?q=water").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 2, "no filter should return both image and video");
}

// -----------------------------------------------------------------------
// Sort order
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_search_default_sort_is_recency() {
    let (_dir, state) = test_state();

    // Item A: older, higher BM25 score (many "dragon" occurrences)
    seed_item(
        &state,
        "uuid-old",
        "old.png",
        "old.png",
        "image/png",
        r#"{"prompt":"dragon dragon dragon dragon"}"#,
        Some(100),
        Some(100),
        1024,
        "2026-01-01T10:00:00Z",
    )
    .await;

    // Item B: newer, lower BM25 score (single "dragon" occurrence)
    seed_item(
        &state,
        "uuid-new",
        "new.png",
        "new.png",
        "image/png",
        r#"{"prompt":"dragon"}"#,
        Some(200),
        Some(200),
        2048,
        "2026-03-01T10:00:00Z",
    )
    .await;

    let app = routes().with_state(state);

    // Default sort (no sort param) = recency — newest first
    let response = app
        .oneshot(Request::builder().uri("/search?q=dragon").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    let data = body["data"].as_array().unwrap();

    assert_eq!(data.len(), 2, "both items should match");
    assert_eq!(data[0]["id"], "uuid-new", "recency sort should put newer item first");
    assert_eq!(data[1]["id"], "uuid-old", "recency sort should put older item second");
}

#[tokio::test]
async fn test_search_sort_explicit_recency() {
    let (_dir, state) = test_state();

    seed_item(
        &state,
        "uuid-first",
        "first.png",
        "first.png",
        "image/png",
        r#"{"tag":"explicit"}"#,
        None,
        None,
        512,
        "2026-02-01T10:00:00Z",
    )
    .await;

    seed_item(
        &state,
        "uuid-second",
        "second.png",
        "second.png",
        "image/png",
        r#"{"tag":"explicit"}"#,
        None,
        None,
        1024,
        "2026-05-01T10:00:00Z",
    )
    .await;

    seed_item(
        &state,
        "uuid-third",
        "third.png",
        "third.png",
        "image/png",
        r#"{"tag":"explicit"}"#,
        None,
        None,
        2048,
        "2027-01-01T10:00:00Z",
    )
    .await;

    let app = routes().with_state(state);

    let response = app
        .oneshot(
            Request::builder().uri("/search?q=explicit&sort=recency").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    let data = body["data"].as_array().unwrap();

    assert_eq!(data.len(), 3);
    assert_eq!(data[0]["id"], "uuid-third", "newest first");
    assert_eq!(data[1]["id"], "uuid-second", "middle");
    assert_eq!(data[2]["id"], "uuid-first", "oldest last");
}

#[tokio::test]
async fn test_search_sort_by_score_preserves_bm25_order() {
    let (_dir, state) = test_state();

    // Item with higher relevance (more term occurrences) but older date
    seed_item(
        &state,
        "uuid-high-score",
        "high_score.png",
        "high_score.png",
        "image/png",
        r#"{"prompt":"dragon dragon dragon dragon"}"#,
        Some(100),
        Some(100),
        1024,
        "2026-01-01T10:00:00Z",
    )
    .await;

    // Item with lower relevance but newer date
    seed_item(
        &state,
        "uuid-low-score",
        "low_score.png",
        "low_score.png",
        "image/png",
        r#"{"prompt":"dragon"}"#,
        Some(200),
        Some(200),
        2048,
        "2026-03-01T10:00:00Z",
    )
    .await;

    let app = routes().with_state(state);

    // sort=score should keep BM25 order: high-score first (more matches)
    let response = app
        .oneshot(Request::builder().uri("/search?q=dragon&sort=score").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    let data = body["data"].as_array().unwrap();

    assert_eq!(data.len(), 2);
    assert_eq!(data[0]["id"], "uuid-high-score", "score sort should put higher BM25 score first");
    assert_eq!(data[1]["id"], "uuid-low-score", "score sort puts lower score second");
}

// -----------------------------------------------------------------------
// B7: batch_get_media_items chunking
// -----------------------------------------------------------------------

#[test]
fn test_batch_get_media_items_chunks_above_limit() {
    let pool = crate::db::pool::create_in_memory_pool();
    {
        let mut conn = pool.get().expect("conn");
        crate::db::migrations::run_migrations(&mut conn).expect("migrations");
    }
    let conn = pool.get().expect("conn");

    // Insert 1100 items (more than one SQLITE_BIND_LIMIT chunk of 999).
    let item_count = 1100;
    for i in 0..item_count {
        conn.execute(
            "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, \
             file_created_at, file_modified_at) \
             VALUES (?1, ?2, ?3, 'image/png', 1024, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            rusqlite::params![
                format!("uuid-{i:04}"),
                format!("file_{i}.png"),
                format!("path/file_{i}.png"),
            ],
        )
        .expect("insert");
    }
    drop(conn);

    // Now query via batch_get_media_items with all 1100 IDs.
    let conn = pool.get().expect("conn");
    let ids: Vec<String> = (0..item_count).map(|i| format!("uuid-{i:04}")).collect();
    let results = super::batch_get_media_items(&conn, &ids, None).expect("batch query");

    assert_eq!(
        results.len(),
        item_count,
        "Should return all {item_count} items even when chunked"
    );

    // Verify each expected ID is present.
    let result_ids: std::collections::HashSet<String> =
        results.into_iter().map(|r| r.id).collect();
    for i in 0..item_count {
        let expected = format!("uuid-{i:04}");
        assert!(
            result_ids.contains(&expected),
            "Missing expected ID: {expected}"
        );
    }
}

#[test]
fn test_batch_get_media_items_empty_ids() {
    let pool = crate::db::pool::create_in_memory_pool();
    let conn = pool.get().expect("conn");
    let results = super::batch_get_media_items(&conn, &[], None).expect("empty batch");
    assert!(results.is_empty(), "Empty input should produce empty results");
}
