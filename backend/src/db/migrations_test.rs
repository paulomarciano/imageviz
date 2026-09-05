//! Tests for migration v004: one-time import of the legacy
//! `config(key='watched_folders')` JSON blob into the `watched_folders` table.

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
        assert_eq!(user_version(&conn), 4);
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

        assert_eq!(user_version(&conn), 4);
        assert!(blob_row(&conn).is_some(), "unparseable blob must be preserved");
        assert!(table_rows(&conn).is_empty(), "nothing can be imported from a corrupt blob");
    }

    #[test]
    fn test_v004_noop_when_no_blob() {
        let mut conn = legacy_v3_conn();

        run_migrations(&mut conn).expect("migrations");

        assert_eq!(user_version(&conn), 4);
        assert!(blob_row(&conn).is_none());
        assert!(table_rows(&conn).is_empty());
    }

    #[test]
    fn test_v004_runs_exactly_once() {
        let mut conn = open_in_memory().expect("in-memory DB");

        // Fresh DB: full migration path, twice (idempotency regression).
        run_migrations(&mut conn).unwrap();
        run_migrations(&mut conn).unwrap();

        assert_eq!(user_version(&conn), 4);
    }
}
