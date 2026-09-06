use rusqlite::{Connection, OptionalExtension, params};

/// Key of the legacy watched-folder JSON blob in the `config` table.
///
/// Pre-7.12 installs persisted folder configuration as a JSON blob in
/// `config` *and* as rows in `watched_folders`. Migration v004 makes the
/// table the single source of truth by importing the blob once and deleting
/// the row.
const LEGACY_WATCHED_FOLDERS_KEY: &str = "watched_folders";

/// Run all pending migrations, advancing `PRAGMA user_version` as each completes.
///
/// Migrations are wrapped in a single transaction so that a crash mid-migration
/// never leaves the database in a partial state. Each migration step uses
/// `CREATE TABLE IF NOT EXISTS` and `CREATE INDEX IF NOT EXISTS` so that
/// re-running a completed version is safe.
pub fn run_migrations(conn: &mut Connection) -> Result<(), rusqlite::Error> {
    let version: i32 =
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0)).unwrap_or(0);

    if version < 1 {
        let tx = conn.transaction()?;
        tx.execute_batch(super::schema::CREATE_MEDIA_ITEMS)?;
        tx.execute_batch(super::schema::CREATE_CONFIG_TABLE)?;
        tx.execute_batch(super::schema::CREATE_IDX_MEDIA_SORT)?;
        tx.execute_batch(super::schema::CREATE_IDX_MEDIA_PATH)?;
        tx.execute_batch(super::schema::CREATE_IDX_MEDIA_MIME)?;
        tx.pragma_update(None, "user_version", 1)?;
        tx.commit()?;
    }

    if version < 2 {
        let tx = conn.transaction()?;
        tx.execute_batch(super::schema::MIGRATION_V002)?;
        tx.pragma_update(None, "user_version", 2)?;
        tx.commit()?;
    }

    if version < 3 {
        let tx = conn.transaction()?;
        tx.execute_batch(super::schema::MIGRATION_V003)?;
        tx.pragma_update(None, "user_version", 3)?;
        tx.commit()?;
    }

    if version < 4 {
        let tx = conn.transaction()?;
        import_legacy_config_blob(&tx)?;
        tx.pragma_update(None, "user_version", 4)?;
        tx.commit()?;
    }

    Ok(())
}

/// Migration v004: import the legacy `config(key='watched_folders')` JSON blob
/// into the `watched_folders` table, then delete the blob row.
///
/// - Import is idempotent by `path`: rows already present in the table win and
///   are left untouched (a warning is logged — the two stores had drifted).
/// - Blob-provided ids are preserved for newly imported paths, so existing
///   `media_items.folder_id` references stay valid.
/// - An unparseable blob is left in place with a warning instead of failing
///   the migration (startup must not break).
fn import_legacy_config_blob(conn: &Connection) -> Result<(), rusqlite::Error> {
    let blob: Option<String> = conn
        .query_row(
            "SELECT value FROM config WHERE key = ?1",
            params![LEGACY_WATCHED_FOLDERS_KEY],
            |r| r.get(0),
        )
        .optional()?;

    let Some(blob) = blob else { return Ok(()) };

    let config: crate::config::AppConfig = match serde_json::from_str(&blob) {
        Ok(config) => config,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "Legacy watched_folders config blob is not valid JSON — leaving row in place; \
                 folders must be re-added via PUT /api/v1/config"
            );
            return Ok(());
        }
    };

    for folder in &config.watched_folders {
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM watched_folders WHERE path = ?1",
                params![folder.path],
                |r| r.get(0),
            )
            .optional()?;

        if existing.is_some() {
            tracing::warn!(
                path = %folder.path,
                "Legacy config blob disagrees with the watched_folders table — table wins"
            );
            continue;
        }

        let id = folder.id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO watched_folders (id, path, label) VALUES (?1, ?2, ?3)",
            params![id, folder.path, folder.label],
        )?;

        if inserted == 0 {
            // The insert was ignored: either the id is taken by a different
            // path (drift/corruption) or this path appears twice in the blob.
            // Re-import under a fresh id only when the path is genuinely
            // missing — never silently drop the folder.
            let path_present: Option<String> = conn
                .query_row(
                    "SELECT id FROM watched_folders WHERE path = ?1",
                    params![folder.path],
                    |r| r.get(0),
                )
                .optional()?;

            if path_present.is_none() {
                tracing::warn!(
                    path = %folder.path,
                    "Legacy config blob id is already in use by another folder — assigning a fresh id"
                );
                conn.execute(
                    "INSERT INTO watched_folders (id, path, label) VALUES (?1, ?2, ?3)",
                    params![uuid::Uuid::new_v4().to_string(), folder.path, folder.label],
                )?;
            }
        }
    }

    conn.execute("DELETE FROM config WHERE key = ?1", params![LEGACY_WATCHED_FOLDERS_KEY])?;
    Ok(())
}
