//! r2d2 connection pool for SQLite.
//!
//! Provides a thread-safe connection pool that replaces the previous
//! `Arc<Mutex<Connection>>` pattern. WAL mode allows concurrent reads
//! while writes are serialised by SQLite's internal locking.

use r2d2::{ManageConnection, Pool};
use rusqlite::Connection;
use std::path::Path;
use std::time::Duration;

/// Default pool size: max 10 connections.
const DEFAULT_POOL_SIZE: u32 = 10;

/// Connection manager for rusqlite connections.
///
/// Opens file-backed or in-memory SQLite databases with foreign keys enabled
/// on every new connection.  File-backed connections additionally use WAL
/// journal mode and a 5-second busy timeout for better concurrent access.
pub struct SqliteConnectionManager {
    db_path: Option<std::path::PathBuf>,
    in_memory: bool,
}

impl SqliteConnectionManager {
    /// Create a manager for a file-backed SQLite database.
    pub fn file(path: &Path) -> Self {
        Self { db_path: Some(path.to_path_buf()), in_memory: false }
    }

    /// Create a manager for an in-memory SQLite database.
    pub fn memory() -> Self {
        Self { db_path: None, in_memory: true }
    }
}

impl ManageConnection for SqliteConnectionManager {
    type Connection = Connection;
    type Error = rusqlite::Error;

    fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let conn = if self.in_memory {
            Connection::open_in_memory()?
        } else if let Some(ref path) = self.db_path {
            Connection::open(path)?
        } else {
            return Err(rusqlite::Error::InvalidPath(std::path::PathBuf::from(
                "no path configured",
            )));
        };

        if self.in_memory {
            conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        } else {
            conn.execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA foreign_keys=ON;
                 PRAGMA busy_timeout=5000;",
            )?;
        }

        Ok(conn)
    }

    fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        conn.execute_batch("SELECT 1")
    }

    fn has_broken(&self, _conn: &mut Self::Connection) -> bool {
        false
    }
}

/// Create a connection pool for a file-based SQLite database.
///
/// Each connection is initialised with WAL mode, foreign keys, and a busy
/// timeout so that concurrent access works correctly.
pub fn create_pool(path: &Path) -> Result<Pool<SqliteConnectionManager>, r2d2::Error> {
    let manager = SqliteConnectionManager::file(path);

    Pool::builder()
        .max_size(DEFAULT_POOL_SIZE)
        .connection_timeout(Duration::from_secs(5))
        .build(manager)
}

/// Create an in-memory connection pool for testing.
///
/// Capped at **one** connection: every `sqlite::memory:` connection is a
/// separate, empty database, so a multi-connection pool would hand each
/// caller a different database. A single connection makes every pooled
/// handle observe the same data.
pub fn create_in_memory_pool() -> Pool<SqliteConnectionManager> {
    let manager = SqliteConnectionManager::memory();

    Pool::builder().max_size(1).build(manager).expect("in-memory pool")
}

#[cfg(test)]
#[path = "pool_test.rs"]
mod tests;
