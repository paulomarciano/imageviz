use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::header,
    response::IntoResponse,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio_util::io::ReaderStream;

use crate::middleware::validation;
use crate::routes::error::AppError;
use crate::thumbnails::cache::{CacheError, probe_thumbnail};

use super::MediaState;

/// Stream a thumbnail file from `path` with the standard thumbnail headers.
///
/// Shared by the cache-hit and cache-miss paths so both responses carry
/// identical headers (content type, length, immutable cache policy).
async fn serve_thumbnail_file(path: std::path::PathBuf) -> Result<impl IntoResponse, AppError> {
    let file = tokio::fs::File::open(&path).await.map_err(|e| {
        tracing::error!(error = %e, "Failed to open thumbnail file");
        AppError::Internal("Internal server error")
    })?;
    let content_length = file
        .metadata()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to read thumbnail metadata");
            AppError::Internal("Internal server error")
        })?
        .len();
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok((
        [
            (header::CONTENT_TYPE, "image/webp".to_string()),
            (header::CONTENT_LENGTH, content_length.to_string()),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable".to_string()),
        ],
        body,
    ))
}

/// GET /api/v1/media/{id}/thumbnail — serve a WebP thumbnail.
pub(super) async fn serve_thumbnail(
    State(state): State<Arc<MediaState>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    validation::validate_media_id(&id)?;

    // Parse optional width parameter (default 200, range 100-500)
    let width: u32 = params.get("width").and_then(|w| w.parse().ok()).unwrap_or(200);
    validation::validate_thumbnail_width(width)?;

    // One shared resolution statement supplies path, MIME type, and checksum.
    // The connection is released before the await (rusqlite::Connection is
    // not `Sync` — its borrow must not cross the await point).
    let conn = state.db.get()?;
    let resolved = super::file::resolve_media_row(&conn, &id)?;
    drop(conn);
    let super::file::ResolvedMedia { full_path: file_path, mime_type, checksum, .. } =
        super::file::verify_on_disk(resolved).await?;

    // Cache hit: serve directly — cached responses bypass the generation
    // limiter entirely (wave 8.9 / review P5).
    if let Some(cached) = probe_thumbnail(&state.thumbnail_cache_dir, &checksum, width) {
        return serve_thumbnail_file(cached).await;
    }

    // Cache miss: acquire a generation permit, then generate. The permit is
    // taken *after* the probe so hits never queue behind in-flight
    // generations. Concurrent misses stay safe: the per-key lock and cache
    // re-check inside `get_or_generate_thumbnail` dedup generation, so at
    // most one generation runs per cache key regardless of limiter order.
    let _permit = state.thumbnail_limiter.acquire().await.map_err(|_| {
        AppError::ServiceUnavailable("Too many thumbnail requests. Try again later.")
    })?;

    // Generate or retrieve cached thumbnail (re-checks the cache under the
    // per-key lock, covering the probe→permit race window).
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
            CacheError::SourceNotFound(_) => AppError::NotFound("File not found on disk"),
            _ => AppError::Internal("Failed to generate thumbnail"),
        }
    })?;

    serve_thumbnail_file(thumbnail).await
}
