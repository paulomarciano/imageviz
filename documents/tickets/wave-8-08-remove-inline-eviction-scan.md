# Wave 8.8 — Remove the Per-Miss Inline Cache-Eviction Scan

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 08 |
| **Estimate** | 30 minutes |
| **Depends on** | 8.6 (same file: `thumbnails/cache.rs`) |
| **Parallel** | No |
| **Source** | Code review §4 R3 (🟡) |

---

## Overview

In addition to the 5-minute background eviction timer, **every single thumbnail generation** spawns a fire-and-forget `evict_if_needed` (`backend/src/thumbnails/cache.rs:256-265`), which calls `dir_size()` — a complete `read_dir` + stat of every cached file. With a large cache (100K+ files) and a burst of misses (first run of a new library), that's many concurrent full-directory scans competing with the generations that triggered them.

The timer already covers eviction. Delete the inline spawn.

## Prerequisites

- 8.6 merged (both tickets modify `thumbnails/cache.rs`)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §4 R3
- `backend/src/thumbnails/cache.rs:256-265` — inline `evict_if_needed` spawn
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/thumbnails/cache.rs       # inline spawn deleted; timer path untouched
backend/src/thumbnails/cache_test.rs  # assertion that generation does not scan the directory
```

## Acceptance Criteria (Pass/Fail)

- [ ] A cache-miss generation triggers no `dir_size()` call (test observes zero scans)
- [ ] The background 5-minute eviction timer still runs and evicts per `THUMBNAIL_CACHE_MAX_MB` / `MIN_FREE_DISK_MB`
- [ ] `evict_if_needed` remains reachable **only** from the timer
- [ ] Existing eviction tests (7.4) pass unchanged
- [ ] `cargo test` green

## Implementation Notes

- Straight deletion; no probabilistic/counter-based burst protection unless a real need appears (the review offers those only as options — KISS says ship without).
- While in the file, confirm the timer spawn isn't accidentally cancelled by unrelated refactors (the 5-min `tokio::spawn` with interval must stay alive for server lifetime).

## Test Strategy

- Instrument or wrap `dir_size` behind a test-visible counter (e.g., `AtomicUsize` in `cfg(test)`) — assert it is 0 after N generations, and > 0 after advancing the eviction timer (tokio time pause + advance).
- Keep the existing 7.4 eviction tests as the behavioral guard.
