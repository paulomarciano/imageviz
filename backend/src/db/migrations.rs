use rusqlite::Connection;

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

    Ok(())
}
