mod tests {
    use super::super::*;
    use rusqlite::params;

    #[test]
    fn test_create_tables_and_insert() {
        let mut conn = open_in_memory().unwrap();
        migrations::run_migrations(&mut conn).unwrap();

        // Verify tables exist
        let count: i32 =
            conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get::<_, i32>(0)).unwrap();
        assert_eq!(count, 0);

        // Insert a media item
        conn.execute(
            "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params!["test-uuid", "test.png", "2025/test.png", "image/png", 1024, "2025-01-01T00:00:00Z", "2025-01-01T00:00:00Z"],
        ).unwrap();

        // Retrieve it
        let fetched: String = conn
            .query_row(
                "SELECT filename FROM media_items WHERE id = ?1",
                params!["test-uuid"],
                |r| r.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(fetched, "test.png");
    }

    #[test]
    fn test_wal_mode_enabled() {
        // WAL mode only applies to file-based databases, not in-memory.
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = open(&db_path).unwrap();
        let journal_mode: String =
            conn.pragma_query_value(None, "journal_mode", |r| r.get::<_, String>(0)).unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");
        // tempdir is cleaned up when `dir` drops
    }

    #[test]
    fn test_same_relative_path_different_folders_allowed() {
        let mut conn = open_in_memory().unwrap();
        migrations::run_migrations(&mut conn).unwrap();

        // Seed watched folders (needed for FK constraint on folder_id).
        conn.execute(
            "INSERT INTO watched_folders (id, path) VALUES ('fid-a', '/media/a'), ('fid-b', '/media/b')",
            [],
        )
        .unwrap();

        // Insert the same relative_path in folder A — should succeed.
        conn.execute(
            "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, folder_id, file_created_at, file_modified_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params!["uuid-a", "a.png", "shared/path.png", "image/png", 100, "fid-a", "2025-01-01T00:00:00Z", "2025-01-01T00:00:00Z"],
        ).unwrap();

        // Insert the SAME relative_path in folder B — should ALSO succeed
        // (compound UNIQUE on (folder_id, relative_path) allows this).
        conn.execute(
            "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, folder_id, file_created_at, file_modified_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params!["uuid-b", "b.png", "shared/path.png", "image/png", 200, "fid-b", "2025-01-01T00:00:00Z", "2025-01-01T00:00:00Z"],
        ).unwrap();

        // Insert the same relative_path in folder A again — should FAIL
        // (compound UNIQUE prevents duplicates within a single folder).
        let result = conn.execute(
            "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, folder_id, file_created_at, file_modified_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params!["uuid-c", "c.png", "shared/path.png", "image/png", 300, "fid-a", "2025-01-01T00:00:00Z", "2025-01-01T00:00:00Z"],
        );
        assert!(
            result.is_err(),
            "Same relative_path in same folder must be rejected by compound UNIQUE"
        );

        // Verify both items exist (2 rows, not 3).
        let count: i32 =
            conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get::<_, i32>(0)).unwrap();
        assert_eq!(count, 2, "Should have exactly 2 items (one per folder)");
    }

    #[test]
    fn test_config_table() {
        let mut conn = open_in_memory().unwrap();
        migrations::run_migrations(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO config (key, value) VALUES (?1, ?2)",
            params!["watched_folders", r#"[]"#],
        )
        .unwrap();

        let value: String = conn
            .query_row("SELECT value FROM config WHERE key = ?1", params!["watched_folders"], |r| {
                r.get::<_, String>(0)
            })
            .unwrap();
        assert_eq!(value, "[]");
    }

    #[test]
    fn test_migration_idempotent() {
        let mut conn = open_in_memory().unwrap();
        // Run migrations twice
        migrations::run_migrations(&mut conn).unwrap();
        migrations::run_migrations(&mut conn).unwrap();
        // user_version should be 4 after both runs (v1..v4 are cumulative)
        let version: i32 =
            conn.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0)).unwrap();
        assert_eq!(version, 4);
        // Tables should still exist (no duplicate errors)
        let count: i32 =
            conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get::<_, i32>(0)).unwrap();
        assert_eq!(count, 0);
    }
}
