//! Full-text search endpoint — `/api/v1/search`
//!
//! Searches the Tantivy index for media items matching a query string against
//! the `metadata_json` (TEXT) and `filename` (STRING) fields.  Results are
//! enriched with full records from SQLite and returned in the same list-view
//! format as the media listing endpoint.
//!
//! # Cursor Pagination
//!
//! Tantivy score ordering is inherently versioned, so cursors are best-effort:
//! we fetch `limit + 1` documents and return the last item's `created_at` /
//! `id` as the cursor.  Subsequent requests that include these cursors are
//! **not** re-applied to the Tantivy query (Tantivy does not natively support
//! cursor-based pagination across score-ordered results); the cursors are
//! provided so that the caller can implement client-side offset if needed.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::Value as TantivyValue;
use tokio::sync::Mutex;

use crate::search::IndexManager;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Shared application state for the search endpoint.
pub struct SearchState {
    pub index_manager: Arc<IndexManager>,
    pub db: Arc<Mutex<rusqlite::Connection>>,
}

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

/// Search query parameters.
#[derive(Deserialize, Default)]
#[allow(dead_code)]
struct SearchParams {
    q: Option<String>,
    /// Maximum items per page (default 100, max 500).
    #[serde(default = "default_limit")]
    limit: u32,
    /// Opaque cursor for pagination (ISO 8601 date of last item).
    cursor: Option<String>,
    /// Tiebreaker cursor: UUID of the last item.
    cursor_id: Option<String>,
}

fn default_limit() -> u32 {
    100
}

// ---------------------------------------------------------------------------
// Route factory
// ---------------------------------------------------------------------------

pub fn routes() -> Router<Arc<SearchState>> {
    Router::new().route("/search", get(search_handler))
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// GET /api/v1/search?q=<query>&limit=<n>&cursor=<cursor>&cursor_id=<id>
async fn search_handler(
    State(state): State<Arc<SearchState>>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let query_str = match &params.q {
        Some(q) if !q.trim().is_empty() => q.trim().to_string(),
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Query parameter 'q' is required and must not be empty"})),
            ));
        }
    };

    let limit = (params.limit.min(500)) as usize;

    // ---- Search Tantivy ----
    let schema = state.index_manager.schema();
    let reader = state.index_manager.reader();
    let searcher = reader.searcher();

    let metadata_json_field = schema.get_field("metadata_json").map_err(|e| {
        tracing::error!(error = %e, "Missing metadata_json field in Tantivy schema");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Internal server error"})),
        )
    })?;
    let filename_field = schema.get_field("filename").map_err(|e| {
        tracing::error!(error = %e, "Missing filename field in Tantivy schema");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Internal server error"})),
        )
    })?;

    let query_parser = QueryParser::for_index(state.index_manager.index(), vec![
        metadata_json_field,
        filename_field,
    ]);

    let query = match query_parser.parse_query(&query_str) {
        Ok(q) => q,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("Invalid query: {}", e)})),
            ));
        }
    };

    let collector = TopDocs::with_limit(limit + 1).order_by_score();
    let top_docs = match searcher.search(&query, &collector) {
        Ok(docs) => docs,
        Err(e) => {
            tracing::error!(error = %e, "Tantivy search failed");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Search failed"})),
            ));
        }
    };

    // ---- Resolve Tantivy hits → SQLite records ----
    let has_more = top_docs.len() > limit;
    let docs = &top_docs[..top_docs.len().min(limit)];

    let id_field = schema.get_field("id").unwrap();
    let mut media_items: Vec<MediaItemSummary> = Vec::with_capacity(docs.len());

    for (_score, doc_address) in docs {
        let tantivy_doc: tantivy::TantivyDocument = match searcher.doc(*doc_address) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to retrieve Tantivy document");
                continue;
            }
        };

        let item_id = tantivy_doc
            .get_first(id_field)
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if item_id.is_empty() {
            continue;
        }

        let db = state.db.lock().await;
        match get_media_item_by_id(&db, item_id) {
            Ok(Some(item)) => media_items.push(item),
            Ok(None) => { /* item deleted between Tantivy search and DB lookup */ }
            Err(e) => {
                tracing::warn!(error = %e, id = %item_id, "DB lookup failed for search hit");
            }
        }
    }

    let (next_cursor, next_cursor_id) = if has_more {
        if let Some(last) = media_items.last() {
            (Some(last.created_at.clone()), Some(last.id.clone()))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    Ok(Json(json!({
        "data": media_items,
        "meta": {
            "next_cursor": next_cursor,
            "next_cursor_id": next_cursor_id,
            "has_more": has_more,
            "total": media_items.len() as u64,
            "query": query_str,
        }
    })))
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Lightweight media item returned in search results.
///
/// Matches the list-view format defined in the API contract so that the
/// frontend can reuse the same rendering components.
#[derive(serde::Serialize)]
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

// ---------------------------------------------------------------------------
// Database helpers
// ---------------------------------------------------------------------------

/// Fetch a single media item summary from SQLite by UUID.
///
/// Returns `Ok(None)` when the id does not exist (e.g. deleted between
/// Tantivy search and DB lookup).
fn get_media_item_by_id(
    db: &rusqlite::Connection,
    id: &str,
) -> Result<Option<MediaItemSummary>, rusqlite::Error> {
    let mut stmt = db.prepare(
        "SELECT id, filename, relative_path, mime_type, width, height, file_size, \
                file_created_at, file_modified_at
         FROM media_items WHERE id = ?1",
    )?;

    let mut rows = stmt.query_map(rusqlite::params![id], |row| {
        Ok(MediaItemSummary {
            id: row.get(0)?,
            filename: row.get(1)?,
            path: row.get(2)?,
            mime_type: row.get(3)?,
            width: row.get(4)?,
            height: row.get(5)?,
            file_size: row.get(6)?,
            created_at: row.get(7)?,
            modified_at: row.get(8)?,
            thumbnail_url: format!("/api/v1/media/{}/thumbnail", row.get::<_, String>(0)?),
        })
    })?;

    match rows.next() {
        Some(Ok(item)) => Ok(Some(item)),
        _ => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tantivy::doc;
    use tower::ServiceExt;

    /// Build a test `SearchState` with an in-memory SQLite database, a
    /// temporary Tantivy index, and no seeded data.
    ///
    /// Returns the `TempDir` guard so the on-disk Tantivy index lives as
    /// long as the test.
    fn test_state() -> (tempfile::TempDir, Arc<SearchState>) {
        let dir = tempfile::tempdir().expect("tempdir");

        let mut conn = crate::db::open_in_memory()
            .expect("Failed to create in-memory database");
        crate::db::migrations::run_migrations(&mut conn)
            .expect("Failed to run migrations");

        let index_manager =
            IndexManager::open_or_create(&dir.path().join("tantivy")).expect("IndexManager");

        let state = Arc::new(SearchState {
            index_manager: Arc::new(index_manager),
            db: Arc::new(Mutex::new(conn)),
        });

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
            let db = state.db.lock().await;
            db.execute(
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
            .oneshot(
                Request::builder()
                    .uri("/search?q=dragon")
                    .body(Body::empty())
                    .unwrap(),
            )
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
                Request::builder()
                    .uri("/search?q=filename:dragon.png")
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
            .oneshot(
                Request::builder()
                    .uri("/search?q=")
                    .body(Body::empty())
                    .unwrap(),
            )
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
                Request::builder()
                    .uri("/search?q=nonexistent_term_xyz")
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
            .oneshot(
                Request::builder()
                    .uri("/search?q=common")
                    .body(Body::empty())
                    .unwrap(),
            )
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

        // Request limit=1000 — should be capped at 500+1 (for has_more check)
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/search?q=capped&limit=1000")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();

        let data = body["data"].as_array().unwrap();
        assert_eq!(
            data.len(),
            500,
            "limit should be capped at 500 items per page"
        );
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
                Request::builder()
                    .uri("/search?q=paginated&limit=10")
                    .body(Body::empty())
                    .unwrap(),
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
            let db = state.db.lock().await;
            db.execute("DELETE FROM media_items WHERE id = 'uuid-deleted'", [])
                .expect("delete");
        }

        let app = routes().with_state(state);

        // Search should still work — deleted item is silently skipped
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/search?q=prompt")
                    .body(Body::empty())
                    .unwrap(),
            )
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
                Request::builder()
                    .uri("/search?q=invalid///syntax***")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Tantivy is generally permissive, but this should not produce a 500
        assert!(
            response.status() == StatusCode::OK
                || response.status() == StatusCode::BAD_REQUEST,
            "malformed query should return either 200 or 400, never 500"
        );
    }
}
