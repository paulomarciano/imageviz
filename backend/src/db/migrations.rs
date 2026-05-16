use rusqlite::Connection;

/// Run all pending migrations, advancing `PRAGMA user_version` as each completes.
///
/// Migrations are idempotent — running them multiple times is safe because each
/// statement uses `CREATE TABLE IF NOT EXISTS` and `CREATE INDEX IF NOT EXISTS`.
pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i32 = conn
        .pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
        .unwrap_or(0);

    if version < 1 {
        conn.execute_batch(super::schema::CREATE_MEDIA_ITEMS)?;
        conn.execute_batch(super::schema::CREATE_CONFIG_TABLE)?;
        conn.execute_batch(super::schema::CREATE_IDX_MEDIA_SORT)?;
        conn.execute_batch(super::schema::CREATE_IDX_MEDIA_PATH)?;
        conn.execute_batch(super::schema::CREATE_IDX_MEDIA_MIME)?;
        conn.pragma_update(None, "user_version", 1)?;
    }

    Ok(())
}
