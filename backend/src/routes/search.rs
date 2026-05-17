//! Full-text search endpoint — `/api/v1/search`
//!
//! Searches the Tantivy index for media items matching a query string against
//! the `metadata_json` (TEXT) and `filename` (STRING) fields.  Results are
//! enriched with full records from SQLite and returned in the same list-view
//! format as the media listing endpoint.
//!
//! # Cursor Pagination
//!
//! Tantivy score ordering is inherently versioned, so cursors are best-effort:
//! we fetch `limit + 1` documents and return the last item's `created_at` /
//! `id` as the cursor.  Subsequent requests that include these cursors are
//! **not** re-applied to the Tantivy query (Tantivy does not natively support
//! cursor-based pagination across score-ordered results); the cursors are
//! provided so that the caller can implement client-side offset if needed.

use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
};
use r2d2::Pool;

use crate::db::SqliteConnectionManager;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::Value as TantivyValue;

use crate::middleware::validation;
use crate::search::IndexManager;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Shared application state for the search endpoint.
pub struct SearchState {
    pub index_manager: Arc<IndexManager>,
    pub db: Pool<SqliteConnectionManager>,
}

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

/// Search query parameters.
#[derive(Deserialize, Default)]
#[allow(dead_code)]
struct SearchParams {
    q: Option<String>,
    /// Maximum items per page (default 100, max 500).
    #[serde(default = "default_limit")]
    limit: u32,
    /// Opaque cursor for pagination (ISO 8601 date of last item).
    cursor: Option<String>,
    /// Tiebreaker cursor: UUID of the last item.
    cursor_id: Option<String>,
    /// MIME type filter (e.g. `image/%`, `video/%`) — SQL LIKE pattern.
    mime_type: Option<String>,
    /// Sort order — `"recency"` (newest first, default) or `"score"` (BM25 relevance).
    #[serde(default = "default_sort")]
    sort: String,
}

fn default_sort() -> String {
    "recency".to_string()
}

fn default_limit() -> u32 {
    100
}

// ---------------------------------------------------------------------------
// Route factory
// ---------------------------------------------------------------------------

pub fn routes() -> Router<Arc<SearchState>> {
    Router::new().route("/search", get(search_handler))
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// GET /api/v1/search?q=<query>&limit=<n>&cursor=<cursor>&cursor_id=<id>
async fn search_handler(
    State(state): State<Arc<SearchState>>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    validation::validate_search_query(&params.q)?;
    validation::validate_limit(params.limit)?;
    validation::validate_cursor(params.cursor.as_deref())?;
    validation::validate_cursor_id(params.cursor_id.as_deref())?;

    let query_str = params.q.as_ref().unwrap().trim().to_string();
    let limit = params.limit as usize;

    // ---- Search Tantivy ----
    let schema = state.index_manager.schema();
    let reader = state.index_manager.reader();
    let searcher = reader.searcher();

    let metadata_json_field = schema.get_field("metadata_json").map_err(|e| {
        tracing::error!(error = %e, "Missing metadata_json field in Tantivy schema");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Internal server error"})))
    })?;
    let filename_field = schema.get_field("filename").map_err(|e| {
        tracing::error!(error = %e, "Missing filename field in Tantivy schema");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Internal server error"})))
    })?;

    let query_parser = QueryParser::for_index(
        state.index_manager.index(),
        vec![metadata_json_field, filename_field],
    );

    let query = match query_parser.parse_query(&query_str) {
        Ok(q) => q,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("Invalid query: {}", e)})),
            ));
        }
    };

    let collector = TopDocs::with_limit(limit + 1).order_by_score();
    let top_docs = match searcher.search(&query, &collector) {
        Ok(docs) => docs,
        Err(e) => {
            tracing::error!(error = %e, "Tantivy search failed");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Search failed"})),
            ));
        }
    };

    // ---- Resolve Tantivy hits → SQLite records ----
    let has_more = top_docs.len() > limit;
    let docs = &top_docs[..top_docs.len().min(limit)];

    let id_field = schema.get_field("id").unwrap();

    // Collect IDs from Tantivy hits.
    let item_ids: Vec<String> = docs
        .iter()
        .filter_map(|(_score, doc_address)| {
            let tantivy_doc: tantivy::TantivyDocument = match searcher.doc(*doc_address) {
                Ok(d) => d,
                Err(_) => return None,
            };
            let item_id = match tantivy_doc.get_first(id_field).and_then(|v| v.as_str()) {
                Some(id) if !id.is_empty() => id.to_string(),
                _ => return None,
            };
            Some(item_id)
        })
        .collect();

    // Single DB connection + single batch query instead of N per-hit queries.
    let conn = state.db.get().map_err(|e| {
        tracing::error!(error = %e, "Failed to acquire database connection");
        (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Service temporarily unavailable"})))
    })?;

    let mut media_items = batch_get_media_items(&conn, &item_ids, params.mime_type.as_deref())
        .map_err(|e| {
            tracing::error!(error = %e, "Batch DB lookup failed for search hits");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Search lookup failed"})))
        })?;

    // ---- Sort results ----
    if params.sort == "recency" {
        media_items.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    }

    let (next_cursor, next_cursor_id) = if has_more {
        if let Some(last) = media_items.last() {
            (Some(last.created_at.clone()), Some(last.id.clone()))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    Ok(Json(json!({
        "data": media_items,
        "meta": {
            "next_cursor": next_cursor,
            "next_cursor_id": next_cursor_id,
            "has_more": has_more,
            "total": media_items.len() as u64,
            "query": query_str,
        }
    })))
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Lightweight media item returned in search results.
///
/// Matches the list-view format defined in the API contract so that the
/// frontend can reuse the same rendering components.
#[derive(serde::Serialize)]
struct MediaItemSummary {
    id: String,
    filename: String,
    path: String,
    mime_type: String,
    thumbnail_url: String,
    width: Option<i64>,
    height: Option<i64>,
    file_size: i64,
    created_at: String,
    modified_at: String,
}

// ---------------------------------------------------------------------------
// Database helpers
// ---------------------------------------------------------------------------

/// Fetch media items from SQLite by IDs — single batch query instead of N
/// individual lookups.
///
/// Uses a dynamic `WHERE id IN (?1, ?2, ..., ?N)` clause.  An optional MIME
/// type filter is appended as an additional parameter.
fn batch_get_media_items(
    conn: &rusqlite::Connection,
    ids: &[String],
    mime_type: Option<&str>,
) -> Result<Vec<MediaItemSummary>, rusqlite::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    // Build parameterised IN clause: (?1, ?2, ..., ?N)
    let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
    let mut sql = format!(
        "SELECT id, filename, relative_path, mime_type, width, height, file_size, \
         file_created_at, file_modified_at \
         FROM media_items WHERE id IN ({})",
        placeholders.join(", "),
    );

    if mime_type.is_some() {
        sql.push_str(&format!(" AND mime_type LIKE ?{}", ids.len() + 1));
    }

    let mut stmt = conn.prepare(&sql)?;

    let mapper = |row: &rusqlite::Row| -> Result<MediaItemSummary, rusqlite::Error> {
        Ok(MediaItemSummary {
            id: row.get(0)?,
            filename: row.get(1)?,
            path: row.get(2)?,
            mime_type: row.get(3)?,
            width: row.get(4)?,
            height: row.get(5)?,
            file_size: row.get(6)?,
            created_at: row.get(7)?,
            modified_at: row.get(8)?,
            thumbnail_url: format!("/api/v1/media/{}/thumbnail", row.get::<_, String>(0)?),
        })
    };

    // Collect dynamic parameters as trait objects, then convert to slice refs
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> =
        ids.iter().map(|id| Box::new(id.clone()) as Box<dyn rusqlite::types::ToSql>).collect();
    if let Some(mime) = mime_type {
        values.push(Box::new(mime.to_string()) as Box<dyn rusqlite::types::ToSql>);
    }

    let params: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|v| v.as_ref()).collect();
    let rows = stmt.query_map(params.as_slice(), mapper)?;

    rows.collect::<Result<Vec<_>, _>>()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "search_test.rs"]
mod tests;
