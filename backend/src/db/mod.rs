use rusqlite::Connection;
use std::path::Path;

pub mod migrations;
pub mod pool;
pub mod schema;

pub use pool::SqliteConnectionManager;

#[cfg(test)]
mod migrations_test;
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

/// Open an in-memory SQLite database (for testing) with foreign keys enabled.
///
/// Note: WAL journal mode is intentionally omitted — it has no effect on
/// in-memory databases in SQLite. The file-based [`open()`] function
/// enables WAL, which is the production path.
pub fn open_in_memory() -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    Ok(conn)
}
