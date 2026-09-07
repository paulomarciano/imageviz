//! Tests for migration v004 (one-time import of the legacy
//! `config(key='watched_folders')` JSON blob into the `watched_folders`
//! table) and migration v005 (drop of the write-only `thumbnail_path`
//! column).

mod tests {
    use super::super::migrations::run_migrations;
    use super::super::{open_in_memory, schema};
    use rusqlite::params;
    use serde_json::json;

    /// Build a database that looks like a pre-7.12 install: schema complete
    /// through migration v003 (`user_version = 3`) with the legacy config
    /// blob as the only folder store.
    fn legacy_v3_conn() -> rusqlite::Connection {
        let conn = open_in_memory().expect("in-memory DB");
        conn.execute_batch(schema::CREATE_MEDIA_ITEMS).unwrap();
        conn.execute_batch(schema::CREATE_CONFIG_TABLE).unwrap();
        conn.execute_batch(schema::MIGRATION_V002).unwrap();
        conn.pragma_update(None, "user_version", 3).unwrap();
        conn
    }

    fn user_version(conn: &rusqlite::Connection) -> i32 {
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0)).unwrap()
    }

    fn table_rows(conn: &rusqlite::Connection) -> Vec<(String, String, Option<String>)> {
        let mut stmt =
            conn.prepare("SELECT id, path, label FROM watched_folders ORDER BY path").unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    }

    fn blob_row(conn: &rusqlite::Connection) -> Option<String> {
        conn.query_row("SELECT value FROM config WHERE key = 'watched_folders'", [], |r| r.get(0))
            .ok()
    }

    #[test]
    fn test_v004_imports_legacy_blob_and_deletes_row() {
        let mut conn = legacy_v3_conn();

        let blob = json!({
            "watched_folders": [
                {"path": "/legacy/only", "label": "Legacy folder", "id": "fid-legacy"}
            ]
        })
        .to_string();
        conn.execute(
            "INSERT INTO config (key, value) VALUES ('watched_folders', ?1)",
            params![blob],
        )
        .unwrap();

        run_migrations(&mut conn).expect("migrations");

        // The folder is imported with its blob id preserved (media_items
        // rows may already reference it).
        assert_eq!(
            table_rows(&conn),
            vec![("fid-legacy".into(), "/legacy/only".into(), Some("Legacy folder".into()))]
        );
        // The legacy blob row is gone.
        assert!(blob_row(&conn).is_none(), "legacy blob row must be deleted after import");
        assert_eq!(user_version(&conn), 5);
    }

    #[test]
    fn test_v004_table_wins_on_conflict() {
        let mut conn = legacy_v3_conn();

        // Table already tracks /conflict with its own id and label.
        conn.execute(
            "INSERT INTO watched_folders (id, path, label) VALUES ('fid-table', '/conflict', 'From table')",
            [],
        )
        .unwrap();

        // Blob has the same path with a different id/label, plus a new path.
        let blob = json!({
            "watched_folders": [
                {"path": "/conflict", "label": "From blob", "id": "fid-blob"},
                {"path": "/blob/new", "id": "fid-new"}
            ]
        })
        .to_string();
        conn.execute(
            "INSERT INTO config (key, value) VALUES ('watched_folders', ?1)",
            params![blob],
        )
        .unwrap();

        run_migrations(&mut conn).expect("migrations");

        // Table row is untouched (table wins); blob-only path is imported
        // with its blob id.
        let rows = table_rows(&conn);
        assert_eq!(rows.len(), 2, "conflicting path must not be duplicated");
        assert!(
            rows.contains(&("fid-table".into(), "/conflict".into(), Some("From table".into()))),
            "table row must win on conflict, got: {rows:?}"
        );
        assert!(
            rows.contains(&("fid-new".into(), "/blob/new".into(), None)),
            "blob-only path must be imported, got: {rows:?}"
        );
        assert!(blob_row(&conn).is_none());
    }

    #[test]
    fn test_v004_blob_id_collision_gets_fresh_id() {
        let mut conn = legacy_v3_conn();

        // The table already uses id 'fid-x' for a different path.
        conn.execute("INSERT INTO watched_folders (id, path) VALUES ('fid-x', '/table/path')", [])
            .unwrap();

        // The blob references the same id for another path (drift/corruption).
        let blob = json!({"watched_folders": [{"path": "/blob/other", "id": "fid-x"}]}).to_string();
        conn.execute(
            "INSERT INTO config (key, value) VALUES ('watched_folders', ?1)",
            params![blob],
        )
        .unwrap();

        run_migrations(&mut conn).expect("migrations");

        // The folder must still be imported — under a fresh id, not silently
        // dropped by the PK conflict.
        let rows = table_rows(&conn);
        assert_eq!(rows.len(), 2, "both folders must exist, got: {rows:?}");
        assert!(
            rows.iter().any(|(id, path, _)| path == "/blob/other" && id != "fid-x"),
            "blob folder whose id is taken must be re-imported under a fresh id, got: {rows:?}"
        );
        assert!(blob_row(&conn).is_none());
    }

    #[test]
    fn test_v004_duplicate_path_inside_blob_is_not_duplicated() {
        let mut conn = legacy_v3_conn();

        let blob = json!({
            "watched_folders": [
                {"path": "/dup", "id": "fid-a"},
                {"path": "/dup", "id": "fid-b"}
            ]
        })
        .to_string();
        conn.execute(
            "INSERT INTO config (key, value) VALUES ('watched_folders', ?1)",
            params![blob],
        )
        .unwrap();

        run_migrations(&mut conn).expect("migrations");

        let rows = table_rows(&conn);
        assert_eq!(rows.len(), 1, "duplicate path inside the blob must not be inserted twice");
        assert_eq!(rows[0].1, "/dup");
        assert!(blob_row(&conn).is_none());
    }

    #[test]
    fn test_v004_corrupt_blob_is_left_in_place() {
        let mut conn = legacy_v3_conn();

        conn.execute(
            "INSERT INTO config (key, value) VALUES ('watched_folders', ?1)",
            params!["{not valid json"],
        )
        .unwrap();

        // Migration must succeed (not fail the startup), keep the blob for
        // inspection, and still advance the version.
        run_migrations(&mut conn).expect("migrations must not fail on corrupt blob");

        assert_eq!(user_version(&conn), 5);
        assert!(blob_row(&conn).is_some(), "unparseable blob must be preserved");
        assert!(table_rows(&conn).is_empty(), "nothing can be imported from a corrupt blob");
    }

    #[test]
    fn test_v004_noop_when_no_blob() {
        let mut conn = legacy_v3_conn();

        run_migrations(&mut conn).expect("migrations");

        assert_eq!(user_version(&conn), 5);
        assert!(blob_row(&conn).is_none());
        assert!(table_rows(&conn).is_empty());
    }

    #[test]
    fn test_v004_runs_exactly_once() {
        let mut conn = open_in_memory().expect("in-memory DB");

        // Fresh DB: full migration path, twice (idempotency regression).
        run_migrations(&mut conn).unwrap();
        run_migrations(&mut conn).unwrap();

        assert_eq!(user_version(&conn), 5);
    }

    /// Build a database that looks like a v0.7.0 install: the full schema
    /// chain through migration v003, checkpointed at `user_version = 4`
    /// (post-v004), including the now-obsolete `thumbnail_path` column.
    fn legacy_v4_conn() -> rusqlite::Connection {
        let conn = open_in_memory().expect("in-memory DB");
        conn.execute_batch(schema::CREATE_MEDIA_ITEMS).unwrap();
        conn.execute_batch(schema::CREATE_CONFIG_TABLE).unwrap();
        conn.execute_batch(schema::MIGRATION_V002).unwrap();
        conn.execute_batch(schema::MIGRATION_V003).unwrap();
        conn.pragma_update(None, "user_version", 4).unwrap();
        conn
    }

    #[test]
    fn test_v005_drops_thumbnail_path_and_preserves_rows() {
        let conn = legacy_v4_conn();

        // Seed a watched folder + a media item carrying thumbnail_path data,
        // as a v0.7.0 install would have after serving thumbnails.
        conn.execute("INSERT INTO watched_folders (id, path) VALUES ('fid-a', '/media/a')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, \
             file_created_at, file_modified_at, indexed_at, thumbnail_path, folder_id) \
             VALUES ('uuid-1', 'a.png', 'a.png', 'image/png', 100, \
             '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', \
             '/cache/abcdef1234567890_200.webp', 'fid-a')",
            [],
        )
        .unwrap();

        let mut conn = conn;
        run_migrations(&mut conn).expect("migrations");

        assert_eq!(user_version(&conn), 5);

        // The column is gone.
        let has_column: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('media_items') \
                 WHERE name = 'thumbnail_path'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_column, 0, "thumbnail_path column must be dropped");

        // Existing rows survive intact.
        let (count, filename, relative_path, mime_type): (i64, String, String, String) = conn
            .query_row(
                "SELECT COUNT(*), filename, relative_path, mime_type FROM media_items",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(count, 1, "row must survive the migration");
        assert_eq!(filename, "a.png");
        assert_eq!(relative_path, "a.png");
        assert_eq!(mime_type, "image/png");
    }
}
