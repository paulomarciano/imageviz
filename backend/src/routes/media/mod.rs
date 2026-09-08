use axum::{Router, routing::get};
use r2d2::Pool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::db::SqliteConnectionManager;
use crate::thumbnails::limiter::ThumbnailLimiter;

pub mod detail;
pub mod file;
pub mod list;
pub mod thumbnail;

/// Per-filter total-count cache entries: filter key (`""` = unfiltered)
/// → `(count, cached-at)`.
pub type CountCache = HashMap<String, (i64, Instant)>;

/// Shared application state for media endpoints.
pub struct MediaState {
    pub db: Pool<SqliteConnectionManager>,
    pub thumbnail_cache_dir: PathBuf,
    pub thumbnail_limiter: Arc<ThumbnailLimiter>,
    /// Cache of total media counts keyed by mime filter (`""` = unfiltered),
    /// refreshed every 30 seconds. Avoids a `SELECT COUNT(*)` full index scan
    /// on every page load, including mime-filtered pages.
    pub total_count_cache: Arc<Mutex<CountCache>>,
}

pub fn routes() -> Router<Arc<MediaState>> {
    Router::new()
        .route("/media", get(list::list_media))
        .route("/media/{id}", get(detail::get_media_item))
        .route("/media/{id}/metadata", get(detail::get_media_metadata))
        .route("/media/{id}/file", get(file::serve_file))
        .route("/media/{id}/thumbnail", get(thumbnail::serve_thumbnail))
}

#[cfg(test)]
mod tests;
