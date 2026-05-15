# ImageViz — Development Plan

> **Version**: 1.0  
> **Date**: 2026-05-15  
> **Status**: Draft  
> **Author**: AI-assisted planning

---

## 1. Project Overview

**ImageViz** is a browser-based image and video visualization tool for large datasets (100K–1M media files). It provides:

- Real-time thumbnail grid with infinite scroll (sorted by date, newest first)
- Configuration-based folder watching (point at folders → auto-index)
- Full-text metadata search with live filtering
- Click-to-preview with a larger viewer
- Drag-and-drop to external applications (OS-level drag)
- Live refresh on file system changes

**Primary dataset**: ComfyUI output (`~/ComfyUI/output/`) — ~14K PNG images with generation metadata embedded in PNG `tEXt` chunks, plus WEBM/MP4 videos, organized in `YYYY-MM-DD/` date-folders.

### Architecture

```
┌─────────────────────────────────────────────────────┐
│                    Browser (React)                    │
│  ┌──────────┐  ┌──────────┐  ┌────────────────────┐ │
│  │ Thumbnail │  │  Search  │  │  Detail Viewer     │ │
│  │ Grid      │  │  Bar     │  │  (image/video)     │ │
│  │ (virtual) │  │          │  │                    │ │
│  └─────┬─────┘  └────┬─────┘  └────────┬───────────┘ │
│        │              │                 │             │
│  ┌─────┴──────────────┴─────────────────┴───────────┐ │
│  │           TanStack Query + Jotai                  │ │
│  │      (data fetching + state management)           │ │
│  └────────────────────────┬─────────────────────────┘ │
└───────────────────────────┼───────────────────────────┘
                            │ HTTP REST + SSE
┌───────────────────────────┼───────────────────────────┐
│                    Rust Backend (Axum)                 │
│  ┌────────────────────────┴─────────────────────────┐ │
│  │              API Layer (Axum routes)              │ │
│  │  /media  /search  /config  /events(SSE)  /files  │ │
│  └───┬──────────────────┬──────────────────┬────────┘ │
│      │                  │                  │          │
│  ┌───┴──────┐  ┌────────┴──────┐  ┌──────┴────────┐  │
│  │ Indexer  │  │  Tantivy      │  │  Thumbnail    │  │
│  │ (notify) │  │  (full-text)  │  │  Generator    │  │
│  └───┬──────┘  └────────┬──────┘  └──────┬────────┘  │
│      │                  │                 │           │
│  ┌───┴──────────────────┴─────────────────┴────────┐  │
│  │              SQLite (metadata store)             │  │
│  └─────────────────────────────────────────────────┘  │
│      │                  │                             │
│  ┌───┴──────────────────┴─────────────────────────┐   │
│  │              File System Watcher                │   │
│  │         (notify crate + debouncer)              │   │
│  └────────────────────────────────────────────────┘   │
└───────────────────────────────────────────────────────┘
```

---

## 2. Technology Stack

### Backend (Rust)

| Component | Choice | Version | Rationale |
|-----------|--------|---------|-----------|
| **Web framework** | **Axum** | 0.8.9 | Tokio-native, Tower middleware, built-in SSE/WebSocket, macro-free fast compiles |
| **Async runtime** | **Tokio** | 1.49 | Standard Rust async; all dependencies (notify, tantivy) integrate natively |
| **Metadata store** | **SQLite** (rusqlite) | 0.36 | SQL filtering, secondary indexes, WAL mode for concurrency, zero ops overhead |
| **Full-text search** | **Tantivy** | 0.26 | Lucene-like inverted index; fuzzy/regex queries; facet aggregation; 10x faster than SQL LIKE |
| **File watching** | **notify** + notify-debouncer | 8.2 | Cross-platform (inotify/FSEvents/ReadDirectoryChangesW), debounced for batch events |
| **Image processing** | **image** | 0.25 | `thumbnail()` for fast previews, `Lanczos3` for quality, WebP encoding for thumbnails |
| **Video thumbnails** | **ffmpeg** (sidecar subprocess) | system | Extract keyframe via `ffmpeg -ss 00:00:01 -vframes 1`; only Rust crate is ffmpeg-next (complex) |
| **PNG metadata** | **png** crate + **serde_json** | — | Read `tEXt`/`iTXt` chunks; parse ComfyUI JSON workflow/parameters |
| **Serialization** | **serde** + **serde_json** | 1.x | Request/response (de)serialization |
| **CORS** | **tower-http** | 0.6 | CORS, compression, trace layers |
| **Logging** | **tracing** + **tracing-subscriber** | 0.1 | Structured async-aware logging |
| **Testing** | **cargo test** + **reqwest** (integration) | — | Built-in test runner + HTTP client for integration tests |

### Frontend (TypeScript + React)

| Component | Choice | Version | Rationale |
|-----------|--------|---------|-----------|
| **Build tool** | **Vite** | 8.0 | Fast HMR, native Vitest integration, CRA is deprecated |
| **Virtual scroll** | **react-virtuoso** | 4.18 | Auto variable-sized items, masonry grid, 100K+ items proven |
| **Drag & drop (external)** | **react-dnd** | 16.0 | Only library supporting HTML5 native drag to OS/external apps |
| **Drag & drop (internal)** | **dnd-kit** | 6.3 | Accessibility-first, sortable grid (if internal sorting needed) |
| **Data fetching** | **TanStack Query** | 5.100 | `useInfiniteQuery` for cursor pagination, `maxPages` for memory control |
| **State management** | **Jotai** | 2.20 | Atomic updates, `onMount` for WebSocket lifecycle, minimal boilerplate |
| **Styling** | **Tailwind CSS** | 4.3 | Zero-runtime, microsecond incremental builds in v4, utility-first minimalistic |
| **Type checking** | **TypeScript** | 5.x | Strict mode, type-safe API contracts |
| **Testing** | **Vitest** + **Testing Library** | latest | Vite-native, fast, React Testing Library for component tests |
| **Linting** | **ESLint** + **Prettier** | latest | Code quality, consistent formatting |

### Deviations / Explicit Non-Choices

- **Not using TanStack Virtual directly** — react-virtuoso handles variable-height masonry out of the box
- **Not using RocksDB or sled** — sled is unmaintained, RocksDB is overkill for metadata; SQLite with WAL is perfect
- **Not using Actix-web** — its custom actor runtime adds complexity; Axum's tokio-native approach composes with notify/tantivy/tokio::sync without bridging
- **Not using Next.js** — no SSR needed; pure client-side SPA with Vite is simpler and faster for this use case
- **Not using GraphQL** — REST is simpler for this resource model; SSE covers real-time needs

---

## 3. API Contract

### 3.1 Base URL

```
http://localhost:3001/api/v1
```

### 3.2 Endpoints

#### Media Items

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/media` | List media items (cursor-based, infinite scroll) |
| `GET` | `/media/:id` | Get single media item with full metadata |
| `GET` | `/media/:id/thumbnail` | Get thumbnail image (WebP, cached) |
| `GET` | `/media/:id/file` | Get original file (for preview/drag) |
| `GET` | `/media/:id/metadata` | Get structured metadata for an item |

#### Search

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/search?q=<query>&cursor=<cursor>&limit=<n>` | Full-text search with cursor pagination |

#### Configuration

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/config` | Get current configuration (watched folders) |
| `PUT` | `/config` | Update watched folders (triggers re-index) |
| `GET` | `/stats` | Index statistics (total files, last indexed, etc.) |

#### Real-time

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/events` | SSE stream of file-system events (new/changed/deleted) |

### 3.3 Data Models

#### MediaItem (list view — lightweight)

```json
{
  "id": "uuid-v4",
  "filename": "ComfyUI_23767_.png",
  "path": "2025-08-05/ComfyUI_23767_.png",
  "mime_type": "image/png",
  "thumbnail_url": "/api/v1/media/uuid/thumbnail",
  "width": 896,
  "height": 1216,
  "file_size": 245760,
  "created_at": "2025-08-05T14:32:00Z",
  "modified_at": "2025-08-05T14:32:00Z"
}
```

#### MediaItem (detail view — full)

```json
{
  "id": "uuid-v4",
  "filename": "ComfyUI_23767_.png",
  "path": "2025-08-05/ComfyUI_23767_.png",
  "mime_type": "image/png",
  "thumbnail_url": "/api/v1/media/uuid/thumbnail",
  "file_url": "/api/v1/media/uuid/file",
  "width": 896,
  "height": 1216,
  "file_size": 245760,
  "created_at": "2025-08-05T14:32:00Z",
  "modified_at": "2025-08-05T14:32:00Z",
  "metadata": {
    "prompt": { "3": { "inputs": { "seed": 1025918819518817, ... } } },
    "workflow": { "nodes": [ ... ] }
  }
}
```

#### Cursor Pagination Response

```json
{
  "data": [ /* MediaItem[] */ ],
  "meta": {
    "next_cursor": "2025-08-05T14:32:00Z",
    "next_cursor_id": "uuid-of-last-item",
    "has_more": true,
    "total": 14433
  }
}
```

#### Search Response (same structure)

```json
{
  "data": [ /* MediaItem[] */ ],
  "meta": {
    "next_cursor": null,
    "next_cursor_id": null,
    "has_more": false,
    "total": 42,
    "query": "seed:1025918819518817"
  }
}
```

#### SSE Event Format

```
event: file_created
data: {"id":"uuid","filename":"ComfyUI_99999_.png","path":"2026-05-15/ComfyUI_99999_.png","mime_type":"image/png","thumbnail_url":"...","width":896,"height":1216}

event: file_deleted
data: {"id":"uuid","path":"2025-08-05/ComfyUI_old.png"}

event: file_modified
data: {"id":"uuid","filename":"ComfyUI_23767_.png","metadata_updated":true}

event: indexing_complete
data: {"total":14433,"duration_ms":2340}
```

### 3.4 Query Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `cursor` | ISO 8601 | none | Pagination cursor (date of last item) |
| `cursor_id` | UUID | none | Secondary cursor for tie-breaking (same date) |
| `limit` | int | 100 | Items per page (max 500) |
| `q` | string | none | Search query for full-text search |
| `mime_type` | string | none | Filter by mime type (`image/*`, `video/*`) |

---

## 4. Database Schema (SQLite)

```sql
-- Media items table
CREATE TABLE media_items (
    id TEXT PRIMARY KEY NOT NULL,          -- UUID v4
    filename TEXT NOT NULL,                -- Original filename
    relative_path TEXT NOT NULL UNIQUE,    -- Relative path from watched root
    mime_type TEXT NOT NULL,               -- e.g. "image/png", "video/webm"
    width INTEGER,                         -- Image/video width in pixels
    height INTEGER,                        -- Image/video height in pixels
    file_size INTEGER NOT NULL,            -- File size in bytes
    thumbnail_path TEXT,                   -- Path to generated thumbnail (NULL if not yet generated)
    file_created_at TEXT NOT NULL,         -- ISO 8601 (file system creation time)
    file_modified_at TEXT NOT NULL,        -- ISO 8601 (file system modification time)
    indexed_at TEXT NOT NULL DEFAULT (datetime('now')), -- When this record was indexed
    metadata_json TEXT,                    -- Raw metadata JSON blob (for API responses)
    checksum TEXT                          -- SHA-256 of file (for change detection)
);

-- Index for cursor-based pagination (sorted by date DESC, then id)
CREATE INDEX idx_media_sort ON media_items(file_created_at DESC, id);

-- Index for path lookups
CREATE INDEX idx_media_path ON media_items(relative_path);

-- Index for mime type filtering
CREATE INDEX idx_media_mime ON media_items(mime_type);

-- Application configuration
CREATE TABLE config (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
```

### Tantivy Index Schema

```rust
// Schema fields for full-text search
schema_builder.add_text_field("id", STRING | STORED);
schema_builder.add_text_field("filename", STRING | STORED);
schema_builder.add_text_field("mime_type", STRING);
schema_builder.add_text_field("metadata_json", TEXT);  // Indexed for full-text search
schema_builder.add_date_field("created_at", INDEXED);
schema_builder.add_u64_field("file_size", INDEXED);
schema_builder.add_u64_field("width", STORED);
schema_builder.add_u64_field("height", STORED);
```

---

## 5. Development Waves & Task Breakdown

### Wave 0 — Project Scaffolding & CI

**Goal**: Monorepo setup, build tooling, CI pipeline, health-check endpoints.

**Estimated**: 4–6 hours | **Depends on**: nothing

| # | Task | Files | Est. | Deps | Verification |
|---|------|-------|------|------|-------------|
| 0.1 | Initialize monorepo structure | `imageviz/`, `backend/`, `frontend/`, `.gitignore`, `README.md` | 30m | — | Directory structure exists |
| 0.2 | Scaffold Rust backend with Axum hello-world | `backend/Cargo.toml`, `backend/src/main.rs` | 45m | 0.1 | `cargo run` responds on :3001 |
| 0.3 | Scaffold React frontend with Vite + TypeScript + Tailwind | `frontend/` (Vite template), `tailwind.config.ts`, `frontend/src/App.tsx` | 45m | 0.1 | `npm run dev` shows Tailwind-styled page |
| 0.4 | Add health-check endpoints | `backend/src/routes/health.rs`, `frontend/src/hooks/use-health.ts` | 30m | 0.2, 0.3 | `GET /api/v1/health` returns `{"status":"ok"}` |
| 0.5 | Set up Vite proxy to backend | `frontend/vite.config.ts` | 15m | 0.4 | Frontend can call `/api/v1/health` without CORS issues |
| 0.6 | Configure ESLint + Prettier + Rustfmt + Clippy | `.eslintrc.cjs`, `.prettierrc`, `backend/rustfmt.toml` | 30m | 0.2, 0.3 | Lint passes on both projects |
| 0.7 | Write backend health endpoint unit tests | `backend/tests/health_test.rs` | 30m | 0.4 | `cargo test` passes |
| 0.8 | Write frontend App smoke test | `frontend/src/App.test.tsx` | 30m | 0.3 | `npm test` passes |
| 0.9 | Add CI configuration (GitHub Actions) | `.github/workflows/ci.yml` | 45m | 0.6, 0.7, 0.8 | CI runs lint + test on push |

---

### Wave 1 — Backend: File System Scanner & Metadata Extraction

**Goal**: Scan configured folders, extract metadata from PNG chunks and video files, store in SQLite.

**Estimated**: 10–14 hours | **Depends on**: Wave 0

| # | Task | Files | Est. | Deps | Verification |
|---|------|-------|------|------|-------------|
| 1.1 | Design and implement SQLite schema + migrations | `backend/src/db/mod.rs`, `backend/src/db/schema.rs`, `backend/src/db/migrations.rs` | 1.5h | 0.2 | Unit test: can create tables and query |
| 1.2 | Implement configuration management (watched folders) | `backend/src/config/mod.rs`, `backend/src/routes/config.rs` | 1.5h | 1.1 | `PUT /config` stores folders; `GET /config` returns them |
| 1.3 | Implement file system scanner (walk directory tree) | `backend/src/scanner/mod.rs`, `backend/src/scanner/walker.rs` | 2h | 1.2 | Scan 14K-folder in < 5s, returns file list |
| 1.4 | Implement PNG metadata extraction (tEXt/iTXt chunks) | `backend/src/metadata/png.rs` | 2h | — | Parse ComfyUI prompt+workflow from sample PNGs |
| 1.5 | Implement video metadata extraction (ffmpeg sidecar) | `backend/src/metadata/video.rs` | 2h | — | Extract dimensions, duration from WEBM/MP4 |
| 1.6 | Implement file type detection (MIME + dimensions) | `backend/src/metadata/detect.rs` | 1.5h | 1.4, 1.5 | Returns MIME, width, height, size for any media file |
| 1.7 | Implement file hash computation (change detection) | `backend/src/scanner/hasher.rs` | 1h | — | SHA-256 computed efficiently (streaming, 4MB buffer) |
| 1.8 | Implement indexer orchestration (scan → extract → store) | `backend/src/indexer/mod.rs` | 2.5h | 1.1, 1.3, 1.6, 1.7 | End-to-end: scan folder → SQLite has all entries |
| 1.9 | Add indexer progress reporting | `backend/src/indexer/progress.rs` | 1h | 1.8 | SSE or polling shows indexing progress |
| 1.10 | Write integration test: index sample dataset | `backend/tests/indexer_test.rs` | 1.5h | 1.8 | Index 100 real ComfyUI files, verify all metadata extracted |

---

### Wave 2 — Backend: Thumbnail Generation & Media Serving

**Goal**: Generate WebP thumbnails on demand, serve media files and thumbnails efficiently.

**Estimated**: 8–10 hours | **Depends on**: Wave 1

| # | Task | Files | Est. | Deps | Verification |
|---|------|-------|------|------|-------------|
| 2.1 | Implement thumbnail generator (image crate) | `backend/src/thumbnails/mod.rs`, `backend/src/thumbnails/image.rs` | 2.5h | — | Generate 200px WebP thumbnails from PNG/JPG |
| 2.2 | Implement video thumbnail extraction (ffmpeg keyframe) | `backend/src/thumbnails/video.rs` | 2h | — | Extract frame at 1s as thumbnail PNG → WebP |
| 2.3 | Implement thumbnail cache (on-disk, content-addressed) | `backend/src/thumbnails/cache.rs` | 1.5h | 2.1, 2.2 | Second request for same thumbnail returns instantly |
| 2.4 | Implement thumbnail serving endpoint | `backend/src/routes/media.rs` (thumbnail handler) | 1h | 2.3 | `GET /media/:id/thumbnail` returns WebP image |
| 2.5 | Implement original file serving endpoint | `backend/src/routes/media.rs` (file handler) | 1h | — | `GET /media/:id/file` streams original file with Content-Type |
| 2.6 | Add caching headers (ETag, Cache-Control, Last-Modified) | `backend/src/routes/media.rs` (middleware) | 45m | 2.4, 2.5 | Response includes caching headers; 304 on re-request |
| 2.7 | Add Range request support for video seeking | `backend/src/routes/media.rs` (range handler) | 1.5h | 2.5 | Video previews can seek; Accept-Ranges header present |
| 2.8 | Write integration tests for media endpoints | `backend/tests/media_test.rs` | 1.5h | 2.4–2.7 | All media endpoints tested with real files |

---

### Wave 3 — Backend: Search, Cursor Pagination & Real-time SSE

**Goal**: Full-text search via Tantivy, cursor-based pagination, SSE real-time updates.

**Estimated**: 10–14 hours | **Depends on**: Wave 2

| # | Task | Files | Est. | Deps | Verification |
|---|------|-------|------|------|-------------|
| 3.1 | Set up Tantivy index schema and writer | `backend/src/search/mod.rs`, `backend/src/search/schema.rs` | 2h | — | Create index, write documents, verify they exist |
| 3.2 | Implement Tantivy index population (from SQLite) | `backend/src/search/indexer.rs` | 1.5h | 1.8, 3.1 | All media items indexed; re-index is incremental |
| 3.3 | Implement full-text search endpoint | `backend/src/routes/search.rs` | 2h | 3.2 | `GET /search?q=seed:12345` returns matching items |
| 3.4 | Implement cursor-based pagination for media list | `backend/src/routes/media.rs` (list handler) | 2h | 3.2 | `GET /media?cursor=...&limit=100` returns next page |
| 3.5 | Implement file system watcher (notify + debouncer) | `backend/src/watcher/mod.rs` | 2.5h | — | Adding a file to watched folder triggers event within 500ms |
| 3.6 | Wire watcher events → indexer → broadcast channel | `backend/src/watcher/handler.rs` | 1.5h | 3.5, 1.8 | File events update SQLite + Tantivy + broadcast to SSE |
| 3.7 | Implement SSE endpoint for real-time updates | `backend/src/routes/events.rs` | 2h | 3.6 | `GET /events` streams file events; reconnect works |
| 3.8 | Add stats endpoint | `backend/src/routes/stats.rs` | 45m | 1.8 | `GET /stats` returns total/histogram/indexing status |
| 3.9 | Write integration tests for search + SSE | `backend/tests/search_test.rs`, `backend/tests/events_test.rs` | 2h | 3.3, 3.7 | Search returns expected results; SSE receives events |

---

### Wave 4 — Frontend: Core Layout & Infinite Scroll

**Goal**: Project layout, virtualized thumbnail grid, infinite scroll, TanStack Query integration.

**Estimated**: 12–16 hours | **Depends on**: Wave 3 (API available)

| # | Task | Files | Est. | Deps | Verification |
|---|------|-------|------|------|-------------|
| 4.1 | Define TypeScript API types from contract | `frontend/src/types/media.ts`, `frontend/src/types/api.ts` | 1h | — | Types match API contract exactly |
| 4.2 | Implement API client layer | `frontend/src/api/client.ts`, `frontend/src/api/media.ts`, `frontend/src/api/search.ts` | 1.5h | 4.1 | Can fetch from all backend endpoints |
| 4.3 | Implement `useInfiniteMedia` hook (TanStack Query) | `frontend/src/hooks/use-infinite-media.ts` | 2h | 4.2 | Hook returns pages, `fetchNextPage`, loading states |
| 4.4 | Implement `useSearch` hook | `frontend/src/hooks/use-search.ts` | 1.5h | 4.2 | Hook accepts query, returns results with pagination |
| 4.5 | Build application shell layout (header, main, config panel) | `frontend/src/components/layout/app-shell.tsx`, `frontend/src/components/layout/header.tsx` | 2h | — | Minimalistic layout: header bar + main content area |
| 4.6 | Build thumbnail card component | `frontend/src/components/media/thumbnail-card.tsx` | 2h | 4.1 | Renders thumbnail image, filename, dimensions; loading skeleton |
| 4.7 | Build virtualized thumbnail grid (react-virtuoso) | `frontend/src/components/media/thumbnail-grid.tsx` | 3h | 4.3, 4.6 | Grid scrolls infinitely, renders 100K+ items smoothly |
| 4.8 | Implement responsive masonry layout | `frontend/src/components/media/thumbnail-grid.tsx` (layout logic) | 2h | 4.7 | Grid adapts to window width; items fill columns evenly |
| 4.9 | Add scroll position restoration | `frontend/src/hooks/use-scroll-restore.ts` | 1h | 4.7 | Returning from detail view restores scroll position |
| 4.10 | Write component tests (grid, card, hooks) | `frontend/src/components/media/__tests__/`, `frontend/src/hooks/__tests__/` | 2h | 4.3–4.9 | Vitest tests pass; grid renders multiple pages |

---

### Wave 5 — Frontend: Search, Detail View & Drag-and-Drop

**Goal**: Search bar with live filtering, image/video detail viewer, drag-and-drop to OS.

**Estimated**: 12–16 hours | **Depends on**: Wave 4

| # | Task | Files | Est. | Deps | Verification |
|---|------|-------|------|------|-------------|
| 5.1 | Build search bar component with debounced input | `frontend/src/components/search/search-bar.tsx` | 1.5h | 4.4 | Typing filters grid in real-time with debounce (300ms) |
| 5.2 | Implement search → grid wiring (Jotai atoms) | `frontend/src/store/search-atoms.ts`, `frontend/src/store/media-atoms.ts` | 1.5h | 5.1, 4.7 | Search query updates grid results |
| 5.3 | Build detail viewer (image mode — zoom/pan) | `frontend/src/components/viewer/image-viewer.tsx` | 3h | 4.1 | Click thumbnail → full-size image with zoom/pan |
| 5.4 | Build detail viewer (video mode — playback) | `frontend/src/components/viewer/video-viewer.tsx` | 2.5h | 4.1 | Video plays with controls; Range request seeking works |
| 5.5 | Build metadata panel (JSON tree view) | `frontend/src/components/viewer/metadata-panel.tsx` | 2h | 5.3 | Side panel shows parsed metadata with collapsible JSON |
| 5.6 | Build detail view shell (modal with navigation) | `frontend/src/components/viewer/detail-view.tsx` | 2h | 5.3, 5.4, 5.5 | Modal opens on click; ← → navigates between items; Esc closes |
| 5.7 | Implement external drag-and-drop (react-dnd) | `frontend/src/components/media/drag-source.tsx` | 2h | 4.6 | Drag thumbnail → drop in file explorer or external app |
| 5.8 | Implement keyboard navigation in grid | `frontend/src/hooks/use-keyboard-nav.ts` | 1.5h | 4.7 | Arrow keys navigate grid; Enter opens detail; Space selects |
| 5.9 | Write integration tests (search + detail + drag) | `frontend/src/components/__tests__/integration/` | 2.5h | 5.1–5.8 | User journeys tested: search → click → view → close |

---

### Wave 6 — Frontend: Real-time SSE, Config UI & Polish

**Goal**: SSE real-time updates, configuration panel, empty states, loading states, accessibility.

**Estimated**: 10–14 hours | **Depends on**: Wave 5

| # | Task | Files | Est. | Deps | Verification |
|---|------|-------|------|------|-------------|
| 6.1 | Implement SSE connection hook | `frontend/src/hooks/use-sse.ts` | 2h | 4.2 | Connects to `/events`, handles reconnect with backoff |
| 6.2 | Implement real-time grid updates (Jotai + SSE) | `frontend/src/store/sse-atoms.ts` | 2h | 6.1, 4.7 | New files appear in grid without refresh; deleted files removed |
| 6.3 | Build configuration panel (folder picker + list) | `frontend/src/components/config/config-panel.tsx` | 2.5h | 4.2 | Add/remove watched folders; shows indexing status |
| 6.4 | Implement folder path suggestion (server-side) | `backend/src/routes/config.rs` (suggest endpoint) | 1h | 1.2 | `GET /config/suggest?path=~/` returns subdirectories |
| 6.5 | Build empty state component | `frontend/src/components/shared/empty-state.tsx` | 45m | — | Shows when no folders configured or no files match search |
| 6.6 | Build error boundary and error states | `frontend/src/components/shared/error-boundary.tsx`, `frontend/src/components/shared/error-state.tsx` | 1.5h | — | API errors show retry button; unhandled errors caught |
| 6.7 | Add loading skeletons (grid, detail, config) | `frontend/src/components/media/skeleton-grid.tsx` | 1h | — | Consistent skeleton loading across all views |
| 6.8 | Add keyboard shortcuts panel | `frontend/src/components/shared/shortcuts-panel.tsx` | 1h | 5.8 | `?` key shows shortcuts overlay |
| 6.9 | Accessibility audit and fixes (ARIA, focus, contrast) | Multiple files | 2h | — | Keyboard navigable, screen reader friendly, WCAG AA |
| 6.10 | Performance profiling and optimization | Multiple files | 2h | — | Grid scrolls at 60fps with 10K visible items; search < 200ms |
| 6.11 | Write E2E tests (Playwright or Cypress) | `frontend/e2e/` | 3h | 6.1–6.10 | Full user journeys tested end-to-end |

---

### Wave 7 — Production Readiness & Hardening

**Goal**: Error handling edge cases, security, deployment config, documentation.

**Estimated**: 8–10 hours | **Depends on**: Wave 6

| # | Task | Files | Est. | Deps | Verification |
|---|------|-------|------|------|-------------|
| 7.1 | Implement graceful shutdown (in-flight requests) | `backend/src/main.rs` (shutdown signal) | 1h | — | `Ctrl+C` drains active requests before exiting |
| 7.2 | Add request timeout middleware | `backend/src/middleware/timeout.rs` | 30m | — | Long-running requests return 408 |
| 7.3 | Add concurrency limiting for thumbnail generation | `backend/src/thumbnails/limiter.rs` | 1h | 2.3 | Max N concurrent thumbnail generations |
| 7.4 | Implement disk space monitoring for thumbnail cache | `backend/src/thumbnails/cache.rs` (eviction) | 1.5h | 2.3 | LRU eviction when cache exceeds limit |
| 7.5 | Add security headers (Content-Security-Policy, etc.) | `backend/src/middleware/security.rs` | 45m | — | Helmet-like headers on all responses |
| 7.6 | Add input validation and sanitization for all endpoints | Multiple backend route files | 1.5h | — | Invalid inputs return 400 with descriptive errors |
| 7.7 | Add structured logging (request ID, duration, status) | `backend/src/middleware/logging.rs` | 1h | — | Each request logged with trace ID + duration |
| 7.8 | Create production build scripts (backend release, frontend bundle) | `scripts/build.sh`, `scripts/dev.sh` | 1h | — | Single command to build both; single command to run dev |
| 7.9 | Add graceful degradation for missing thumbnails | `frontend/src/components/media/thumbnail-card.tsx` | 30m | — | Broken thumbnail shows placeholder, not error |
| 7.10 | Write project README | `README.md` | 2h | — | Setup, run, configure, contribute, architecture overview |

---

## 6. Dependency Graph

```
Wave 0 (Scaffolding)
  │
  ├── Wave 1 (Backend: Scanner + Metadata)
  │     │
  │     ├── Wave 2 (Backend: Thumbnails + Serving)
  │     │     │
  │     │     └── Wave 3 (Backend: Search + Pagination + SSE)
  │     │           │
  │     │           └── Wave 4 (Frontend: Core UI + Infinite Scroll)
  │     │                 │
  │     │                 └── Wave 5 (Frontend: Search + Detail + Drag)
  │     │                       │
  │     │                       └── Wave 6 (Frontend: SSE + Config + Polish)
  │     │                             │
  │     │                             └── Wave 7 (Production Hardening)
  │     │
  │     Note: Waves 1-3 are backend-only. Frontend work (Wave 4) 
  │     can begin as soon as Wave 3 API is available (even if not 100% complete).
  │     Some Wave 4 tasks (e.g., 4.1 types, 4.2 API client) can start 
  │     in parallel with Wave 3 since they only depend on the API contract.
```

### Parallel Opportunities

- **Within Wave 1**: Tasks 1.4 (PNG metadata), 1.5 (video metadata), and 1.7 (hashing) can run in parallel (all independent)
- **Within Wave 2**: Tasks 2.1 (image thumbnails) and 2.2 (video thumbnails) can run in parallel
- **Within Wave 5**: Tasks 5.3 (image viewer) and 5.4 (video viewer) can run in parallel
- **Waves 1–3 (Backend)**: Entirely independent from Waves 4–6 (Frontend) once the API contract is defined
- **Wave 6.9 (A11y)** and **6.10 (Performance)**: Can run in parallel

---

## 7. Testing Strategy (TDD-First)

### 7.1 Testing Pyramid

```
         ╱  E2E  ╲          ~10 tests (Playwright: full user journeys)
        ╱──────────╲
       ╱ Integration ╲       ~40 tests (API integration, component integration)
      ╱────────────────╲
     ╱   Unit Tests      ╲    ~150+ tests (pure functions, hooks, extractors, indexers)
    ╱──────────────────────╲
```

### 7.2 Backend Testing

| Level | Tool | What | Target |
|-------|------|------|--------|
| **Unit** | `cargo test` | Pure functions (metadata parsing, hash computation, path handling) | 100% coverage of critical paths |
| **Integration** | `cargo test` + `reqwest` (test HTTP client) | API endpoints with real SQLite, real file system | All endpoints tested |
| **Property** | `proptest` crate (optional) | Fuzz metadata parsing with random valid/invalid inputs | Robustness |

#### Backend Test Structure

```
backend/
├── src/
│   ├── metadata/
│   │   ├── png.rs
│   │   └── png_test.rs       # ← Co-located tests
│   ├── scanner/
│   │   ├── hasher.rs
│   │   └── hasher_test.rs
│   └── ...
└── tests/
    ├── common/mod.rs          # Test helpers (temp dirs, DB fixtures)
    ├── health_test.rs
    ├── indexer_test.rs
    ├── media_test.rs
    ├── search_test.rs
    └── events_test.rs
```

### 7.3 Frontend Testing

| Level | Tool | What | Target |
|-------|------|------|--------|
| **Unit** | Vitest | Hooks (`useInfiniteMedia`, `useSearch`), pure utility functions | 100% coverage |
| **Component** | Vitest + Testing Library | Thumbnail card, search bar, detail viewer (isolated) | 90%+ coverage |
| **Integration** | Vitest + MSW (Mock Service Worker) | Grid + search + detail interactions | Key user journeys |
| **E2E** | Playwright | Full user journeys (config → scan → scroll → search → view → drag) | Critical paths |

#### Frontend Test Structure

```
frontend/src/
├── components/
│   ├── media/
│   │   ├── thumbnail-card.tsx
│   │   └── __tests__/
│   │       └── thumbnail-card.test.tsx
│   └── ...
├── hooks/
│   ├── use-infinite-media.ts
│   └── __tests__/
│       └── use-infinite-media.test.ts
└── __tests__/
    └── integration/
        ├── search-flow.test.tsx
        └── detail-view-flow.test.tsx
```

### 7.4 TDD Workflow (Per Task)

1. **Write a failing test** that defines the expected behavior
2. **Implement the minimum code** to make the test pass
3. **Refactor** while keeping tests green
4. **Add edge case tests** (empty inputs, nulls, errors, boundaries)
5. **Verify** with `cargo test` / `npm test` before marking task complete

### 7.5 Test Data Strategy

- **Fixtures**: Small set of real ComfyUI PNGs (3–5 files) committed to `test-fixtures/`
- **Generated**: Programmatically create files with known metadata for edge cases
- **Shared**: `backend/tests/common/mod.rs` and `frontend/src/test-utils/` for test helpers

---

## 8. Performance Targets & Considerations

### 8.1 Performance Budget

| Operation | Target | Measurement |
|-----------|--------|-------------|
| Grid scroll FPS | 60fps sustained | Chrome DevTools Performance |
| Initial page load (first 100 thumbnails) | < 1s | Lighthouse |
| Search response (100K dataset) | < 200ms | Server-side timing |
| Thumbnail generation | < 50ms per image | Server-side timing |
| SSE event latency (file added → UI update) | < 500ms | End-to-end timing |
| Memory usage (frontend, 100K items in grid) | < 500MB | Chrome Task Manager |
| Index scan speed | > 500 files/sec | Server-side timing |

### 8.2 Key Performance Decisions

1. **Cursor-based pagination** (not offset-based): `WHERE (created_at, id) < (?, ?)` is O(log n) with index; `OFFSET 10000` is O(n)
2. **Virtual scrolling** (react-virtuoso): Only ~50 DOM nodes at any time regardless of dataset size
3. **Thumbnail caching**: Content-addressed on-disk cache; no regeneration for unchanged files
4. **Tantivy for search**: Inverted index, not SQL `LIKE '%term%'` — 10-100x faster for full-text
5. **SQLite WAL mode**: Reads don't block writes; concurrent queries OK
6. **Debounced file watcher**: Batch 500ms of FS events into single index cycle
7. **Streaming file serving**: `tokio::fs::File` + `ReaderStream` — never loads full file into memory
8. **Image lazy loading**: `loading="lazy"` on thumbnail `<img>` tags; intersection observer fallback
9. **WebP thumbnails**: 25-35% smaller than JPEG at same quality

### 8.3 Memory Management

**Backend**:
- Thumbnail generation: `spawn_blocking` for CPU-bound image work (avoids blocking async runtime)
- Database connections: `r2d2` connection pool with max 10 connections
- Tantivy: Single writer, multiple readers; commit every N seconds

**Frontend**:
- TanStack Query `maxPages: 10` — only keep last 10 pages (1000 items) in memory
- Jotai atoms with cleanup — remove detail-view data when modal closes
- `useMemo`/`useCallback` on thumbnail components to prevent re-renders
- Image preloading: only preload visible + 1 viewport ahead

---

## 9. Risk Register

| Risk | Severity | Mitigation |
|------|----------|------------|
| Tantivy index grows too large for memory | Medium | Use `RamStorage` for index — only hot segments in RAM; flush to disk |
| react-virtuoso can't handle variable-height items well | Medium | Prototype with 100K items early (Wave 4.7); fallback to TanStack Virtual if needed |
| ComfyUI metadata format changes | Low | JSON parsing is lenient; store raw blob, not parsed fields |
| Very large video files cause memory issues | Medium | Stream via Range requests; never load full video into server memory |
| SQLite write contention under heavy index load | Low | WAL mode + single indexer writer; reads from separate connections |
| Cross-platform file watcher inconsistencies | Medium | Use `notify`'s recommended watcher per platform; test on Linux + macOS |

---

## 10. Open Questions & Decisions Needed

1. **Thumbnail size**: Target 200px? 300px? Depends on design. (Recommend: 200px default, configurable)
	1. Let's go with recommended, but I want to target being able to display 3-4 images per horizontal row on a vertical screen at 1080p. The 200-300px range seems spot-on, but since we're making it configurable, let's allow changes from 100-500 px, horizontal.
2. **Video thumbnail**: Extract from 1s? 10%? First non-black frame? (Recommend: 1s for simplicity, make configurable)
	1. The first 5s, looping, configurable. Animated thumbnail can be toggled off, then in that case we use the frame at 1s.
3. **Authentication**: Needed? This is a local tool. (Recommend: No auth for v1; optional API key for v2)
	1. No Auth, Local first. We can consider auth after we're done with this plan.
4. **Multiple watched folders vs single**: Allow multiple folders? (Recommend: Multiple, with folder labels)
	1. Allow multiple folders. Also track subfolders.
5. **Metadata search syntax**: Free-text only? Or structured queries like `seed:12345`? (Recommend: Start with free-text; add field-specific filters in v2)
	1. Let's start with free text.
6. **Image formats to support**: PNG, JPG, WEBP? Also AVIF, HEIC, TIFF? (Recommend: PNG, JPG, WEBP, GIF for v1)
	1. We'll go with recommendation.
7. **Video formats to support**: MP4, WEBM? Also MOV, AVI, MKV? (Recommend: MP4, WEBM for v1)
	1. MP4 and WEBM are good.
8. **Detail view zoom**: Simple click-to-zoom or full pan/zoom? (Recommend: Click-to-fit + scroll-to-zoom)
	1. Go with recommendation.
9. **Metadata display**: Raw JSON or parsed/structured view? (Recommend: Collapsible JSON tree with syntax highlighting)
	1. Go with recommendation.
10. **Deployment target**: Desktop only? Responsive? (Recommend: Desktop-first responsive; mobile as best-effort)
	1. This is desktop only, but we should optimize the frontend for vertical and horizontal screens, as I will be using it mostly on a vertical screen to display vertical aspect ratio images and videos.

---

## 11. Total Estimates

| Wave | Name | Estimated Hours | Cumulative |
|------|------|-----------------|------------|
| 0 | Scaffolding & CI | 4–6h | 6h |
| 1 | Backend: Scanner + Metadata | 10–14h | 20h |
| 2 | Backend: Thumbnails + Serving | 8–10h | 30h |
| 3 | Backend: Search + SSE | 10–14h | 44h |
| 4 | Frontend: Core + Infinite Scroll | 12–16h | 60h |
| 5 | Frontend: Search + Detail + Drag | 12–16h | 76h |
| 6 | Frontend: SSE + Config + Polish | 10–14h | 90h |
| 7 | Production Hardening | 8–10h | 100h |
| **Total** | | **74–100 hours** | |

> **Note**: Estimates are for a single experienced developer. Parallel work (backend + frontend by different developers) could reduce calendar time by 40-50%.

---

## 12. Project Structure (Target)

```
imageviz/
├── README.md
├── .gitignore
├── .github/
│   └── workflows/
│       └── ci.yml
├── documents/
│   └── plans/
│       └── development-plan.md          # ← This file
├── test-fixtures/
│   ├── sample_comfyui.png               # Real ComfyUI PNG with metadata
│   ├── sample_no_metadata.png           # Clean PNG (no tEXt chunks)
│   ├── sample_video.webm                # Short test video
│   └── sample_video.mp4                 # Short test video
├── backend/
│   ├── Cargo.toml
│   ├── rustfmt.toml
│   ├── .cargo/
│   │   └── config.toml
│   ├── src/
│   │   ├── main.rs                      # Server entry point
│   │   ├── app.rs                       # Router assembly + state
│   │   ├── config/
│   │   │   ├── mod.rs
│   │   │   └── settings.rs             # Env-based configuration
│   │   ├── db/
│   │   │   ├── mod.rs
│   │   │   ├── schema.rs               # SQL table definitions
│   │   │   ├── migrations.rs           # Schema migrations
│   │   │   └── queries.rs              # Prepared query functions
│   │   ├── scanner/
│   │   │   ├── mod.rs
│   │   │   ├── walker.rs               # Directory tree walker
│   │   │   └── hasher.rs               # File hash computation
│   │   ├── indexer/
│   │   │   ├── mod.rs                  # Orchestrator
│   │   │   └── progress.rs             # Progress tracking
│   │   ├── metadata/
│   │   │   ├── mod.rs
│   │   │   ├── png.rs                  # PNG chunk parser
│   │   │   ├── video.rs               # ffmpeg metadata extraction
│   │   │   └── detect.rs              # MIME type + dimensions
│   │   ├── thumbnails/
│   │   │   ├── mod.rs
│   │   │   ├── image.rs               # Image thumbnail generation
│   │   │   ├── video.rs               # Video thumbnail extraction
│   │   │   ├── cache.rs               # On-disk thumbnail cache
│   │   │   └── limiter.rs             # Concurrency limiter
│   │   ├── search/
│   │   │   ├── mod.rs
│   │   │   ├── schema.rs              # Tantivy schema
│   │   │   └── indexer.rs             # Tantivy writer + reader
│   │   ├── watcher/
│   │   │   ├── mod.rs                 # notify watcher setup
│   │   │   └── handler.rs             # Event → indexer → broadcast
│   │   ├── routes/
│   │   │   ├── mod.rs
│   │   │   ├── health.rs
│   │   │   ├── media.rs               # GET /media, /media/:id, /media/:id/thumbnail, /media/:id/file
│   │   │   ├── search.rs              # GET /search
│   │   │   ├── config.rs              # GET/PUT /config
│   │   │   ├── events.rs              # GET /events (SSE)
│   │   │   └── stats.rs               # GET /stats
│   │   └── middleware/
│   │       ├── mod.rs
│   │       ├── logging.rs
│   │       ├── security.rs
│   │       └── timeout.rs
│   └── tests/
│       ├── common/
│       │   └── mod.rs                  # Test helpers
│       ├── health_test.rs
│       ├── indexer_test.rs
│       ├── media_test.rs
│       ├── search_test.rs
│       └── events_test.rs
├── frontend/
│   ├── package.json
│   ├── tsconfig.json
│   ├── vite.config.ts
│   ├── tailwind.config.ts
│   ├── index.html
│   ├── src/
│   │   ├── main.tsx                    # Entry point
│   │   ├── App.tsx                     # Root component
│   │   ├── types/
│   │   │   ├── media.ts               # MediaItem, SearchResult types
│   │   │   └── api.ts                 # API response types
│   │   ├── api/
│   │   │   ├── client.ts              # Fetch wrapper (base URL, error handling)
│   │   │   ├── media.ts               # Media API functions
│   │   │   └── search.ts              # Search API functions
│   │   ├── hooks/
│   │   │   ├── use-infinite-media.ts   # TanStack Query infinite scroll
│   │   │   ├── use-search.ts          # Search with debounce
│   │   │   ├── use-sse.ts             # SSE connection management
│   │   │   ├── use-scroll-restore.ts  # Scroll position memory
│   │   │   └── use-keyboard-nav.ts    # Keyboard navigation
│   │   ├── store/
│   │   │   ├── media-atoms.ts         # Media list state (Jotai)
│   │   │   ├── search-atoms.ts        # Search query + results
│   │   │   ├── sse-atoms.ts           # Real-time event state
│   │   │   └── ui-atoms.ts            # UI state (detail open, config open)
│   │   ├── components/
│   │   │   ├── layout/
│   │   │   │   ├── app-shell.tsx
│   │   │   │   └── header.tsx
│   │   │   ├── media/
│   │   │   │   ├── thumbnail-card.tsx
│   │   │   │   ├── thumbnail-grid.tsx
│   │   │   │   ├── skeleton-grid.tsx
│   │   │   │   └── drag-source.tsx
│   │   │   ├── search/
│   │   │   │   └── search-bar.tsx
│   │   │   ├── viewer/
│   │   │   │   ├── detail-view.tsx
│   │   │   │   ├── image-viewer.tsx
│   │   │   │   ├── video-viewer.tsx
│   │   │   │   └── metadata-panel.tsx
│   │   │   ├── config/
│   │   │   │   └── config-panel.tsx
│   │   │   └── shared/
│   │   │       ├── empty-state.tsx
│   │   │       ├── error-boundary.tsx
│   │   │       ├── error-state.tsx
│   │   │       └── shortcuts-panel.tsx
│   │   └── test-utils/
│   │       ├── msw-handlers.ts        # Mock Service Worker handlers
│   │       └── render-utils.tsx       # Test render with providers
│   ├── __tests__/
│   │   └── integration/
│   │       ├── search-flow.test.tsx
│   │       └── detail-view-flow.test.tsx
│   └── e2e/
│       ├── basic-navigation.spec.ts
│       └── search-and-view.spec.ts
└── scripts/
    ├── dev.sh                          # Start backend + frontend dev servers
    └── build.sh                        # Production build
```

---

## 13. Appendix: Key Library Versions (Lock File Reference)

### Backend (Cargo.toml)

```toml
[package]
name = "imageviz-backend"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace", "compression-gzip", "limit"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rusqlite = { version = "0.36", features = ["bundled"] }
tantivy = "0.26"
notify = { version = "8", features = ["macos_kqueue"] }
notify-debouncer-mini = "0.7"
image = "0.25"
uuid = { version = "1", features = ["v4"] }
sha2 = "0.10"
mime_guess = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tokio-stream = "0.1"
futures-util = "0.3"
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
reqwest = { version = "0.12", features = ["json"] }
tempfile = "3"
```

### Frontend (package.json — key deps)

```json
{
  "dependencies": {
    "react": "^19.0",
    "react-dom": "^19.0",
    "react-virtuoso": "^4.18",
    "react-dnd": "^16.0",
    "react-dnd-html5-backend": "^16.0",
    "@tanstack/react-query": "^5.100",
    "jotai": "^2.20"
  },
  "devDependencies": {
    "@vitejs/plugin-react": "^4.5",
    "vite": "^8.0",
    "typescript": "^5.8",
    "tailwindcss": "^4.3",
    "@tailwindcss/vite": "^4.3",
    "vitest": "^3.1",
    "@testing-library/react": "^16.3",
    "@testing-library/jest-dom": "^6.6",
    "@testing-library/user-event": "^14.6",
    "msw": "^2.10",
    "playwright": "^1.54",
    "eslint": "^9.0",
    "prettier": "^3.5"
  }
}
```
