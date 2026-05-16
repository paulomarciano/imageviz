use rusqlite::Connection;
use std::path::Path;

pub mod migrations;
pub mod schema;

#[cfg(test)]
mod schema_test;

/// Open a SQLite database connection with WAL mode, foreign keys, and busy timeout enabled.
pub fn open(path: impl AsRef<Path>) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;",
    )?;
    Ok(conn)
}

/// Open an in-memory SQLite database (for testing) with WAL mode and foreign keys enabled.
pub fn open_in_memory() -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;",
    )?;
    Ok(conn)
}
