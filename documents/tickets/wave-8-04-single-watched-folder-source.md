# Wave 8.4 — Watched Folders: Make the Table the Single Source of Truth

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 04 |
| **Estimate** | 2 hours |
| **Depends on** | — |
| **Parallel** | Yes (independent of Phase-1 tickets) |
| **Source** | Code review §1 K1 (🔴) / §2 D1 (🔴) |

---

## Overview

Folder configuration is persisted **twice**: as a JSON blob in `config(key='watched_folders')` *and* as rows in the `watched_folders` table. Every consumer must decide which to read:

- `resolve_media_path()` tries the table, then falls back to parsing the JSON blob (`routes/media/file.rs:68-91`) — 30 lines of legacy path on the hottest media-serving route.
- `load_watched_folders()` (`watcher/handler.rs:275-284`) reads the JSON, not the table.
- `assign_folder_ids()` (`config/mod.rs:48-79`) carries a comment describing a production log-flood bug caused precisely by the two stores drifting.
- `update_config()` writes both, in a specific order, to keep them aligned.

Make the `watched_folders` table the single source of truth, derive `AppConfig` from it, and delete the blob plus every fallback. This removes ~60 lines and an entire class of sync bugs (K1 and D1 are the same fix — resolution logic duplicated in `resolve_media_path`, `load_watched_folders`, and `folder_id_map` collapses with it).

## Prerequisites

- None (v0.7.0 baseline)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §1 K1, §2 D1
- `backend/src/config/mod.rs` — `load_config`, `save_config`, `assign_folder_ids`, `folder_id_map`
- `backend/src/routes/media/file.rs:68-91` — JSON fallback in `resolve_media_path`
- `backend/src/routes/config.rs:102-117` — `update_config` double write
- `backend/src/watcher/handler.rs:275-284` — `load_watched_folders`
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/config/mod.rs           # AppConfig derived from table; blob code deleted
backend/src/routes/config.rs        # single-table write path
backend/src/routes/media/file.rs    # fallback removed
backend/src/watcher/handler.rs      # reads table
backend/src/db/schema.rs            # one-time legacy-blob migration (if needed)
```

## Acceptance Criteria (Pass/Fail)

- [ ] `watched_folders` table is the only persisted location; the `config` key `'watched_folders'` no longer exists after any PUT
- [ ] `GET/PUT /api/v1/config` behavior is unchanged at the API level (roundtrip test passes unchanged)
- [ ] `resolve_media_path` contains no JSON-blob fallback
- [ ] Watcher and indexer read watched folders exclusively from the table
- [ ] One-time migration: a DB that still carries the legacy blob (pre-7.12 style) gets its folders imported into the table on startup, then the blob row is deleted
- [ ] The drift-bug comment in `assign_folder_ids` is removed together with the code it describes
- [ ] `grep -r "watched_folders"` shows no reads of the `config` table for folder data
- [ ] `cargo test` green

## Implementation Notes

- `load_config(conn)` becomes `SELECT id, path, label FROM watched_folders ORDER BY id`.
- `update_config` becomes: validate → diff against table → insert/delete rows → assign folder ids. One write, no ordering constraints.
- Migration shape (KISS): on startup, if the legacy blob exists **and** it parses, upsert its paths into the table (idempotent by `path`), then delete the blob row. Log a warning if blob and table already disagree (drift detection), table wins.
- Keep the API contract (`watched_folders: [{path, label?}]`) identical so the frontend needs no change.

## Test Strategy

- Existing config roundtrip integration tests pass **without modification** (API stability proof).
- New test: seed DB with legacy blob only → startup migration → folders present in table, blob row gone.
- New test: seed DB with *conflicting* blob and table → table wins, warning logged.
- New test: `resolve_media_path` for a media item whose folder exists only in the old blob returns 404 after migration-less startup (fallback is gone).
