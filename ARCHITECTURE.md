# ImageViz Architecture

## System Overview

ImageViz is a desktop-first media browser and search application for large datasets (100K–1M media files). It scans local folders for images and videos, extracts metadata (PNG tEXt/iTXt chunks, video via ffmpeg), builds a full-text search index with Tantivy, and serves a responsive React SPA with real-time updates via SSE.

> **Primary use case**: Browsing ComfyUI output (~14K PNG images with generation metadata embedded in PNG tEXt chunks, plus WEBM/MP4 videos) organized in `YYYY-MM-DD/` date-folders.

```
┌─────────────┐     ┌───────────────┐     ┌──────────────┐
│  File System │────▶│  Scanner +    │────▶│   SQLite     │
│  (watched    │     │  File Watcher │     │  (WAL mode)  │
│   folders)   │     │  (notify)     │     │  (r2d2 pool) │
└─────────────┘     └───────┬───────┘     └──────┬───────┘
                            │                     │
                            │                     ▼
                            │             ┌──────────────┐
                            │             │    Tantivy   │
                            └────────────▶│  (full-text  │
                                          │   search)    │
                                          └──────┬───────┘
                                                 │
                    ┌────────────────────────────┼────────────────────────────┐
                    │                            ▼                            │
                    │                    ┌──────────────┐                    │
                    │                    │  Axum HTTP   │                    │
                    │                    │   Server     │                    │
                    │                    │  (Tokio 1)   │                    │
                    │                    └──────┬───────┘                    │
                    │                           │                           │
                    │           ┌───────────────┼───────────────┐           │
                    │           ▼               ▼               ▼           │
                    │    ┌──────────┐   ┌───────────┐   ┌──────────────┐    │
                    │    │  Media   │   │  Search   │   │    SSE       │    │
                    │    │  Serving │   │  + Cursor │   │   Events     │    │
                    │    │ (Range,  │   │ Pagination│   │  (real-time) │    │
                    │    │  ETag)   │   │           │   │              │    │
                    │    └────┬─────┘   └─────┬─────┘   └──────┬───────┘    │
                    │         │               │                │            │
                    └─────────┼───────────────┼────────────────┼────────────┘
                              ▼               ▼                ▼
                    ┌─────────────────────────────────────────────────────┐
                    │              Browser (React 19 SPA)                 │
                    │  ┌──────────┐  ┌──────────┐  ┌──────────────────┐  │
                    │  │ Thumbnail│  │  Search  │  │  Detail Viewer   │  │
                    │  │ Grid     │  │  Bar     │  │  (image/video)   │  │
                    │  │ (virtual)│  │          │  │                  │  │
                    │  └──────────┘  └──────────┘  └──────────────────┘  │
                    │         TanStack Query + Jotai                     │
                    └─────────────────────────────────────────────────────┘
```

## Technology Stack

### Backend

| Component            | Choice                    | Version | Purpose                                                        |
| -------------------- | ------------------------- | ------- | -------------------------------------------------------------- |
| HTTP Framework       | **Axum**                  | 0.8     | REST API — tokio-native, Tower middleware, built-in SSE support |
| Async Runtime        | **Tokio**                 | 1       | Async I/O, task spawning, channels (broadcast, mpsc)            |
| Database             | **SQLite** (rusqlite)     | 0.36    | Media item metadata store, configuration storage               |
| Connection Pool      | **r2d2**                  | 0.8     | Thread-safe connection pool (max 10 connections, WAL-compatible) |
| Full-Text Search     | **Tantivy**               | 0.26    | BM25-ranked search across filenames and metadata JSON           |
| Thumbnails           | **image** crate + **WebP**| 0.25    | On-disk content-addressed thumbnail cache                      |
| Video Thumbnails     | **ffmpeg** (subprocess)   | system  | Keyframe extraction (`-ss 00:00:01 -vframes 1`)                |
| File Watching        | **notify** + debouncer    | 8       | Cross-platform file system change detection (inotify/FSEvents) |
| SSE                  | `tokio::sync::broadcast`  | —       | Fan-out real-time events to all connected clients              |
| PNG Metadata         | **png** crate             | 0.18    | Read tEXt/iTXt chunks; parse ComfyUI JSON workflow             |
| Serialization        | **serde** + serde_json    | 1       | Request/response (de)serialization                             |
| CORS / HTTP layers   | **tower-http**            | 0.6     | CORS allowlist (`middleware/cors.rs`), gzip compression, trace  |
| Logging              | **tracing** + subscriber  | 0.1     | Structured async-aware logging with env-filter                 |
| HTTP Client (tests)  | **reqwest**               | 0.12    | Integration test HTTP client                                   |

### Frontend

| Component           | Choice                    | Version | Purpose                                                  |
| ------------------- | ------------------------- | ------- | -------------------------------------------------------- |
| Framework           | **React**                 | 19      | UI components                                            |
| Build Tool          | **Vite**                  | 8       | Dev server, HMR, production builds                       |
| Type System         | **TypeScript** (strict)   | 5       | Type safety, strict mode                                 |
| State Management    | **Jotai**                 | 2.20    | Atomic state — `onMount` for SSE lifecycle               |
| Server State        | **TanStack Query**        | 5.100   | Caching, cursor pagination (`useInfiniteQuery`), refetch |
| Virtual Scroll      | **react-virtuoso**        | 4.18    | Variable-height masonry grid — 100K+ items               |
| Drag & Drop         | **react-dnd** (HTML5)     | 16      | OS-level drag to external applications                   |
| Styling             | **Tailwind CSS**          | 4       | Utility-first CSS, zero-runtime, microsecond builds      |
| Component Tests     | **Vitest** + Testing Lib  | latest  | Component and integration tests                          |
| Integration Tests   | **MSW** (Mock Service Worker) | 2   | API mocking for frontend tests                           |
| E2E Tests           | **Playwright**            | latest  | Full user journey tests                                  |
| Linting             | **ESLint** + Prettier     | latest  | Code quality, consistent formatting                      |

## Key Architectural Decisions

### Cursor-Based Pagination (not offset)

Uses `WHERE (file_created_at, id) < (?, ?)` with the index `idx_media_sort` instead of `LIMIT/OFFSET`. This is **O(log n)** regardless of page depth vs O(n) for offset-based pagination. The cursor is a tuple of the last item's `file_created_at` timestamp + `id`.

The response includes `meta.next_cursor`, `meta.next_cursor_id`, and `meta.has_more` so the client knows exactly where to resume.

### WAL Mode for SQLite

Write-Ahead Logging allows concurrent reads while a single writer holds the lock. This is critical for the two-phase startup:
- **Phase 1**: SQLite scan populates the database.
- **Phase 2**: A separate dedicated SQLite connection (opened directly via `db::open`, not from the pool; used only for reading) feeds the Tantivy reindex while the API continues serving requests. WAL permits the concurrent reader.

Without this separation, `GET /api/v1/media` would block until the Tantivy index finished rebuilding.

### r2d2 Connection Pool (not Mutex)

Replaced the earlier `Arc<Mutex<Connection>>` pattern with an r2d2 pool. Pool size defaults to 10 connections, each initialised with WAL mode, foreign keys, and a 5-second busy timeout. This eliminates lock contention for read-heavy workloads while keeping writes serialised by SQLite's internal locking.

### Content-Addressed Thumbnail Cache

Cache key is `{sha256_prefix[:16]}_{width}.webp`. Same content always maps to the same cache entry — no invalidation needed. Content change produces a different checksum and a new cache entry. Thumbnails are generated directly into the cache directory (`{key}.tmp` + atomic rename — no `/tmp` staging). Eviction runs solely on a 5-minute background timer (`evict_if_needed`); cache generations never trigger inline scans. Concurrent requests for the same checksum are deduplicated by weak-valued per-checksum `Mutex`es in a `DashMap` (entries evicted when the last holder releases), and the concurrency semaphore is acquired only on cache misses.

### SSE Over WebSocket

Server-Sent Events are simpler for unidirectional server → client streaming. The browser `EventSource` API is built-in (no client library). Reconnection is automatic. Events include `file_created`, `file_deleted`, `file_modified`, and `indexing_complete`.

### Per-Key Mutex for Thumbnail Generation

A `DashMap` of per-key mutexes ensures only the first caller generates a thumbnail; concurrent callers block and then read the cached result. This prevents wasted CPU on duplicate generation when the same thumbnail is requested simultaneously.

### Incremental Startup Indexing

- **Phase 1** (`incremental_index`): scan watched folders → skip files whose size+mtime are unchanged (no re-hash, no ffprobe) → for new/modified files: SHA-256 hash → media detection → metadata extraction → SQLite upsert. Hash/ffprobe work runs with bounded concurrency (`INDEX_CONCURRENCY`, default: CPU cores capped at 8). Deletion cleanup diffs in memory and deletes in a single transaction.
- **Phase 2** (`full_reindex`): read all SQLite rows through a **separate dedicated connection** (opened directly, not from the pool; used only for reading) → rebuild the Tantivy index on a blocking thread.

Both phases run in a background `tokio::spawn` task, so the API is available immediately. The file-watcher event handler activates only after initial indexing completes; events accumulated during the scan are drained first (the full reindex captures those files anyway).

### Streaming File Serving

Uses `tokio::fs::File` + `axum::body::Body::from_stream` — never loads a full file into memory. Range requests are supported for video seeking (`Accept-Ranges: bytes`). ETag + `If-None-Match` enables 304 Not Modified responses.

### Debounced File Watcher

`notify` + `notify-debouncer-mini` with 500ms debounce. File system events (create/modify/delete) are batched into a single index cycle. An in-memory `mpsc` channel decouples the watcher from the event handler.

## Module Dependency Graph

The backend is organised into the following modules, all declared in `lib.rs`:

```
lib.rs                          ─── health_router() — mounts /api/v1/health
├── config/                     ─── AppConfig, Settings from environment
│   ├── mod.rs
│   └── settings.rs             ─── Env-based config (IMAGEVIZ_DB_PATH, etc.)
├── db/                         ─── SQLite connection, pool, schema, migrations
│   ├── mod.rs
│   ├── pool.rs                 ─── r2d2 SqliteConnectionManager
│   ├── schema.rs               ─── Table definitions
│   └── migrations.rs           ─── Schema migrations
├── media_types.rs             ─── File type detection and categorization
├── indexer/                    ─── Full/incremental file scan orchestration
│   ├── mod.rs
│   └── progress.rs             ─── Progress tracking for long scans
├── metadata/                   ─── PNG tEXt/iTXt, video via ffmpeg, MIME detect
│   ├── mod.rs
│   ├── png.rs                  ─── PNG chunk parser (ComfyUI metadata)
│   ├── video.rs                ─── ffmpeg subprocess metadata extraction
│   └── detect.rs               ─── MIME type + dimension detection
├── middleware/                  ─── Axum middleware layers
│   ├── mod.rs
│   ├── cors.rs                 ─── CORS allowlist built from CORS_ALLOW_ORIGINS
│   ├── logging.rs              ─── Request logging (one line per request)
│   ├── security.rs             ─── Security headers (CSP, X-Frame-Options, etc.)
│   ├── timeout.rs              ─── Per-group timeout middleware
│   └── validation.rs           ─── Input validation and sanitization
├── routes/                     ─── HTTP handlers — all under /api/v1
│   ├── mod.rs
│   ├── error.rs                ─── `AppError` enum + `IntoResponse`
│   ├── response.rs             ─── Shared response types (data/meta envelope)
│   ├── health.rs               ─── GET /health
│   ├── media/                  ─── Sub-module route group
│   │   ├── mod.rs
│   │   ├── list.rs             ─── GET /media (cursor-based pagination)
│   │   ├── detail.rs           ─── GET /media/:id and /media/:id/metadata
│   │   ├── file.rs             ─── GET /media/:id/file (streaming + Range)
│   │   ├── thumbnail.rs        ─── GET /media/:id/thumbnail
│   │   └── tests.rs            ─── Media route tests
│   ├── search.rs               ─── GET /search?q=&cursor=&limit=
│   ├── config.rs
│   ├── config/                 ─── Config sub-module
│   │   └── suggest.rs          ─── GET /config/suggest?path=
│   ├── events.rs               ─── GET /events (SSE stream)
│   └── stats.rs                ─── GET /stats
├── scanner/                    ─── Directory walker, file hasher
│   ├── mod.rs
│   ├── walker.rs               ─── Recursive directory walk
│   └── hasher.rs               ─── Streaming SHA-256 computation
├── search/                     ─── Tantivy IndexManager, schema, indexer
│   ├── mod.rs
│   ├── schema.rs               ─── Tantivy schema definition
│   └── indexer.rs              ─── Writer + reader, full_reindex
├── thumbnails/                 ─── Image generation, cache, concurrency limiter
│   ├── mod.rs
│   ├── image.rs                ─── WebP thumbnail generation (image crate)
│   ├── video.rs                ─── ffmpeg keyframe → thumbnail
│   ├── cache.rs                ─── Content-addressed cache + eviction
│   └── limiter.rs              ─── Per-key mutex + concurrency limit
├── watcher/                    ─── File system watcher, event pipeline
│   ├── mod.rs                  ─── FileWatcher struct, notify setup
│   ├── handler.rs              ─── Event → stages pipeline
│   └── stages/                 ─── Processing pipeline stages
│       ├── mod.rs
│       ├── extract.rs          ─── Extract metadata from changed file
│       ├── store.rs            ─── Upsert into SQLite + Tantivy
│       └── broadcast.rs        ─── Send SSE event to clients
├── profiler.rs                  ─── /debug/pprof endpoint (compiled only under the `dev-tools` feature)
├── util.rs                      ─── Shared helpers
└── test_support.rs             ─── #[cfg(test)] — fixture_path helper
```

### Dependency Flow Between Modules

```
                ┌─────────────┐
                │   config/   │
                └──────┬──────┘
                       │ settings
          ┌────────────┼────────────┐
          ▼            ▼            ▼
    ┌──────────┐ ┌──────────┐ ┌──────────┐
    │ scanner/ │ │metadata/ │ │  db/     │
    └────┬─────┘ └─────┬────┘ └────┬─────┘
         │             │           │
         └──────┬──────┘           │
                ▼                  │
         ┌──────────┐              │
         │ indexer/ │──────────────┤
         └────┬─────┘              │
              │                    │
              ▼                    ▼
         ┌──────────┐      ┌──────────┐
         │ search/  │      │   db/    │
         │ (Tantivy)│      │ (SQLite) │
         └──────────┘      └──────────┘
              │                    │
              └──────┬─────────────┘
                     ▼
              ┌──────────┐
              │ routes/  │
              └────┬─────┘
                   │
         ┌─────────┼─────────┐
         ▼         ▼         ▼
   ┌─────────┐┌─────────┐┌─────────┐
   │watcher/ ││thumbnails││middleware│
   └─────────┘└─────────┘└─────────┘
```

## Data Flow

### 1. Ingestion (Startup)

```
User configures watched folders ──▶ main.rs spawns background task
                                        │
                                        ▼
                              ┌──────────────────┐
                              │  Phase 1:        │
                              │  incremental_    │
                              │  index()         │
                              └────────┬─────────┘
                                        │
                          (unchanged files — same size +
                           mtime — skip everything below)
                                        │
                         ┌─────────────┼─────────────┐
                         ▼             ▼             ▼
                   ┌──────────┐  ┌──────────┐  ┌──────────┐
                   │ walker   │──│ hasher   │──│ detect   │
                   │ (walk    │  │ (SHA-256)│  │ (MIME +  │
                   │  dirs)   │  │          │  │  dims)   │
                   └──────────┘  └──────────┘  └────┬─────┘
                                                    │
                                                    ▼
                                            ┌──────────────┐
                                            │  metadata/   │
                                            │  png.rs or   │
                                            │  video.rs    │
                                            └──────┬───────┘
                                                   │
                                                   ▼
                                            ┌──────────────┐
                                            │  SQLite      │
                                            │  upsert      │
                                            └──────┬───────┘
                                                   │
                                                  Phase 2
                                                   │
                                                   ▼
                                            ┌──────────────┐
                                            │  Tantivy     │
                                            │  reindex     │
                                            │  (separate   │
                                            │   read conn) │
                                            └──────────────┘
                                                   │
                                                   ▼
                                            ┌──────────────┐
                                            │  SSE:        │
                                            │  indexing_   │
                                            │  complete    │
                                            └──────────────┘
```

### 2. Real-Time Updates (File System Changes)

```
File system change (create/modify/delete)
        │
        ▼
┌────────────────┐
│ notify crate   │
│ (inotify/      │
│  FSEvents)     │
└───────┬────────┘
        │
        ▼
┌────────────────────┐
│ notify-debouncer-  │
│ mini (500ms batch) │
└───────┬────────────┘
        │ events (mpsc channel)
        ▼
┌────────────────────┐
│ watcher/handler.rs │
│ run_event_handler  │
└───────┬────────────┘
        │
        ▼
┌────────────────────┐
│  stages/extract.rs │─── extract metadata from changed file
└───────┬────────────┘
        │
        ▼
┌────────────────────┐
│  stages/store.rs   │─── SQLite upsert + Tantivy update
└───────┬────────────┘
        │
        ▼
┌────────────────────┐
│ stages/broadcast   │─── send SseEvent to broadcast channel
│ .rs                │
└───────┬────────────┘
        │
        ▼
┌────────────────────┐
│ SSE /events endpoint│─── all connected clients receive event
└────────────────────┘
```

### 3. Serving (HTTP Request)

```
Browser (React SPA)
        │
        ▼
┌────────────────────┐
│ GET /api/v1/media  │
│ ?cursor=...&limit= │
│ &mime_type=...     │
└───────┬────────────┘
        │
        ▼
┌────────────────────┐
│ routes/media/      │
│ list.rs handler    │
└───────┬────────────┘
        │
        ▼
┌────────────────────┐
│ SQLite query:      │
│ SELECT ... FROM    │
│ media_items WHERE  │
│ (file_created_at,  │
│ id) < (?, ?)       │
│ ORDER BY           │
│ file_created_at    │
│ DESC, id DESC      │
│ LIMIT ?            │
└───────┬────────────┘
        │
        ▼
┌────────────────────┐
│ JSON response:     │
│ { data, meta: {    │
│   next_cursor,     │
│   next_cursor_id,  │
│   has_more, total }}│
└────────────────────┘
```

### 4. Search Request

```
Browser (React SPA)
        │
        ▼
┌────────────────────┐
│ GET /api/v1/search │
│ ?q=seed:12345      │
│ &cursor=...&limit= │
└───────┬────────────┘
        │
        ▼
┌────────────────────┐
│ routes/search.rs   │
│ handler            │
└───────┬────────────┘
        │
        ├──────────────────────┐
        ▼                      ▼
┌────────────────┐    ┌────────────────┐
│ Tantivy query  │    │ SQLite fetch   │
│ (BM25, re-     │───▶│ by IDs from   │
│  turn doc IDs) │    │ search result  │
└────────────────┘    └───────┬────────┘
                              │
                              ▼
                     ┌────────────────┐
                     │ JSON response  │
                     │ (same format   │
                     │  as media list)│
                     └────────────────┘
```

### 5. Thumbnail Serving

```
GET /api/v1/media/:id/thumbnail
        │
        ▼
┌────────────────────┐
│ routes/media/      │
│ thumbnail.rs       │
└───────┬────────────┘
        │
        ▼
┌──────────────────────────────┐
│ thumbnails/cache.rs          │
│ compute cache key from hash │
│ ┌──────────┐   ┌──────────┐ │
│ │ Cache    │   │ Cache    │ │
│ │ HIT ────────▶│ return   │ │
│ │          │   │ WebP     │ │
│ │ MISS     │   │ generate │ │
│ └──────────┘   └────┬─────┘ │
└─────────────────────┼───────┘
                      │
                      ▼
             ┌────────────────┐
             │ Per-key mutex  │
             │ (DashMap)      │
             │ ┌──────────┐   │
             │ │ spawn_   │   │
             │ │ blocking │   │
             │ │ ┌────────┴┐  │
             │ │ │ image/  │  │
             │ │ │ video   │  │
             │ │ │ crate   │  │
             │ │ └────────┘  │
             │ │ store WebP  │
             │ │ return      │
             │ └─────────────┘
             └────────────────┘
                      │
                      ▼
             ┌────────────────┐
             │ HTTP response  │
             │ Content-Type:  │
             │ image/webp     │
             │ Cache-Control: │
             │ public, max-   │
             │ age=31536000   │
             │ ETag: <hash>   │
             └────────────────┘
```

## API Endpoints

All routes are mounted under `/api/v1`.

| Method | Path                        | Handler              | Description                              |
| ------ | --------------------------- | -------------------- | ---------------------------------------- |
| `GET`  | `/health`                   | `routes::health`     | Health check — returns `{"status":"ok"}` |
| `GET`  | `/media`                    | `routes::media::list`| List media items (cursor-based, infinite)|
| `GET`  | `/media/:id`                | `routes::media::detail` | Single item with full metadata        |
| `GET`  | `/media/:id/metadata`       | `routes::media::detail` | Structured metadata for a single item |
| `GET`  | `/media/:id/thumbnail`      | `routes::media::thumbnail` | Thumbnail (WebP, cached)          |
| `GET`  | `/media/:id/file`           | `routes::media::file`| Original file (streamed, Range requests) |
| `GET`  | `/search`                   | `routes::search`     | Full-text search with cursor pagination  |
| `GET`  | `/config`                   | `routes::config`     | Get current configuration (watched dirs) |
| `PUT`  | `/config`                   | `routes::config`     | Update watched folders (triggers re-index)|
| `GET`  | `/config/suggest`           | `routes::config::suggest` | Suggest subdirectory paths         |
| `GET`  | `/events`                   | `routes::events`     | SSE stream of file-system events         |
| `GET`  | `/stats`                    | `routes::stats`      | Index statistics (total, status)         |

## Database Schema (SQLite)

Live schema after migrations v001–v005 (v002: `folder_id` + `watched_folders`; v003: rebuild without column-level UNIQUE; v004: `watched_folders` becomes the config source of truth; v005: drop the write-only `thumbnail_path` column):

```sql
CREATE TABLE media_items (
    id               TEXT PRIMARY KEY NOT NULL,   -- UUID v4
    filename         TEXT NOT NULL,               -- Original filename
    relative_path    TEXT NOT NULL,               -- Relative to the watched folder root
    folder_id        TEXT REFERENCES watched_folders(id),
    mime_type        TEXT NOT NULL,               -- "image/png", "video/webm", etc.
    width            INTEGER,                     -- Pixels (NULL if unknown)
    height           INTEGER,                     -- Pixels (NULL if unknown)
    file_size        INTEGER NOT NULL,            -- Bytes
    file_created_at  TEXT NOT NULL,               -- ISO 8601
    file_modified_at TEXT NOT NULL,               -- ISO 8601
    indexed_at       TEXT NOT NULL DEFAULT (datetime('now')),
    metadata_json    TEXT,                        -- Raw metadata JSON blob
    checksum         TEXT                         -- SHA-256 hex string
);

CREATE INDEX idx_media_sort ON media_items(file_created_at DESC, id);
CREATE INDEX idx_media_mime ON media_items(mime_type);
CREATE UNIQUE INDEX idx_media_folder_path ON media_items(folder_id, relative_path);

CREATE TABLE watched_folders (
    id    TEXT PRIMARY KEY NOT NULL,              -- UUID v4
    path  TEXT NOT NULL UNIQUE,
    label TEXT
);
```

Uniqueness of `(folder_id, relative_path)` is enforced **only** by the compound unique index — the same relative path may exist in different watched folders. The legacy `config` key-value table still exists but is never read: migration v004 imported its JSON blob into `watched_folders` once, and `AppConfig` is derived from `watched_folders` only.

### Tantivy Index Schema

| Field          | Type   | Options                                |
| -------------- | ------ | -------------------------------------- |
| `id`           | Text   | `STRING \| STORED`                     |
| `filename`     | Text   | `STRING \| STORED`                     |
| `mime_type`    | Text   | `STRING`                               |
| `metadata_json`| Text   | `TEXT` (indexed for full-text search)  |
| `created_at`   | Date   | `INDEXED \| FAST`                      |
| `file_size`    | U64    | `INDEXED`                              |
| `width`        | U64    | `STORED`                               |
| `height`       | U64    | `STORED`                               |

## Server Startup Sequence

```
main()
  │
  ├── 1. Parse environment (Settings::from_env)
  ├── 2. Create directories (DB, cache, Tantivy)
  ├── 3. Create r2d2 connection pool + run migrations (v001–v005)
  ├── 4. Open Tantivy index (IndexManager::open_or_create, 200MB writer buffer)
  ├── 5. Create ThumbnailLimiter + ProgressTracker
  ├── 6. Spawn cache eviction timer (5-minute interval)
  ├── 7. Create SSE broadcast channel (capacity 256)
  ├── 8. Load watched-folder config from SQLite (watched_folders table)
  ├── 9. Start FileWatcher (always, even with no folders — the config route
  │       adds watches dynamically at runtime)
  ├── 10. Build route states (ConfigState, MediaState, SearchState, StatsState, EventsState)
  ├── 11. Spawn background indexing (Phase 1 → Phase 2); when it completes,
  │        drain stale events and activate the watcher event handler
  ├── 12. Assemble router: health_router() + /api/v1 nests with per-group
  │        timeouts (media 120s, SSE 3600s, default REQUEST_TIMEOUT_SECS)
  ├── 13. Apply logging + CORS (env allowlist) layers; security headers outermost
  ├── 14. Bind 127.0.0.1:{PORT}, axum::serve with graceful shutdown (SIGINT/SIGTERM)
  └── 15. On shutdown: commit Tantivy index (30s cleanup timeout)
```

## Environment Variables

| Variable                  | Default                                     | Purpose                                        |
| ------------------------- | ------------------------------------------- | ---------------------------------------------- |
| `PORT`                    | `3001`                                      | HTTP server port                               |
| `REQUEST_TIMEOUT_SECS`    | `60`                                        | Default request timeout (media: 120s, SSE: 3600s) |
| `THUMBNAIL_CONCURRENCY`   | `4`                                         | Max concurrent thumbnail generations           |
| `THUMBNAIL_CACHE_MAX_MB`  | `2000`                                      | Max thumbnail cache size (0 = unlimited)       |
| `INDEX_CONCURRENCY`       | CPU cores capped at `8`                     | Max concurrently processed files in Phase 1    |
| `MIN_FREE_DISK_MB`        | `500`                                       | Min free disk before aggressive cache eviction |
| `CORS_ALLOW_ORIGINS`      | `http://localhost:5173,http://127.0.0.1:5173` | Comma-separated CORS origin allowlist        |
| `IMAGEVIZ_DB_PATH`        | `{data_dir}/imageviz.db`                    | SQLite database location                       |
| `IMAGEVIZ_CACHE_DIR`      | `{data_dir}/thumbnails`                     | On-disk thumbnail cache                        |
| `IMAGEVIZ_TANTIVY_DIR`    | `{data_dir}/tantivy`                        | Tantivy index directory                        |

Where `{data_dir}` = `$XDG_DATA_HOME/imageviz` (Linux, falling back to `~/.local/share/imageviz`), `~/Library/Application Support/imageviz` (macOS), or `./data` (fallback).

## Development & Testing

| Action                | Backend                           | Frontend                   |
| --------------------- | --------------------------------- | -------------------------- |
| Run dev server        | `cargo run` (port 3001)           | `npm run dev` (Vite proxied)|
| Run all tests         | `cargo test`                      | `npm test` (Vitest)        |
| Run single test       | `cargo test test_name`            | `npx vitest run -t "test name"` |
| Lint                  | `cargo clippy -- -D warnings`     | `npm run lint`             |
| Type check            | `cargo check`                     | `npm run typecheck`        |
| Format check          | `cargo fmt --check`               | `npm run format:check`     |
| E2E tests             | —                                 | `npx playwright test`      |
| Production build/run  | `./scripts/build.sh` / `./scripts/start.sh` |                   |

See `documents/plans/development-plan.md` for the complete development roadmap (Waves 0–8), task breakdown, dependency graph, testing strategy, and performance budgets.
