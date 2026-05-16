# Wave 3.2 — Implement Tantivy Index Population (from SQLite)

| Field | Value |
|-------|-------|
| **Wave** | 3 — Backend: Search, Cursor Pagination & Real-time SSE |
| **Seq** | 02 |
| **Estimate** | 1.5 hours |
| **Depends on** | 1.8 (indexer), 3.1 (Tantivy schema) |
| **Parallel** | No |

---

## Overview

Implement the bridge between SQLite and Tantivy: read all media items from SQLite and populate the Tantivy full-text index. Support both full re-indexing and incremental updates (only index new/modified items since last commit).

## Prerequisites

- Indexer orchestrator with SQLite storage (1.8)
- Tantivy IndexManager (3.1)
- Media items exist in SQLite

## Reference Files

- `documents/plans/development-plan.md` — §3 Search endpoints, §4 Tantivy Schema, §5 Wave 3
- `.opencode/context/development/principles/clean-code.md`

## Deliverables

```
backend/src/search/
├── mod.rs                       # Updated: re-export indexer
└── indexer.rs                   # SQLite → Tantivy population
```

## Acceptance Criteria (Pass/Fail)

- [ ] `full_reindex(db, tantivy)` reads all media items from SQLite and indexes them in Tantivy
- [ ] `incremental_reindex(db, tantivy, since_timestamp)` indexes only items modified after the given timestamp
- [ ] Tantivy document fields map correctly from media_items columns
- [ ] `metadata_json` is indexed as TEXT (full-text searchable)
- [ ] Date field uses Tantivy's `DatePrecision::Seconds`
- [ ] Large datasets (14K+ items) indexed in a single transaction
- [ ] Commit called after indexing completes
- [ ] Empty index → first run → all items indexed; second run → no duplicates

## Implementation Notes

**Full reindex:**
```rust
pub fn full_reindex(
    db: &Connection,
    index_manager: &IndexManager,
) -> Result<ReindexStats, Error> {
    let mut stmt = db.prepare(
        "SELECT id, filename, mime_type, metadata_json, file_created_at, file_size, width, height FROM media_items"
    )?;
    
    let rows = stmt.query_map([], |row| {
        Ok(IndexableDoc {
            id: row.get(0)?,
            filename: row.get(1)?,
            mime_type: row.get(2)?,
            metadata_json: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            created_at: parse_iso8601_to_tantivy(&row.get::<_, String>(4)?)?,
            file_size: row.get(5)?,
            width: row.get::<_, Option<u64>>(6)?.unwrap_or(0),
            height: row.get::<_, Option<u64>>(7)?.unwrap_or(0),
        })
    })?;
    
    let mut count = 0;
    for row in rows {
        let doc = row?;
        index_manager.add_document(doc)?;
        count += 1;
    }
    
    let ops = index_manager.commit()?;
    
    Ok(ReindexStats {
        indexed_count: count,
        ops,
    })
}
```

**ISO 8601 to Tantivy DateTime:**
```rust
use chrono::{DateTime, Utc};

fn parse_iso8601_to_tantivy(ts: &str) -> Result<tantivy::DateTime, Error> {
    let dt: DateTime<Utc> = ts.parse().map_err(|e| Error::DateParse(e.to_string()))?;
    let timestamp = dt.timestamp(); // Unix timestamp in seconds
    Ok(tantivy::DateTime::from_timestamp_secs(timestamp))
}
```

**Incremental reindex:**
```rust
pub fn incremental_reindex(
    db: &Connection,
    index_manager: &IndexManager,
    since: &chrono::DateTime<Utc>,
) -> Result<ReindexStats, Error> {
    let mut stmt = db.prepare(
        "SELECT ... FROM media_items WHERE indexed_at > ?1 OR file_modified_at > ?1"
    )?;
    // ... same as full_reindex but with WHERE clause
}
```

**Deduplication** — Before adding, delete the existing document by ID:
```rust
let id_field = schema.get_field("id").unwrap();
let id_term = tantivy::Term::from_field_text(id_field, &doc_data.id);
writer.delete_term(id_term);
writer.add_document(doc)?;
```

This makes re-indexing idempotent.

## Test Strategy

```rust
#[test]
fn test_full_reindex() {
    let dir = tempfile::tempdir().unwrap();
    let db = create_test_db_with_media_items(10); // Helper: insert 10 items
    let index_manager = IndexManager::open(dir.path()).unwrap();
    
    let stats = full_reindex(&db, &index_manager).unwrap();
    assert_eq!(stats.indexed_count, 10);
    
    // Verify searchable
    let reader = index_manager.reader();
    let searcher = reader.searcher();
    assert_eq!(searcher.num_docs(), 10);
}

#[test]
fn test_full_reindex_is_idempotent() {
    // Reindex twice → still 10 docs, not 20
}
```
