use axum::{Router, extract::{Query, State}, http::StatusCode, response::Json, routing::get};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::config::AppConfig;

/// Shared application state for config endpoints.
///
/// Wraps a SQLite connection behind an Arc<Mutex<>> so that concurrent requests
/// are serialized — acceptable for infrequent config reads/writes.  The Arc is
/// shared with other state structs (e.g. MediaState) that need DB access.
pub struct ConfigState {
    pub db: Arc<Mutex<rusqlite::Connection>>,
}

pub fn routes() -> Router<Arc<ConfigState>> {
    Router::new()
        .route("/config", get(get_config).put(update_config))
        .route("/config/suggest", get(suggest_folders))
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

/// Query parameters for `GET /config/suggest`.
#[derive(Deserialize)]
struct SuggestParams {
    path: String,
}

/// A single path suggestion returned by the suggest endpoint.
#[derive(Serialize)]
struct PathSuggestion {
    path: String,
    name: String,
    is_directory: bool,
}

/// Response body for `GET /config/suggest`.
#[derive(Serialize)]
struct SuggestResponse {
    suggestions: Vec<PathSuggestion>,
}

/// Expand a leading `~` to the user's home directory.
fn resolve_path(path: &str) -> String {
    if path == "~" {
        return std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            let mut resolved = home;
            resolved.push('/');
            resolved.push_str(rest);
            return resolved;
        }
    }
    path.to_string()
}

/// GET /config/suggest — return subdirectory suggestions for a path prefix.
///
/// Used by the frontend config panel to power a folder autocomplete.  This
/// is a stateless filesystem operation — no database access is needed.
async fn suggest_folders(
    Query(params): Query<SuggestParams>,
) -> Result<Json<SuggestResponse>, (StatusCode, Json<Value>)> {
    let resolved = resolve_path(&params.path);
    let path = Path::new(&resolved);

    let (search_dir, prefix) = if path.exists() && path.is_dir() {
        (path.to_path_buf(), String::new())
    } else {
        let parent = path.parent().unwrap_or(Path::new("/"));
        let prefix = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        (parent.to_path_buf(), prefix)
    };

    let mut suggestions = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&search_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // Skip hidden entries
            if name.starts_with('.') {
                continue;
            }
            // Filter by prefix when the path doesn't exist as a directory
            if !prefix.is_empty() && !name.starts_with(&prefix) {
                continue;
            }
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            if !is_dir {
                continue;
            }
            suggestions.push(PathSuggestion {
                path: entry.path().to_string_lossy().to_string(),
                name,
                is_directory: is_dir,
            });
        }
    }

    suggestions.sort_by(|a, b| a.name.cmp(&b.name));
    suggestions.truncate(50);

    Ok(Json(SuggestResponse { suggestions }))
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
        let mut conn =
            crate::db::open_in_memory().expect("Failed to create in-memory database");
        crate::db::migrations::run_migrations(&mut conn)
            .expect("Failed to run migrations");
        Arc::new(ConfigState { db: Arc::new(Mutex::new(conn)) })
    }

    /// Create a state with a database that has no tables at all.
    ///
    /// Any query against the config table will fail with "no such table",
    /// triggering the 500 error path in route handlers. Kept as a raw
    /// in-memory connection (no migrations) so the MISSING-TABLE error
    /// path remains exercised.
    fn bad_state() -> Arc<ConfigState> {
        let conn =
            rusqlite::Connection::open_in_memory().expect("Failed to create in-memory database");
        Arc::new(ConfigState { db: Arc::new(Mutex::new(conn)) })
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

    // -----------------------------------------------------------------------
    // suggest_folders tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_suggest_root_directories() {
        let app = Router::new().route("/config/suggest", get(super::suggest_folders));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/config/suggest?path=/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let suggestions = body["suggestions"].as_array().unwrap();

        // On any Linux system, /tmp should exist and be a directory
        let names: Vec<&str> =
            suggestions.iter().map(|s| s["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"tmp"), "expected 'tmp' in root directory suggestions");
    }

    #[tokio::test]
    async fn test_suggest_nonexistent_path_returns_empty() {
        let app = Router::new().route("/config/suggest", get(super::suggest_folders));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/config/suggest?path=/nonexistent/xyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let suggestions = body["suggestions"].as_array().unwrap();
        assert!(suggestions.is_empty(), "expected empty suggestions for nonexistent path");
    }

    #[tokio::test]
    async fn test_suggest_excludes_hidden() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_string_lossy().to_string();

        // Create visible and hidden directories inside the temp dir
        std::fs::create_dir(dir.path().join("visible")).unwrap();
        std::fs::create_dir(dir.path().join(".hidden")).unwrap();

        let app = Router::new().route("/config/suggest", get(super::suggest_folders));

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/config/suggest?path={}", dir_path))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let suggestions = body["suggestions"].as_array().unwrap();

        let names: Vec<&str> =
            suggestions.iter().map(|s| s["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"visible"), "expected 'visible' in suggestions");
        assert!(!names.contains(&".hidden"), "did not expect '.hidden' in suggestions");
    }
}
