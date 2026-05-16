use axum::{
    extract::{Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Shared application state for media endpoints.
pub struct MediaState {
    pub db: Arc<Mutex<rusqlite::Connection>>,
}

pub fn routes() -> Router<Arc<MediaState>> {
    Router::new().route("/media/{id}/file", get(serve_file))
}

/// GET /api/v1/media/{id}/file — stream the original file.
async fn serve_file(
    State(state): State<Arc<MediaState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    let db = state.db.lock().await;

    // Query the media item from DB
    let (relative_path, mime_type, filename): (String, String, String) = db
        .query_row(
            "SELECT relative_path, mime_type, filename FROM media_items WHERE id = ?1",
            rusqlite::params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                (StatusCode::NOT_FOUND, Json(json!({"error": "Media not found"})))
            }
            _ => {
                tracing::error!(error = %e, "Database error fetching media item");
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Internal server error"})))
            }
        })?;

    // Resolve the relative path to an absolute path using watched folders from config
    let config_str: String = db
        .query_row(
            "SELECT value FROM config WHERE key = 'watched_folders'",
            [],
            |row| row.get(0),
        )
        .map_err(|_| {
            (StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"})))
        })?;

    let config: Value = serde_json::from_str(&config_str).map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Invalid configuration"})))
    })?;

    let folders = config["watched_folders"].as_array().ok_or_else(|| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Invalid configuration format"})))
    })?;

    // Find the file in any watched folder
    let file_path = folders
        .iter()
        .filter_map(|folder| {
            let base = folder["path"].as_str()?;
            let full = std::path::Path::new(base).join(&relative_path);
            if full.exists() { Some(full) } else { None }
        })
        .next();

    let path = file_path.ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"})))
    })?;

    // Open and stream the file
    let file = tokio::fs::File::open(&path).await.map_err(|e| {
        tracing::error!(error = %e, path = %path.display(), "Failed to open file");
        (StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"})))
    })?;

    let stream = tokio_util::io::ReaderStream::new(file);
    let body = axum::body::Body::from_stream(stream);

    // Content-Type and Content-Disposition headers are set; Content-Length is
    // omitted intentionally — Axum's streaming body handles chunked transfer.
    Ok((
        [
            (header::CONTENT_TYPE, mime_type),
            (header::CONTENT_DISPOSITION, format!("inline; filename=\"{}\"", filename)),
        ],
        body,
    ))
}
