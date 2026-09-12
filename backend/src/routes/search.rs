//! Full-text search endpoint — `/api/v1/search`
//!
//! Searches the Tantivy index for media items matching a query string against
//! the `metadata_json` (TEXT) and `filename` (STRING) fields.  Results are
//! enriched with full records from SQLite and returned in the same list-view
//! format as the media listing endpoint.
//!
//! # Offset-based Pagination
//!
//! The `cursor` parameter is a numeric offset (cumulative count of items shown).
//! The backend uses Tantivy's `and_offset` to skip past already-seen results.
//! `has_more` is determined by requesting `limit + 1` items: if the +1 item
//! exists, there are more pages.  `next_cursor` is `offset + returned_count`.
//!
//! Sort order:
//! - `"recency"` (default): `order_by_fast_field("created_at", Desc)` +
//!   tiebreaker re-sort by `(created_at DESC, id DESC)` after SQLite enrichment.
//! - `"score"`: Tantivy BM25 relevance with `order_by_score`.

use axum::{
    Router,
    extract::{Query, State},
    response::Json,
    routing::get,
};
use r2d2::Pool;

use crate::db::SqliteConnectionManager;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tantivy::SegmentReader;
use tantivy::collector::{Count, MultiCollector, TopDocs};
use tantivy::query::QueryParser;
use tantivy::schema::Value as TantivyValue;

use crate::middleware::validation;
use crate::routes::error::AppError;
use crate::routes::response::MediaItemSummary;
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
///
/// `cursor` is a numeric offset representing the cumulative count of items
/// already shown.  The backend uses Tantivy's `and_offset` to skip past
/// already-seen results.  When absent, defaults to 0 (first page).
#[derive(Deserialize, Default)]
struct SearchParams {
    q: Option<String>,
    /// Maximum items per page (default 100, max 500).
    #[serde(default = "default_limit")]
    limit: u32,
    /// Numeric cursor (cumulative offset) for pagination.
    cursor: Option<String>,
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

/// GET /api/v1/search?q=<query>&limit=<n>&cursor=<offset>&mime_type=<type>&sort=<order>
async fn search_handler(
    State(state): State<Arc<SearchState>>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Value>, AppError> {
    validation::validate_search_query(&params.q)?;
    validation::validate_limit(params.limit)?;
    validation::validate_numeric_cursor(params.cursor.as_deref())?;

    let query_str = params.q.as_ref().unwrap().trim().to_string();
    let limit = params.limit as usize;
    let offset: usize = params
        .cursor
        .as_deref()
        .and_then(|c| if c.is_empty() { None } else { c.parse().ok() })
        .unwrap_or(0);

    // ---- Search Tantivy ----
    let schema = state.index_manager.schema();
    let reader = state.index_manager.reader();
    let searcher = reader.searcher();

    let metadata_json_field = schema_field(schema, "metadata_json")?;
    let filename_field = schema_field(schema, "filename")?;

    let mut query_parser = QueryParser::for_index(
        state.index_manager.index(),
        vec![metadata_json_field, filename_field],
    );

    // When sorting by recency (newest first, the default), use AND semantics
    // so that multi-term queries require ALL terms to match.  Score-based
    // relevance ranking keeps the default OR semantics because BM25 scoring
    // benefits from broader matching.
    if params.sort != "score" {
        query_parser.set_conjunction_by_default();
    }

    let query = query_parser
        .parse_query(&query_str)
        .map_err(|e| AppError::BadRequest(format!("Invalid query: {e}")))?;

    let id_field = schema.get_field("id").unwrap();

    // Build the collector: offset-based pagination with appropriate ordering.
    // Both branches return Vec<(f32, DocAddress)> so we can share a single code path.
    let collector: TopDocs = TopDocs::with_limit(limit + 1).and_offset(offset);

    // Single index traversal: count + top docs are collected together via
    // MultiCollector instead of walking all matching docs twice.
    let search_result = if params.sort == "score" {
        search_single_traversal(&searcher, &query, collector.order_by_score())
    } else {
        // Default: recency — custom scoring = created_at timestamp (µs).
        let score_fn = move |segment_reader: &SegmentReader| {
            let date_reader = segment_reader
                .fast_fields()
                .date("created_at")
                .expect("created_at not a fast field; add FAST to schema");
            move |doc_id: tantivy::DocId| {
                date_reader.first(doc_id).map(|dt| dt.into_timestamp_micros() as f32).unwrap_or(0.0)
            }
        };
        search_single_traversal(&searcher, &query, collector.order_by(score_fn))
    };

    let (total_hits, top_docs): (usize, Vec<(f32, tantivy::DocAddress)>) =
        search_result.map_err(|e| {
            tracing::error!(error = %e, "Tantivy search failed");
            AppError::Internal("Search failed")
        })?;

    // ---- Resolve Tantivy hits → SQLite records ----
    let has_more = top_docs.len() > limit;
    let docs = &top_docs[..top_docs.len().min(limit)];

    // Collect IDs from Tantivy hits.
    let item_ids: Vec<String> = docs
        .iter()
        .filter_map(|(_score, doc_address)| {
            let doc: tantivy::TantivyDocument = match searcher.doc(*doc_address) {
                Ok(d) => d,
                Err(_) => return None,
            };
            doc.get_first(id_field)
                .and_then(|v| v.as_str())
                .filter(|id| !id.is_empty())
                .map(String::from)
        })
        .collect();

    // Single DB connection + single batch query instead of N per-hit queries.
    let conn = state.db.get()?;

    let mut media_items = batch_get_media_items(&conn, &item_ids, params.mime_type.as_deref())
        .map_err(|e| {
            tracing::error!(error = %e, "Batch DB lookup failed for search hits");
            AppError::Internal("Search lookup failed")
        })?;

    // ---- Sort results ----
    // For recency: re-sort by (created_at DESC, id DESC) for the tiebreaker
    // (Tantivy's order_by_fast_field does not guarantee id ordering for equal
    // timestamps).  For score: keep Tantivy BM25 ordering.
    if params.sort != "score" {
        media_items.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    }

    // ---- Pagination metadata ----
    let next_cursor = if has_more { Some((offset + media_items.len()).to_string()) } else { None };

    Ok(Json(json!({
        "data": media_items,
        "meta": {
            "next_cursor": next_cursor,
            "next_cursor_id": null,
            "has_more": has_more,
            "total": total_hits as u64,
            "query": query_str,
        }
    })))
}

// ---------------------------------------------------------------------------
// Search execution
// ---------------------------------------------------------------------------

/// Run `query` in a single index traversal, returning `(total_matches, top_docs)`.
///
/// Tantivy's `MultiCollector` feeds both the count and the top-docs collectors
/// during one walk over the matching documents — half the traversal cost of
/// running `Count` and `TopDocs` in separate `searcher.search` calls.
fn search_single_traversal<C>(
    searcher: &tantivy::Searcher,
    query: &dyn tantivy::query::Query,
    top_docs_collector: C,
) -> Result<(usize, Vec<(f32, tantivy::DocAddress)>), tantivy::TantivyError>
where
    C: tantivy::collector::Collector<Fruit = Vec<(f32, tantivy::DocAddress)>>,
{
    let mut multicollector = MultiCollector::new();
    let docs_handle = multicollector.add_collector(top_docs_collector);
    let count_handle = multicollector.add_collector(Count);
    let mut multifruit = searcher.search(query, &multicollector)?;
    let total = count_handle.extract(&mut multifruit);
    let top_docs = docs_handle.extract(&mut multifruit);
    Ok((total, top_docs))
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Look up a Tantivy schema field, mapping a missing field to the standard
/// internal-server-error response (a missing field means index/schema drift).
fn schema_field(
    schema: &tantivy::schema::Schema,
    name: &str,
) -> Result<tantivy::schema::Field, AppError> {
    schema.get_field(name).map_err(|e| {
        tracing::error!(error = %e, field = name, "Missing field in Tantivy schema");
        AppError::Internal("Internal server error")
    })
}

// ---------------------------------------------------------------------------
// Database helpers
// ---------------------------------------------------------------------------

const SQLITE_BIND_LIMIT: usize = 999;

/// Fetch media items from SQLite by IDs — batched to stay within SQLite's
/// parameter limit of ~32K (we use a safe margin of 999 per batch).
///
/// An optional MIME type filter is applied to every chunk.
fn batch_get_media_items(
    conn: &rusqlite::Connection,
    ids: &[String],
    mime_type: Option<&str>,
) -> Result<Vec<MediaItemSummary>, rusqlite::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

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

    let mut results = Vec::with_capacity(ids.len());

    for chunk in ids.chunks(SQLITE_BIND_LIMIT) {
        // Build parameterised IN clause: (?1, ?2, ..., ?N)
        let placeholders: Vec<String> = (1..=chunk.len()).map(|i| format!("?{i}")).collect();
        let mut sql = format!(
            "SELECT id, filename, relative_path, mime_type, width, height, file_size, \
             file_created_at, file_modified_at \
             FROM media_items WHERE id IN ({})",
            placeholders.join(", "),
        );

        if mime_type.is_some() {
            sql.push_str(&format!(" AND mime_type LIKE ?{}", chunk.len() + 1));
        }

        let mut stmt = conn.prepare(&sql)?;

        // Collect dynamic parameters as trait objects, then convert to slice refs
        let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = chunk
            .iter()
            .map(|id| Box::new(id.clone()) as Box<dyn rusqlite::types::ToSql>)
            .collect();
        if let Some(mime) = mime_type {
            values.push(Box::new(mime.to_string()) as Box<dyn rusqlite::types::ToSql>);
        }

        let params: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|v| v.as_ref()).collect();
        let rows = stmt.query_map(params.as_slice(), mapper)?;

        results.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "search_test.rs"]
mod tests;
