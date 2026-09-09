//! Bridge between SQLite `media_items` and the Tantivy full-text index.
//!
//! Provides [`full_reindex`] to rebuild the entire Tantivy index from SQLite
//! data.
//!
//! # Architecture
//!
//! The function operates synchronously and should be called from
//! [`tokio::task::spawn_blocking`] when used in async contexts to avoid
//! blocking the Tokio runtime.
//!
//! # Idempotency
//!
//! `full_reindex` first deletes all existing Tantivy documents, then
//! re-adds every SQLite row — calling it twice yields the same result
//! as calling it once (no duplicate documents).

use crate::search::IndexManager;
use rusqlite::Connection;
use tantivy::DateTime;
use tantivy::doc;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Statistics from a single index-population run.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ReindexStats {
    /// Number of documents successfully added to the Tantivy index.
    pub indexed_count: usize,
    /// Number of errors encountered.
    pub errors: usize,
}

/// Intermediate row data read from `media_items`.
struct MediaItemRow {
    id: String,
    filename: String,
    mime_type: String,
    metadata_json: String,
    created_at_str: String,
    file_size: i64,
    width: Option<i64>,
    height: Option<i64>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Full reindex: delete all Tantivy documents, then re-add every row from
/// the `media_items` table.
///
/// This is **idempotent** — calling it twice produces the same final index
/// state (no duplicate documents).
pub fn full_reindex(
    db: &Connection,
    index_manager: &IndexManager,
) -> Result<ReindexStats, Box<dyn std::error::Error>> {
    // 1. Delete all existing Tantivy documents
    index_manager.delete_all_documents()?;

    // 2. Query all media items from SQLite
    let mut stmt = db.prepare(
        "SELECT id, filename, mime_type, COALESCE(metadata_json, ''), 
                file_created_at, file_size, width, height 
         FROM media_items",
    )?;
    let rows = stmt.query_map([], map_media_item_row)?;

    // 3. Build and add Tantivy documents
    let stats = index_rows(index_manager, rows)?;

    // 4. Commit and refresh the reader
    index_manager.commit()?;

    Ok(stats)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map a `media_items` row to the intermediate representation used by
/// [`index_rows`].
fn map_media_item_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MediaItemRow> {
    Ok(MediaItemRow {
        id: row.get(0)?,
        filename: row.get(1)?,
        mime_type: row.get(2)?,
        metadata_json: row.get(3)?,
        created_at_str: row.get(4)?,
        file_size: row.get(5)?,
        width: row.get(6)?,
        height: row.get(7)?,
    })
}

/// Build Tantivy documents from `rows` and add them to the index.
///
/// The single home of the schema-field lookups, the row → `doc!` mapping, and
/// per-row error accounting: a row that fails to read, to parse, or to add is
/// counted in [`ReindexStats::errors`] and logged, never propagated. Only
/// failures that abort the whole run (e.g. an unknown schema field) return
/// `Err`.
fn index_rows<I>(
    index_manager: &IndexManager,
    rows: I,
) -> Result<ReindexStats, Box<dyn std::error::Error>>
where
    I: Iterator<Item = rusqlite::Result<MediaItemRow>>,
{
    let schema = index_manager.schema();
    let id_field = schema.get_field("id")?;
    let filename_field = schema.get_field("filename")?;
    let mime_type_field = schema.get_field("mime_type")?;
    let metadata_json_field = schema.get_field("metadata_json")?;
    let created_at_field = schema.get_field("created_at")?;
    let file_size_field = schema.get_field("file_size")?;
    let width_field = schema.get_field("width")?;
    let height_field = schema.get_field("height")?;

    let mut stats = ReindexStats::default();

    for row_result in rows {
        let row = match row_result {
            Ok(row) => row,
            Err(e) => {
                stats.errors += 1;
                tracing::warn!("Skipping row due to DB error: {e}");
                continue;
            }
        };

        let created_at = match parse_iso8601_to_tantivy(&row.created_at_str) {
            Ok(dt) => dt,
            Err(e) => {
                stats.errors += 1;
                tracing::warn!("Skipping row {}: bad date '{}': {e}", row.id, row.created_at_str);
                continue;
            }
        };

        let doc = tantivy::doc!(
            id_field => row.id,
            filename_field => row.filename,
            mime_type_field => row.mime_type,
            metadata_json_field => row.metadata_json,
            created_at_field => created_at,
            file_size_field => row.file_size as u64,
            width_field => row.width.unwrap_or(0) as u64,
            height_field => row.height.unwrap_or(0) as u64,
        );

        if let Err(e) = index_manager.add_document(doc) {
            stats.errors += 1;
            tracing::warn!("Failed to add document: {e}");
        } else {
            stats.indexed_count += 1;
        }
    }

    Ok(stats)
}

/// Parse an ISO 8601 / RFC 3339 timestamp string to a Tantivy DateTime.
///
/// Tantivy stores dates as `i64` microseconds since the Unix epoch.  This
/// helper accepts any format supported by chrono's `FromStr` implementation
/// for `DateTime<Utc>` (e.g. `"2026-01-15T12:30:00Z"` or
/// `"2026-01-15T12:30:00+00:00"`).
fn parse_iso8601_to_tantivy(ts: &str) -> Result<DateTime, Box<dyn std::error::Error>> {
    use chrono::DateTime as ChronoDateTime;
    use chrono::Utc;

    let dt: ChronoDateTime<Utc> = ts.parse()?;
    let timestamp = dt.timestamp(); // Unix seconds
    Ok(DateTime::from_timestamp_secs(timestamp))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "indexer_test.rs"]
mod tests;
