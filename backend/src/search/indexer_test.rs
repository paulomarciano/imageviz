//! Tests for [`crate::search::indexer`] — `full_reindex` is the contract.
//!
//! Wave 8.14 (Path B): the dead Tantivy-side `incremental_index` was removed in
//! wave 8.13; these tests pin `full_reindex` behavior through the `index_rows`
//! core extraction (field lookups, row mapping, `doc!` construction, and
//! per-row error accounting must remain unchanged).

use super::*;
use crate::db;
use crate::search::IndexManager;
use tantivy::DateTime;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
    let manager = IndexManager::open_or_create(&dir.path().join("tantivy"), 50_000_000)
        .expect("IndexManager");
    (dir, manager)
}

/// Count the total number of documents in the Tantivy index.
fn count_tantivy_docs(manager: &IndexManager) -> u64 {
    let reader = manager.reader();
    let searcher = reader.searcher();
    searcher.segment_readers().iter().map(|sr| sr.num_docs() as u64).sum()
}

// ---------------------------------------------------------------------------
// full_reindex tests
// ---------------------------------------------------------------------------

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
    let query_parser = QueryParser::for_index(manager.index(), vec![filename, metadata_json]);
    let query = query_parser.parse_query("dragon").expect("parse query");
    let collector = TopDocs::with_limit(10).order_by_score();
    let top_docs = searcher.search(&query, &collector).expect("search should succeed");

    assert_eq!(top_docs.len(), 1, "should find exactly one dragon document");
}

// ---------------------------------------------------------------------------
// Per-row error accounting (pinned by wave 8.14)
// ---------------------------------------------------------------------------

#[test]
fn test_full_reindex_counts_unparseable_dates_as_errors() {
    let mut conn = db::open_in_memory().expect("in-memory DB");
    db::migrations::run_migrations(&mut conn).expect("migrations");

    // One well-formed row + one row whose `file_created_at` cannot be parsed.
    for (id, created_at) in [("uuid-good", "2026-01-15T12:00:00Z"), ("uuid-bad", "not-a-date")] {
        conn.execute(
            "INSERT INTO media_items
                (id, filename, relative_path, mime_type, file_size,
                 file_created_at, file_modified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            rusqlite::params![
                id,
                format!("{id}.png"),
                format!("path/{id}.png"),
                "image/png",
                1024,
                created_at,
            ],
        )
        .expect("insert row");
    }

    let (_dir, manager) = setup_tantivy();
    let stats = full_reindex(&conn, &manager).expect("full_reindex");

    assert_eq!(stats.indexed_count, 1, "only the well-formed row is indexed");
    assert_eq!(stats.errors, 1, "the unparseable date must count as one error");
    assert_eq!(count_tantivy_docs(&manager), 1, "the bad row must not reach the index");
}

// ---------------------------------------------------------------------------
// parse_iso8601_to_tantivy tests
// ---------------------------------------------------------------------------

#[test]
fn test_parse_iso8601() {
    use chrono::DateTime as ChronoDateTime;
    use chrono::Utc;

    // Verify correctness by comparison with chrono-based construction
    let chrono_val: ChronoDateTime<Utc> = "2026-01-15T12:30:00Z".parse().unwrap();
    let expected = DateTime::from_timestamp_secs(chrono_val.timestamp());

    let dt = parse_iso8601_to_tantivy("2026-01-15T12:30:00Z")
        .expect("should parse Z-suffixed timestamp");
    assert_eq!(dt, expected, "Z-suffixed timestamp should match chrono-derived value");

    // RFC 3339 with explicit UTC offset
    let dt_offset = parse_iso8601_to_tantivy("2026-01-15T12:30:00+00:00")
        .expect("should parse +00:00 timestamp");
    assert_eq!(dt, dt_offset, "Z and +00:00 should produce identical DateTime");

    // Non-UTC timezone (should convert to UTC correctly)
    let dt_non_utc = parse_iso8601_to_tantivy("2026-01-15T14:30:00+02:00")
        .expect("should parse non-UTC timezone");
    assert_eq!(dt, dt_non_utc, "14:30+02:00 should equal 12:30Z");
}

#[test]
fn test_parse_iso8601_invalid_date() {
    let result = parse_iso8601_to_tantivy("not-a-date");
    assert!(result.is_err(), "invalid date string should return an error");
}
