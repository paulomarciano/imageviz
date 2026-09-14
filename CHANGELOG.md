# Changelog

All notable changes to ImageViz are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.8.0] - 2026-09-13

Wave 8 — post-audit performance, resource-load, and hygiene pass (30 tickets from the
v0.7.0 code review, `documents/code-review-kiss-dry-performance-resources.md`).

### Added
- Incremental startup indexing: unchanged files (same size + mtime) are skipped without
  re-hashing — warm restarts no longer re-hash the whole library (was: full SHA-256 of
  every file on every launch)
- Parallel Phase-1 indexing with bounded concurrency (`INDEX_CONCURRENCY`, default: CPU
  cores capped at 8; was: one file at a time)
- `CORS_ALLOW_ORIGINS` env var: comma-separated origin allowlist (was: `CorsLayer::permissive()`)
- `dev-tools` Cargo feature (off by default): tokio-console + `/debug/pprof` endpoint are
  no longer compiled into release builds; releases no longer require `--cfg tokio_unstable`
- SQLite migration v004: `watched_folders` table becomes the single source of truth; the
  legacy `config(key='watched_folders')` JSON blob is imported once and never read again
- `AppError` enum with `IntoResponse` replacing ~20 hand-written error tuples across routes

### Changed
- Startup index runs the incremental path (`run_index` core shared with the full wrapper;
  deletion cleanup diffs in memory and deletes in a single transaction)
- Thumbnails generate directly into the content-addressed cache (`{key}.tmp` + rename) —
  the never-cleaned `/tmp` staging cache and its path-keyed temp files are removed
- Thumbnail lock map is checksum-keyed with weak-value eviction (was: unbounded per-key
  `DashMap` that grew forever)
- Thumbnail semaphore is acquired only on cache misses (cache hits no longer queue behind
  in-flight generations)
- Search uses Tantivy `MultiCollector` — one index traversal per query (was: two)
- Total-count cache is keyed per mime filter and no query runs while holding the lock
- Media path resolution: single `LEFT JOIN` query + async existence checks (was: 3 queries
  + blocking `std::fs::exists()` on the request path)
- Stats endpoint folds COUNT/SUM/MAX into a single scan; the frontend settings panel
  refreshes via SSE events instead of polling every 5 s
- Request logging emits one line per request (dropped the redundant `→ request` line);
  tokio runtime uses default worker count instead of hard-coded 4
- Watcher loads watched-folder config once per event batch (was: per deletion event)
- Thumbnail downscale is single-pass (`ImageReader` + `thumbnail()`) with a decode-size cap,
  bounding transient memory spikes on large sources
- `.mov` files are detected via the ffprobe video path (was: accepted by the scanner but
  rejected by detection, erroring on every startup)
- `.png` metadata serialization and ISO timestamp formatting each live in one shared
  function (previously duplicated between indexer and watcher)
- Frontend: single data layer shared by grid and detail view (search sort/mime-filter atoms
  honored everywhere; arrow-key order always matches grid order), shared `useEscape` hook,
  shared `formatFileSize` (GB/TB), typed `put<T>()` client helper, dead SSE `onmessage`
  branch removed

### Fixed
- In-memory test pool uses a single shared connection (was: one empty database per pooled
  connection — a latent test-correctness trap)

### Removed
- Write-only `thumbnail_path` column and its per-thumbnail-request UPDATE (migration)
- Per-cache-miss full-directory eviction scan (the 5-minute timer covers eviction)
- Dead code: `IndexManager::refresh()`, `Mutex<Option<IndexWriter>>` impossible state,
  redundant `unsafe impl Send/Sync` in `db/pool.rs`, unreachable JSON-blob fallback in
  media path resolution

## [0.7.0] - 2026-05-17

### Added
- Graceful shutdown (SIGINT/SIGTERM) with Tantivy index commit before exit
- Request timeout middleware (configurable via `REQUEST_TIMEOUT_SECS`, SSE: 3600s, media: 120s, default: 60s)
- Thumbnail generation concurrency limiter (configurable via `THUMBNAIL_CONCURRENCY`, default: 4)
- Security headers middleware (CSP, X-Frame-Options, X-Content-Type-Options, Referrer-Policy, Permissions-Policy, X-XSS-Protection)
- Input validation middleware (limit, cursor, UUID, path traversal checks, search query max length)
- Structured request logging with trace_id per request (UUID v4, method, URI path, status, duration)
- r2d2 connection pool for SQLite (default 10 connections, WAL mode, 5s busy timeout)
- Background Tantivy indexing with separate read-only DB connection (API responds before full reindex completes)
- Thumbnail cache eviction (LRU, configurable max size via `THUMBNAIL_CACHE_MAX_MB`, default 2GB)
- Route splitting (media.rs, search.rs, config.rs → sub-modules for independent testability)
- Watcher pipeline extraction (handler.rs → stages/: extract.rs, store.rs, broadcast.rs)
- Shared `media_types` constant (`.mov` extension inconsistency fixed between scanner and watcher)
- Tantivy writer memory tuning (configurable buffer size: 200MB during reindex, 50MB incremental)
- DashMap for lock-free thumbnail cache synchronization
- Background cache eviction timer (5-minute interval via `tokio::time::interval`)
- Cross-platform free disk space check via `fs2` crate (`free_disk_space()` no-op → actual `available_space()` call)
- Shared `useFocusTrap` hook (3 components deduplicated: detail-view, shortcuts-panel, config-panel)
- Shared `useCursorPagination` hook (`getNextPageParam`, `initialPageParam` deduplicated across `useInfiniteMedia` and `useSearch`)
- Shared `useDebounce` hook (search-bar inline debounce replaced with reusable hook)
- Shared Icons component (10 SVG icons, 9 components migrated)
- Ref-based image drag/pan (60fps, no React reconciliation per mousemove event)
- SSE event time-based pruning (5-minute TTL on recent events)
- Conditional query firing (browse/search modes don't fetch simultaneously, `useInfiniteMedia` gains `enabled` parameter)
- Detail view TanStack Query cache cleanup on close
- Conditional hook enabling in `App.tsx` based on `viewMode`
- Cargo.toml release profile optimization (LTO fat, codegen-units 1, strip symbols)
- Production build scripts (`scripts/dev.sh`, `scripts/build.sh`)
- CHANGELOG.md, CONTRIBUTING.md, ARCHITECTURE.md, SECURITY.md documentation files
- Graceful degradation for broken thumbnails (placeholder instead of error)

### Changed
- Cargo.toml release profile (LTO fat, codegen-units 1, strip symbols)
- `media.rs` → `routes/media/` module (list, detail, file, thumbnail sub-modules)
- `search.rs` refactored (300-400 lines, extracted helper functions)
- `config.rs` refactored (split suggest + CRUD)
- `watcher/handler.rs` → orchestrator + `stages/` pipeline (≤ 50-line pure functions)
- SkeletonGrid removed from `thumbnail-grid.tsx` → shared import

### Fixed
- `.mov` extension inconsistency between scanner and watcher (both now reference shared constant)
- `free_disk_space()` no-op stub → actual `fs2::available_space()` call
- Unused `use-health` hook removed
- Redundant `<Skeleton>` component verified no other imports

## [0.6.0] - 2026-05-10

### Added
- SSE real-time grid updates (file_created, file_deleted, file_modified events)
- SSE connection hook with automatic reconnect and exponential backoff
- Jotai-based real-time grid state integration
- Configuration panel with watched folder management (add/remove, live index stats)
- Folder path suggestion endpoint (`GET /config/suggest`)
- Empty state component (no folders configured, no search results)
- Error boundary and error state components with retry buttons
- Loading skeleton components (grid, detail, config views)
- Keyboard shortcuts panel (`?` key to toggle)
- Accessibility audit (ARIA attributes, focus management, keyboard navigation, WCAG AA)
- Playwright E2E tests covering full user journeys
- Performance profiling and optimization (60fps grid with 10K visible items, search < 200ms)

## [0.5.0] - 2026-05-05

### Added
- Search bar with debounced full-text search (300ms debounce)
- Search → grid wiring via Jotai atoms (live filtering)
- Image viewer with zoom (scroll), pan (drag), double-click fit toggle
- Video viewer with playback controls (Space play/pause, arrow seek 5s, F fullscreen)
- Metadata panel (collapsible JSON tree view of ComfyUI prompt and workflow data)
- Detail view modal with arrow-key navigation (← → between items, Esc to close)
- OS-level drag-and-drop via react-dnd (drag thumbnail to file explorer/external apps)
- Keyboard navigation in grid (arrows, Enter, Space)
- Integration tests for search, detail view, and drag-and-drop user journeys

## [0.4.0] - 2026-04-28

### Added
- Thumbnail generation (WebP via `image` crate, Lanczos3 filtering)
- Video thumbnail extraction (ffmpeg keyframe at 1s, piped to WebP)
- Content-addressed on-disk thumbnail cache (second request returns instantly)
- Thumbnail serving endpoint (`GET /media/:id/thumbnail`, configurable width 100–500px)
- Original file serving endpoint (`GET /media/:id/file`, streaming via `tokio::fs::File`)
- Caching headers (ETag, Cache-Control, Last-Modified, 304 Not Modified responses)
- Range request support for video seeking (Accept-Ranges header, partial content)
- Integration tests for all media endpoints with real files

## [0.3.0] - 2026-04-20

### Added
- Tantivy full-text search index (Lucene-like inverted index, fuzzy/regex queries)
- Tantivy index population from SQLite (incremental re-index)
- Full-text search endpoint (`GET /search?q=...&cursor=...&limit=...`)
- Cursor-based pagination for media list (`WHERE (created_at, id) < (?, ?)`, O(log n))
- File system watcher via notify + notify-debouncer-mini (500ms debounce)
- Watcher → indexer → broadcast channel wiring
- SSE endpoint for real-time file events (`GET /events`, reconnect support)
- Stats endpoint (`GET /stats`: total files, histogram, indexing status)
- Media list endpoint (`GET /media`, cursor-based, mime_type filter)

## [0.2.0] - 2026-04-10

### Added
- React 19 application scaffold via Vite 8
- TypeScript strict configuration (strict mode, type-safe API contracts)
- Tailwind CSS v4 setup (zero-runtime, utility-first styling)
- Vitest + @testing-library/react test setup (jsdom environment)
- ESLint flat config (typescript-eslint recommended, no-unused-vars error, no-explicit-any warn)
- Prettier configuration (single quotes, trailing commas, semicolons, 100 print width)
- Application shell layout (header bar + main content area)
- Thumbnail card component (image, filename, dimensions, loading skeleton)
- Virtualized thumbnail grid via react-virtuoso (100K+ items, masonry layout)
- Responsive layout (adaptive column count, smooth resize)
- Scroll position restoration (returning from detail view restores position)
- TypeScript API types matching backend contract
- API client layer (typed fetch wrappers)
- `useInfiniteMedia` hook (TanStack Query infinite query with cursor pagination)
- `useSearch` hook (debounced full-text search with results)
- Component and hook tests (Vitest)

### Changed
- Initial project scaffolding from Vite template to full application structure

## [0.1.0] - 2026-03-01

### Added
- Rust backend scaffold with Axum 0.8 web framework and Tokio 1.x async runtime
- Health-check endpoint (`GET /api/v1/health` returns `{"status":"ok"}`)
- Vite proxy configuration (frontend → backend, no CORS issues in dev)
- Project monorepo structure (backend/, frontend/, test-fixtures/)
- `.gitignore` with Rust/Node/OS exclusions
- Rustfmt configuration (max_width 100, tab_spaces 4, edition 2024)
- Cargo workspace with dependencies (Axum, Tokio, SQLite, Tantivy, notify, image, serde, tracing)
- ESLint + Prettier configuration for frontend linting/formatting
- GitHub Actions CI pipeline (4 parallel jobs: backend-lint, backend-test, frontend-lint, frontend-test)
- Health endpoint unit tests (backend: `tower::ServiceExt::oneshot`, frontend: smoke test)
- Initial development plan (documents/plans/development-plan.md)
- SQLite database schema (media_items, config tables with indexes)
- Test fixture generation script (`scripts/generate-fixtures.sh`)

[0.8.0]: https://github.com/paulomarciano/imageviz/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/paulomarciano/imageviz/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/paulomarciano/imageviz/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/paulomarciano/imageviz/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/paulomarciano/imageviz/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/paulomarciano/imageviz/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/paulomarciano/imageviz/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/paulomarciano/imageviz/releases/tag/v0.1.0
