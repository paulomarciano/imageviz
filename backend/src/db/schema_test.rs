#[cfg(test)]
mod tests {
    use super::super::*;
    use rusqlite::params;

    #[test]
    fn test_create_tables_and_insert() {
        let conn = open_in_memory().unwrap();
        migrations::run_migrations(&conn).unwrap();

        // Verify tables exist
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get::<_, i32>(0))
            .unwrap();
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
        let journal_mode: String = conn
            .pragma_query_value(None, "journal_mode", |r| r.get::<_, String>(0))
            .unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");
        // tempdir is cleaned up when `dir` drops
    }

    #[test]
    fn test_unique_relative_path() {
        let conn = open_in_memory().unwrap();
        migrations::run_migrations(&conn).unwrap();

        conn.execute(
            "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params!["uuid-1", "a.png", "same/path.png", "image/png", 100, "2025-01-01T00:00:00Z", "2025-01-01T00:00:00Z"],
        ).unwrap();

        let result = conn.execute(
            "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params!["uuid-2", "b.png", "same/path.png", "image/png", 200, "2025-01-01T00:00:00Z", "2025-01-01T00:00:00Z"],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_config_table() {
        let conn = open_in_memory().unwrap();
        migrations::run_migrations(&conn).unwrap();

        conn.execute(
            "INSERT INTO config (key, value) VALUES (?1, ?2)",
            params!["watched_folders", r#"[]"#],
        )
        .unwrap();

        let value: String = conn
            .query_row(
                "SELECT value FROM config WHERE key = ?1",
                params!["watched_folders"],
                |r| r.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(value, "[]");
    }

    #[test]
    fn test_migration_idempotent() {
        let conn = open_in_memory().unwrap();
        // Run migrations twice
        migrations::run_migrations(&conn).unwrap();
        migrations::run_migrations(&conn).unwrap();
        // Should not error — tables already exist
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get::<_, i32>(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
