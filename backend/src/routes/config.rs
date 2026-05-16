use axum::{Router, extract::{Query, State}, http::StatusCode, response::Json, routing::get};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::config::AppConfig;
use crate::indexer::progress::ProgressTracker;
use crate::search::IndexManager;
use crate::watcher::FileWatcher;

/// Shared application state for config endpoints.
///
/// Wraps a SQLite connection, file watcher, Tantivy index manager, and
/// progress tracker so that the [`update_config`] handler can dynamically
/// add watched folders at runtime and trigger a background re-index.
pub struct ConfigState {
    pub db: Arc<Mutex<rusqlite::Connection>>,
    /// File-system watcher — used to add/remove watches for new folders.
    pub watcher: Arc<Mutex<FileWatcher>>,
    /// Tantivy search index manager — needed for background re-index.
    pub index_manager: Arc<IndexManager>,
    /// Indexing progress tracker (shared with stats route).
    pub progress: Arc<ProgressTracker>,
    /// Path to the SQLite database (needed to open a separate read‑only
    /// connection for Tantivy re-indexing).
    pub db_path: PathBuf,
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
///
/// In addition to persisting the config to the database, this handler:
/// 1. Diffs old vs. new folder lists and updates the file watcher.
/// 2. Spawns a background re-index so existing files in newly-added
///    folders are immediately visible in the search index and gallery.
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

    // Load the old config from the database *before* overwriting so that
    // we can diff the folder lists and know which paths to add/remove.
    let old_config = {
        let db = state.db.lock().await;
        crate::config::load_config(&db).map_err(|e| {
            tracing::error!(error = %e, "Failed to load config from database");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to load configuration"})))
        })?
    };

    // Persist the new config.
    {
        let db = state.db.lock().await;
        crate::config::save_config(&db, &config).map_err(|e| {
            tracing::error!(error = %e, "Failed to save config to database");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to save configuration"})))
        })?;
    }

    // Diff old vs. new watched folders and update the file watcher.
    {
        let old_paths: Vec<PathBuf> = old_config
            .watched_folders
            .iter()
            .map(|f| PathBuf::from(&f.path))
            .collect();
        let new_paths: Vec<PathBuf> = config
            .watched_folders
            .iter()
            .map(|f| PathBuf::from(&f.path))
            .collect();

        let mut watcher = state.watcher.lock().await;

        // Add watches for newly-added folders.
        for path in &new_paths {
            if !old_paths.contains(path) {
                if let Err(e) = watcher.watch(path) {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "Failed to start watching new folder",
                    );
                } else {
                    tracing::info!(path = %path.display(), "Now watching new folder");
                }
            }
        }

        // Remove watches for removed folders (best-effort — the underlying
        // notify backend on some platforms may not clean up sub‑directory
        // watches, but the watch handle is released).
        for path in &old_paths {
            if !new_paths.contains(path) && let Err(e) = watcher.unwatch(path) {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "Failed to stop watching removed folder",
                );
            }
        }
    }

    // Spawn a background re-index so that existing files in newly-added
    // folders are indexed immediately (not just new files created after
    // the watcher was added).
    let db = Arc::clone(&state.db);
    let config_clone = config.clone();
    let im = Arc::clone(&state.index_manager);
    let progress = Arc::clone(&state.progress);
    let db_path = state.db_path.clone();

    tokio::spawn(async move {
        // Phase 1: scan files and populate SQLite.
        if let Err(e) = crate::indexer::full_index(&db, &config_clone, &progress).await {
            tracing::error!(error = %e, "Re-index after config update failed (Phase 1)");
            return;
        }

        // Phase 2: rebuild Tantivy full-text index from SQLite using a
        // separate read‑only connection (WAL mode permits concurrent
        // readers) so that the shared db Mutex stays available for API
        // requests during the re-index.
        let read_conn = match crate::db::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "Failed to open DB for Tantivy reindex");
                return;
            }
        };

        if let Err(e) = crate::search::indexer::full_reindex(&read_conn, &im) {
            tracing::error!(error = %e, "Tantivy reindex after config update failed (Phase 2)");
        }
    });

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
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        let mut resolved = home;
        resolved.push('/');
        resolved.push_str(rest);
        return resolved;
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

    /// Build a full ConfigState for tests.  The caller **must** retain the
    /// returned `TempDir` for the lifetime of the test so that the Tantivy
    /// index directory is not removed while `IndexManager` holds open handles.
    fn test_state() -> (Arc<ConfigState>, tempfile::TempDir) {
        let tantivy_dir = tempfile::tempdir().expect("tempdir");
        let mut conn =
            crate::db::open_in_memory().expect("Failed to create in-memory database");
        crate::db::migrations::run_migrations(&mut conn)
            .expect("Failed to run migrations");

        let index_manager = Arc::new(
            crate::search::IndexManager::open_or_create(
                &tantivy_dir.path().join("tantivy"),
            )
            .expect("IndexManager"),
        );

        let (watcher, _rx) =
            crate::watcher::FileWatcher::new(&[]).expect("FileWatcher");

        let state = Arc::new(ConfigState {
            db: Arc::new(Mutex::new(conn)),
            watcher: Arc::new(Mutex::new(watcher)),
            index_manager,
            progress: Arc::new(crate::indexer::progress::ProgressTracker::new()),
            db_path: tantivy_dir.path().join("imageviz.db"),
        });

        (state, tantivy_dir)
    }

    /// Create a state with a database that has no tables at all.
    ///
    /// Any query against the config table will fail with "no such table",
    /// triggering the 500 error path in route handlers. Kept as a raw
    /// in-memory connection (no migrations) so the MISSING-TABLE error
    /// path remains exercised.  The watcher and index manager fields are
    /// populated with valid but quiescent instances — they are never
    /// reached in the error path.
    fn bad_state() -> (Arc<ConfigState>, tempfile::TempDir) {
        let tantivy_dir = tempfile::tempdir().expect("tempdir");
        let conn = rusqlite::Connection::open_in_memory()
            .expect("Failed to create in-memory database");

        let index_manager = Arc::new(
            crate::search::IndexManager::open_or_create(
                &tantivy_dir.path().join("tantivy"),
            )
            .expect("IndexManager"),
        );

        let (watcher, _rx) =
            crate::watcher::FileWatcher::new(&[]).expect("FileWatcher");

        let state = Arc::new(ConfigState {
            db: Arc::new(Mutex::new(conn)),
            watcher: Arc::new(Mutex::new(watcher)),
            index_manager,
            progress: Arc::new(crate::indexer::progress::ProgressTracker::new()),
            db_path: tantivy_dir.path().join("imageviz.db"),
        });

        (state, tantivy_dir)
    }

    #[tokio::test]
    async fn test_get_config_empty() {
        let (state, _dir) = test_state();
        let app = routes().with_state(state);

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
        let (state, _dir) = test_state();
        let app = routes().with_state(state);

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
        let (state, _dir) = test_state();
        let app = routes().with_state(state);

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
        let (state, _dir) = bad_state();
        let app = routes().with_state(state);

        let response = app
            .oneshot(Request::builder().uri("/config").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_put_config_without_table_returns_500() {
        let (state, _dir) = bad_state();
        let app = routes().with_state(state);

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
