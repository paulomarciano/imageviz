# Wave 7.11 — Replace `Arc<Mutex<Connection>>` with r2d2 Connection Pool

| Field | Value |
|-------|-------|
| **Wave** | 7 — Production Readiness & Hardening |
| **Seq** | 11 |
| **Estimate** | 2 hours |
| **Depends on** | 0.2 (backend scaffold), 1.1 (SQLite schema) |
| **Parallel** | Can run in parallel with other Wave 7 tasks |

---

## Overview

The backend currently wraps a single `rusqlite::Connection` in `Arc<Mutex<Connection>>`, serializing **all** database access — every API request, every indexer batch, every file watcher event must acquire the same Mutex before touching the database. This is safe but creates a throughput bottleneck.

Replace with **r2d2** connection pooling so multiple requests can query (and in WAL mode, read concurrently with writes). The pool sits behind an `Arc<Pool>` shared across all state structs, replacing the current `Arc<Mutex<Connection>>` pattern.

## Prerequisites

- `r2d2` crate + `r2d2_sqlite` (or `r2d2::rusqlite::SqliteConnectionManager`)
- Current code uses `Arc<Mutex<Connection>>` in 5 state structs

## Reference Files

- `documents/plans/development-plan.md` — §8.3 Memory Management (r2d2 pool, max 10 connections)
- `backend/src/main.rs` — current state assembly, all state structs
- `backend/src/db/mod.rs` — `open()` and `open_in_memory()` helpers
- `backend/src/routes/config.rs`, `routes/media.rs`, `routes/search.rs`, `routes/stats.rs`, `routes/events.rs` — state definitions
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/db/
├── mod.rs                   # Updated: add r2d2 pool type + helper fn
└── pool.rs                  # NEW: r2d2 pool setup, connection manager
backend/src/routes/
├── config.rs                # Updated: ConfigState uses Pool instead of Arc<Mutex<Connection>>
├── media.rs                 # Updated: MediaState uses Pool
├── search.rs                # Updated: SearchState uses Pool
├── stats.rs                 # Updated: StatsState uses Pool
└── events.rs                # Updated: EventsState uses Pool (unchanged, SSE doesn't need DB)
backend/src/main.rs          # Updated: create pool instead of Arc<Mutex<Connection>>
backend/src/indexer/mod.rs   # Updated: accept Pool instead of &Mutex<Connection>
backend/src/watcher/handler.rs # Updated: accept Pool for event handler
backend/Cargo.toml           # Added: r2d2, r2d2_sqlite
```

## Acceptance Criteria (Pass/Fail)

- [ ] r2d2 `Pool<SqliteConnectionManager>` created at startup with max 10 connections
- [ ] All 5 state structs hold `Arc<Pool>` instead of `Arc<Mutex<Connection>>`
- [ ] Route handlers acquire a connection via `pool.get()` instead of `db.lock().await`
- [ ] Indexer (`full_index`, `incremental_index`) uses pooled connections with batched transactions
- [ ] File watcher handler uses pooled connections (separate connection per event batch)
- [ ] WAL mode is still enabled (already configured via `db::open`)
- [ ] Busy timeout configured via `r2d2::Pool::builder().connection_customizer()`
- [ ] All 193+ existing tests still pass (no API behavior change)
- [ ] `open_in_memory()` still works for tests (create a pool around an in-memory DB)

## Implementation Notes

**Cargo.toml additions:**
```toml
r2d2 = "0.8"
r2d2_sqlite = "0.25"
```

**Pool creation helper (`db/pool.rs`):**
```rust
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use std::path::Path;
use std::time::Duration;

const DEFAULT_POOL_SIZE: u32 = 10;

/// Create a connection pool for the given database path.
///
/// The pool opens connections lazily (not on creation). Each connection
/// inherits the WAL-mode + foreign-keys + busy-timeout pragmas from
/// [`db::open`].
pub fn create_pool(path: &Path) -> Result<Pool<SqliteConnectionManager>, r2d2::Error> {
    let manager = SqliteConnectionManager::file(path)
        .with_init(|conn| {
            conn.execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA foreign_keys=ON;
                 PRAGMA busy_timeout=5000;",
            )?;
            Ok(())
        });

    Pool::builder()
        .max_size(DEFAULT_POOL_SIZE)
        .connection_timeout(Duration::from_secs(5))
        .build(manager)
}

/// Create an in-memory pool for testing.
///
/// Pools 3 connections so concurrent test helpers don't block each other.
pub fn create_in_memory_pool() -> Pool<SqliteConnectionManager> {
    let manager = SqliteConnectionManager::memory()
        .with_init(|conn| {
            conn.execute_batch("PRAGMA foreign_keys=ON;")?;
            Ok(())
        });

    Pool::builder()
        .max_size(3)
        .build(manager)
        .expect("in-memory pool")
}
```

**State struct migration pattern:**
```rust
// Before
pub struct MediaState {
    pub db: Arc<Mutex<rusqlite::Connection>>,
    pub thumbnail_cache_dir: PathBuf,
}

// After
pub struct MediaState {
    pub db: r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
    pub thumbnail_cache_dir: PathBuf,
}
```

Handler pattern after migration:
```rust
// Before
let db = state.db.lock().await;
let total: i64 = db.query_row(/* ... */)?;
// lock held until db is dropped

// After
let conn = state.db.get().map_err(|e| {
    tracing::error!(error = %e, "Failed to acquire DB connection from pool");
    StatusCode::SERVICE_UNAVAILABLE
})?;
let total: i64 = conn.query_row(/* ... */)?;
// connection returned to pool when `conn` is dropped
```

**Indexer changes:**
The `full_index` function currently takes `&Mutex<Connection>`. It should take a reference that can produce connections. The simplest approach: accept `&Pool<SqliteConnectionManager>` and acquire connections as needed for each batch:

```rust
pub async fn full_index(
    pool: &Pool<SqliteConnectionManager>,
    config: &AppConfig,
    progress: &ProgressTracker,
) -> Result<IndexStats, IndexError> {
    // ...
    for chunk in all_files.chunks(BATCH_SIZE) {
        // Phase 2: acquire a connection from the pool for each batch
        let conn = pool.get().map_err(IndexError::from)?;
        // ... use conn ...
    }
}
```

**Thread safety notes:**
- `r2d2::Pool` is `Send + Sync + Clone` (clone is cheap — Arc bump)
- Pool exhaustion returns `PoolError` immediately (not a deadlock like Mutex)
- WAL mode means readers don't block writers; contention only happens on actual WAL checkpoint
- Each route handler gets its own connection from the pool, so slow queries don't queue up behind each other

## Test Strategy

- **No new tests needed** for the pool itself (r2d2 is well-tested upstream)
- All 193+ existing tests must pass unchanged (they create in-memory connections; add `create_in_memory_pool()` for test helpers)
- Specific tests to verify:
  - Concurrent requests don't block each other (two simultaneous `GET /api/v1/media` calls)
  - Pool exhaustion returns 503, not a hang
  - WAL mode still active on pooled connections

**Test pattern for in-memory pool:**
```rust
fn test_pool() -> r2d2::Pool<r2d2_sqlite::SqliteConnectionManager> {
    let pool = crate::db::pool::create_in_memory_pool();
    let conn = pool.get().unwrap();
    crate::db::migrations::run_migrations_on_conn(&conn).unwrap();
    pool
}
```

## Known Risks

- **r2d2_sqlite 0.25 may need a specific r2d2 version** — verify compatibility in Cargo.toml
- **Connection customizer vs `db::open`** — the init pragmas must match. The pool's `with_init` closure replaces `db::open` for production use
- **Tantivy IndexManager writer guard** — `index_manager.commit()` is synchronous and uses its own `std::sync::Mutex`; the connection pool won't affect this bottleneck
