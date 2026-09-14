# Wave 8.7 — Remove the thumbnail_path Column and Its Per-Request Write

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 07 |
| **Estimate** | 30 minutes |
| **Depends on** | — |
| **Parallel** | Yes (different files than 8.6; 8.9 builds on this) |
| **Source** | Code review §4 R5 (🟡) |

---

## Overview

Every served thumbnail — cache hit or miss — runs `UPDATE media_items SET thumbnail_path = …` (`backend/src/routes/media/thumbnail.rs:75-86`). Nothing ever **reads** `thumbnail_path`; the update exists, per its own comment, "so the column is no longer dead". A 100-thumbnail grid page therefore causes 100 pooled connection checkouts + WAL writes for data no one consumes.

KISS option (preferred per review): delete the write **and the column**.

## Prerequisites

- None (v0.7.0 baseline)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §4 R5
- `backend/src/routes/media/thumbnail.rs:75-86` — the UPDATE
- `backend/src/db/schema.rs:15` — column definition; migration runner for the drop
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/routes/media/thumbnail.rs   # UPDATE removed
backend/src/db/schema.rs                # column dropped for fresh DBs
backend/src/db/migrations (or schema.rs) # new migration: ALTER TABLE media_items DROP COLUMN thumbnail_path
```

## Acceptance Criteria (Pass/Fail)

- [ ] Serving a thumbnail (hit or miss) performs no writes to `media_items`
- [ ] Fresh database schema has no `thumbnail_path` column
- [ ] Existing v0.7.0 database is migrated (column dropped) without data loss or manual steps
- [ ] Thumbnail endpoint behavior unchanged (status codes, headers, ETag/304)
- [ ] `grep -rn thumbnail_path backend/src` returns no production-code hits
- [ ] `cargo test` green

## Implementation Notes

- SQLite ≥ 3.35 supports `ALTER TABLE … DROP COLUMN`; the bundled rusqlite feature set includes it — verify with a migration test against a seeded pre-migration DB.
- Keep the migration in the existing sequential migration runner; do not add a migration framework.
- If a future fast-path cache is wanted, the review notes it would need to actually **read** the column — deleting now keeps that door open via the content-addressed cache, which already serves the same purpose.

## Test Strategy

- Integration test: request a thumbnail twice (miss then hit) → no `UPDATE`-induced WAL growth: assert `media_items` row unchanged (e.g., compare full row before/after).
- Migration test: create a DB with the old schema (pre-drop), run migrations, assert column gone and rows intact.
