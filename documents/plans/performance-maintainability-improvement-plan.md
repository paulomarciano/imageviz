# ImageViz — Performance & Maintainability Improvement Plan

> **Version**: 2.0  
> **Date**: 2026-05-17  
> **Status**: Updated  
> **Author**: Code review — full-stack audit  

---

## 1. Motivation

Waves 0–6 delivered a working application: file scanning, metadata extraction, full-text search, thumbnail generation, SSE real-time updates, and a responsive React frontend with virtual scroll, keyboard navigation, and drag-and-drop. The codebase is well-structured with good module boundaries, thorough tests, and consistent error handling.

This plan was updated after a second audit on 2026-05-17 confirming the implementation status of all items.

**Changes from v1.0**:
- Wave 7 (Section 3) items are now **all verified as ✅ complete** — the v1.0 plan was drafted before these were implemented
- Backend performance items 5.1 (SQLite pool) and 5.2 (progressive Tantivy indexing) are also ✅ complete
- Middleware module (4.4) is ✅ complete
- Several **new findings** from the v2.0 audit have been added: scanner/watcher extension inconsistency, synchronous cache eviction, `Cargo.toml` release profile, unconditional hook firing, and documentation gaps
- Total remaining effort revised downward

**Current scope**:

- **Wave 7 gap items** — ✅ All complete; retained in v2.0 as a verified record (marked with ✅) so readers know they are done
- **Backend Structural** — 5 items remaining: route splits, watcher pipeline, shared extensions constant (with new .mov inconsistency fix), release profile, SkeletonGrid dedup
- **Backend Performance** — 4 items: Tantivy memory tuning, DashMap for locks, background cache eviction, `free_disk_space()` no-op
- **Frontend Improvements** — 9 items (unchanged from v1.0, all still pending)
- **Documentation** — New section: CHANGELOG, CONTRIBUTING, ARCHITECTURE, SECURITY

**Total estimated effort**: 18–26 hours  
**Depends on**: Wave 6 (all existing functionality is stable); Wave 7 verified complete

---

## 2. Dependency Graph

```
 Wave 7 (Section 3) ──── ✅ ALL COMPLETE — not shown in task plan
                                                                     
 Backend Structural (Section 4) ── Sequential within sections ──── Phase 1
        │
        └── Backend Performance (Section 5) ── Some depend on 4 ─── Phase 2
                      │
                      └── Frontend Improvements (Section 6) ──────── Phase 3
                                    │
                                    └── Documentation (Section 7) ── Phase 4
```

**Parallel opportunities**:

| Phase | Tasks can run in parallel |
|-------|--------------------------|
| **1** | Route splitting (4.1), Watcher pipeline (4.2), shared constant (4.3), release profile (4.6), SkeletonGrid dedup (4.5) — all independent |
| **2** | Tantivy memory tuning (5.3), DashMap (5.4), background eviction (5.5) — independent; free_disk_space fix (5.6) depends on 5.5 |
| **3** | Most frontend items (6.1–6.9) are independent of each other |
| **4** | Documentation items (7.1–7.4) are independent of each other |

---

## 3. Wave 7 — Production Hardening (Gap Closure) — ✅ Complete

**All items in this section are verified as implemented in the codebase as of 2026-05-17.**  
The v1.0 plan was drafted while these were in progress; v2.0 marks them complete and retains the descriptions as a record.

### 3.1 Graceful Shutdown ✅
- **Files**: `backend/src/main.rs`
- **Implementation**: `shutdown_signal()` function (line 194) handles both SIGINT and SIGTERM. Passed to `axum::serve` via `.with_graceful_shutdown()`. `cleanup_resources()` (line 219) commits the Tantivy index before exit. File watcher is kept alive via `_watcher_guard` and drops cleanly when main exits.

### 3.2 Request Timeout Middleware ✅
- **Files**: `backend/src/middleware/timeout.rs`
- **Implementation**: `TimeoutLayer::with_status_code(408)` applied per-route-group. SSE endpoints get 3600s, media routes get 120s, everything else uses `REQUEST_TIMEOUT_SECS` env var (default 60s). Unit tests verify 408 on slow handlers.

### 3.3 Thumbnail Generation Concurrency Limiter ✅
- **Files**: `backend/src/thumbnails/limiter.rs`
- **Implementation**: `ThumbnailLimiter` wraps `tokio::sync::Semaphore` with max N (default 4 via `THUMBNAIL_CONCURRENCY` env var, line 87). Acquire has 120s timeout. Unit tests verify acquire/release/auto-release.

### 3.4 Disk Space Monitoring & Cache Eviction ✅
- **Files**: `backend/src/thumbnails/cache.rs`
- **Implementation**: `evict_if_needed()` (line 314) evaluates cache size after every thumbnail generation. Default max 2GB (`THUMBNAIL_CACHE_MAX_MB`), evicts LRU down to 80% of max. `MIN_FREE_DISK_MB` env var supported (but `free_disk_space()` always returns `u64::MAX` — see 5.6). Tracking uses filesystem `atime`.

### 3.5 Security Headers Middleware ✅
- **Files**: `backend/src/middleware/security.rs`
- **Implementation**: Six headers applied via `SetResponseHeaderLayer`: `X-Content-Type-Options: nosniff`, `X-Frame-Options: SAMEORIGIN`, `X-XSS-Protection: 0`, `Referrer-Policy: strict-origin-when-cross-origin`, `Permissions-Policy`, and `Content-Security-Policy`. Applied as outermost layer so they appear on all responses. Tests verify all headers present on 200 and 404 responses.

### 3.6 Input Validation & Sanitization Audit ✅
- **Files**: `backend/src/middleware/validation.rs`
- **Implementation**: `validate_limit` [1..500], `validate_cursor` (ISO 8601 or NaiveDateTime), `validate_cursor_id` (UUID v4), `validate_search_query` (max 1000 chars), `validate_media_id` (max 128 chars), `validate_thumbnail_width` [100..500], `validate_watched_folders` (non-empty, no `..` traversal, max 4096 chars). Comprehensive unit tests for all validators.

### 3.7 Structured Request Logging ✅
- **Files**: `backend/src/middleware/logging.rs`
- **Implementation**: `TraceLayer` with custom `MakeRequestSpan` (UUID v4 request ID, method, URI path). `LogOnRequest` logs incoming requests. `LogOnResponse` logs status and duration at INFO (2xx/3xx), WARN (4xx), or ERROR (5xx) level.

### 3.8 Production Build Scripts ✅
- **Files**: `scripts/dev.sh`, `scripts/build.sh`
- **Implementation**: `dev.sh` starts both servers with single command. `build.sh` runs `cargo build --release` + `npm run build`.

### 3.9 Project README ✅
- **Files**: `README.md`
- **Implementation**: 253-line README covering all required sections: features, prerequisites, quick start, usage (browsing, searching, viewing, keyboard shortcuts, real-time updates), configuration (env vars table), API reference, project structure tree, development commands, E2E testing, fixture generation, contributing guidelines, tech stack.

---

## 4. Backend Structural Improvements

**Estimated**: 6–9 hours  
**Goal**: Reduce file sizes, extract reusable components, eliminate inconsistencies.

### 4.1 Split Massive Route Files

| | |
|---|---|
| **Files** | `backend/src/routes/media.rs` (→ `media/` module), `backend/src/routes/search.rs`, `backend/src/routes/config.rs` |
| **Estimate** | 3–4h |
| **Verification** | No file exceeds 500 lines; all existing tests pass without modification |

**Plan**:

```
backend/src/routes/
├── mod.rs
├── health.rs               (19 lines — keep as-is)
├── media/
│   ├── mod.rs              # Router::new().route(...) assembly
│   ├── list.rs             # GET /media — pagination + filters
│   ├── detail.rs           # GET /media/{id} + /media/{id}/metadata
│   ├── file.rs             # GET /media/{id}/file — Range, ETag, 304
│   └── thumbnail.rs        # GET /media/{id}/thumbnail
├── search.rs               # 1270 lines → 300–400 after extracting helpers
├── config.rs               # 550 lines → split suggest + crud
├── events.rs               # 395 lines — review for extraction
└── stats.rs                # Keep compact
```

The extraction is primarily mechanical: move handler functions into sub-modules, share state via `MediaState` (already exists), and re-export the router from `media/mod.rs`. No logic changes.

**Why this matters**: Each sub-module becomes independently testable, reviewable in a single screen, and navigable by name rather than by searching through 1800 lines.

---

### 4.2 Watcher Handler Pipeline Extraction

| | |
|---|---|
| **Files** | `backend/src/watcher/handler.rs` |
| **Estimate** | 1.5h |
| **Verification** | `handle_file_created_or_modified` broken into ≤ 50-line pure functions; existing tests pass |

**Pipeline breakdown**:

```rust
// handler.rs — was 944 lines, becomes orchestrator (~150 lines)

async fn handle_file_created_or_modified(path: &Path, ctx: &HandlerCtx) -> Result<ChangeType> {
    let metadata = extract_on_disk_metadata(path).await?;       // step 1: async I/O
    let hash = compute_file_hash(&path).await?;                 // step 2: async I/O
    let db_entry = ctx.load_db_entry(&metadata.relative_path);   // step 3: sync read

    if db_entry.as_ref().map(|e| &e.checksum) == Some(&hash) {
        return Ok(ChangeType::Skipped);                          // early exit: no change
    }

    let changed = ctx.upsert_to_db(&metadata, &hash).await?;    // step 4: spawn_blocking
    ctx.update_tantivy(&metadata, &hash).await?;                 // step 5: spawn_blocking
    ctx.broadcast_sse(changed_to_event(&changed)).await?;        // step 6: async send
    Ok(changed)
}
```

```
handler.rs → orchestrator + pipeline stages
stages/
├── extract.rs   (from path → metadata + hash)
├── store.rs     (DB upsert + Tantivy update)
└── broadcast.rs (SSE event formatting + sending)
```

---

### 4.3 Shared Supported-Extensions Constant (fix inconsistency)

| | |
|---|---|
| **Files** | `backend/src/scanner/walker.rs`, `backend/src/watcher/mod.rs` |
| **Estimate** | 15m |
| **Verification** | Both modules reference the same constant; `.mov` extension handled consistently |

**Bug found**: The scanner (`walker.rs:18`) defines `SUPPORTED_EXTENSIONS` excluding `"mov"`, but the watcher (`mod.rs:151`) accepts `"mov"` via a hardcoded `matches!()` macro. This means `.mov` files are watched but never scanned — a silent inconsistency.

**Implementation**: Define in `backend/src/lib.rs` or a new `backend/src/media_types.rs`:

```rust
pub const SUPPORTED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "mp4", "webm", "mov"];
pub const SUPPORTED_MIME_PREFIXES: &[&str] = &["image/", "video/"];
```

Replace duplicated `is_supported_media()` functions with calls to this constant. Ensure both scanner and watcher agree on the same list.

---

### 4.4 Create Middleware Module ✅

| | |
|---|---|
| **Files** | `backend/src/middleware/mod.rs` |
| **Estimate** | 30m |
| **Verification** | `backend/src/middleware/` exists with mod.rs re-exporting all middleware layers |

**Status**: ✅ Complete. Module exists at `backend/src/middleware/mod.rs` exporting `logging`, `security`, `timeout`, and `validation` sub-modules. All four files were created during Wave 7 implementation.

---

### 4.5 Remove SkeletonGrid Duplication

| | |
|---|---|
| **Files** | `frontend/src/components/media/thumbnail-grid.tsx` (inline SkeletonGrid lines 37–50) |
| **Estimate** | 15m |
| **Verification** | `thumbnail-grid.tsx` imports `SkeletonCard`; inline `SkeletonGrid` removed |

---

### 4.6 Cargo.toml Release Profile Optimization

| | |
|---|---|
| **Files** | `backend/Cargo.toml` |
| **Estimate** | 5m |
| **Verification** | `cargo build --release` uses LTO; binary size decreases 15–25% |

**Issue**: No `[profile.release]` section exists in `Cargo.toml`. The Rust compiler defaults to thin-LTO with 16 codegen units, which optimises for compile time over runtime performance. For a binary doing CPU-bound image processing (WebP thumbnails, SHA-256 hashing, Tantivy indexing), this leaves 10–20% performance on the table.

**Implementation**:

```toml
[profile.release]
lto = "fat"           # Full link-time optimisation
codegen-units = 1     # Maximise per-function optimisation
strip = "symbols"     # Remove debug symbols (already stripped by CI)
```

---

## 5. Backend Performance Improvements

**Estimated**: 4–6 hours  
**Goal**: Reduce DB contention, optimize thumbnail generation, tune search indexing.

### 5.1 SQLite Read/Write Connection Separation ✅

| | |
|---|---|
| **Files** | `backend/src/db/pool.rs` |
| **Estimate** | 2–3h |
| **Verification** | Read queries use a pooled connection; writes use a dedicated connection |

**Status**: ✅ Complete. Implemented via `r2d2::Pool<SqliteConnectionManager>` (`db/pool.rs`). Pool size defaults to 10 connections (`DEFAULT_POOL_SIZE`), all initialized with WAL mode, foreign keys, and 5s busy timeout. All route handlers and the background indexer use `pool.get()` for read/write access. In-memory pool (max 3 connections) available for tests via `create_in_memory_pool()`.

---

### 5.2 Progressive Tantivy Indexing During Startup ✅

| | |
|---|---|
| **Files** | `backend/src/main.rs`, `backend/src/indexer/mod.rs`, `backend/src/search/indexer.rs` |
| **Estimate** | 1.5h |
| **Verification** | API responds to requests before full Tantivy reindex completes |

**Status**: ✅ Complete. `spawn_background_indexing()` (main.rs line 243) spawns Phase 2 (Tantivy reindex) in a `tokio::spawn` task while the HTTP server starts immediately after Phase 1 (SQLite). The Tantivy reindex opens a **separate read-only SQLite connection** (WAL mode allows concurrent readers) so that the pool remains available for API requests during reindex. The `search/indexer.rs` `full_reindex` function accepts this standalone connection.

---

### 5.3 Tantivy Writer Memory Tuning

| | |
|---|---|
| **Files** | `backend/src/search/mod.rs` |
| **Estimate** | 15m |
| **Verification** | `open_or_create` accepts configurable `writer_memory` parameter |

**Change**: Make the 50MB `writer_memory` a parameter of `IndexManager::open_or_create`. Use 200MB during full reindex (faster, fewer segments), 50MB during incremental (lower memory footprint).

---

### 5.4 DashMap for Thumbnail Lock Map (Optional)

| | |
|---|---|
| **Files** | `backend/src/thumbnails/cache.rs` |
| **Estimate** | 30m |
| **Verification** | No functional change; contention on global `Mutex<HashMap>` eliminated |

**Change**: Replace `LazyLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>` (cache.rs line 131) with `dashmap::DashMap<String, Arc<tokio::sync::Mutex<()>>>`. Simpler, lock-free reads. Add `dashmap` to `Cargo.toml`.

---

### 5.5 Background Cache Eviction Timer

| | |
|---|---|
| **Files** | `backend/src/thumbnails/cache.rs`, `backend/src/main.rs` |
| **Estimate** | 1.5h |
| **Verification** | Eviction runs asynchronously; no latency impact on thumbnail responses |

**Issue**: `evict_if_needed()` is called synchronously at the end of every `get_or_generate_thumbnail()` (cache.rs line 265). It performs `read_dir` + metadata scan + sort + file deletion, adding 50–500ms of latency to the first cache-miss response, particularly on large caches. This blocks the async response.

**Change**: Move eviction to a background `tokio::spawn` task with `tokio::time::interval` (every 5 minutes). The eviction task runs independently of individual thumbnail requests. Keep the inline call as a fallback for large caches, but make it best-effort (fire-and-forget spawn).

```rust
// In main.rs (or cache.rs initializer):
tokio::spawn(async {
    let mut interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        interval.tick().await;
        if let Err(e) = evict_if_needed(&cache_dir, max_cache_size(), min_free_disk_space()) {
            tracing::warn!(error = %e, "Background cache eviction failed");
        }
    }
});
```

---

### 5.6 `free_disk_space()` Returns `u64::MAX` (No-op)

| | |
|---|---|
| **Files** | `backend/src/thumbnails/cache.rs` |
| **Estimate** | 15m |
| **Verification** | Eviction responds to low disk space (platform-specific) |

**Issue**: `free_disk_space()` (cache.rs line 293) always returns `u64::MAX`, meaning the `MIN_FREE_DISK_MB` env var and the disk-space branch in `evict_if_needed()` are effectively dead code. The eviction is solely driven by `max_cache_size()`.

**Change**: Add a proper free-disk-space check using platform-specific APIs. On Linux, read `/sys/fs/...` or use `statvfs` via the `fs2` crate. On macOS, use `statfs`. The `fs2` crate provides a cross-platform `available_space()` function.

```toml
# Cargo.toml
fs2 = "0.4"
```

```rust
fn free_disk_space(path: &Path) -> u64 {
    fs2::available_space(path).unwrap_or(u64::MAX)
}
```

---

## 6. Frontend Improvements

**Estimated**: 4–6 hours  
**Goal**: DRY up duplicated patterns, reduce render churn, improve bundle organization.

### 6.1 Shared `useFocusTrap` Hook

| | |
|---|---|
| **Files** | `frontend/src/hooks/use-focus-trap.ts` |
| **Also edits** | `detail-view.tsx`, `shortcuts-panel.tsx`, `config-panel.tsx` |
| **Estimate** | 30m |
| **Verification** | All three modals still trap focus correctly |

```typescript
export function useFocusTrap(
  containerRef: React.RefObject<HTMLElement | null>,
  isActive = true,
) {
  useEffect(() => {
    if (!isActive) return;
    const el = containerRef.current;
    if (!el) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Tab') return;
      const focusable = el.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
      );
      if (focusable.length === 0) return;
      const first = focusable[0]!;
      const last = focusable[focusable.length - 1]!;
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
      else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
    };

    el.addEventListener('keydown', handleKeyDown);
    const firstFocusable = el.querySelector<HTMLElement>(focusableSelector);
    firstFocusable?.focus();
    return () => el.removeEventListener('keydown', handleKeyDown);
  }, [containerRef, isActive]);
}
```

---

### 6.2 Shared `useCursorPagination` Hook

| | |
|---|---|
| **Files** | `frontend/src/hooks/use-cursor-pagination.ts` |
| **Also edits** | `use-infinite-media.ts`, `use-search.ts` |
| **Estimate** | 1h |
| **Verification** | Both hooks behave identically; `allItems` flattening consolidated |

See Section 2 of the proposal for the API. This eliminates ~40 lines of duplicated `getNextPageParam` / `initialPageParam` boilerplate.

---

### 6.3 Conditional Hook Enabling in `App.tsx`

| | |
|---|---|
| **Files** | `frontend/src/App.tsx`, `frontend/src/hooks/use-search.ts` |
| **Estimate** | 15m |
| **Verification** | Only one of `useInfiniteMedia` / `useSearch` is actively fetching at any time |

Add an `enabled` parameter to `useSearch` and pass `viewMode === 'search'`.

---

### 6.4 `useDebounce` Hook Extraction

| | |
|---|---|
| **Files** | `frontend/src/hooks/use-debounce.ts` |
| **Also edits** | `search-bar.tsx` |
| **Estimate** | 15m |
| **Verification** | `search-bar.tsx` uses `useDebounce(localQuery, 300)`; behavior unchanged |

```typescript
export function useDebounce<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delay);
    return () => clearTimeout(timer);
  }, [value, delay]);
  return debounced;
}
```

---

### 6.5 Shared Icon Component

| | |
|---|---|
| **Files** | `frontend/src/components/shared/icons.tsx` |
| **Also edits** | `header.tsx`, `config-panel.tsx`, `shortcuts-panel.tsx`, `detail-view.tsx`, `thumbnail-card.tsx`, `empty-state.tsx`, `error-state.tsx`, `error-boundary.tsx`, `video-viewer.tsx` |
| **Estimate** | 30m |
| **Verification** | All icons render identically; no visual regressions |

```typescript
export const Icons = {
  Gear: () => (/* gear SVG */),
  Close: () => (/* X SVG */),
  Search: () => (/* magnifier SVG */),
  ChevronLeft: () => (/* arrow SVG */),
  ChevronRight: () => (/* arrow SVG */),
  ImageBroken: () => (/* broken image SVG */),
  AlertTriangle: () => (/* warning SVG */),
  // etc.
} as const;
```

---

### 6.6 Image Viewer: Ref-Based Drag/Pan (Performance)

| | |
|---|---|
| **Files** | `frontend/src/components/viewer/image-viewer.tsx` |
| **Estimate** | 1h |
| **Verification** | Zoom/pan feels smoother; no stutter during mousedown-mousemove-mouseup |

**Change**: Use `useRef` for drag state and manually apply CSS transforms during drag. Only call `setZoom` / `setPosition` on `mouseup` or `wheel` end. This eliminates re-render per mousemove event (60fps → no React reconciliation).

---

### 6.7 Detail View Cache Cleanup on Close

| | |
|---|---|
| **Files** | `frontend/src/App.tsx` (handleClose) |
| **Estimate** | 15m |
| **Verification** | Closing detail view evicts the specific media item detail from TanStack Query cache |

```typescript
const handleClose = useCallback(() => {
  setDetailOpen(false);
  if (selectedItem) {
    queryClient.removeQueries({ queryKey: ['media', 'item', selectedItem.id] });
  }
  setSelectedItem(null);
}, [setDetailOpen, setSelectedItem, selectedItem, queryClient]);
```

---

### 6.8 SSE Event Store Time-Based Pruning

| | |
|---|---|
| **Files** | `frontend/src/hooks/use-sse-grid-updates.ts` |
| **Estimate** | 15m |
| **Verification** | `recentSseEventsAtom` never contains events older than 5 minutes |

```typescript
const FIVE_MINUTES_MS = 5 * 60 * 1000;

setRecentEvents((prev) => {
  const now = Date.now();
  const filtered = [{ event, timestamp: now } as SseEvent, ...prev]
    .filter((e) => now - parseTimestamp(e) < FIVE_MINUTES_MS)
    .slice(0, 50);
  return filtered;
});
```

---

### 6.9 Clean Up Unused / Redundant Components

| | |
|---|---|
| **Files** | `frontend/src/hooks/use-health.ts`, `frontend/src/components/shared/skeleton.tsx` |
| **Estimate** | 15m |
| **Verification** | `git grep` confirms no dead code removed |

The `use-health` hook was scaffolded in Wave 0.4 but is never imported anywhere. Remove it. The `<Skeleton>` primitive is used by `SkeletonCard` only — verify no other imports.

---

### 6.10 Conditional Query Firing (Both Hooks Always Run)

| | |
|---|---|
| **Files** | `frontend/src/components/media/thumbnail-grid.tsx` |
| **Estimate** | 15m |
| **Verification** | `useInfiniteMedia` does not fire during search mode; `useSearch` does not fire during browse mode |

**Issue**: In `thumbnail-grid.tsx` (lines 59–60), both `useInfiniteMedia` and `useSearch` are called unconditionally:

```typescript
const browseData = useInfiniteMedia(100, mimeType);
const searchData = useSearch(searchQuery, 100, mimeType, sort);
```

While `useSearch` has an internal `enabled` flag (via `useInfiniteQuery`), `useInfiniteMedia` does not — it fires a `/api/v1/media` query even when the user is in search mode. This wastes network bandwidth and memory.

**Change**: Add an `enabled` parameter to `useInfiniteMedia` (matching the pattern already used by `useSearch`), and pass `viewMode !== 'search'`.

```typescript
export function useInfiniteMedia(limit = 100, mimeType?: string, enabled = true) {
  const query = useInfiniteQuery<PaginatedResponse<MediaItem>, Error>({
    queryKey: ['media', 'list', { limit, mimeType: mimeType ?? 'all' }],
    // ...
    enabled,
  });
  // ...
}
```

---

## 7. Documentation Improvements

**Estimated**: 3–5 hours  
**Goal**: Create discoverable, self-contained documentation files for contributors and users.

The project already has excellent content — a 253-line README and an 895-line development plan — but lacks the standard discoverable files that new contributors look for first.

### 7.1 CHANGELOG.md

| | |
|---|---|
| **Files** | `CHANGELOG.md` |
| **Estimate** | 1h |
| **Verification** | CHANGELOG tracks versions with dates, added/changed/fixed sections |

**Implementation**: Create a CHANGELOG following [Keep a Changelog](https://keepachangelog.com/) conventions. Populate initial entries by extracting key milestones from the 79 task tickets in `documents/tickets/` and the git log.

```markdown
# Changelog

Version 0.7.0 — 2026-05-17
### Added
- Graceful shutdown (SIGINT/SIGTERM)
- Request timeout middleware (configurable via REQUEST_TIMEOUT_SECS)
- Thumbnail generation concurrency limiter (configurable via THUMBNAIL_CONCURRENCY)
- Security headers middleware (CSP, X-Frame-Options, etc.)
- Input validation middleware (limit, cursor, UUID, path traversal checks)
- Structured request logging (trace_id per request)
- r2d2 connection pool for SQLite
- Background Tantivy indexing with separate read-only DB connection
- Thumbnail cache eviction (LRU, configurable max size)

### Changed
- media.rs, search.rs, config.rs, watcher/handler.rs — refactored (Phase 1)

Version 0.6.0 — 2026-05-10
### Added
- SSE real-time grid updates (file_created, file_deleted, file_modified)
- Configuration panel with folder suggestion
- Empty states, error boundaries, loading skeletons
- Keyboard shortcuts panel
- Accessibility audit (ARIA, focus management)

Version 0.5.0 — 2026-05-05
### Added
- Search bar with debounced full-text search
- Image viewer with zoom/pan
- Video viewer with playback controls
- Metadata panel (collapsible JSON tree)
- Detail view modal with arrow-key navigation
- OS-level drag-and-drop via react-dnd
```

### 7.2 CONTRIBUTING.md

| | |
|---|---|
| **Files** | `CONTRIBUTING.md` |
| **Estimate** | 1h |
| **Verification** | New contributor can follow setup → first change → PR workflow |

**Content**: Extract the development workflow from the README's "Contributing" section and expand it. Include:

- Development environment setup (Rust, Node.js, ffmpeg)
- TDD workflow with step-by-step instructions
- Code style and linting (cargo fmt + clippy, prettier + eslint)
- Commit message conventions
- PR workflow (branch → commit → CI → review → merge)
- Project conventions (TDD, co-located tests, AAA pattern)
- Where to find help (AGENTS.md for AI-assisted dev)

### 7.3 ARCHITECTURE.md

| | |
|---|---|
| **Files** | `ARCHITECTURE.md` |
| **Estimate** | 1.5h |
| **Verification** | Developer can understand system architecture without reading the full 895-line development plan |

**Implementation**: Extract and condense the architecture information currently buried in `documents/plans/development-plan.md` (Sections 1–4). Create a standalone document with:

- System overview and architecture diagram (ASCII)
- Technology stack table (backend + frontend)
- Data flow diagram (file on disk → scanner → SQLite → Tantivy → API → SSE)
- Key architectural decisions (WAL mode, cursor pagination, content-addressed cache)
- Module dependency graph (which crate depends on which)

### 7.4 SECURITY.md

| | |
|---|---|
| **Files** | `SECURITY.md` |
| **Estimate** | 15m |
| **Verification** | Security policy and header documentation in discoverable location |

**Implementation**: Brief document covering:

- Security headers applied (reproduced from `middleware/security.rs`)
- Input validation coverage (reproduced from `middleware/validation.rs`)
- Known security posture (local-only tool, no auth, no network exposure)
- Reporting vulnerabilities

---

## 8. Task Breakdown

### Phase 1 — Backend Structural (5 tasks, fully parallel)

| ID | Task | Files | Est. | Parallel | Verification |
|----|------|-------|------|----------|-------------|
| 4.1 | Split route files | `routes/media/` → 4 files, `search.rs`, `config.rs` | 3–4h | ✅ | No file > 500 lines; all tests pass |
| 4.2 | Watcher pipeline extraction | `watcher/handler.rs` → `watcher/stages/{extract,store,broadcast}.rs` | 1.5h | ✅ | Pipeline ≤ 50-line pure functions; tests pass |
| 4.3 | Shared extensions constant + .mov fix | `media_types.rs`, `walker.rs`, `watcher/mod.rs` | 15m | ✅ | Single const; `.mov` consistent |
| 4.5 | Remove SkeletonGrid duplication | `thumbnail-grid.tsx` | 15m | ✅ | Uses shared import |
| 4.6 | Cargo.toml release profile | `Cargo.toml` | 5m | ✅ | LTO enabled; binary smaller |

### Phase 2 — Backend Performance (4 tasks, mostly parallel)

| ID | Task | Files | Est. | Depends on | Parallel |
|----|------|-------|------|------------|----------|
| 5.3 | Tantivy writer memory tuning | `search/mod.rs` | 15m | — | ✅ (independent) |
| 5.4 | DashMap for thumbnail locks | `thumbnails/cache.rs`, `Cargo.toml` | 30m | — | ✅ (independent) |
| 5.5 | Background cache eviction timer | `thumbnails/cache.rs`, `main.rs` | 1.5h | — | ✅ (independent) |
| 5.6 | Fix `free_disk_space()` no-op | `thumbnails/cache.rs`, `Cargo.toml` | 15m | 5.5 (edits same file) | ❌ |

### Phase 3 — Frontend Improvements (10 tasks, highly parallel)

| ID | Task | Files | Est. | Parallel | Verification |
|----|------|-------|------|----------|-------------|
| 6.1 | `useFocusTrap` hook | New + 3 edits | 30m | ✅ | All 3 modals trap correctly |
| 6.2 | `useCursorPagination` hook | New + 2 edits | 1h | ✅ | Both hooks share pagination logic |
| 6.3 | Conditional hook enabling | `App.tsx`, `use-search.ts` | 15m | ✅ | Only active hook fetches |
| 6.4 | `useDebounce` hook | New + 1 edit | 15m | ✅ | Behavior unchanged |
| 6.5 | Shared icons component | New + multiple edits | 30m | ✅ | No visual regressions |
| 6.6 | Ref-based image drag/pan | `image-viewer.tsx` | 1h | ✅ | 60fps drag, no React reconciliation |
| 6.7 | Detail cache cleanup | `App.tsx` | 15m | ✅ | Query evicted on close |
| 6.8 | SSE event time pruning | `use-sse-grid-updates.ts` | 15m | ✅ | No events > 5 min old |
| 6.9 | Remove dead code | `use-health.ts` | 5m | ✅ | Confirmed unused |
| 6.10 | Conditional query firing | `thumbnail-grid.tsx`, `use-infinite-media.ts` | 15m | ✅ | No wasted queries |

### Phase 4 — Documentation (4 tasks, fully parallel)

| ID | Task | Files | Est. | Parallel | Verification |
|----|------|-------|------|----------|-------------|
| 7.1 | Create CHANGELOG.md | `CHANGELOG.md` | 1h | ✅ | Tracks versions since inception |
| 7.2 | Create CONTRIBUTING.md | `CONTRIBUTING.md` | 1h | ✅ | New dev can set up and contribute |
| 7.3 | Create ARCHITECTURE.md | `ARCHITECTURE.md` | 1.5h | ✅ | Standalone system overview |
| 7.4 | Create SECURITY.md | `SECURITY.md` | 15m | ✅ | Policy and posture documented |

---

## 9. Total Estimates

| Phase | Name | Hours | Cumulative |
|-------|------|-------|------------|
| 1 | Backend Structural | 4–6h | 6h |
| 2 | Backend Performance | 2–3h | 9h |
| 3 | Frontend Improvements | 4–6h | 15h |
| 4 | Documentation | 3–5h | 20h |
| **Total** | | **13–20 hours** | |

---

## 10. TDD Workflow (Per Task)

Per the project convention, each task follows:

1. **Write a failing test** that defines the expected behavior
2. **Implement minimum code** to make it pass
3. **Refactor** while keeping tests green
4. **Add edge case tests** (empty inputs, nulls, errors, boundaries)
5. **Verify** with `cargo test` / `npm test` before marking complete

---

## 11. Verification

### Backend

```bash
cd backend
cargo test                    # All existing tests + new tests pass
cargo clippy -- -D warnings   # No new warnings
cargo fmt --check             # Formatting consistent
```

### Frontend

```bash
cd frontend
npm test                      # All existing tests + new tests pass
npm run typecheck             # tsc --noEmit passes
npm run lint                  # ESLint clean
npx prettier --check .        # Formatting consistent
```

### Manual Smoke Test

- Start dev: `./scripts/dev.sh` (or `cargo run` + `npm run dev`)
- Visit `http://localhost:5173`
- Grid loads, thumbnails visible
- Search works, debounce feels snappy
- Click thumbnail → detail view opens with zoom/pan
- Arrow keys navigate between items
- Config panel opens, folders can be added
- Shortcuts panel opens on `?`
- `Ctrl+C` gracefully shuts down server (check logs for "graceful shutdown")
- `curl -I http://localhost:3001/api/v1/health` shows security headers

---

## 12. Performance Targets (Updated)

| Operation | Current | Target | Measurement | Status |
|-----------|---------|--------|-------------|--------|
| Grid scroll FPS | 60fps (virtual scroll) | 60fps sustained | Chrome DevTools Performance | 🟢 On target |
| API response under indexing load | < 50ms (pooled) | < 50ms median | Server-side timing | 🟢 Achieved via r2d2 pool |
| Thumbnail generation (cache miss) | < 100ms | < 50ms | Server-side timing | 🟡 Needs LTO + codegen-units |
| Concurrent thumbnail generation | Max 4 (limited) | Max 4 | Server-side timing | 🟢 Achieved via semaphore |
| Search response (100K dataset) | < 200ms | < 200ms (unchanged) | Server-side timing | 🟢 On target |
| SSE event → UI latency | < 500ms | < 500ms (unchanged) | End-to-end | 🟢 On target |
| Thumbnail cache max size | < 2GB (evicting) | < 2GB | Filesystem monitoring | 🟢 Achieved with configurable limit |
| API first-byte latency | < 60s (timeout) | < 60s | 408 after timeout | 🟢 Achieved via TimeoutLayer |
| Startup to API-ready | SQLite then background Tantivy | SQLite-only then background Tantivy | Wall clock | 🟢 Achieved via spawn_background_indexing |
| Image drag smoothness | React reconciliation per frame | 60fps with refs | Chrome DevTools FPS | 🔴 Needs ref-based drag (6.6) |
| Cache eviction latency | Synchronous after generation | Background timer | Wall clock | 🟡 Needs background timer (5.5) |

---

## 13. Risk Register

| Risk | Severity | Mitigation |
|------|----------|------------|
| Route splitting breaks existing imports | Medium | Keep old function names as re-exports from `mod.rs` during transition |
| Focus trap refactor breaks keyboard nav | Low | Existing tests cover keyboard navigation; run after every edit |
| Image viewer ref-based transforms regress | Medium | Existing `image-viewer.test.tsx` covers zoom/pan; add test for smooth drag |
| Conditional hook enabling hides bugs | Low | Both paths exercised in integration tests (browse + search modes) |
| Cache eviction wrongfully deletes thumbnails | Medium | Start with conservative 2GB limit, log evictions to tracing |
| Background eviction timer races with inline eviction | Low | Inline call becomes a no-op if the background timer already cleaned up |
| `free_disk_space()` with `fs2` crate may return platform errors | Low | Fall back to `u64::MAX` on error (same as current no-op behaviour) |
| LTO in release profile slows CI builds | Low | CI build time increase (~1 min) is acceptable for release; dev builds unaffected |
| CHANGELOG becomes outdated | Low | Treat as a post-merge checklist item: "Did you update CHANGELOG?" |
