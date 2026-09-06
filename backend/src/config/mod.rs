pub mod settings;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WatchedFolder {
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
    /// Stable UUID assigned by the server on PUT /config.
    /// Persisted in the `watched_folders` table.
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    #[serde(default)]
    pub watched_folders: Vec<WatchedFolder>,
}

/// Load configuration from the `watched_folders` table — the single source
/// of truth for watched-folder configuration.
///
/// Returns `AppConfig::default()` (empty watched folders) when the table has
/// no rows. Folders are returned in the order they were last persisted.
pub fn load_config(conn: &Connection) -> Result<AppConfig, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT id, path, label FROM watched_folders ORDER BY rowid")?;
    let folders = stmt
        .query_map([], |row| {
            Ok(WatchedFolder { id: Some(row.get(0)?), path: row.get(1)?, label: row.get(2)? })
        })?
        .filter_map(|row| row.ok())
        .collect();
    Ok(AppConfig { watched_folders: folders })
}

/// Ensure every folder in `config` has a stable UUID and persist the rows to
/// the `watched_folders` table (upsert-only).
///
/// Rows whose paths are absent from `config` are intentionally left in place:
/// deletion is [`save_config`]'s responsibility, because the indexer calls
/// this function on every run and never removes folders.
pub fn assign_folder_ids(conn: &Connection, config: &mut AppConfig) -> Result<(), rusqlite::Error> {
    for folder in &mut config.watched_folders {
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM watched_folders WHERE path = ?1",
                params![folder.path],
                |r| r.get(0),
            )
            .optional()?
            .flatten();

        folder.id = Some(existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()));
    }

    for folder in &config.watched_folders {
        if let Some(ref id) = folder.id {
            // INSERT OR REPLACE is FK-safe here: the id is always either the
            // existing row's own id (from the by-path lookup above) or a
            // fresh UUID, so the delete+reinsert never orphans
            // media_items.folder_id children. Do NOT swap this for a
            // differing id — and do not replace it with ON CONFLICT DO
            // UPDATE, which would preserve rowids and break the "GET returns
            // the last PUT's folder order" contract (load_config orders by
            // rowid).
            conn.execute(
                "INSERT OR REPLACE INTO watched_folders (id, path, label) VALUES (?1, ?2, ?3)",
                params![id, folder.path, folder.label],
            )?;
        }
    }

    Ok(())
}

/// Build a map from watched-folder path to its stable UUID.
///
/// Used by the indexer and watcher to resolve `folder_id` for files found
/// inside each watched folder.
pub fn folder_id_map(config: &AppConfig) -> std::collections::HashMap<String, String> {
    config
        .watched_folders
        .iter()
        .filter_map(|f| f.id.as_ref().map(|id| (f.path.clone(), id.clone())))
        .collect()
}

/// Replace the stored configuration with `config` in a single transactional
/// write: assign stable ids, upsert every row, and delete rows for paths that
/// are no longer configured.
///
/// Existing paths keep their UUID so `media_items.folder_id` references stay
/// valid across configuration updates. On error the database changes roll
/// back, but `config` may already carry freshly assigned (uncommitted) ids —
/// do not reuse the struct after a failed call.
pub fn save_config(conn: &Connection, config: &mut AppConfig) -> Result<(), rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;
    assign_folder_ids(&tx, config)?;

    let keep: std::collections::HashSet<&str> =
        config.watched_folders.iter().map(|f| f.path.as_str()).collect();
    let existing_paths: Vec<String> = {
        let mut stmt = tx.prepare("SELECT path FROM watched_folders")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.filter_map(|row| row.ok()).collect()
    };
    for path in existing_paths {
        if !keep.contains(path.as_str()) {
            // media_items.folder_id is an FK child of watched_folders.id —
            // remove dependents first or the parent DELETE fails (FK = ON).
            tx.execute(
                "DELETE FROM media_items WHERE folder_id IN \
                 (SELECT id FROM watched_folders WHERE path = ?1)",
                params![path],
            )?;
            tx.execute("DELETE FROM watched_folders WHERE path = ?1", params![path])?;
        }
    }

    tx.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory connection with the full schema applied and foreign keys
    /// enabled (matching production pool posture).
    fn migrated_conn() -> Connection {
        let mut conn = crate::db::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&mut conn).unwrap();
        conn
    }

    fn folder(path: &str, label: Option<&str>) -> WatchedFolder {
        WatchedFolder { path: path.to_string(), label: label.map(str::to_string), id: None }
    }

    #[test]
    fn test_default_config_empty() {
        let config = AppConfig::default();
        assert!(config.watched_folders.is_empty());
    }

    #[test]
    fn test_load_config_returns_empty_when_no_folders() {
        let conn = migrated_conn();
        let config = load_config(&conn).unwrap();
        assert!(config.watched_folders.is_empty());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let conn = migrated_conn();

        let mut config = AppConfig {
            watched_folders: vec![
                folder("/tmp/images", Some("Test images")),
                folder("/tmp/videos", None),
            ],
        };
        save_config(&conn, &mut config).unwrap();
        assert!(
            config.watched_folders.iter().all(|f| f.id.is_some()),
            "save_config must assign ids"
        );

        let loaded = load_config(&conn).unwrap();
        assert_eq!(loaded.watched_folders.len(), 2);
        assert_eq!(loaded.watched_folders[0].path, "/tmp/images");
        assert_eq!(loaded.watched_folders[0].label.as_deref(), Some("Test images"));
        assert_eq!(loaded.watched_folders[1].path, "/tmp/videos");
        assert!(loaded.watched_folders[1].label.is_none());
        // PUT order is preserved and ids survive the roundtrip.
        assert_eq!(loaded.watched_folders[0].id, config.watched_folders[0].id);
        assert_eq!(loaded.watched_folders[1].id, config.watched_folders[1].id);
    }

    #[test]
    fn test_save_config_preserves_existing_path_ids() {
        let conn = migrated_conn();
        conn.execute(
            "INSERT INTO watched_folders (id, path, label) VALUES ('fid-known', '/known', NULL)",
            [],
        )
        .unwrap();

        let mut config =
            AppConfig { watched_folders: vec![folder("/known", None), folder("/new", None)] };
        save_config(&conn, &mut config).unwrap();

        // The pre-existing path keeps its stable id (media_items.folder_id
        // references depend on this).
        assert_eq!(config.watched_folders[0].id.as_deref(), Some("fid-known"));
        assert!(config.watched_folders[1].id.is_some());

        let loaded = load_config(&conn).unwrap();
        assert_eq!(loaded.watched_folders[0].id.as_deref(), Some("fid-known"));
    }

    #[test]
    fn test_save_config_removes_paths_no_longer_configured() {
        let conn = migrated_conn();

        let mut first =
            AppConfig { watched_folders: vec![folder("/first", None), folder("/second", None)] };
        save_config(&conn, &mut first).unwrap();

        let mut second = AppConfig { watched_folders: vec![folder("/second", None)] };
        save_config(&conn, &mut second).unwrap();

        let loaded = load_config(&conn).unwrap();
        assert_eq!(loaded.watched_folders.len(), 1);
        assert_eq!(loaded.watched_folders[0].path, "/second");
    }

    #[test]
    fn test_save_config_removal_with_referencing_media_items() {
        let conn = migrated_conn();

        conn.execute("INSERT INTO watched_folders (id, path) VALUES ('fid-1', '/gone')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, \
                 folder_id, file_created_at, file_modified_at) \
             VALUES ('m-1', 'a.png', 'a.png', 'image/png', 1, 'fid-1', \
                 '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        // Removing the folder must succeed even though media items reference
        // it: media_items.folder_id is an FK child of watched_folders.id, so
        // the folder's media rows are removed in the same transaction before
        // the folder row.
        let mut config = AppConfig { watched_folders: vec![] };
        save_config(&conn, &mut config).unwrap();

        let folders: i64 =
            conn.query_row("SELECT COUNT(*) FROM watched_folders", [], |r| r.get(0)).unwrap();
        let items: i64 =
            conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0)).unwrap();
        assert_eq!((folders, items), (0, 0), "folder and its media rows must be removed together");
    }

    #[test]
    fn test_save_config_keeps_media_items_when_folder_still_configured() {
        let conn = migrated_conn();

        conn.execute("INSERT INTO watched_folders (id, path) VALUES ('fid-1', '/stay')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, \
                 folder_id, file_created_at, file_modified_at) \
             VALUES ('m-1', 'a.png', 'a.png', 'image/png', 1, 'fid-1', \
                 '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        // Re-PUT the same folder (label changed) — it must keep its id and
        // its media rows.
        let mut config = AppConfig { watched_folders: vec![folder("/stay", Some("renamed"))] };
        save_config(&conn, &mut config).unwrap();

        let items: i64 = conn
            .query_row("SELECT COUNT(*) FROM media_items WHERE folder_id = 'fid-1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let id: String = conn
            .query_row("SELECT id FROM watched_folders WHERE path = '/stay'", [], |r| r.get(0))
            .unwrap();
        let label: Option<String> = conn
            .query_row("SELECT label FROM watched_folders WHERE path = '/stay'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            (items, id.as_str(), label.as_deref()),
            (1, "fid-1", Some("renamed")),
            "kept folder must keep its id and media rows"
        );
    }

    #[test]
    fn test_assign_folder_ids_is_idempotent() {
        let conn = migrated_conn();

        let mut config = AppConfig { watched_folders: vec![folder("/stable", None)] };
        assign_folder_ids(&conn, &mut config).unwrap();
        let first_id = config.watched_folders[0].id.clone().unwrap();

        assign_folder_ids(&conn, &mut config).unwrap();
        assert_eq!(config.watched_folders[0].id.as_deref(), Some(first_id.as_str()));
    }

    #[test]
    fn test_deserialize_optional_label_defaults_to_none() {
        let json = r#"{"watched_folders": [{"path": "/foo"}]}"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.watched_folders.len(), 1);
        assert_eq!(config.watched_folders[0].path, "/foo");
        assert!(config.watched_folders[0].label.is_none());
    }
}
