# Wave 1.1 — Design and Implement SQLite Schema + Migrations

| Field | Value |
|-------|-------|
| **Wave** | 1 — Backend: File System Scanner & Metadata Extraction |
| **Seq** | 01 |
| **Estimate** | 1.5 hours |
| **Depends on** | 0.2 (Rust backend scaffolded) |
| **Parallel** | No (foundation for all Wave 1 tasks) |

---

## Overview

Implement the SQLite database layer: define the schema (tables, indexes), create a migration system, and establish the database connection pattern used by all subsequent backend modules. Use WAL mode for concurrent reads.

## Prerequisites

- Rust backend with Axum (from 0.2)
- `rusqlite` in `Cargo.toml` (bundled feature)

## Reference Files

- `documents/plans/development-plan.md` — §4 Database Schema (lines 248–281), §8.2 Key Performance Decisions (WAL mode)
- `.opencode/context/development/principles/clean-code.md` — Rust patterns (ownership, Result types)

## Deliverables

```
backend/src/db/
├── mod.rs                       # Public interface, connection pool helpers
├── schema.rs                    # CREATE TABLE statements
└── migrations.rs                # Migration runner (version-based)
```

## Acceptance Criteria (Pass/Fail)

- [ ] SQLite database creates tables matching §4 schema:
  - `media_items` with all columns: `id TEXT PK`, `filename`, `relative_path UNIQUE`, `mime_type`, `width`, `height`, `file_size`, `thumbnail_path`, `file_created_at`, `file_modified_at`, `indexed_at`, `metadata_json`, `checksum`
  - `config` with `key TEXT PK`, `value TEXT`
- [ ] Indexes are created:
  - `idx_media_sort` on `(file_created_at DESC, id)` — for cursor pagination
  - `idx_media_path` on `(relative_path)` — for path lookups
  - `idx_media_mime` on `(mime_type)` — for type filtering
- [ ] Database opens in **WAL mode** (`PRAGMA journal_mode=WAL`)
- [ ] Foreign keys enforced (`PRAGMA foreign_keys=ON`)
- [ ] Migration system supports versioned migrations (runs `CREATE TABLE IF NOT EXISTS` idempotently)
- [ ] Unit test: can create tables, insert a row, and query it back

## Implementation Notes

**WAL mode setup:**
```rust
use rusqlite::Connection;

pub fn open(path: &str) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(path)?;
    conn.execute_batch("
        PRAGMA journal_mode=WAL;
        PRAGMA foreign_keys=ON;
        PRAGMA busy_timeout=5000;
    ")?;
    Ok(conn)
}
```

**Migration strategy — simple version-based:**
```rust
pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    
    if version < 1 {
        conn.execute_batch(include_str!("../sql/v001_initial.sql"))?;
        conn.pragma_update(None, "user_version", 1)?;
    }
    // Future migrations: if version < 2 { ... }
    Ok(())
}
```

**Strongly prefer** embedding SQL in `.sql` files under `backend/sql/` and using `include_str!()` rather than building SQL strings in Rust.

**Schema file (`schema.rs`)** should expose the CREATE TABLE statements as constants for use in both migrations and tests.

## Test Strategy

**TDD approach:**
1. Write a test that opens an in-memory DB, runs migrations, verifies tables exist
2. Write a test that inserts a media item and retrieves it
3. Write a test that verifies WAL mode is enabled
4. Write a test that verifies the UNIQUE constraint on `relative_path`

```rust
#[test]
fn test_create_tables_and_insert() {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    
    // Verify table exists
    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM media_items",
        [],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(count, 0);
    
    // Insert and retrieve
    conn.execute(
        "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, file_created_at, file_modified_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params!["test-uuid", "test.png", "2025/test.png", "image/png", 1024, "2025-01-01T00:00:00Z", "2025-01-01T00:00:00Z"],
    ).unwrap();
    
    let fetched: String = conn.query_row(
        "SELECT filename FROM media_items WHERE id = ?1",
        params!["test-uuid"],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(fetched, "test.png");
}
```

## External Docs

Use **ExternalScout** to fetch current `rusqlite` documentation for:
- WAL mode pragmas
- Connection pooling patterns (note: `r2d2` will be added in later waves)
- `params![]` macro usage
