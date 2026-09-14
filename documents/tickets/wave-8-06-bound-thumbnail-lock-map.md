# Wave 8.6 — Bound the Thumbnail Lock Map (Weak-Value Eviction)

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 06 |
| **Estimate** | 1.5 hours |
| **Depends on** | — |
| **Parallel** | Yes |
| **Source** | Code review §4 R1 (🔴) |

---

## Overview

`static LOCKS: DashMap<String, Arc<Mutex<()>>>` (`backend/src/thumbnails/cache.rs:130-146`) gains one entry per unique `{checksum[:16]}_{width}` key and **nothing ever removes entries**. For a 1M-file library viewed at a couple of widths that's millions of retained `String + Arc<Mutex>` entries — plausibly hundreds of MB of RAM in a map whose entries are needed only while a generation is in flight.

Fix (review options 1 + 2, simplest first):
1. **Shrink the key**: lock on `checksum[:16]` only, not per-width — fewer entries, still correct.
2. **Evict after use**: store `Weak<Mutex<()>>` values; after releasing, remove the entry if no other holder exists (conditional `remove` via the `dashmap` entry API).

## Prerequisites

- None (v0.7.0 baseline)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §4 R1
- `backend/src/thumbnails/cache.rs:130-146` — `LOCKS` definition and usage
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/thumbnails/cache.rs      # LOCKS: DashMap<String, Weak<Mutex<()>>> keyed by checksum
backend/src/thumbnails/cache_test.rs # lifecycle tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] Locks are keyed by `checksum[:16]` only (same file at different widths share one lock entry while in flight)
- [ ] Entries are removed once the last in-flight generation for that key finishes (map returns to its prior size)
- [ ] Concurrent generation for the same checksum is still deduplicated (only one ffmpeg/image run; second request waits and then cache-hits)
- [ ] Concurrent generations for different checksums proceed in parallel
- [ ] No deadlock or lock-upgrade race when a request arrives while another is evicting (stress test)
- [ ] Memory behavior: after a generation burst completes, `LOCKS.len()` is bounded (test asserts == 0)
- [ ] `cargo test` green

## Implementation Notes

```rust
static LOCKS: Lazy<DashMap<String, Weak<Mutex<()>>>> = Lazy::new(DashMap::new);

fn lock_for(key: &str) -> Arc<Mutex<()>> {
    loop {
        let arc = LOCKS.entry(key.to_owned()).or_insert_with(|| Arc::new(Mutex::new(())).into()).clone();
        if let Some(strong) = arc.upgrade() { return strong; }
        // entry was concurrently evicted — retry
    }
}

fn release_lock(key: &str) {
    LOCKS.remove_if(key, |_, v| v.strong_count() == 1);
}
```

- Race-safety: `or_insert_with` + `upgrade` + retry loop handles the case where eviction removes the entry between lookup and upgrade.
- Removing at `strong_count() == 1` means only the map still holds it — safe to evict.
- Do **not** build an LRU (review marks it overkill).

## Test Strategy

- Lifecycle: generate thumbnail for key A → drop all guards → `LOCKS.len() == 0`.
- Dedup preserved: two concurrent first-requests for the same checksum (different widths) → single generation each, no interleaved double-write.
- Stress (tokio test, e.g. 64 tasks over 8 keys, random sleeps): completes, map empties, no panic — guards against the upgrade/evict race.
