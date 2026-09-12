use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::middleware::validation;
use crate::routes::error::AppError;

use super::MediaState;

// ---------------------------------------------------------------------------
// Media item detail (GET /media/:id)
// ---------------------------------------------------------------------------

/// Full media item response for the detail view.
#[derive(Serialize)]
struct MediaItemDetail {
    id: String,
    filename: String,
    path: String,
    mime_type: String,
    thumbnail_url: String,
    file_url: String,
    width: Option<i64>,
    height: Option<i64>,
    file_size: i64,
    created_at: String,
    modified_at: String,
    metadata: Option<Value>,
}

/// GET /api/v1/media/{id} — return a single media item with full metadata.
pub(super) async fn get_media_item(
    State(state): State<Arc<MediaState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    validation::validate_media_id(&id)?;

    let conn = state.db.get()?;

    let row = conn
        .query_row(
            "SELECT id, filename, relative_path, mime_type, width, height, file_size,
                    file_created_at, file_modified_at, metadata_json
             FROM media_items WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                let id: String = row.get(0)?;
                let metadata_raw: Option<String> = row.get(9)?;
                Ok(MediaItemDetail {
                    id: id.clone(),
                    filename: row.get(1)?,
                    path: row.get(2)?,
                    mime_type: row.get(3)?,
                    thumbnail_url: format!("/api/v1/media/{}/thumbnail", id),
                    file_url: format!("/api/v1/media/{}/file", id),
                    width: row.get(4)?,
                    height: row.get(5)?,
                    file_size: row.get(6)?,
                    created_at: row.get(7)?,
                    modified_at: row.get(8)?,
                    metadata: metadata_raw.and_then(|s| serde_json::from_str(&s).ok()),
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AppError::NotFound("Media not found"),
            other => other.into(),
        })?;

    Ok(Json(json!(row)))
}

/// GET /api/v1/media/{id}/metadata — return the structured metadata for an item.
pub(super) async fn get_media_metadata(
    State(state): State<Arc<MediaState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    validation::validate_media_id(&id)?;

    let conn = state.db.get()?;

    let metadata_json: Option<String> = conn
        .query_row(
            "SELECT metadata_json FROM media_items WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AppError::NotFound("Media not found"),
            other => other.into(),
        })?;

    match metadata_json {
        Some(json_str) => match serde_json::from_str(&json_str) {
            Ok(val) => Ok(Json(val)),
            Err(_) => {
                // Non-JSON metadata — return empty rather than serving
                // raw strings that would confuse the frontend.
                Ok(Json(json!({})))
            }
        },
        None => Ok(Json(json!({}))),
    }
}
