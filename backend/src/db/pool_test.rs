//! Tests for the SQLite connection pool.
//!
//! Regression guard (wave-8-21 K5): every connection to `sqlite::memory:` is
//! a *separate, empty* database. The in-memory pool must therefore expose
//! exactly one connection, so every handle drawn from the pool observes the
//! same data.

use std::time::Duration;

#[test]
fn in_memory_pool_connections_share_one_database() {
    let pool = super::create_in_memory_pool();

    // Write through one handle.
    {
        let conn = pool.get().unwrap();
        conn.execute_batch("CREATE TABLE t (v TEXT); INSERT INTO t VALUES ('x');").unwrap();
    }

    // A different pooled handle must observe the same data. This only holds
    // if every handle is backed by the same underlying connection.
    {
        let conn = pool.get().unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM t", [], |row| row.get(0)).unwrap();
        assert_eq!(n, 1, "pooled handle must see data written via another handle");
    }

    // The pool is single-connection *by design* (max_size(1)): holding the
    // connection must starve a second acquisition. If max_size were raised
    // again, the second get would succeed against a fresh, empty database —
    // silently reintroducing the bug this test guards against.
    {
        let _conn = pool.get().unwrap();
        assert!(
            pool.get_timeout(Duration::from_millis(50)).is_err(),
            "in-memory pool must expose exactly one connection"
        );
    }
}
