# ImageViz — Performance & Maintainability Improvement Plan

> **Version**: 1.0  
> **Date**: 2026-05-16  
> **Status**: Draft  
> **Author**: Code review — full-stack audit  

---

## 1. Motivation

Waves 0–6 delivered a working application: file scanning, metadata extraction, full-text search, thumbnail generation, SSE real-time updates, and a responsive React frontend with virtual scroll, keyboard navigation, and drag-and-drop. The codebase is well-structured with good module boundaries, thorough tests, and consistent error handling.

This plan addresses two categories of findings from a comprehensive codebase audit:

- **Wave 7 gap items** — Production hardening tasks that were planned but never started (graceful shutdown, security headers, concurrency limiting, etc.)
- **Cross-cutting improvements** — Maintainability debt (monolithic files, duplicated patterns), performance bottlenecks (single-threaded DB access, render churn), and minor inconsistencies

**Total estimated effort**: 22–32 hours  
**Depends on**: Wave 6 (all existing functionality is stable)

---

## 2. Dependency Graph

```
Wave 7 Hardening (Section 3) ──── Independent parallel tasks ──── Phase 1
                                                                    │
Backend Structural (Section 4) ── Sequential within sections ────── Phase 2
       │
       └── Backend Performance (Section 5) ── Some depend on 4 ─── Phase 3
                     │
                     └── Frontend Improvements (Section 6) ──────── Phase 4
```

**Parallel opportunities**:

| Phase | Tasks can run in parallel |
|-------|--------------------------|
| **1** | All Wave 7 items (3.1–3.9) are independent |
| **2** | Route splitting (4.1) and Watcher pipeline (4.2) are independent |
| **3** | Query pool (5.1) and Focus trap (6.1) can overlap |
| **4** | Most frontend items are independent |

---

## 3. Wave 7 — Production Hardening (Gap Closure)

**Estimated**: 8–12 hours  
**Goal**: Graceful shutdown, concurrency controls, security headers, structured logging, documentation.

### 3.1 Graceful Shutdown

| | |
|---|---|
| **Files** | `backend/src/main.rs` |
| **Estimate** | 1h |
| **Verification** | `Ctrl+C` drains active requests before exiting; no in-flight work is lost |

**Implementation**:

```rust
use tokio::signal;

async fn shutdown_signal() {
    let ctrl_c = async { signal::ctrl_c().await.expect("Failed to install Ctrl+C handler"); };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv().await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received Ctrl+C, starting graceful shutdown"),
        _ = terminate => tracing::info!("Received SIGTERM, starting graceful shutdown"),
    }
}
```

Then pass `shutdown_signal` to `axum::serve`:

```rust
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await?;
```

Also ensure the Tantivy writer commits and the file watcher is dropped cleanly.

---

### 3.2 Request Timeout Middleware

| | |
|---|---|
| **Files** | `backend/src/middleware/mod.rs`, `backend/src/middleware/timeout.rs`, `backend/src/lib.rs` |
| **Estimate** | 30m |
| **Verification** | Requests taking > 60s return 408; media file streaming exempted via path exclusion |

**Implementation**: Create `middleware/` module with a tower `Layer` wrapping `tower_http::timeout::TimeoutLayer` with a 60-second limit. Apply it after the CORS layer but before route handlers. Exclude `/api/v1/media/*/file` (streaming) and `/api/v1/events` (SSE).

---

### 3.3 Thumbnail Generation Concurrency Limiter

| | |
|---|---|
| **Files** | `backend/src/thumbnails/limiter.rs`, `backend/src/thumbnails/mod.rs` |
| **Estimate** | 1h |
| **Verification** | 50 concurrent thumbnail requests result in at most 4 concurrent `spawn_blocking` calls |

**Implementation**: Add `tokio::sync::Semaphore` with `MAX_CONCURRENT = 4` in `get_or_generate_thumbnail`. Acquire a permit before entering `spawn_blocking`. The existing per-key mutex already serializes per-file; this adds a global cap.

---

### 3.4 Disk Space Monitoring & Cache Eviction

| | |
|---|---|
| **Files** | `backend/src/thumbnails/cache.rs` |
| **Estimate** | 1.5h |
| **Verification** | When cache exceeds 1GB, oldest-accessed files are evicted until below 800MB |

**Implementation**: Add a background task that checks `cache_dir` size every 5 minutes. If > 1GB, sort files by `modified_at` and delete oldest entries until < 800MB. Use `tokio::spawn` with `interval`. Track `accessed_at` via a simple in-memory `HashMap<Path, Epoch>` or use filesystem `atime`.

---

### 3.5 Security Headers Middleware

| | |
|---|---|
| **Files** | `backend/src/middleware/security.rs`, `backend/src/middleware/mod.rs` |
| **Estimate** | 30m |
| **Verification** | `curl -I http://localhost:3001/api/v1/health` returns `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, etc. |

**Implementation**: Use `tower_http::set_header::SetResponseHeaderLayer` for each header, or create a custom middleware. Headers to include:

```
X-Content-Type-Options: nosniff
X-Frame-Options: DENY
Content-Security-Policy: default-src 'self'; img-src 'self' data:; media-src 'self'
Referrer-Policy: strict-origin-when-cross-origin
```

---

### 3.6 Input Validation & Sanitization Audit

| | |
|---|---|
| **Files** | Multiple route files (review + tests) |
| **Estimate** | 1.5h |
| **Verification** | All endpoint inputs have confirmed validation: bounds checks, type coercion, length limits, path traversal prevention |

**Checklist**:

- [ ] `GET /media?limit=` — clamped to `[1, 500]` (done, confirm tests)
- [ ] `GET /media/:id` — UUID format check
- [ ] `GET /media/:id/thumbnail?width=` — clamped to `[100, 500]` (done)
- [ ] `GET /search?q=` — max query length (e.g., 500 chars)
- [ ] `PUT /config` — non-empty path validation (done)
- [ ] `GET /config/suggest?path=` — path traversal prevention
- [ ] All IDs — reject non-UUID strings with 404 before DB lookup

---

### 3.7 Structured Request Logging

| | |
|---|---|
| **Files** | `backend/src/middleware/logging.rs`, `backend/src/middleware/mod.rs` |
| **Estimate** | 1h |
| **Verification** | Each request logged with `method`, `path`, `status`, `duration_ms`, `trace_id` |

**Implementation**: Use `tower_http::trace::TraceLayer` with custom `MakeSpan` and `OnResponse` callbacks. Generate a `uuid::Uuid::new_v4()` per request as trace ID and inject it into the tracing span. The log line should look like:

```
2026-05-16T10:30:00.123Z INFO request{method=GET path=/api/v1/media trace_id=abc123}: completed status=200 duration_ms=45
```

---

### 3.8 Production Build Scripts

| | |
|---|---|
| **Files** | `scripts/dev.sh`, `scripts/build.sh` |
| **Estimate** | 1h |
| **Verification** | `./scripts/build.sh` produces `backend/target/release/imageviz-backend` + `frontend/dist/` |

**`scripts/dev.sh`** — Starts both dev servers with a single command, uses `cargo run` and `npm run dev` with proper process management.

**`scripts/build.sh`** — Runs `cargo build --release` (backend) and `npm run build` (frontend), outputs to `dist/`.

---

### 3.9 Project README

| | |
|---|---|
| **Files** | `README.md` |
| **Estimate** | 2h |
| **Verification** | New developer can clone, configure, build, and run from README alone |

**Sections**:
- Overview (one paragraph with screenshot mockup)
- Prerequisites (Rust, Node.js, ffmpeg)
- Quick start (clone, generate fixtures, run dev)
- Configuration (environment variables, watched folders)
- Architecture (one-paragraph + diagram)
- Project structure (tree)
- Development workflow (test, lint, TDD)
- Performance targets

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

### 4.3 Shared Supported-Extensions Constant

| | |
|---|---|
| **Files** | `backend/src/scanner/walker.rs`, `backend/src/watcher/mod.rs` |
| **Estimate** | 15m |
| **Verification** | Both modules reference the same constant; MOV added to scanner if desired |

**Implementation**: Define in `backend/src/lib.rs` or a new `backend/src/media_types.rs`:

```rust
pub const SUPPORTED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "mp4", "webm", "mov"];
pub const SUPPORTED_MIME_PREFIXES: &[&str] = &["image/", "video/"];
```

Replace duplicated `is_supported_media()` functions with calls to this constant.

---

### 4.4 Create Middleware Module

| | |
|---|---|
| **Files** | `backend/src/middleware/mod.rs`, files for 3.2, 3.5, 3.7 |
| **Estimate** | 30m |
| **Verification** | `backend/src/middleware/` exists with mod.rs re-exporting all middleware layers |

---

### 4.5 Remove SkeletonGrid Duplication

| | |
|---|---|
| **Files** | `frontend/src/components/media/thumbnail-grid.tsx` (inline SkeletonGrid lines 37–50) |
| **Estimate** | 15m |
| **Verification** | `thumbnail-grid.tsx` imports `SkeletonCard`; inline `SkeletonGrid` removed |

---

## 5. Backend Performance Improvements

**Estimated**: 4–6 hours  
**Goal**: Reduce DB contention, optimize thumbnail generation, tune search indexing.

### 5.1 SQLite Read/Write Connection Separation

| | |
|---|---|
| **Files** | `backend/src/db/mod.rs`, `backend/src/main.rs`, all route files that use `db` state |
| **Estimate** | 2–3h |
| **Verification** | Read queries use a pooled connection; write operations use a dedicated writer; no test regressions |

**Current**: `Arc<Mutex<Connection>>` — every operation contends for the same mutex, defeating WAL's concurrent-reader advantage.
**Target**: One write-dedicated `Arc<Mutex<Connection>>` + a `r2d2` pool (or similar) of read-only connections from the same WAL database.

**Approach A** (simpler — preferred): Keep `Arc<Mutex<Connection>>` for writes, add `tokio::sync::RwLock<Connection>` or `r2d2::Pool<Connection>` for reads. Both share the same file — WAL allows the read pool to see committed writes without blocking.

**Approach B** (least risk): Use `rusqlite::Connection::open()` to open two separate connections to the same file. One behind `Arc<Mutex>` for writes, one behind `Arc<RwLock>` for reads. WAL ensures reads see the latest committed state.

```
AppState {
    db_write: Arc<Mutex<Connection>>,   // INSERT/UPDATE/DELETE
    db_read: r2d2::Pool<Connection>,     // SELECT queries (max 5 connections)
    // ...
}
```

**Changes needed**:
- Add `r2d2` + `r2d2_sqlite` (or manual `Connection` duplication) to `Cargo.toml`
- Initialize both in `main.rs`
- Split existing queries into read vs write families
- Route read queries through the pool, writes through the mutex
- Update `TestApp` to match

---

### 5.2 Progressive Tantivy Indexing During Startup

| | |
|---|---|
| **Files** | `backend/src/main.rs`, `backend/src/indexer/mod.rs`, `backend/src/search/indexer.rs` |
| **Estimate** | 1.5h |
| **Verification** | API responds to requests before full Tantivy reindex completes; Tantivy indexes in background |

**Current**: Full startup does: (1) `full_index` → SQLite populated, then (2) `full_reindex` → Tantivy populated. The API only starts serving after both complete.

**Target**: Start serving after SQLite phase. Tantivy reindex runs in background. A `reindex_in_progress` flag in the search route causes it to fall back to SQL-only search (slower but functional) until Tantivy is ready.

```rust
// main.rs
let (reindex_done_tx, reindex_done_rx) = tokio::sync::watch::channel(false);

// Phase 1: SQLite (fast, required for API)
let db_clone = db.clone();
full_index(&watched_folders, db_clone).await?;

// Phase 2: Tantivy (background)
let db_clone = db.clone();
let index_manager = index_manager.clone();
tokio::spawn(async move {
    full_reindex(&index_manager, db_clone).await?;
    reindex_done_tx.send(true)?;
    Ok::<_, Box<dyn std::error::Error>>(())
});

// Start serving immediately — search route checks reindex_done_rx
```

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

**Change**: Replace `LazyLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>` with `dashmap::DashMap<String, Arc<tokio::sync::Mutex<()>>>`. Simpler, lock-free reads.

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
| **Files** | `frontend/src/hooks/use-health.ts` (check if used), `frontend/src/components/shared/skeleton.tsx` (check if used directly) |
| **Estimate** | 15m |
| **Verification** | `git grep` confirms no dead code removed |

The `use-health` hook was scaffolded in Wave 0.4 but the health status is never displayed in the UI. Remove it. The `<Skeleton>` primitive is used by `SkeletonCard` only — verify no other imports.

---

## 7. Task Breakdown

### Phase 1 — Wave 7 Gap Closure (12–16 items, parallel)

| ID | Task | Files | Est. | Parallel | Verification |
|----|------|-------|------|----------|-------------|
| 7.1 | Graceful shutdown | `main.rs` | 1h | ✅ | Ctrl+C drains requests |
| 7.2 | Request timeout middleware | `middleware/timeout.rs` | 30m | ✅ | 408 on timeout |
| 7.3 | Thumbnail concurrency limiter | `thumbnails/limiter.rs` | 1h | ✅ | ≤4 concurrent spawn_blocking |
| 7.4 | Cache eviction | `thumbnails/cache.rs` | 1.5h | ✅ | Cache stays < 1GB |
| 7.5 | Security headers | `middleware/security.rs` | 30m | ✅ | Headers on all responses |
| 7.6 | Input validation audit | Multiple routes | 1.5h | ✅ | All inputs validated |
| 7.7 | Structured logging | `middleware/logging.rs` | 1h | ✅ | trace_id per request |
| 7.8 | Dev/build scripts | `scripts/dev.sh`, `scripts/build.sh` | 1h | ✅ | Single-command dev/build |
| 7.9 | README | `README.md` | 2h | ✅ | Self-documenting setup |

### Phase 2 — Backend Structural (3 tasks, partially parallel)

| ID | Task | Files | Est. | Depends on | Parallel |
|----|------|-------|------|------------|----------|
| 4.1 | Split route files | `routes/media/`, `search.rs`, `config.rs` | 3–4h | — | ✅ 4.1a + 4.1b independent |
| 4.2 | Watcher pipeline | `watcher/handler.rs` → `watcher/stages/` | 1.5h | — | ✅ (independent of 4.1) |
| 4.3 | Shared extensions constant | `media_types.rs`, + 2 edits | 15m | — | ✅ |
| 4.5 | Remove skeleton duplication | `thumbnail-grid.tsx` | 15m | — | ✅ |

### Phase 3 — Backend Performance (2 tasks, after structural changes)

| ID | Task | Files | Est. | Depends on |
|----|------|-------|------|------------|
| 5.1 | DB read/write separation | `db/mod.rs`, `main.rs`, route states | 2–3h | 4.1 (eases file edits) |
| 5.2 | Progressive Tantivy indexing | `main.rs`, `search/indexer.rs` | 1.5h | — |
| 5.3 | Tantivy writer memory tuning | `search/mod.rs` | 15m | — |
| 5.4 | DashMap for thumbnail locks | `thumbnails/cache.rs` | 30m | — |

### Phase 4 — Frontend Improvements (9 tasks, highly parallel)

| ID | Task | Files | Est. | Parallel |
|----|------|-------|------|----------|
| 6.1 | `useFocusTrap` hook | New + 3 edits | 30m | ✅ |
| 6.2 | `useCursorPagination` hook | New + 2 edits | 1h | ✅ |
| 6.3 | Conditional hook enabling | `App.tsx`, `use-search.ts` | 15m | ✅ |
| 6.4 | `useDebounce` hook | New + 1 edit | 15m | ✅ |
| 6.5 | Shared icons component | New + multiple edits | 30m | ✅ |
| 6.6 | Ref-based image drag/pan | `image-viewer.tsx` | 1h | ✅ |
| 6.7 | Detail cache cleanup | `App.tsx` | 15m | ✅ |
| 6.8 | SSE event time pruning | `use-sse-grid-updates.ts` | 15m | ✅ |
| 6.9 | Remove dead code | `use-health.ts`, misc | 15m | ✅ |

---

## 8. Total Estimates

| Phase | Name | Hours | Cumulative |
|-------|------|-------|------------|
| 1 | Wave 7 Gap Closure | 8–12h | 12h |
| 2 | Backend Structural | 4–6h | 18h |
| 3 | Backend Performance | 4–5h | 23h |
| 4 | Frontend Improvements | 4–6h | 29h |
| **Total** | | **22–32 hours** | |

---

## 9. TDD Workflow (Per Task)

Per the project convention, each task follows:

1. **Write a failing test** that defines the expected behavior
2. **Implement minimum code** to make it pass
3. **Refactor** while keeping tests green
4. **Add edge case tests** (empty inputs, nulls, errors, boundaries)
5. **Verify** with `cargo test` / `npm test` before marking complete

---

## 10. Verification

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

## 11. Performance Targets (Updated)

| Operation | Current | Target | Measurement |
|-----------|---------|--------|-------------|
| Grid scroll FPS | 60fps (virtual scroll) | 60fps sustained | Chrome DevTools Performance |
| API response under indexing load | Degraded (mutex contention) | < 50ms median | Server-side timing |
| Thumbnail generation (cache miss) | < 100ms | < 50ms | Server-side timing |
| Concurrent thumbnail generation | N (unbounded) | Max 4 | Server-side timing |
| Search response (100K dataset) | < 200ms | < 200ms (unchanged) | Server-side timing |
| SSE event → UI latency | < 500ms | < 500ms (unchanged) | End-to-end |
| Thumbnail cache max size | Unbounded | < 1GB | Filesystem monitoring |
| API first-byte latency | N/A (no timeout) | < 60s | 408 after timeout |
| Startup to API-ready | SQLite + Tantivy sequential | SQLite-only then background Tantivy | Wall clock |

---

## 12. Risk Register

| Risk | Severity | Mitigation |
|------|----------|------------|
| Route splitting breaks existing imports | Medium | Keep old function names as re-exports from `mod.rs` during transition |
| DB connection pooling adds complexity | Low | Start with simplest approach (two connections, not r2d2) |
| Focus trap refactor breaks keyboard nav | Low | Existing tests cover keyboard navigation; run after every edit |
| Image viewer ref-based transforms regress | Medium | Existing `image-viewer.test.tsx` covers zoom/pan; add test for smooth drag |
| Conditional hook enabling hides bugs | Low | Both paths exercised in integration tests (browse + search modes) |
| Cache eviction wrongfully deletes thumbnails | Medium | Start with conservative 2GB limit, log evictions to tracing |
