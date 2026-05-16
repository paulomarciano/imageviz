//! Bridge between SQLite `media_items` and the Tantivy full-text index.
//!
//! Provides [`full_reindex`] to rebuild the entire Tantivy index from SQLite
//! data, and [`incremental_index`] to process only recently-modified rows.
//!
//! # Architecture
//!
//! Both functions operate synchronously and should be called from
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
use rusqlite::OptionalExtension;
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
    /// Number of rows that were skipped (e.g. unchanged in incremental mode).
    pub skipped_count: usize,
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

    let rows = stmt.query_map([], |row| {
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
    })?;

    // 3. Build and add Tantivy documents
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
            Ok(r) => r,
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

    // 4. Commit and refresh the reader
    index_manager.commit()?;

    Ok(stats)
}

/// Incremental index: process only rows whose `file_modified_at` is newer
/// than the last recorded index timestamp.
///
/// The last-indexed timestamp is tracked in the `config` table under the key
/// `search_last_indexed_at`. If no such config entry exists (e.g. on the first
/// run), this falls back to a full reindex.
///
/// After processing, the `search_last_indexed_at` config value is updated to
/// the current wall-clock time so that subsequent calls only pick up newer
/// modifications.
pub fn incremental_index(
    db: &Connection,
    index_manager: &IndexManager,
) -> Result<ReindexStats, Box<dyn std::error::Error>> {
    // 1. Read the last-indexed timestamp from config
    let last_indexed: Option<String> = db
        .query_row(
            "SELECT value FROM config WHERE key = 'search_last_indexed_at'",
            [],
            |row| row.get(0),
        )
        .optional()?;

    // 2. If no prior index exists, do a full rebuild
    let since = match last_indexed {
        Some(ts) => ts,
        None => {
            let stats = full_reindex(db, index_manager)?;
            record_indexed_at(db)?;
            return Ok(stats);
        }
    };

    // 3. Query items modified after that timestamp
    let mut stmt = db.prepare(
        "SELECT id, filename, mime_type, COALESCE(metadata_json, ''), 
                file_created_at, file_size, width, height 
         FROM media_items 
         WHERE file_modified_at > ?1",
    )?;

    let rows = stmt.query_map(rusqlite::params![since], |row| {
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
    })?;

    // 4. Delete stale Tantivy documents, then add updated ones
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
            Ok(r) => r,
            Err(e) => {
                stats.errors += 1;
                tracing::warn!("Skipping incremental row due to DB error: {e}");
                continue;
            }
        };

        // Delete the old Tantivy document for this id before re-adding
        index_manager.delete_document_by_field("id", &row.id)?;

        let created_at = match parse_iso8601_to_tantivy(&row.created_at_str) {
            Ok(dt) => dt,
            Err(e) => {
                stats.errors += 1;
                tracing::warn!(
                    "Skipping incremental row {}: bad date '{}': {e}",
                    row.id,
                    row.created_at_str
                );
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
            tracing::warn!("Failed to add incremental document: {e}");
        } else {
            stats.indexed_count += 1;
        }
    }

    // 5. Commit and refresh reader
    index_manager.commit()?;

    // 6. Update the tracking timestamp
    record_indexed_at(db)?;

    Ok(stats)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

/// Record the current wall-clock time in the `config` table so that the next
/// incremental index can determine which rows have changed since this run.
fn record_indexed_at(db: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let now = chrono::Utc::now().to_rfc3339();
    db.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES ('search_last_indexed_at', ?1)",
        rusqlite::params![now],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::search::IndexManager;
    use tantivy::collector::TopDocs;
    use tantivy::query::QueryParser;
    use tantivy::DateTime;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Create an in-memory SQLite database with the full schema and
    /// `n` sample media items inserted.
    fn setup_db_with_items(n: usize) -> Connection {
        let mut conn = db::open_in_memory().expect("in-memory DB");
        db::migrations::run_migrations(&mut conn).expect("migrations");

        let mut stmt = conn
            .prepare(
                "INSERT INTO media_items
                    (id, filename, relative_path, mime_type, file_size,
                     file_created_at, file_modified_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            )
            .expect("prepare insert");

        for i in 0..n {
            stmt.execute(rusqlite::params![
                format!("uuid-{i:04}"),
                format!("file_{i}.png"),
                format!("path/file_{i}.png"),
                "image/png",
                1024 * (i + 1),
                "2026-01-15T12:00:00Z",
            ])
            .expect("insert row");
        }

        // Drop stmt before returning conn (stmt borrows conn)
        drop(stmt);
        conn
    }

    /// Create a temporary Tantivy index managed by an `IndexManager`.
    fn setup_tantivy() -> (tempfile::TempDir, IndexManager) {
        let dir = tempfile::tempdir().expect("tempdir");
        let manager =
            IndexManager::open_or_create(&dir.path().join("tantivy")).expect("IndexManager");
        (dir, manager)
    }

    /// Count the total number of documents in the Tantivy index.
    fn count_tantivy_docs(manager: &IndexManager) -> u64 {
        let reader = manager.reader();
        let searcher = reader.searcher();
        searcher
            .segment_readers()
            .iter()
            .map(|sr| sr.num_docs() as u64)
            .sum()
    }

    // -----------------------------------------------------------------------
    // full_reindex tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_full_reindex_populates_index() {
        let conn = setup_db_with_items(10);
        let (_dir, manager) = setup_tantivy();

        let stats = full_reindex(&conn, &manager).expect("full_reindex");
        assert_eq!(stats.indexed_count, 10);
        assert_eq!(stats.errors, 0);

        let total_docs = count_tantivy_docs(&manager);
        assert_eq!(total_docs, 10, "should have 10 Tantivy documents");
    }

    #[test]
    fn test_full_reindex_is_idempotent() {
        let conn = setup_db_with_items(10);
        let (_dir, manager) = setup_tantivy();

        // First reindex
        let s1 = full_reindex(&conn, &manager).expect("first reindex");
        assert_eq!(s1.indexed_count, 10);

        // Second reindex
        let s2 = full_reindex(&conn, &manager).expect("second reindex");
        assert_eq!(s2.indexed_count, 10);

        // Still 10, not 20
        let total_docs = count_tantivy_docs(&manager);
        assert_eq!(total_docs, 10, "reindex should not produce duplicate documents");
    }

    #[test]
    fn test_reindex_handles_empty_db() {
        let mut conn = db::open_in_memory().expect("in-memory DB");
        db::migrations::run_migrations(&mut conn).expect("migrations");
        let (_dir, manager) = setup_tantivy();

        let stats = full_reindex(&conn, &manager).expect("full_reindex on empty DB");
        assert_eq!(stats.indexed_count, 0);
        assert_eq!(stats.errors, 0);

        let total_docs = count_tantivy_docs(&manager);
        assert_eq!(total_docs, 0, "empty DB should result in 0 docs");
    }

    #[test]
    fn test_full_reindex_searches_find_documents() {
        let mut conn = db::open_in_memory().expect("in-memory DB");
        db::migrations::run_migrations(&mut conn).expect("migrations");

        // Insert items, one with searchable metadata
        conn.execute(
            "INSERT INTO media_items
                (id, filename, relative_path, mime_type, file_size,
                 file_created_at, file_modified_at, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                "uuid-search-1",
                "dragon.png",
                "fantasy/dragon.png",
                "image/png",
                20480,
                "2026-03-01T10:00:00Z",
                "2026-03-01T10:00:00Z",
                r#"{"prompt":"a majestic dragon flying over mountains"}"#,
            ],
        )
        .expect("insert dragon item");

        conn.execute(
            "INSERT INTO media_items
                (id, filename, relative_path, mime_type, file_size,
                 file_created_at, file_modified_at, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                "uuid-search-2",
                "castle.png",
                "fantasy/castle.png",
                "image/png",
                15360,
                "2026-03-02T10:00:00Z",
                "2026-03-02T10:00:00Z",
                r#"{"prompt":"a medieval castle at sunset"}"#,
            ],
        )
        .expect("insert castle item");

        let (_dir, manager) = setup_tantivy();
        full_reindex(&conn, &manager).expect("full_reindex");

        // Search for "dragon" — should find exactly 1 match
        let schema = manager.schema().clone();
        let filename = schema.get_field("filename").unwrap();
        let metadata_json = schema.get_field("metadata_json").unwrap();

        let reader = manager.reader();
        let searcher = reader.searcher();
        let query_parser =
            QueryParser::for_index(manager.index(), vec![filename, metadata_json]);
        let query = query_parser.parse_query("dragon").expect("parse query");
        let collector = TopDocs::with_limit(10).order_by_score();
        let top_docs = searcher
            .search(&query, &collector)
            .expect("search should succeed");

        assert_eq!(top_docs.len(), 1, "should find exactly one dragon document");
    }

    // -----------------------------------------------------------------------
    // parse_iso8601_to_tantivy tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_iso8601() {
        use chrono::DateTime as ChronoDateTime;
        use chrono::Utc;

        // Verify correctness by comparison with chrono-based construction
        let chrono_val: ChronoDateTime<Utc> = "2026-01-15T12:30:00Z".parse().unwrap();
        let expected = DateTime::from_timestamp_secs(chrono_val.timestamp());

        let dt = parse_iso8601_to_tantivy("2026-01-15T12:30:00Z")
            .expect("should parse Z-suffixed timestamp");
        assert_eq!(
            dt, expected,
            "Z-suffixed timestamp should match chrono-derived value"
        );

        // RFC 3339 with explicit UTC offset
        let dt_offset = parse_iso8601_to_tantivy("2026-01-15T12:30:00+00:00")
            .expect("should parse +00:00 timestamp");
        assert_eq!(
            dt, dt_offset,
            "Z and +00:00 should produce identical DateTime"
        );

        // Non-UTC timezone (should convert to UTC correctly)
        let dt_non_utc = parse_iso8601_to_tantivy("2026-01-15T14:30:00+02:00")
            .expect("should parse non-UTC timezone");
        assert_eq!(
            dt, dt_non_utc,
            "14:30+02:00 should equal 12:30Z"
        );
    }

    #[test]
    fn test_parse_iso8601_invalid_date() {
        let result = parse_iso8601_to_tantivy("not-a-date");
        assert!(result.is_err(), "invalid date string should return an error");
    }

    // -----------------------------------------------------------------------
    // incremental_index tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_incremental_index_falls_back_to_full() {
        // No config entry — incremental_index should call full_reindex internally
        let conn = setup_db_with_items(5);
        let (_dir, manager) = setup_tantivy();

        let stats = incremental_index(&conn, &manager).expect("incremental_index");
        assert_eq!(stats.indexed_count, 5);
        assert_eq!(stats.errors, 0);

        let total_docs = count_tantivy_docs(&manager);
        assert_eq!(total_docs, 5, "should index all items via fallback to full");
    }

    #[test]
    fn test_incremental_index_only_new_items() {
        let conn = setup_db_with_items(3);
        let (_dir, manager) = setup_tantivy();

        // First call: full reindex (no config entry)
        let s1 = incremental_index(&conn, &manager).expect("first incremental");
        assert_eq!(s1.indexed_count, 3);

        // Add a new item with a later modification time
        conn.execute(
            "INSERT INTO media_items
                (id, filename, relative_path, mime_type, file_size,
                 file_created_at, file_modified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                "uuid-new",
                "new_file.png",
                "new/new_file.png",
                "image/png",
                4096,
                "2026-06-01T10:00:00Z",
                "2026-06-01T10:00:00Z", // after the indexed_at config timestamp
            ],
        )
        .expect("insert new item");

        // Second call: should only index the new item
        let s2 = incremental_index(&conn, &manager).expect("second incremental");
        assert_eq!(s2.indexed_count, 1, "only the new item should be indexed");
        assert_eq!(s2.errors, 0);

        let total_docs = count_tantivy_docs(&manager);
        assert_eq!(total_docs, 4, "total should be 3 original + 1 new");
    }
}
