# Wave 7.12 — Add `folder_id` Column to Resolve Path Ambiguity

| Field | Value |
|-------|-------|
| **Wave** | 7 — Production Readiness & Hardening |
| **Seq** | 12 |
| **Estimate** | 2 hours |
| **Depends on** | 1.1 (SQLite schema), 1.2 (config management), 1.8 (indexer orchestration) |
| **Parallel** | No — touches schema, indexer, watcher, and config |

---

## Overview

When multiple watched folders are configured, the `relative_path` column in `media_items` is computed relative to the folder root. If two folders contain a file with the same relative path (e.g., both `/media/photos` and `/media/downloads` contain `image.png`), the second `INSERT OR REPLACE` **silently overwrites** the first entry.

This is a known limitation documented in the indexer source (`backend/src/indexer/mod.rs:168-171`).

**Fix:** Add a `folder_id` column to `media_items` and a `watched_folders` table. The `UNIQUE` constraint becomes `(folder_id, relative_path)` instead of `relative_path` alone.

## Prerequisites

- SQLite schema with `media_items` table (1.1)
- Config management with `PUT /config` (1.2)
- Indexer orchestration with file scanning (1.8)
- File watcher event handler (3.5, 3.6)

## Reference Files

- `documents/plans/development-plan.md` — §4 Database Schema (lines 248–281)
- `backend/src/indexer/mod.rs` — `store_file()` function with known-limitation comment (lines 168–171)
- `backend/src/watcher/handler.rs` — `handle_file_created_or_modified()` and `handle_file_deleted()`
- `backend/src/db/schema.rs` — schema constants
- `backend/src/db/migrations.rs` — migration runner
- `backend/src/config/mod.rs` — `WatchedFolder` struct
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/sql/
└── v002_folder_id.sql              # NEW: migration SQL for folder_id support
backend/src/config/
└── mod.rs                          # Updated: WatchedFolder gets stable id
backend/src/db/
├── schema.rs                       # Updated: add watched_folders table, new UNIQUE constraint
└── migrations.rs                   # Updated: version 2 migration
backend/src/indexer/
└── mod.rs                          # Updated: store_file() resolves folder_id, path resolution
backend/src/watcher/
└── handler.rs                      # Updated: resolve_relative_path returns folder_id too
backend/src/routes/
├── config.rs                       # Updated: assign stable folder ids on PUT
└── media.rs                        # Updated: resolve_media_path uses folder_id
```

## Acceptance Criteria (Pass/Fail)

- [ ] New migration `v002` creates `watched_folders` table (columns: `id TEXT PK`, `path TEXT UNIQUE`, `label TEXT`)
- [ ] Migration removes the old `idx_media_path` index and creates a new unique index on `(folder_id, relative_path)`
- [ ] Migration adds `folder_id TEXT NOT NULL REFERENCES watched_folders(id)` to `media_items`
- [ ] `PUT /api/v1/config` assigns a stable UUID to each folder entry (persisted to `watched_folders` table)
- [ ] If a folder's `path` changes, it gets a new `folder_id` (old media items keep the old id)
- [ ] `store_file()` in `indexer/mod.rs` resolves the correct `folder_id` for each scanned file before upserting
- [ ] File watcher event handler resolves `folder_id` when processing create/modify/delete events
- [ ] `resolve_media_path()` in `routes/media.rs` uses `folder_id` to look up the correct watched folder root
- [ ] Items from different folders with the same relative path now coexist in the database
- [ ] `GET /api/v1/media` still works (adds `folder_id` to the response or uses it internally)
- [ ] All existing tests pass with the new schema (test helpers must create the new table)
- [ ] `cargo test` passes with 0 failures

## Implementation Notes

**New SQL migration (`sql/v002_folder_id.sql`):**
```sql
-- Watched folders registry (stable IDs survive path changes)
CREATE TABLE IF NOT EXISTS watched_folders (
    id TEXT PRIMARY KEY NOT NULL,
    path TEXT NOT NULL UNIQUE,
    label TEXT
);

-- Add folder_id to media_items (nullable for backward compat during migration)
ALTER TABLE media_items ADD COLUMN folder_id TEXT REFERENCES watched_folders(id);

-- Remove old unique index on relative_path
DROP INDEX IF EXISTS idx_media_path;

-- New compound unique index
CREATE UNIQUE INDEX IF NOT EXISTS idx_media_folder_path
    ON media_items(folder_id, relative_path);
```

**Config management changes:**
```rust
// In config.rs routes, on PUT /api/v1/config:
fn assign_folder_ids(config: &mut AppConfig, conn: &Connection) {
    for folder in &mut config.watched_folders {
        // If folder already has an ID in the DB, reuse it
        let existing: Option<String> = conn.query_row(
            "SELECT id FROM watched_folders WHERE path = ?1",
            params![folder.path],
            |r| r.get(0),
        ).optional().unwrap_or(None);
        
        folder.id = existing.unwrap_or_else(|| Uuid::new_v4().to_string());
    }
    
    // Persist new/changed folder entries
    for folder in &config.watched_folders {
        conn.execute(
            "INSERT OR REPLACE INTO watched_folders (id, path, label) VALUES (?1, ?2, ?3)",
            params![folder.id, folder.path, folder.label],
        ).ok();
    }
}
```

**WatchedFolder struct update:**
```rust
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WatchedFolder {
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub id: Option<String>,  // Stable UUID, assigned by the server on PUT
}
```

**Path resolution changes:**
```rust
// resolve_relative_path now returns (relative_path, folder_id)
fn resolve_relative_path(
    absolute_path: &Path,
    watched_folders: &[(String, String)],  // (path, id)
) -> Option<(String, String)> {
    watched_folders.iter()
        .find_map(|(folder_path, folder_id)| {
            absolute_path.strip_prefix(Path::new(folder_path)).ok()
                .map(|rel| (rel.to_string_lossy().into_owned(), folder_id.clone()))
        })
}
```

**Indexer store_file update:**
```rust
fn store_file(conn: &Connection, processed: &ProcessedFile<'_>) -> Result<IndexChange, IndexError> {
    // Now uses (folder_id, relative_path) for dedup instead of just relative_path
    let existing: Option<(String, Option<String>)> = conn.query_row(
        "SELECT id, checksum FROM media_items WHERE folder_id = ?1 AND relative_path = ?2",
        params![processed.folder_id, processed.file.relative_path],
        // ...
    )?;
    // ...
}
```

## Test Strategy

- **Unit tests for migration:** Open an old-schema DB, run v002 migration, verify columns exist
- **Unit tests for duplicate paths:** Configure two watched folders pointing at different directories, place a file with the same name in both, run indexing, verify both entries exist in DB
- **Unit tests for path resolution:** `resolve_relative_path` returns correct `folder_id`
- **Config roundtrip:** `PUT /api/v1/config` assigns stable IDs, re-PUT doesn't change them
- **Regression:** All existing `relative_path` queries (media list, file serving, search) still produce correct results

## Known Risks

- **ALTER TABLE ADD COLUMN locks the table** — SQLite's `ALTER TABLE ADD COLUMN` is a write-lock operation. For large databases (>100K rows), this could block writes for several seconds. Run migration during maintenance.
- **Media items with no folder_id** — existing rows will have `NULL` folder_id after migration. A backfill step should assign them based on path matching:
  ```sql
  UPDATE media_items SET folder_id =
      (SELECT id FROM watched_folders WHERE media_items.relative_path LIKE watched_folders.path || '%' LIMIT 1)
  WHERE folder_id IS NULL;
  ```
- **Front-end impact** — the `GET /api/v1/media` response format doesn't change (folder_id is an internal concept), but the frontend may benefit from showing which folder a result comes from. Optional: add `folder_label` to the list response.
