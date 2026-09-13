use axum::{
    extract::{Query, State},
    response::Json,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::middleware::validation;
use crate::routes::error::AppError;
use crate::routes::response::MediaItemSummary;

use super::{CountCache, MediaState};

#[cfg(test)]
#[path = "list_test.rs"]
mod list_test;

// ---------------------------------------------------------------------------
// Total-count cache (per mime filter, 30s TTL)
// ---------------------------------------------------------------------------

/// TTL for cached total counts, shared by all filter keys.
const COUNT_CACHE_TTL: Duration = Duration::from_secs(30);

/// Cache key for the unfiltered total count.
const UNFILTERED_COUNT_KEY: &str = "";

/// Normalize an optional mime filter into a stable cache key
/// (`""` for the unfiltered count).
fn count_cache_key(filter: Option<&str>) -> String {
    filter.unwrap_or(UNFILTERED_COUNT_KEY).to_ascii_lowercase()
}

/// An entry is fresh when its age is below [`COUNT_CACHE_TTL`]. Entries
/// stamped "in the future" relative to `now` (concurrent stores) count as
/// fresh — `checked_duration_since` avoids the `duration_since` panic.
fn is_fresh(at: Instant, now: Instant) -> bool {
    now.checked_duration_since(at).is_none_or(|age| age < COUNT_CACHE_TTL)
}

/// Return the cached count for `key` if its entry is fresh.
fn fresh_count(cache: &CountCache, key: &str, now: Instant) -> Option<i64> {
    cache.get(key).filter(|(_, at)| is_fresh(*at, now)).map(|(count, _)| *count)
}

/// Compute the total count for `filter`, caching results per filter key for
/// 30 seconds.
///
/// Lock discipline: the cache mutex is held only for the map read and the
/// map write — never across `run_count` — so concurrent list requests never
/// serialize behind a long-running `COUNT(*)` query.
fn cached_count(
    cache: &Mutex<CountCache>,
    filter: Option<&str>,
    now: Instant,
    run_count: impl FnOnce() -> i64,
) -> i64 {
    let key = count_cache_key(filter);
    if let Some(fresh) = fresh_count(&cache.lock().unwrap(), &key, now) {
        return fresh;
    }
    let count = run_count();
    {
        let mut map = cache.lock().unwrap();
        // Prune expired entries so the map stays bounded by the filters seen
        // within one TTL window.
        map.retain(|_, (_, at)| is_fresh(*at, now));
        map.insert(key, (count, now));
    }
    count
}

// ---------------------------------------------------------------------------
// Media list (cursor-based pagination)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(super) struct MediaListParams {
    #[serde(default = "default_limit")]
    limit: u32,
    cursor: Option<String>,
    cursor_id: Option<String>,
    mime_type: Option<String>,
}

fn default_limit() -> u32 {
    100
}

/// Map a `media_items` row (list projection) to its summary representation.
fn media_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MediaItemSummary> {
    let id: String = row.get(0)?;
    Ok(MediaItemSummary {
        thumbnail_url: format!("/api/v1/media/{id}/thumbnail"),
        id,
        filename: row.get(1)?,
        path: row.get(2)?,
        mime_type: row.get(3)?,
        width: row.get(4)?,
        height: row.get(5)?,
        file_size: row.get(6)?,
        created_at: row.get(7)?,
        modified_at: row.get(8)?,
    })
}

/// GET /api/v1/media — list media items with cursor-based pagination.
///
/// Query parameters:
/// - `limit` (default 100, max 500): number of items per page
/// - `cursor` (ISO 8601 date): exclusive cursor from the last item's `created_at`
/// - `cursor_id` (UUID): tiebreaker for items with the same `file_created_at`
/// - `mime_type` (e.g. `image/%`): optional MIME type filter (SQL LIKE)
///
/// Returns a JSON object with `data` (array of `MediaItemSummary`) and `meta`
/// (pagination metadata: `next_cursor`, `next_cursor_id`, `has_more`, `total`).
pub(super) async fn list_media(
    State(state): State<Arc<MediaState>>,
    Query(params): Query<MediaListParams>,
) -> Result<Json<Value>, AppError> {
    validation::validate_limit(params.limit)?;
    validation::validate_cursor(params.cursor.as_deref())?;
    validation::validate_cursor_id(params.cursor_id.as_deref())?;

    let limit = params.limit;
    let fetch_limit = limit + 1;
    let has_cursor = params.cursor.is_some() && params.cursor_id.is_some();
    let has_mime = params.mime_type.is_some();

    let conn = state.db.get()?;

    // Total count — cached per mime-filter key for 30s to avoid a full index
    // scan on every page load, including mime-filtered pages. The cache mutex
    // is never held across the COUNT query (see `cached_count`).
    let total: i64 = {
        let mime_filter = params.mime_type.as_deref();
        cached_count(&state.total_count_cache, mime_filter, Instant::now(), || match mime_filter {
            Some(mime) => conn
                .query_row(
                    "SELECT COUNT(*) FROM media_items WHERE mime_type LIKE ?1",
                    rusqlite::params![mime],
                    |row| row.get(0),
                )
                .unwrap_or(0),
            None => conn
                .query_row("SELECT COUNT(*) FROM media_items", [], |row| row.get(0))
                .unwrap_or(0),
        })
    };

    // Build SQL dynamically with anonymous `?` placeholders — bound in
    // order of appearance via `params_from_iter` below.
    let mut sql = String::from(
        "SELECT id, filename, relative_path, mime_type, width, height, file_size, \
         file_created_at, file_modified_at FROM media_items",
    );

    let mut where_parts: Vec<&str> = Vec::new();
    if has_cursor {
        where_parts.push("(file_created_at, id) < (?, ?)");
    }
    if has_mime {
        where_parts.push("mime_type LIKE ?");
    }

    if !where_parts.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&where_parts.join(" AND "));
    }

    sql.push_str(" ORDER BY file_created_at DESC, id DESC LIMIT ?");

    // Parameter values in the same order as their placeholders.
    let mut values: Vec<rusqlite::types::Value> = Vec::new();
    if let (Some(cursor), Some(cursor_id)) = (&params.cursor, &params.cursor_id) {
        values.push(rusqlite::types::Value::Text(cursor.clone()));
        values.push(rusqlite::types::Value::Text(cursor_id.clone()));
    }
    if let Some(ref mime_type) = params.mime_type {
        values.push(rusqlite::types::Value::Text(mime_type.clone()));
    }
    values.push(rusqlite::types::Value::Integer(fetch_limit as i64));

    let mut items: Vec<MediaItemSummary> = {
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(values), media_summary_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    drop(conn);

    let has_more = items.len() > limit as usize;
    items.truncate(limit as usize);

    let (next_cursor, next_cursor_id) = if has_more {
        let last = items.last().expect("items non-empty when has_more is true");
        (Some(last.created_at.clone()), Some(last.id.clone()))
    } else {
        (None, None)
    };

    Ok(Json(json!({
        "data": items,
        "meta": {
            "next_cursor": next_cursor,
            "next_cursor_id": next_cursor_id,
            "has_more": has_more,
            "total": total,
        }
    })))
}
