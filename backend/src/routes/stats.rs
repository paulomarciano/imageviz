use axum::{Router, extract::State, http::StatusCode, response::Json, routing::get};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::indexer::progress::{IndexStatus, ProgressTracker};

/// Shared application state for the stats endpoint.
pub struct StatsState {
    pub db: Arc<Mutex<rusqlite::Connection>>,
    pub progress: Arc<ProgressTracker>,
}

pub fn routes() -> Router<Arc<StatsState>> {
    Router::new().route("/stats", get(get_stats))
}

/// Aggregate statistics about indexed media.
#[derive(Serialize)]
pub struct IndexStats {
    pub total: u64,
    pub by_mime_type: HashMap<String, u64>,
    pub total_file_size: u64,
    pub indexing: IndexingInfo,
}

/// Indexing status snapshot from the ProgressTracker.
#[derive(Serialize)]
pub struct IndexingInfo {
    pub status: String,
    pub total: usize,
    pub processed: usize,
    pub errors: Vec<String>,
}

/// GET /api/v1/stats — return aggregate index statistics.
async fn get_stats(
    State(state): State<Arc<StatsState>>,
) -> Result<Json<IndexStats>, (StatusCode, Json<Value>)> {
    let db = state.db.lock().await;

    // Total file count
    let total: u64 = db
        .query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0))
        .unwrap_or(0);

    // Total file size
    let total_file_size: u64 = db
        .query_row(
            "SELECT COALESCE(SUM(file_size), 0) FROM media_items",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // MIME type histogram
    let mut stmt = db
        .prepare(
            "SELECT mime_type, COUNT(*) as cnt FROM media_items \
             GROUP BY mime_type ORDER BY cnt DESC",
        )
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to prepare mime_type histogram query");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Internal server error"})),
            )
        })?;

    let by_mime_type: HashMap<String, u64> = stmt
        .query_map([], |row| {
            let mime: String = row.get(0)?;
            let count: u64 = row.get(1)?;
            Ok((mime, count))
        })
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to execute mime_type histogram query");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Internal server error"})),
            )
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Indexing status from the ProgressTracker
    let snapshot = state.progress.snapshot();
    let indexing = IndexingInfo {
        status: match snapshot.status {
            IndexStatus::Idle => "Idle",
            IndexStatus::Scanning => "Scanning",
            IndexStatus::Indexing => "Indexing",
            IndexStatus::Complete => "Complete",
        }
        .to_string(),
        total: snapshot.total,
        processed: snapshot.processed,
        errors: snapshot.errors,
    };

    Ok(Json(IndexStats {
        total,
        by_mime_type,
        total_file_size,
        indexing,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    /// Build a test `StatsState` with an in-memory SQLite database and a fresh
    /// ProgressTracker.  The database is pre-populated with the
    /// `media_items` table so queries return clean results.
    fn test_state() -> Arc<StatsState> {
        let conn = rusqlite::Connection::open_in_memory()
            .expect("Failed to create in-memory database");
        conn.execute_batch(
            "CREATE TABLE media_items (
                id TEXT PRIMARY KEY NOT NULL,
                filename TEXT NOT NULL,
                relative_path TEXT NOT NULL UNIQUE,
                mime_type TEXT NOT NULL,
                width INTEGER,
                height INTEGER,
                file_size INTEGER NOT NULL DEFAULT 0,
                thumbnail_path TEXT,
                file_created_at TEXT NOT NULL DEFAULT '',
                file_modified_at TEXT NOT NULL DEFAULT '',
                indexed_at TEXT NOT NULL DEFAULT (datetime('now')),
                metadata_json TEXT,
                checksum TEXT
            );",
        )
        .expect("Failed to create media_items table");

        let progress = Arc::new(ProgressTracker::new());

        Arc::new(StatsState {
            db: Arc::new(Mutex::new(conn)),
            progress,
        })
    }

    #[tokio::test]
    async fn test_stats_empty_database() {
        let state = test_state();
        let app = routes().with_state(state);

        let response = app
            .oneshot(Request::builder().uri("/stats").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(body["total"], 0);
        assert!(body["by_mime_type"].as_object().unwrap().is_empty());
        assert_eq!(body["total_file_size"], 0);

        // ProgressTracker defaults to Idle with zeros
        assert_eq!(body["indexing"]["status"], "Idle");
        assert_eq!(body["indexing"]["total"], 0);
        assert_eq!(body["indexing"]["processed"], 0);
        assert!(body["indexing"]["errors"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_stats_with_data() {
        let state = test_state();

        // Seed media items with various MIME types and sizes
        {
            let db = state.db.lock().await;
            db.execute(
                "INSERT INTO media_items \
                 (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at) \
                 VALUES ('a', 'a.png', 'a.png', 'image/png', 100, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
                [],
            ).unwrap();
            db.execute(
                "INSERT INTO media_items \
                 (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at) \
                 VALUES ('b', 'b.jpg', 'b.jpg', 'image/jpeg', 200, '2025-01-02T00:00:00Z', '2025-01-02T00:00:00Z')",
                [],
            ).unwrap();
            db.execute(
                "INSERT INTO media_items \
                 (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at) \
                 VALUES ('c', 'c.webm', 'c.webm', 'video/webm', 5000, '2025-01-03T00:00:00Z', '2025-01-03T00:00:00Z')",
                [],
            ).unwrap();
        }

        let app = routes().with_state(state);

        let response = app
            .oneshot(Request::builder().uri("/stats").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();

        // Total counts
        assert_eq!(body["total"], 3);
        assert_eq!(body["total_file_size"], 5300);

        // MIME type histogram
        let by_mime = body["by_mime_type"].as_object().unwrap();
        assert_eq!(by_mime.len(), 3);
        assert_eq!(by_mime.get("image/png").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(by_mime.get("image/jpeg").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(by_mime.get("video/webm").and_then(|v| v.as_u64()), Some(1));
    }

    #[tokio::test]
    async fn test_stats_mime_type_histogram() {
        let state = test_state();

        // Seed multiple items with the same MIME type
        {
            let db = state.db.lock().await;
            // 2 PNGs, 3 JPEGs, 1 WEBM
            for i in 0..2 {
                let id = format!("png-{}", i);
                db.execute(
                    "INSERT INTO media_items \
                     (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at) \
                     VALUES (?1, ?2, ?2, 'image/png', 100, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
                    rusqlite::params![id, format!("{}.png", id)],
                ).unwrap();
            }
            for i in 0..3 {
                let id = format!("jpg-{}", i);
                db.execute(
                    "INSERT INTO media_items \
                     (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at) \
                     VALUES (?1, ?2, ?2, 'image/jpeg', 200, '2025-01-02T00:00:00Z', '2025-01-02T00:00:00Z')",
                    rusqlite::params![id, format!("{}.jpg", id)],
                ).unwrap();
            }
            db.execute(
                "INSERT INTO media_items \
                 (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at) \
                 VALUES ('webm-0', 'c.webm', 'c.webm', 'video/webm', 5000, '2025-01-03T00:00:00Z', '2025-01-03T00:00:00Z')",
                [],
            ).unwrap();
        }

        let app = routes().with_state(state);

        let response = app
            .oneshot(Request::builder().uri("/stats").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(body["total"], 6);

        let by_mime = body["by_mime_type"].as_object().unwrap();
        assert_eq!(by_mime.get("image/png").and_then(|v| v.as_u64()), Some(2));
        assert_eq!(by_mime.get("image/jpeg").and_then(|v| v.as_u64()), Some(3));
        assert_eq!(by_mime.get("video/webm").and_then(|v| v.as_u64()), Some(1));
    }

    #[tokio::test]
    async fn test_stats_indexing_status_reflects_tracker() {
        let state = test_state();

        // Set progress to simulate an indexing run
        state.progress.set_status(IndexStatus::Scanning);
        state.progress.set_total(100);
        state.progress.increment_processed();
        state.progress.add_error("File not found".to_string());

        let app = routes().with_state(state);

        let response = app
            .oneshot(Request::builder().uri("/stats").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(body["indexing"]["status"], "Scanning");
        assert_eq!(body["indexing"]["total"], 100);
        assert_eq!(body["indexing"]["processed"], 1);
        assert_eq!(
            body["indexing"]["errors"].as_array().unwrap().len(),
            1
        );
        assert_eq!(body["indexing"]["errors"][0], "File not found");
    }
}
