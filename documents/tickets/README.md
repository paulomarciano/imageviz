# ImageViz — Task Tickets

> Waves 0–7 generated from `documents/plans/development-plan.md`  
> Wave 8 generated from `documents/code-review-kiss-dry-performance-resources.md` (v0.7.0 audit)  
> 108 tickets across 9 development waves  
> Last updated: 2026-09-05

---

## Wave 0 — Project Scaffolding & CI (9 tickets, ~4–6h)

| # | Ticket | Est. | Deps | Description |
|---|--------|------|------|-------------|
| 0.1 | [Init monorepo structure](./wave-0-01-init-monorepo.md) | 30m | — | Create directories, .gitignore, minimal README |
| 0.2 | [Scaffold Rust backend](./wave-0-02-scaffold-rust-backend.md) | 45m | 0.1 | Axum hello-world on port 3001 |
| 0.3 | [Scaffold React frontend](./wave-0-03-scaffold-react-frontend.md) | 45m | 0.1 | Vite + React + TypeScript + Tailwind |
| 0.4 | [Health-check endpoints](./wave-0-04-health-check-endpoints.md) | 30m | 0.2, 0.3 | GET /api/v1/health on both sides |
| 0.5 | [Vite proxy to backend](./wave-0-05-vite-proxy.md) | 15m | 0.4 | Proxy /api → localhost:3001 |
| 0.6 | [Linting tooling](./wave-0-06-linting-tooling.md) | 30m | 0.2, 0.3 | ESLint, Prettier, rustfmt, clippy |
| 0.7 | [Backend health tests](./wave-0-07-backend-health-tests.md) | 30m | 0.4 | Unit tests for health endpoint |
| 0.8 | [Frontend smoke test](./wave-0-08-frontend-smoke-test.md) | 30m | 0.3 | App renders without crashing |
| 0.9 | [CI configuration](./wave-0-09-ci-config.md) | 45m | 0.6–0.8 | GitHub Actions on push/PR |

---

## Wave 1 — Backend: Scanner & Metadata (10 tickets, ~10–14h)

| # | Ticket | Est. | Deps | Description |
|---|--------|------|------|-------------|
| 1.1 | [SQLite schema + migrations](./wave-1-01-sqlite-schema.md) | 1.5h | 0.2 | Tables, indexes, WAL mode, migration runner |
| 1.2 | [Config management](./wave-1-02-config-management.md) | 1.5h | 1.1 | GET/PUT /config for watched folders |
| 1.3 | [File system scanner](./wave-1-03-file-system-scanner.md) | 2h | 1.2 | walkdir-based directory walker |
| 1.4 | [PNG metadata extraction](./wave-1-04-png-metadata.md) | 2h | — | tEXt/iTXt chunk parser for ComfyUI PNGs |
| 1.5 | [Video metadata extraction](./wave-1-05-video-metadata.md) | 2h | — | ffprobe sidecar for dimensions/duration |
| 1.6 | [File type detection](./wave-1-06-file-type-detection.md) | 1.5h | 1.4, 1.5 | MIME + dimensions for any media file |
| 1.7 | [File hash computation](./wave-1-07-file-hash.md) | 1h | — | SHA-256 streaming hash for change detection |
| 1.8 | [Indexer orchestration](./wave-1-08-indexer-orchestration.md) | 2.5h | 1.1, 1.3, 1.6, 1.7 | Scan → extract → store pipeline |
| 1.9 | [Indexer progress reporting](./wave-1-09-indexer-progress.md) | 1h | 1.8 | Progress tracking via watch channel |
| 1.10 | [Integration tests: indexer](./wave-1-10-integration-test.md) | 1.5h | 1.8 | End-to-end indexer test with fixtures |

---

## Wave 2 — Backend: Thumbnails & Media Serving (8 tickets, ~8–10h)

| # | Ticket | Est. | Deps | Description |
|---|--------|------|------|-------------|
| 2.1 | [Image thumbnails](./wave-2-01-image-thumbnails.md) | 2.5h | — | WebP thumbnails via image crate + Lanczos3 |
| 2.2 | [Video thumbnails](./wave-2-02-video-thumbnails.md) | 2h | — | ffmpeg keyframe extraction |
| 2.3 | [Thumbnail cache](./wave-2-03-thumbnail-cache.md) | 1.5h | 2.1, 2.2 | Content-addressed on-disk cache |
| 2.4 | [Thumbnail serving](./wave-2-04-thumbnail-serving.md) | 1h | 2.3 | GET /media/:id/thumbnail endpoint |
| 2.5 | [File serving](./wave-2-05-file-serving.md) | 1h | — | GET /media/:id/file streaming |
| 2.6 | [Caching headers](./wave-2-06-caching-headers.md) | 45m | 2.4, 2.5 | ETag, Cache-Control, 304 responses |
| 2.7 | [Range requests](./wave-2-07-range-requests.md) | 1.5h | 2.5 | 206 Partial Content for video seeking |
| 2.8 | [Integration tests: media](./wave-2-08-media-integration-tests.md) | 1.5h | 2.4–2.7 | Full media endpoint testing |

---

## Wave 3 — Backend: Search, Pagination & SSE (9 tickets, ~10–14h)

| # | Ticket | Est. | Deps | Description |
|---|--------|------|------|-------------|
| 3.1 | [Tantivy schema](./wave-3-01-tantivy-schema.md) | 2h | — | Index schema, writer, reader setup |
| 3.2 | [Tantivy population](./wave-3-02-tantivy-index-population.md) | 1.5h | 1.8, 3.1 | SQLite → Tantivy reindex |
| 3.3 | [Search endpoint](./wave-3-03-search-endpoint.md) | 2h | 3.2 | GET /search with full-text query |
| 3.4 | [Cursor pagination](./wave-3-04-cursor-pagination.md) | 2h | 3.2 | GET /media with cursor-based pages |
| 3.5 | [File watcher](./wave-3-05-file-watcher.md) | 2.5h | — | notify + debouncer setup |
| 3.6 | [Watcher → Indexer](./wave-3-06-watcher-to-indexer.md) | 1.5h | 3.5, 1.8 | Events → index updates → broadcast |
| 3.7 | [SSE endpoint](./wave-3-07-sse-endpoint.md) | 2h | 3.6 | GET /events Server-Sent Events |
| 3.8 | [Stats endpoint](./wave-3-08-stats-endpoint.md) | 45m | 1.8 | GET /stats index statistics |
| 3.9 | [Integration tests: search + SSE](./wave-3-09-search-sse-integration-tests.md) | 2h | 3.3, 3.7 | Full search and SSE tests |

---

## Wave 4 — Frontend: Core Layout & Infinite Scroll (10 tickets, ~12–16h)

| # | Ticket | Est. | Deps | Description |
|---|--------|------|------|-------------|
| 4.1 | [TypeScript API types](./wave-4-01-typescript-api-types.md) | 1h | — | Typed API contract definitions |
| 4.2 | [API client layer](./wave-4-02-api-client.md) | 1.5h | 4.1 | Fetch wrapper, typed endpoints |
| 4.3 | [useInfiniteMedia hook](./wave-4-03-use-infinite-media-hook.md) | 2h | 4.2 | TanStack Query infinite scroll |
| 4.4 | [useSearch hook](./wave-4-04-use-search-hook.md) | 1.5h | 4.2 | Debounced search with pagination |
| 4.5 | [App shell layout](./wave-4-05-app-shell-layout.md) | 2h | — | Dark theme header + main content |
| 4.6 | [Thumbnail card](./wave-4-06-thumbnail-card.md) | 2h | 4.1 | Card with image, skeleton, error states |
| 4.7 | [Virtualized grid](./wave-4-07-virtualized-thumbnail-grid.md) | 3h | 4.3, 4.6 | react-virtuoso with infinite scroll |
| 4.8 | [Responsive masonry](./wave-4-08-responsive-masonry.md) | 2h | 4.7 | Adaptive columns for vertical/horizontal screens |
| 4.9 | [Scroll restoration](./wave-4-09-scroll-restore.md) | 1h | 4.7 | Restore position on return from detail view |
| 4.10 | [Component tests](./wave-4-10-component-tests.md) | 2h | 4.3–4.9 | Vitest + MSW for all Wave 4 components |

---

## Wave 5 — Frontend: Search, Detail & Drag (9 tickets, ~12–16h)

| # | Ticket | Est. | Deps | Description |
|---|--------|------|------|-------------|
| 5.1 | [Search bar](./wave-5-01-search-bar.md) | 1.5h | 4.4 | Debounced input with clear/escape |
| 5.2 | [Search → grid wiring](./wave-5-02-search-grid-wiring.md) | 1.5h | 5.1, 4.7 | Jotai atoms connecting search and grid |
| 5.3 | [Image viewer](./wave-5-03-image-viewer.md) | 3h | 4.1 | Zoom/pan full-resolution viewer |
| 5.4 | [Video viewer](./wave-5-04-video-viewer.md) | 2.5h | 4.1 | HTML5 video player with keyboard controls |
| 5.5 | [Metadata panel](./wave-5-05-metadata-panel.md) | 2h | 4.1 | Collapsible JSON tree with syntax highlighting |
| 5.6 | [Detail view shell](./wave-5-06-detail-view-shell.md) | 2h | 5.3–5.5 | Modal with navigation, close, info bar |
| 5.7 | [Drag-and-drop](./wave-5-07-drag-and-drop.md) | 2h | 4.6 | react-dnd for OS-level file drag |
| 5.8 | [Keyboard navigation](./wave-5-08-keyboard-navigation.md) | 1.5h | 4.7 | Arrow keys grid navigation, roving tabindex |
| 5.9 | [Integration tests](./wave-5-09-integration-tests.md) | 2.5h | 5.1–5.8 | User journey tests (search → click → view → close) |

---

## Wave 6 — Frontend: SSE, Config & Polish (11 tickets, ~10–14h)

| # | Ticket | Est. | Deps | Description |
|---|--------|------|------|-------------|
| 6.1 | [SSE hook](./wave-6-01-sse-hook.md) | 2h | 4.2 | EventSource connection with reconnect |
| 6.2 | [Real-time grid updates](./wave-6-02-real-time-grid-updates.md) | 2h | 6.1, 4.7 | SSE events → Query cache manipulation |
| 6.3 | [Config panel](./wave-6-03-config-panel.md) | 2.5h | 4.2 | Folder picker, save, stats display |
| 6.4 | [Folder suggestion](./wave-6-04-folder-suggestion.md) | 1h | 1.2 | Backend endpoint for path autocomplete |
| 6.5 | [Empty state](./wave-6-05-empty-state.md) | 45m | — | Reusable empty/no-results component |
| 6.6 | [Error boundary + states](./wave-6-06-error-boundary.md) | 1.5h | — | React error boundary, retryable error UI |
| 6.7 | [Loading skeletons](./wave-6-07-loading-skeletons.md) | 1h | — | Skeleton grid, card, detail placeholders |
| 6.8 | [Keyboard shortcuts](./wave-6-08-keyboard-shortcuts.md) | 1h | 5.8 | `?` overlay with all shortcuts |
| 6.9 | [Accessibility audit](./wave-6-09-accessibility.md) | 2h | All UI | ARIA, focus, contrast (WCAG AA) |
| 6.10 | [Performance optimization](./wave-6-10-performance.md) | 2h | All UI | 60fps scroll, <500MB memory, profiling |
| 6.11 | [E2E tests](./wave-6-11-e2e-tests.md) | 3h | 6.1–6.10 | Playwright full user journeys |

---

## Wave 7 — Production Readiness & Hardening (12 tickets, ~12–14h)

| # | Ticket | Est. | Deps | Description |
|---|--------|------|------|-------------|
| 7.1 | [Graceful shutdown](./wave-7-01-graceful-shutdown.md) | 1h | 0.2 | Drain in-flight requests on Ctrl+C/SIGTERM |
| 7.2 | [Request timeout](./wave-7-02-request-timeout.md) | 30m | 0.2 | 408 on long-running requests |
| 7.3 | [Thumbnail concurrency limit](./wave-7-03-concurrency-limit.md) | 1h | 2.3 | Semaphore-based generation limiting |
| 7.4 | [Cache eviction (LRU)](./wave-7-04-cache-eviction.md) | 1.5h | 2.3 | Disk space monitoring + LRU eviction |
| 7.5 | [Security headers](./wave-7-05-security-headers.md) | 45m | 0.2 | CSP, X-Frame-Options, etc. |
| 7.6 | [Input validation](./wave-7-06-input-validation.md) | 1.5h | All routes | 400 responses for invalid parameters |
| 7.7 | [Structured logging](./wave-7-07-structured-logging.md) | 1h | 0.2 | Request ID, duration, JSON format |
| 7.8 | [Build scripts](./wave-7-08-build-scripts.md) | 1h | 0.2, 0.3 | dev.sh + build.sh |
| 7.9 | [Missing thumbnail handling](./wave-7-09-missing-thumbnails.md) | 30m | 4.6 | Placeholder for broken thumbnails |
| 7.10 | [Project README](./wave-7-10-readme.md) | 2h | All | Comprehensive project documentation |
| 7.11 | [Connection pool (r2d2)](./wave-7-11-connection-pool.md) | 2h | 1.1 | Replace Arc\<Mutex\<Connection>> with r2d2 pool |
| 7.12 | [folder_id column](./wave-7-12-folder-id-column.md) | 2h | 1.1, 1.2, 1.8 | Resolve path ambiguity across multiple folders |

---

## Wave 8 — Post-Audit: Performance, Resources & Hygiene (30 tickets, ~32–36h)

> Source: `documents/code-review-kiss-dry-performance-resources.md` (v0.7.0 full-stack audit).  
> Ticket numbers in "Covers" refer to the review's finding IDs. Phases follow the review's
> Recommended Order of Work.

### Phase 1 — Startup & Disk (biggest wins)

| # | Ticket | Est. | Deps | Covers | Description |
|---|--------|------|------|--------|-------------|
| 8.1 | [Startup incremental index](./wave-8-01-startup-incremental-index.md) | 1.5h | — | P1a | Wire startup to the existing mtime+size incremental indexer (no full re-hash) |
| 8.2 | [Parallel Phase-1 processing](./wave-8-02-parallel-phase1-processing.md) | 2h | 8.1 | P1b | `buffer_unordered(N)` file processing; `INDEX_CONCURRENCY` env |
| 8.3 | [Remove /tmp thumbnail cache](./wave-8-03-remove-tmp-thumbnail-cache.md) | 1.5h | — | R2 | Generate directly into content-addressed cache; delete never-cleaned temp cache |

### Phase 2 — Data Model

| # | Ticket | Est. | Deps | Covers | Description |
|---|--------|------|------|--------|-------------|
| 8.4 | [Single watched-folder source](./wave-8-04-single-watched-folder-source.md) | 2h | — | K1, D1 | `watched_folders` table as only source of truth; delete JSON blob + fallbacks |
| 8.5 | [.mov file detection](./wave-8-05-mov-file-detection.md) | 1h | — | P2 | ffprobe video path for `.mov`; extension/detection drift guard |

### Phase 3 — Thumbnail Path

| # | Ticket | Est. | Deps | Covers | Description |
|---|--------|------|------|--------|-------------|
| 8.6 | [Bound thumbnail lock map](./wave-8-06-bound-thumbnail-lock-map.md) | 1.5h | — | R1 | Checksum-keyed locks + weak-value eviction (was: unbounded DashMap) |
| 8.7 | [Drop thumbnail_path column](./wave-8-07-drop-thumbnail-path-column.md) | 30m | — | R5 | Remove unread-column write per thumbnail request + migration |
| 8.8 | [Remove inline eviction scan](./wave-8-08-remove-inline-eviction-scan.md) | 30m | 8.6 | R3 | Kill per-miss full-directory scan; timer covers eviction |
| 8.9 | [Semaphore on cache miss only](./wave-8-09-semaphore-on-cache-miss.md) | 1h | 8.7, 8.8 | P5 | Cache hits bypass `THUMBNAIL_CONCURRENCY` limiter |

### Phase 4 — Search & Frontend Correctness

| # | Ticket | Est. | Deps | Covers | Description |
|---|--------|------|------|--------|-------------|
| 8.10 | [Single frontend data layer](./wave-8-10-single-frontend-data-layer.md) | 2h | — | D7 | Jotai atom for grid→detail data; kills duplicate hooks + nav-order drift |
| 8.11 | [Tantivy MultiCollector](./wave-8-11-tantivy-multicollector.md) | 45m | — | P3 | One index traversal per search (count + top-docs) |
| 8.12 | [Count cache by filter](./wave-8-12-count-cache-by-filter.md) | 1h | — | P4 | Per-mime-filter 30s count cache; no lock held across query |

### Phase 5 — Hygiene Batch

| # | Ticket | Est. | Deps | Covers | Description |
|---|--------|------|------|--------|-------------|
| 8.13 | [Consolidate indexer paths](./wave-8-13-consolidate-indexer-paths.md) | 1.5h | 8.1, 8.2 | D2 | One `run_index` core with skip closure; wrappers become thin |
| 8.14 | [Tantivy index_rows core](./wave-8-14-tantivy-index-rows-core.md) | 1h | 8.13 | D3 | Shared Tantivy row indexing (or delete dead incremental path) |
| 8.15 | [Shared metadata + timestamp utils](./wave-8-15-shared-metadata-timestamp-utils.md) | 45m | — | D4, D5 | One `metadata_to_json`; one ISO timestamp formatter |
| 8.16 | [AppError + shared responses](./wave-8-16-app-error-shared-responses.md) | 2h | 8.11, 8.12, 8.17, 8.19 | D6 | `AppError` enum with `IntoResponse`; single `MediaItemSummary` |
| 8.17 | [Media path single query](./wave-8-17-media-path-single-query.md) | 1.5h | 8.4 | P6 | One LEFT JOIN per request; `tokio::fs::try_exists` (no blocking `exists()`) |
| 8.18 | [remove_deleted_items in-memory diff](./wave-8-18-remove-deleted-in-memory-diff.md) | 1h | 8.13 | P8 | HashSet diff + single transaction (was: 1M disk stats) |
| 8.19 | [Stats single scan + SSE refresh](./wave-8-19-stats-single-scan-sse-refresh.md) | 1.5h | — | P7, R4 | 4 scans → 2; frontend driven by SSE, not 5s polling |
| 8.20 | [Feature-gate dev tools](./wave-8-20-feature-gate-dev-tools.md) | 1h | — | K3 | `dev-tools` feature for tokio-console + pprof; clean release builds |
| 8.21 | [Backend KISS & dead-code sweep](./wave-8-21-backend-kiss-dead-code-sweep.md) | 1.5h | 8.13 | K2, K4, K5, K6 | `Mutex<IndexWriter>`, drop `unsafe impl`s, fix in-memory pool, dead code |
| 8.22 | [Backend minor simplifications](./wave-8-22-backend-minor-simplifications.md) | 1h | 8.4, 8.12, 8.16 | K7 | `params_from_iter` SQL builder, single config write, explicit tokio features |
| 8.23 | [Frontend DRY cleanup](./wave-8-23-frontend-dry-cleanup.md) | 1.5h | 8.10, 8.19 | D8, K6, K7 | Shared `formatFileSize`, `useEscape`, typed `put<T>()`, dead `onmessage` |
| 8.24 | [Quiet release: logging + runtime](./wave-8-24-quiet-release-logging-runtime.md) | 30m | 8.20 | R6, R7 | One log line per request; default worker threads (no hard-coded 4) |
| 8.25 | [Watcher batch config load](./wave-8-25-watcher-batch-config-load.md) | 30m | 8.4 | R8 | One watched-folder load per event batch (was: per deletion event) |
| 8.26 | [Restrict CORS origins](./wave-8-26-restrict-cors-origins.md) | 30m | 8.20 | R9 | Allowlist `localhost:5173` via env (was: `CorsLayer::permissive()`) |
| 8.27 | [Thumbnail decode memory](./wave-8-27-thumbnail-decode-memory.md) | 45m | 8.3 | R10 | Single-pass downscale via `ImageReader` + `thumbnail()` |
| 8.28 | [Wave 8 verification & release](./wave-8-28-wave8-verification.md) | 45m | 8.1–8.27 | all | Full checklist, finding audit, docs, v0.8.0 |
| 8.29 | [Serialize index runs vs config updates](./wave-8-29-concurrent-index-folder-resurrection.md) | 1h | 8.4 | W-2 | Folder resurrection race: index runs read the table, runs serialized |
| 8.30 | [Count-cache hygiene](./wave-8-30-count-cache-hygiene.md) | 1h | 8.12 | M2, M3 | Validate mime filter (≤ 100 chars); failed COUNT is not cached |

---

## Estimates Summary

| Wave | Name | Tickets | Est. Range |
|------|------|---------|------------|
| 0 | Scaffolding & CI | 9 | 4–6h |
| 1 | Backend: Scanner + Metadata | 10 | 10–14h |
| 2 | Backend: Thumbnails + Serving | 8 | 8–10h |
| 3 | Backend: Search + SSE | 9 | 10–14h |
| 4 | Frontend: Core + Infinite Scroll | 10 | 12–16h |
| 5 | Frontend: Search + Detail + Drag | 9 | 12–16h |
| 6 | Frontend: SSE + Config + Polish | 11 | 10–14h |
| 7 | Production Hardening | 12 | 12–14h |
| 8 | Post-Audit: Performance, Resources & Hygiene | 30 | 32–36h |
| **Total** | | **108** | **110–140h** |

---

## How to Use These Tickets

1. **Pick a wave** — Waves are sequential. Backend (0–3) and Frontend (4–6) can run in parallel once API contract is defined.
2. **Follow dependencies** — Each ticket lists its prerequisite tickets.
3. **TDD workflow** — Write failing test → implement → refactor → edge cases → verify.
4. **Pass/Fail criteria** — Each ticket has explicit, binary acceptance criteria. All must pass before the ticket is marked complete.
5. **Context files** — Referenced in each ticket. Load `.opencode/context/core/standards/code-quality.md` and `test-coverage.md` before starting any implementation.
