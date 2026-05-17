use axum::{
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Json},
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

use crate::middleware::validation;
use crate::thumbnails::cache::CacheError;

use super::MediaState;

/// GET /api/v1/media/{id}/thumbnail — serve a WebP thumbnail.
pub(super) async fn serve_thumbnail(
    State(state): State<Arc<MediaState>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    validation::validate_media_id(&id)?;

    // Parse optional width parameter (default 200, range 100-500)
    let width: u32 = params.get("width").and_then(|w| w.parse().ok()).unwrap_or(200);
    validation::validate_thumbnail_width(width)?;

    // Look up media item, resolve file path, and get checksum
    let conn = state.db.get().map_err(|e| {
        tracing::error!(error = %e, "Failed to acquire database connection");
        (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Service temporarily unavailable"})))
    })?;
    let (file_path, mime_type, _) = super::file::resolve_media_path(&conn, &id)?;

    let checksum: String = conn
        .query_row(
            "SELECT COALESCE(checksum, '') FROM media_items WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .unwrap_or_default();
    drop(conn);

    // Acquire thumbnail generation permit (limits CPU contention)
    let _permit = state.thumbnail_limiter.acquire().await.map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "Too many thumbnail requests. Try again later."})),
        )
    })?;

    // Generate or retrieve cached thumbnail
    let thumbnail = crate::thumbnails::get_or_generate_thumbnail(
        &file_path,
        &checksum,
        width,
        &state.thumbnail_cache_dir,
        &mime_type,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to generate thumbnail");
        match e {
            CacheError::SourceNotFound(_) => {
                (StatusCode::NOT_FOUND, Json(json!({"error": "File not found on disk"})))
            }
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to generate thumbnail"})),
            ),
        }
    })?;

    // Populate thumbnail_path in the database so the column is no longer
    // dead — it records the on-disk cache path after first generation.
    // This is best-effort: a failure to update is logged but does not
    // prevent the thumbnail from being served.
    if let Ok(conn) = state.db.get()
        && let Err(e) = conn.execute(
            "UPDATE media_items SET thumbnail_path = ?1 WHERE id = ?2",
            rusqlite::params![thumbnail.to_str(), id],
        )
    {
        tracing::warn!(error = %e, id = %id, "Failed to persist thumbnail_path");
    }

    // Read the thumbnail file into memory
    let data = tokio::fs::read(&thumbnail).await.map_err(|e| {
        tracing::error!(error = %e, "Failed to read thumbnail file");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Internal server error"})))
    })?;

    Ok((
        [
            (header::CONTENT_TYPE, "image/webp".to_string()),
            (header::CONTENT_LENGTH, data.len().to_string()),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable".to_string()),
        ],
        data,
    ))
}
