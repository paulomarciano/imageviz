# ImageViz — Final Code Review Report

> **Version**: 1.0  
> **Date**: 2026-05-18  
> **Scope**: Full-stack audit of backend (Rust/Axum/SQLite/Tantivy) and frontend (React/TypeScript/Vite)  
> **Reviewer**: AI-assisted code audit  

---

## Table of Contents

1. [Bugs](#1-bugs)
2. [Performance Issues](#2-performance-issues)
3. [Code Quality & Standards](#3-code-quality--standards)
4. [Drift from Plans](#4-drift-from-plans)
5. [Severity Matrix](#5-severity-matrix)
6. [Recommendations](#6-recommendations)

---

## 1. Bugs

### B1. [CRITICAL] `relative_path UNIQUE` constraint prevents multi-folder support

**Location**: `backend/src/db/schema.rs:10`, `backend/src/db/schema.rs:60-70`

**Problem**: The original `CREATE TABLE media_items` defines `relative_path TEXT NOT NULL UNIQUE`. Migration v002 (`MIGRATION_V002`) adds a `folder_id` column and creates a compound `UNIQUE INDEX` on `(folder_id, relative_path)`, but **never removes the original column-level UNIQUE constraint**. SQLite does not support `ALTER TABLE ... DROP CONSTRAINT`, so the original single-column uniqueness remains active.

**Impact**: If two different watched folders contain a file with the same relative path (e.g. `~/folder-a/image.png` and `~/folder-b/image.png`), the second `INSERT OR REPLACE` will hit a `UNIQUE constraint failed: media_items.relative_path` error. This entirely defeats the purpose of migration v002's `folder_id` model.

**Fix**: Requires a table rebuild:
1. Create a new table without the `UNIQUE` on `relative_path`.
2. Copy all data.
3. Drop the old table.
4. Rename the new table.
5. Recreate indexes.
6. Re-verify the compound `UNIQUE INDEX` on `(folder_id, relative_path)`.

---

### B2. [HIGH] `thumbnail_path` column defined but never written

**Location**: `backend/src/db/schema.rs:15`

**Problem**: The `media_items` schema includes `thumbnail_path TEXT` as a nullable column. No code path — neither the indexer, watcher, thumbnail generator, nor any migration — ever reads or writes this column.

**Impact**: Dead schema column that bloats the row size (SQLite stores NULLs as 0 bytes per row, so this is minor, but it's misleading). Any logic that depends on this column to determine whether a thumbnail exists would return false negatives.

**Fix**: Either:
- Remove the column from the schema in a v003 migration, or
- Populate it when thumbnails are generated (though the content-addressed cache already serves this purpose).

---

### B3. [MEDIUM] SSE `file_modified` event missing `metadata_updated` field

**Location**: `backend/src/watcher/stages/broadcast.rs:44-54`

**Problem**: The API contract (§3.3 of the development plan) specifies the `file_modified` SSE event as:

```json
{"id": "uuid", "filename": "...", "metadata_updated": true}
```

But the actual emitted payload is:

```json
{"id": "uuid", "filename": "..."}
```

The `metadata_updated` field is omitted. The frontend type definition (`frontend/src/types/api.ts:60`) declares `metadata_updated: boolean`, so any code consuming this field will receive `undefined`.

**Impact**: Frontend code that relies on `metadata_updated` to decide whether to refresh search results or reload metadata will silently malfunction.

---

### B4. [MEDIUM] SSE `indexing_complete` missing `duration_ms` field

**Location**: `backend/src/main.rs:353-361`

**Problem**: The API contract specifies:

```json
{"total": 14433, "duration_ms": 2340}
```

The actual event only contains `{"total": ...}`. The frontend type (`frontend/src/types/api.ts:67`) declares `duration_ms: number`, so consumers receive `undefined`.

**Impact**: The frontend cannot display indexing duration. Any retry/backoff logic based on duration cannot function.

---

### B5. [MEDIUM] `free_disk_space()` silently falls back to `u64::MAX`

**Location**: `backend/src/thumbnails/cache.rs:293-295`

**Problem**: `free_disk_space()` calls `fs2::available_space(path).unwrap_or(u64::MAX)`. If the platform API fails (e.g., unsupported filesystem, permission denied), the function returns `u64::MAX`, making the `MIN_FREE_DISK_MB` env var and the disk-space eviction branch in `evict_if_needed()` dead code.

**Impact**: On systems where `statvfs`/`statfs` fails (containers, FUSE mounts, exotic filesystems), the disk-space eviction branch is silently disabled. Cache can grow unbounded if `max_cache_size()` is also set to 0 (unlimited), potentially filling the disk.

**Note**: The performance plan (5.6) flags this and recommends fixing it with `fs2::available_space()` — but `fs2` is _already_ imported and used. The `unwrap_or(u64::MAX)` fallback is the remaining issue; there's no logging when the platform call fails.

**Fix**: Log a warning when `available_space()` returns an error, so operators can detect the silent fallback.

---

### B6. [MEDIUM] Watcher vs indexer metadata storage inconsistency (non-ComfyUI PNGs)

**Location**: `backend/src/watcher/stages/extract.rs:48-58` vs `backend/src/indexer/mod.rs:190-201`

**Problem**: When determining whether to store metadata JSON for PNGs, the two code paths differ:

**Watcher** (`extract.rs:49`):
```rust
if meta.prompt.is_some() || meta.workflow.is_some() {
    serde_json::to_string(&meta).ok()
} else {
    None
}
```

**Indexer** (`mod.rs:192`):
```rust
if meta.prompt.is_some() || meta.workflow.is_some() || !meta.raw_text_entries.is_empty() {
    serde_json::to_string(&meta).ok()
} else {
    None
}
```

**Impact**: PNG files that have only `raw_text_entries` (non-ComfyUI text chunks like `Description`, `Author`, `Software`) but no `prompt` or `workflow` will have their metadata stored during a full re-index but **NOT** when the file is detected via the file watcher. This is an inconsistency: the same file can have metadata or not depending on how it was indexed.

**Fix**: Align both code paths. The watcher should match the indexer's logic.

---

### B7. [LOW] `batch_get_media_items` has SQLite parameter limit vulnerability

**Location**: `backend/src/routes/search.rs:246-283`

**Problem**: The `IN (?1, ?2, ...)` clause is built dynamically. SQLite has a hard limit of 32,766 bind parameters (compile-time default). A search returning more than 32,766 Tantivy hits will cause a runtime crash.

**Impact**: Theoretical only — most searches return far fewer results. With `limit + 1 = 501` collector bound in `search_handler`, this is not reachable through normal use. However, if the collector limit is ever raised, this would fail silently.

**Fix**: Batch the SQL queries in chunks of 999 (safe margin) or use a temporary table join.

---

## 2. Performance Issues

### P1. [HIGH] COUNT(*) on every media list page load

**Location**: `backend/src/routes/media/list.rs:74-83`

**Problem**: Every `GET /api/v1/media` request runs `SELECT COUNT(*) FROM media_items` (with optional `mime_type LIKE` filter). For the target dataset of 100K+ items, this requires a full index scan even with the `idx_media_mime` index. This runs on every scroll/page request — potentially hundreds of times during a browsing session.

**Impact**: Each page load pays O(n) cost for a number that rarely changes. Adds 10-50ms per request at 100K items.

**Fix**: 
- Cache the total count (e.g., updated every 30s or after indexing completes).
- Use `PRAGMA table_info` + estimation (not exact).
- Serve total from `/stats` endpoint and cache it on the frontend instead.

---

### P2. [MEDIUM] Tantivy search always fetches `limit + 1` but cursor is never used

**Location**: `backend/src/routes/search.rs:132, 145-146, 183-191`

**Problem**: Search fetches `limit + 1` documents to determine `has_more`, but **the returned cursors (`next_cursor`, `next_cursor_id`) are never re-applied to subsequent Tantivy queries** (the comment on line 14-16 explains this). The cursors are provided "so the caller can implement client-side offset if needed" — but the frontend has no such logic.

**Impact**: Every search page re-executes the full Tantivy query, paying the full BM25 scoring cost. Pagination within search results is effectively client-side only (via `maxPages: 5`). The `limit + 1` fetch is wasted for the last page of every query.

---

### P3. [MEDIUM] Both browse and search queries fire simultaneously in `thumbnail-grid.tsx`

**Location**: `frontend/src/components/media/thumbnail-grid.tsx:47-48`

**Problem**: 
```typescript
const browseData = useInfiniteMedia(100, mimeType, browseEnabled);
const searchData = useSearch(searchQuery, 100, mimeType, sort);
```

`useInfiniteMedia` has an `enabled` parameter, and `useSearch` has internal `enabled` via `query.trim().length > 0`. However, during search mode, `browseEnabled = false` is passed — so this is partially mitigated. The remaining issue: when query is empty (browse mode), `useSearch` still initializes, sets up query key, and manages cache state even though its query is disabled.

**Impact**: Minor memory overhead from two inactive query observers. The query functions don't fire when disabled, so network impact is zero.

---

### P4. [MEDIUM] Image viewer renders React state updates on every scroll wheel event

**Location**: `frontend/src/components/viewer/image-viewer.tsx:21-26`

**Problem**: `handleWheel` calls `setZoom(prev => ...)` on every scroll delta. For high-resolution scroll wheels (many small events), this triggers rapid React re-renders, each doing reconciliation of the entire viewer subtree.

**Impact**: Noticeable stutter on rapid scrolling at high zoom levels, particularly on lower-end hardware. The drag/pan path is correctly optimized with refs.

**Fix**: Debounce zoom state updates (e.g., accumulate deltas and update every 50ms), or use `useRef` for zoom with CSS transform updates and only sync React state on the final value.

---

### P5. [LOW] Thumbnail file read into memory instead of streamed

**Location**: `backend/src/routes/media/thumbnail.rs:74`

**Problem**: `tokio::fs::read(&thumbnail)` reads the entire WebP file into a `Vec<u8>`. While thumbnails are small (5-50KB), this adds memory pressure proportional to concurrent thumbnail requests.

**Fix**: Use `tokio::fs::File` + `axum::body::Body::from_stream` to stream the file.

---

### P6. [LOW] `incremental_index` in `indexer/mod.rs` is a full re-scan, not incremental

**Location**: `backend/src/indexer/mod.rs:130-136`

**Problem**: 
```rust
pub async fn incremental_index(...) -> Result<IndexStats, IndexError> {
    full_index(pool, config, progress).await
}
```

The file indexer's "incremental" path is a full re-scan that re-hashes every file. Only the Tantivy search indexer (`search/indexer.rs`) has a true incremental path. This means every re-index triggered by config changes is a full scan, which for 100K+ files takes minutes.

**Impact**: After updating watched folders, the re-index takes as long as the initial index. Users wait longer than necessary.

---

## 3. Code Quality & Standards

### Q1. [MEDIUM] Duplicated `is_hidden()` function

**Location**: `backend/src/scanner/walker.rs:95-97` and `backend/src/watcher/mod.rs:153-155`

**Issue**: Both files define identical `is_hidden()` logic. Should be centralized in `media_types.rs`.

```rust
// walker.rs
fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry.file_name().to_str().is_some_and(|s| s.starts_with('.'))
}

// watcher/mod.rs
fn is_hidden(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str().to_str().is_some_and(|s| s.starts_with('.')))
}
```

These are **subtly different**: the walker checks only the file name; the watcher checks all path components. A file like `dir/.hidden/file.png` would be:
- Allowed by the walker (only checks the entry's file name, so `file.png` passes)
- Blocked by the watcher (detects `.hidden` in the path)

**Impact**: Inconsistent behavior between initial scan (walker) and file watcher for files inside hidden directories.

---

### Q2. [MEDIUM] Search `total` field returns page count, not total result count

**Location**: `backend/src/routes/search.rs:199`

**Issue**: The search response reports:
```json
{"meta": {"total": 100, ...}}
```

But `total` is set to `media_items.len()` — the number of items in the current page response — **not** the total number of matching search results. The Tantivy collector returns `limit + 1` documents, but there's no `match_count` or `total_hits` from the query. The frontend (`use-search.ts:62` and `thumbnail-grid.tsx:178`) treats this as the total result count, which is wrong for any page beyond the first where results exceed the page size.

**Impact**: Users see inaccurate search result counts. E.g., "100 results for 'dragon'" might actually be "1,247 results for 'dragon'".

**Fix**: Use Tantivy's `count` method or a separate `TopDocs` + `count` collector to get the accurate total, or set `total` to 0 / a best-effort estimate and label it as such.

---

### Q3. [LOW] `#[allow(dead_code)]` on `SearchParams`

**Location**: `backend/src/routes/search.rs:54`

**Issue**: `cursor` and `cursor_id` fields on `SearchParams` are parsed from the query string and validated, but their values are never used in the Tantivy query — cursors are generated from the response, not consumed from the request. The `#[allow(dead_code)]` suppresses a legitimate warning.

**Fix**: Either remove the fields (they're accepted but ignored) or document the behaviour explicitly. The current comment (lines 14-16) does explain this, but the code should ideally either use the cursors or not accept them.

---

### Q4. [LOW] `watcher/stages/store.rs` deletes Tantivy document before every add

**Location**: `backend/src/watcher/stages/store.rs:117-119`

**Issue**:
```rust
index_manager.delete_document_by_field("id", &id)?;
// ... build doc ...
index_manager.add_document(doc)?;
```

For **new** items (ChangeType::Created), `delete_document_by_field` is a wasteful no-op — deleting a term that doesn't exist in the index triggers an internal Tantivy operation that resolves to nothing.

**Fix**: Only call `delete_document_by_field` when the item is an update, not a create.

---

### Q5. [LOW] Frontend: `useSearch` `maxPages: 5` conflicts with `useInfiniteMedia` `maxPages: 10`

**Location**: `frontend/src/hooks/use-infinite-media.ts:25` vs `frontend/src/hooks/use-search.ts:54`

**Issue**: Both hooks set `maxPages` to different values (10 vs 5). This is inconsistent and undocumented. The `maxPages` controls how many pages of cached data are kept in memory. When switching between browse and search modes, the discarded pages' data is lost and must be re-fetched.

**Impact**: If a user browses deeply (10 pages), then searches briefly (1-2 pages), then clears the search, the browse pages beyond 5 are evicted and must be re-fetched if scrolling resumes.

---

## 4. Drift from Plans

### D1. [HIGH] DB Schema in development plan (§4) lacks `folder_id` and `watched_folders` table

**Location**: `documents/plans/development-plan.md:250-289`

**Issue**: The canonical schema in the plan shows the v1 schema without `folder_id` column or `watched_folders` table. These were added in migration v002 (Wave 7.12). The plan's schema section was never updated.

**Impact**: Anyone reading the plan to understand the DB schema will design code against an outdated schema. The ARCHITECTURE.md has the same issue (§DB Schema section, lines 490-515).

---

### D2. [HIGH] SSE event format drift (see B3, B4)

**Location**: `documents/plans/development-plan.md:220-234`

**Issue**: The SSE event specifications in the plan do not match the implementation:
- `file_modified`: missing `metadata_updated` field
- `indexing_complete`: missing `duration_ms` field

**Impact**: Anyone implementing SSE consumers against the plan will create incompatible code.

---

### D3. [MEDIUM] `thumbnail_path` column defined but never populated (see B2)

**Issue**: The plan specifies `thumbnail_path TEXT` in the schema. The intent was to track which items have had thumbnails generated. The implementation uses a completely separate content-addressed cache directory, rendering this column vestigial.

---

### D4. [MEDIUM] ARCHITECTURE.md DB schema out of date

**Location**: `ARCHITECTURE.md:490-515`

**Issue**: Shows the v1 schema with `idx_media_path` instead of the v2 `idx_media_folder_path`. Does not include `folder_id`, `watched_folders` table, or `mime_type` index.

---

### D5. [LOW] Test file locations differ from plan

**Location**: `documents/plans/development-plan.md:533-552`

**Issue**: The plan's test structure shows:
- `backend/tests/indexer_test.rs` — but the actual file is `backend/src/indexer/indexer_test.rs` (co-located)
- Integration tests for indexer were specified as standalone tests but implemented as co-located unit tests

This is not wrong — the plan allowed for co-location — but the documented location is stale.

---

### D6. [LOW] `dnd-kit` listed in stack but never used

**Location**: `documents/plans/development-plan.md:91`

**Issue**: The stack table lists `dnd-kit 6.3` for "internal drag & drop (sortable grid)". This dependency was never added to `package.json` and never implemented. The app uses `react-dnd` exclusively.

**Impact**: Misleading documentation. Either implement reorderable grid or remove from the stack table.

---

### D7. [LOW] Route structure partially documented

**Issue**: The plan's project structure shows `backend/src/routes/media.rs` as a single file, but the actual code splits it into `media/mod.rs`, `list.rs`, `detail.rs`, `file.rs`, `thumbnail.rs`, and `tests.rs`. The ARCHITECTURE.md correctly shows the split. The development plan's structure is outdated.

---

### D8. [LOW] Plan's Tantivy schema shows `file_size` as `INDEXED` but the code has it as `STORED`

**Location**: `documents/plans/development-plan.md:301`

**Issue**: The plan says `file_size` should be `INDEXED | STORED`, the code (`search/schema.rs:16`) has just `INDEXED` (no `STORED`). This means `file_size` is searchable but cannot be retrieved from the Tantivy document directly — you'd need a separate SQLite lookup to get it. This is intentional (SQLite is the source of truth) but the documentation is wrong.

---

## 5. Severity Matrix

| ID | Category | Severity | Effort to Fix | Impact |
|----|----------|----------|---------------|--------|
| B1 | Bug | **CRITICAL** | 1-2h | Data integrity: multi-folder breaks |
| B2 | Bug | HIGH | 30m | Schema bloat, misleading |
| B3 | Bug | MEDIUM | 5m | SSE API contract violation |
| B4 | Bug | MEDIUM | 5m | SSE API contract violation |
| B5 | Bug | MEDIUM | 5m | Silent eviction failure path |
| B6 | Bug | MEDIUM | 15m | Inconsistent metadata indexing |
| B7 | Bug | LOW | 30m | Edge-case crash (32766+ results) |
| P1 | Performance | HIGH | 1h | 10-50ms added to every page load |
| P2 | Performance | MEDIUM | 1-2h | Wasted Tantivy work per page |
| P3 | Performance | MEDIUM | 15m | Minor memory overhead |
| P4 | Performance | MEDIUM | 30m | Janky zoom on rapid scrolling |
| P5 | Performance | LOW | 15m | Minor memory pressure |
| P6 | Performance | LOW | 2-3h | Slow re-index after config change |
| Q1 | Standards | MEDIUM | 15m | Inconsistent hidden-file handling |
| Q2 | Standards | MEDIUM | 30m | Search shows wrong total count |
| Q3 | Standards | LOW | 5m | Dead-code smell |
| Q4 | Standards | LOW | 15m | Minor Tantivy inefficiency |
| Q5 | Standards | LOW | 5m | Minor config inconsistency |
| D1 | Drift | HIGH | 30m | Plan schema docs are wrong |
| D2 | Drift | HIGH | 15m | SSE event docs are wrong |
| D3 | Drift | MEDIUM | 30m | Dead column in schema |
| D4 | Drift | MEDIUM | 15m | ARCHITECTURE.md stale |
| D5 | Drift | LOW | 5m | Stale test file paths |
| D6 | Drift | LOW | 5m | False dependency listing |
| D7 | Drift | LOW | 5m | Stale file structure |
| D8 | Drift | LOW | 5m | Tantivy schema doc wrong |

---

## 6. Recommendations

### Fix immediately (pre-release)
1. **B1**: Rebuild the `media_items` table to remove the `relative_path UNIQUE` constraint. Multi-folder support is broken without this.
2. **B2**: Either populate `thumbnail_path` on thumbnail generation or drop the column in a v003 migration.
3. **B3, B4**: Fix the SSE event payloads to match the API contract — or update the contract. Either way, the code and docs must agree.
4. **D1, D2, D4**: Update the development plan and ARCHITECTURE.md to reflect the current schema and SSE contract.

### Fix before production deployment
5. **P1**: Cache `COUNT(*)` or serve it from `/stats` instead of computing on every page.
6. **P2**: Either implement proper Tantivy cursor pagination (store `(score, offset)` across requests) or accept that search pagination is best-effort.
7. **Q1**: Centralize `is_hidden()` in `media_types.rs` with consistent semantics.
8. **Q2**: Fix the search `total` field to reflect actual result count, not page size.
9. **B5**: Log a warning when `available_space()` fails.

### Fix when convenient
10. **B6**: Align the watcher's metadata-storage logic with the indexer's.
11. **B7**: Add batch chunking to `batch_get_media_items` for belt-and-suspenders safety.
12. **P4**: Debounce wheel events in the image viewer.
13. **P6**: Implement true incremental indexing in the scanner layer (skip files with matching checksums).
14. **Q3-Q5, D5-D8**: Minor cleanup items — good for a "cleanup sprint."

### Already resolved (verified)
The following items from the performance plan v2.0 are confirmed **complete**:
- Wave 7 items (3.1-3.9): ✅ All done
- Route splitting (4.1): ✅ Done — `media.rs` split into `media/` module
- Watcher pipeline extraction (4.2): ✅ Done — `stages/` submodule exists
- Shared extensions constant (4.3): ✅ Done — `media_types.rs` is the source of truth
- SkeletonGrid dedup (4.5): ✅ Done — imports `SkeletonCard`
- Cargo.toml release profile (4.6): ✅ Done — LTO + codegen-units=1
- r2d2 connection pool (5.1): ✅ Done
- Background Tantivy indexing (5.2): ✅ Done
- Background cache eviction timer (5.5): ✅ Done
- `useFocusTrap` hook (6.1): ✅ Done
- `useCursorPagination` hook (6.2): ✅ Done
- Conditional hook enabling (6.3): ✅ Done in `App.tsx`
- `useDebounce` hook (6.4): ✅ Done
- Shared icons component (6.5): ✅ Done
- Ref-based image drag/pan (6.6): ✅ Done
- Detail cache cleanup (6.7): ✅ Done
- SSE event time pruning (6.8): ✅ Done
- Dead code removal (6.9): ✅ `useHealth`/`use-health` not found in codebase
- Conditional query firing (6.10): ✅ Done
- Documentation (7.1-7.4): ✅ CHANGELOG.md, CONTRIBUTING.md, ARCHITECTURE.md, SECURITY.md all exist

---

## Appendix: Files Examined

### Backend (20+ files)
- `backend/src/main.rs` (418 lines)
- `backend/src/lib.rs` (30 lines)
- `backend/Cargo.toml` (42 lines)
- `backend/src/config/mod.rs`, `settings.rs` (274 lines total)
- `backend/src/db/mod.rs`, `pool.rs`, `schema.rs`, `migrations.rs` (235 lines total)
- `backend/src/indexer/mod.rs`, `progress.rs` (522 lines total)
- `backend/src/metadata/mod.rs`, `detect.rs`, `png.rs`, `video.rs` (466 lines total)
- `backend/src/scanner/mod.rs`, `walker.rs`, `hasher.rs` (209 lines total)
- `backend/src/search/mod.rs`, `schema.rs`, `indexer.rs` (721 lines total)
- `backend/src/thumbnails/mod.rs`, `image.rs`, `video.rs`, `cache.rs`, `limiter.rs` (942 lines total)
- `backend/src/routes/config.rs`, `config/suggest.rs` (580 lines total)
- `backend/src/routes/events.rs` (338 lines)
- `backend/src/routes/health.rs` (19 lines)
- `backend/src/routes/media/mod.rs`, `list.rs`, `detail.rs`, `file.rs`, `thumbnail.rs` (730 lines total)
- `backend/src/routes/search.rs` (294 lines)
- `backend/src/routes/stats.rs` (294 lines)
- `backend/src/middleware/logging.rs`, `security.rs`, `timeout.rs`, `validation.rs` (655 lines total)
- `backend/src/watcher/mod.rs`, `handler.rs`, `stages/extract.rs`, `stages/store.rs`, `stages/broadcast.rs` (1208 lines total)
- `backend/src/media_types.rs` (35 lines)
- `backend/src/test_support.rs` (38 lines)
- `backend/tests/` (6 test files)

### Frontend (20+ files)
- `frontend/src/main.tsx` (27 lines)
- `frontend/src/App.tsx` (126 lines)
- `frontend/vite.config.ts` (28 lines)
- `frontend/package.json` (46 lines)
- `frontend/src/types/media.ts`, `api.ts` (129 lines total)
- `frontend/src/api/client.ts`, `media.ts`, `search.ts` (136 lines total)
- `frontend/src/hooks/use-infinite-media.ts`, `use-search.ts`, `use-sse.ts`, `use-sse-grid-updates.ts`, `use-cursor-pagination.ts`, `use-debounce.ts`, `use-keyboard-nav.ts`, `use-scroll-restore.ts`, `use-focus-trap.ts` (777 lines total)
- `frontend/src/store/media-atoms.ts`, `search-atoms.ts`, `sse-atoms.ts`, `ui-atoms.ts` (76 lines total)
- `frontend/src/components/media/thumbnail-card.tsx`, `thumbnail-grid.tsx`, `drag-source.tsx`, `skeleton-grid.tsx`, `skeleton-card.tsx` (411 lines total)
- `frontend/src/components/viewer/detail-view.tsx`, `image-viewer.tsx`, `video-viewer.tsx`, `metadata-panel.tsx` (567 lines total)
- `frontend/src/components/layout/app-shell.tsx`, `header.tsx` (45 lines total)
- `frontend/src/components/search/search-bar.tsx`, `media-type-filter.tsx`, `sort-toggle.tsx` (137 lines total)
- `frontend/src/components/config/config-panel.tsx` (270 lines)
- `frontend/src/components/shared/empty-state.tsx`, `error-boundary.tsx`, `error-state.tsx`, `shortcuts-panel.tsx`, `skeleton.tsx`, `icons.tsx` (517 lines total)
- `frontend/src/index.css` (14 lines)

### Documentation
- `documents/plans/development-plan.md` (946 lines)
- `documents/plans/performance-maintainability-improvement-plan.md` (805 lines)
- `ARCHITECTURE.md` (575 lines)
- `README.md` (253 lines)
- `AGENTS.md` (1 file)
