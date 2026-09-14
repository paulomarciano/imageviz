# Wave 8.3 — Remove the /tmp Thumbnail Cache; Generate Directly Into the Content-Addressed Cache

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 03 |
| **Estimate** | 1.5 hours |
| **Depends on** | — |
| **Parallel** | Yes (independent of 8.1/8.2) |
| **Source** | Code review §4 R2 (🔴) |

---

## Overview

Every thumbnail generation first writes a WebP to `std::env::temp_dir()` via `thumbnail_output_path` (`backend/src/thumbnails/image.rs:100-108`), keyed by `sha256(path:width)` — and that file is **never deleted**. The cache layer then copies it into the content-addressed cache. Consequences: unbounded second cache in `/tmp` (or RAM on tmpfs systems), and an extra write + read + copy per thumbnail. The temp file only exists to make the final rename atomic — but the cache layer already has its own atomic pattern (`{key}.tmp` + `rename`, `cache.rs:250-254`).

Additionally the `/tmp` cache is **wrong as a cache**: keyed by path, not content, so renamed files regenerate and moved files alias.

## Prerequisites

- None (v0.7.0 baseline)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §4 R2
- `backend/src/thumbnails/image.rs:100-108` — `thumbnail_output_path`
- `backend/src/thumbnails/cache.rs:250-254` — existing atomic write pattern
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/thumbnails/image.rs     # generators write {key}.tmp inside cache_dir; delete thumbnail_output_path
backend/src/thumbnails/cache.rs     # generation → rename flow simplified (no copy from /tmp)
backend/src/thumbnails/*_test.rs    # updated tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] No files are written to the OS temp directory during thumbnail generation (test asserts temp dir is untouched)
- [ ] Generation writes `cache_dir/{key}.tmp` then renames onto `cache_dir/{key}.webp` atomically
- [ ] `thumbnail_output_path` and the path-keyed temp-cache logic are deleted (grep finds no references)
- [ ] Concurrent requests for the same thumbnail are still deduplicated (per-key `Mutex` tests pass)
- [ ] Cache-hit path (existing `{key}.webp`) is unchanged
- [ ] Eviction (7.4) still sees a consistent cache dir
- [ ] `cargo test` green; fixture-dependent thumbnail tests pass (`cargo test -- --ignored`)

## Implementation Notes

- Pass the target `cache_dir` down into the image/video generators (or return bytes and let the cache layer do the atomic write — prefer the smallest diff that removes the temp hop).
- Keep ffmpeg output written to `{key}.tmp` (ffmpeg needs a file target), then rename — same pattern as the image path.
- A leftover `.tmp` file from a crashed generation is harmless: the next generation overwrites it; eviction can also sweep it. No recovery logic needed (KISS).

## Test Strategy

- Test that generation produces exactly one file in the cache dir (`{key}.webp`) and zero in `std::env::temp_dir()`.
- Test that a pre-existing `{key}.tmp` does not break generation.
- Re-run concurrency test from 2.3 (two concurrent first-requests → one generation).
