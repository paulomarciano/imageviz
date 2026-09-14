# Wave 8.17 — Media Path Resolution: One JOIN Query and Async Existence Checks

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 17 |
| **Estimate** | 1.5 hours |
| **Depends on** | 8.4 (JSON fallback must already be gone — this rewrites the same function) |
| **Parallel** | No |
| **Source** | Code review §3 P6 (🟡) |

---

## Overview

`resolve_media_path` (`backend/src/routes/media/file.rs:22-92`) issues: (1) media row lookup, (2) `folder_id` lookup of the *same row*, (3) watched-folder lookup, then possibly (4) config-JSON parsing (removed by 8.4). Meanwhile `serve_file` and `serve_thumbnail` each run a **second** query against the same row for `checksum`/`modified_at`, and `full.exists()` + the fallback-loop `exists()` are **blocking** `std::fs` calls inside async handlers (`file.rs:62,86`).

Fix: one query and one async existence check per request:

```sql
SELECT m.relative_path, m.mime_type, m.filename,
       COALESCE(m.checksum,''), COALESCE(m.file_modified_at,''), w.path
FROM media_items m
LEFT JOIN watched_folders w ON w.id = m.folder_id
WHERE m.id = ?1
```

## Prerequisites

- 8.4 merged (single source of truth; no fallback loop left to preserve)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §3 P6
- `backend/src/routes/media/file.rs:22-92`, `backend/src/routes/media/thumbnail.rs:34-43`
- `tokio::fs::try_exists` docs
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/routes/media/file.rs        # single-query resolve; shared by file + thumbnail routes; tokio::fs::try_exists
backend/src/routes/media/thumbnail.rs   # consumes the shared resolution (no second query)
```

## Acceptance Criteria (Pass/Fail)

- [ ] A media-file request performs exactly **one** SQL statement for resolution (media + folder + checksum + modified_at together)
- [ ] A thumbnail request performs exactly **one** resolution statement (no separate checksum/modified_at query)
- [ ] No `std::fs` blocking calls on the request path — existence checks use `tokio::fs::try_exists` (grep-verified)
- [ ] 404 semantics preserved: missing media row, dangling `folder_id`, missing watched folder, and missing on-disk file each yield the same responses as before
- [ ] Range-request/ETag behavior for files and thumbnails unchanged
- [ ] Existing media integration tests (2.5–2.8) pass unchanged
- [ ] `cargo clippy -- -D warnings` green

## Implementation Notes

- Return a small `ResolvedMedia { full_path, mime_type, filename, checksum, modified_at }` struct from the resolver; both routes destructure what they need.
- `COALESCE` keeps the empty-string contract the ETag/304 code currently relies on — verify against the existing header tests before changing representation.
- LEFT JOIN semantics: a dangling `folder_id` yields `w.path = NULL` → keep the current "treat as missing folder" 404 path explicitly (don't let `NULL` become an empty path that accidentally resolves to CWD).

## Test Strategy

- Query-count assertion in integration test (trace hook or `cfg(test)` counter): file request = 1 resolve statement; thumbnail request = 1.
- 404 matrix test: the four missing-thing cases above.
- Re-run 2.5–2.8 suites (headers, ranges, caching).
