//! Shared response structs for the route layer.
//!
//! `MediaItemSummary` was previously defined twice — once for the media list
//! and once for search (wave-8.16 / review D6) — with identical fields. The
//! list-view contract lives here so the two endpoints cannot drift.

use serde::Serialize;

/// Lightweight media item returned in list views (`GET /media`, `GET /search`).
///
/// Matches the list-view format defined in the API contract so that the
/// frontend can reuse the same rendering components.
#[derive(Debug, Serialize)]
pub struct MediaItemSummary {
    pub id: String,
    pub filename: String,
    pub path: String,
    pub mime_type: String,
    pub thumbnail_url: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub file_size: i64,
    pub created_at: String,
    pub modified_at: String,
}
