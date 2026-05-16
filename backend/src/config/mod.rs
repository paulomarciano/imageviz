pub mod settings;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WatchedFolder {
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    #[serde(default)]
    pub watched_folders: Vec<WatchedFolder>,
}

/// Load configuration from the database.
///
/// Returns `AppConfig::default()` (empty watched folders) when no config row exists.
pub fn load_config(conn: &Connection) -> Result<AppConfig, rusqlite::Error> {
    let result: Result<String, rusqlite::Error> =
        conn.query_row("SELECT value FROM config WHERE key = 'watched_folders'", [], |r| r.get(0));

    match result {
        Ok(json) => serde_json::from_str(&json)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e))),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(AppConfig::default()),
        Err(e) => Err(e),
    }
}

/// Save configuration to the database.
///
/// The entire `watched_folders` array is serialized as JSON and stored in a single
/// config row keyed by `'watched_folders'`.
pub fn save_config(conn: &Connection, config: &AppConfig) -> Result<(), rusqlite::Error> {
    let json = serde_json::to_string(config)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES ('watched_folders', ?1)",
        params![json],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_empty() {
        let config = AppConfig::default();
        assert!(config.watched_folders.is_empty());
    }

    #[test]
    fn test_load_config_returns_default_when_no_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT);").unwrap();
        let config = load_config(&conn).unwrap();
        assert!(config.watched_folders.is_empty());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT);").unwrap();

        let folders = vec![
            WatchedFolder {
                path: "/tmp/images".to_string(),
                label: Some("Test images".to_string()),
            },
            WatchedFolder { path: "/tmp/videos".to_string(), label: None },
        ];
        let config = AppConfig { watched_folders: folders };

        save_config(&conn, &config).unwrap();

        let loaded = load_config(&conn).unwrap();
        assert_eq!(loaded.watched_folders.len(), 2);
        assert_eq!(loaded.watched_folders[0].path, "/tmp/images");
        assert_eq!(loaded.watched_folders[0].label.as_deref(), Some("Test images"));
        assert_eq!(loaded.watched_folders[1].path, "/tmp/videos");
        assert!(loaded.watched_folders[1].label.is_none());
    }

    #[test]
    fn test_save_overwrites_previous() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT);").unwrap();

        let first = AppConfig {
            watched_folders: vec![WatchedFolder { path: "/first".to_string(), label: None }],
        };
        save_config(&conn, &first).unwrap();

        let second = AppConfig {
            watched_folders: vec![WatchedFolder { path: "/second".to_string(), label: None }],
        };
        save_config(&conn, &second).unwrap();

        let loaded = load_config(&conn).unwrap();
        assert_eq!(loaded.watched_folders.len(), 1);
        assert_eq!(loaded.watched_folders[0].path, "/second");
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
