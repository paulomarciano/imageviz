use axum::{Router, extract::State, http::StatusCode, response::Json, routing::get};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::config::AppConfig;

/// Shared application state for config endpoints.
///
/// Wraps a SQLite connection behind a mutex so that concurrent requests are
/// serialized — acceptable for infrequent config reads/writes.
pub struct ConfigState {
    pub db: Mutex<rusqlite::Connection>,
}

pub fn routes() -> Router<Arc<ConfigState>> {
    Router::new().route("/config", get(get_config).put(update_config))
}

/// GET /api/v1/config — return the current watched-folder configuration.
async fn get_config(
    State(state): State<Arc<ConfigState>>,
) -> Result<Json<AppConfig>, (StatusCode, Json<Value>)> {
    let db = state.db.lock().await;
    let config = crate::config::load_config(&db).map_err(|e| {
        tracing::error!(error = %e, "Failed to load config from database");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to load configuration"})))
    })?;
    Ok(Json(config))
}

/// PUT /api/v1/config — replace the watched-folder configuration.
///
/// Body must contain a `watched_folders` array. Each entry must have a
/// non-empty `path` string and may optionally include a `label`.
async fn update_config(
    State(state): State<Arc<ConfigState>>,
    Json(config): Json<AppConfig>,
) -> Result<Json<AppConfig>, (StatusCode, Json<Value>)> {
    // Validate: all paths must be non-empty
    for folder in &config.watched_folders {
        if folder.path.trim().is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Watched folder path cannot be empty"})),
            ));
        }
    }

    let db = state.db.lock().await;
    crate::config::save_config(&db, &config).map_err(|e| {
        tracing::error!(error = %e, "Failed to save config to database");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to save configuration"})))
    })?;
    Ok(Json(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{self, Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use serde_json::json;
    use tower::ServiceExt;

    fn test_state() -> Arc<ConfigState> {
        let conn =
            rusqlite::Connection::open_in_memory().expect("Failed to create in-memory database");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS config (key TEXT PRIMARY KEY, value TEXT);")
            .expect("Failed to create config table");
        Arc::new(ConfigState { db: Mutex::new(conn) })
    }

    /// Create a state with a database that has no `config` table.
    /// Any query against the config table will fail with "no such table",
    /// triggering the 500 error path in route handlers.
    fn bad_state() -> Arc<ConfigState> {
        let conn =
            rusqlite::Connection::open_in_memory().expect("Failed to create in-memory database");
        Arc::new(ConfigState { db: Mutex::new(conn) })
    }

    #[tokio::test]
    async fn test_get_config_empty() {
        let app = routes().with_state(test_state());

        let response = app
            .oneshot(Request::builder().uri("/config").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let folders = body["watched_folders"].as_array().unwrap();
        assert!(folders.is_empty());
    }

    #[tokio::test]
    async fn test_put_and_get_roundtrip() {
        let app = routes().with_state(test_state());

        let input = json!({
            "watched_folders": [
                {"path": "/tmp/test", "label": "Test folder"},
                {"path": "/tmp/another"}
            ]
        });
        let input_bytes = serde_json::to_vec(&input).unwrap();

        // PUT the config (clone app to share the same state)
        let put_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(http::Method::PUT)
                    .uri("/config")
                    .header("content-type", "application/json")
                    .body(Body::from(input_bytes))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(put_response.status(), StatusCode::OK);

        // GET from the same app instance (shared state via Arc)
        let get_response = app
            .oneshot(Request::builder().uri("/config").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(get_response.status(), StatusCode::OK);

        let body_bytes = get_response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let folders = body["watched_folders"].as_array().unwrap();
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0]["path"], "/tmp/test");
        assert_eq!(folders[0]["label"], "Test folder");
        assert_eq!(folders[1]["path"], "/tmp/another");
    }

    #[tokio::test]
    async fn test_put_empty_path_returns_400() {
        let app = routes().with_state(test_state());

        let input = json!({
            "watched_folders": [
                {"path": "   "}
            ]
        });
        let input_bytes = serde_json::to_vec(&input).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method(http::Method::PUT)
                    .uri("/config")
                    .header("content-type", "application/json")
                    .body(Body::from(input_bytes))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert!(body["error"].as_str().unwrap().contains("empty"));
    }

    #[tokio::test]
    async fn test_get_config_without_table_returns_500() {
        let app = routes().with_state(bad_state());

        let response = app
            .oneshot(Request::builder().uri("/config").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_put_config_without_table_returns_500() {
        let app = routes().with_state(bad_state());

        let input = json!({"watched_folders": [{"path": "/tmp/test"}]});
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(http::Method::PUT)
                    .uri("/config")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&input).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
