# ImageViz

A browser-based image and video visualization tool for large datasets (100K–1M media files). Designed primarily for viewing ComfyUI generation outputs with embedded metadata.

![screenshot placeholder](https://img.shields.io/badge/status-active--development-blue)

## Features

- **Infinite-scroll thumbnail grid** — Virtualized masonry grid that handles 100K+ items at 60fps using `react-virtuoso`
- **Full-text metadata search** — Powered by Tantivy (Lucene-like inverted index) for instant search across PNG `tEXt`/`iTXt` chunks and file metadata
- **Real-time file watching** — SSE-based live updates: new files appear, deleted files disappear, modified files update automatically
- **Image viewer** — Click-to-preview with zoom/pan for PNG, JPG, WEBP, GIF
- **Video viewer** — Playback with controls, seeking via HTTP Range requests for MP4 and WEBM
- **Metadata panel** — Collapsible JSON tree view of ComfyUI prompt and workflow data
- **Drag and drop** — OS-level drag from the browser to external applications (file explorer, editor, etc.)
- **Config panel** — Add/remove watched folders with live index statistics
- **Keyboard navigation** — Full keyboard support: arrows, Enter, `/` to search, `?` for shortcuts
- **Dark theme** — Optimized for desktop use on vertical and horizontal monitors

## Quick Start

### Prerequisites

- **Rust** (edition 2024) — Install via [rustup](https://rustup.rs/)
- **Node.js** >= 22 — Install via [nvm](https://nvm.sh/) or your package manager
- **ffmpeg** — Required for video metadata extraction and thumbnail generation (`apt install ffmpeg` / `brew install ffmpeg`)

### 1. Clone and install

```bash
git clone https://github.com/paulomarciano/imageviz.git
cd imageviz

# Build the backend
cd backend && cargo build && cd ..

# Install frontend dependencies
cd frontend && npm install && cd ..
```

### 2. Run development servers

Use the convenience script (starts both servers with one command):

```bash
./scripts/dev.sh
```

Or run them manually in separate terminals:

```bash
# Terminal 1: Backend (Rust, port 3001)
cd backend && cargo run

# Terminal 2: Frontend (Vite, port 5173, proxies /api → :3001)
cd frontend && npm run dev
```

Open **http://localhost:5173** in your browser.

### 3. Configure watched folders

1. Click the ⚙️ **Settings** button in the header
2. Add folder paths (e.g. `~/ComfyUI/output`, `/media/photos`) with optional labels
3. Click **Save** — the indexer will scan the folders and populate the grid

## Usage

### Browsing

- The grid loads thumbnails from the most recent files first
- Scroll down to load more (infinite scroll, cursor-based pagination)
- Thumbnails are cached on disk as WebP for fast re-display

### Searching

- Press `/` or click the search bar, type a query (300ms debounce)
- Search is full-text across filenames and extracted metadata
- Results update in real-time; clear the search to return to the full grid

### Viewing

- Click any thumbnail to open the **detail view** modal
- Use ← → arrow keys to navigate between items
- Images: scroll to zoom, drag to pan, double-click to toggle fit
- Videos: Space to play/pause, ← → to seek 5s, F for fullscreen
- Metadata panel (right side) shows extracted ComfyUI prompt/workflow data
- Press **Escape** or click the × button to close

### Keyboard Shortcuts

| Keys | Action |
|------|--------|
| `?` | Show/hide keyboard shortcuts |
| `/` | Focus search bar |
| `↑ ↓ ← →` | Navigate grid |
| `Enter` | Open detail view |
| `Space` | Select item |
| `Home` / `End` | Jump to first/last item |
| `Esc` | Close panel or detail view |
| `+` / `-` | Zoom in/out (image viewer) |
| `F` | Toggle fullscreen (video viewer) |

### Real-time Updates

When files are added, deleted, or modified in watched folders, the grid updates automatically via SSE (Server-Sent Events). A "New files" counter appears when new items arrive while you're scrolled down.

## Configuration

### Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `IMAGEVIZ_DB_PATH` | `{data_dir}/imageviz.db` | SQLite database location |
| `IMAGEVIZ_CACHE_DIR` | `{data_dir}/thumbnails` | On-disk thumbnail cache |
| `IMAGEVIZ_TANTIVY_DIR` | `{data_dir}/tantivy` | Tantivy search index directory |
| `PORT` | `3001` | HTTP server port |
| `REQUEST_TIMEOUT_SECS` | `60` | Default HTTP request timeout in seconds |
| `THUMBNAIL_CONCURRENCY` | `4` | Max concurrent thumbnail generations |
| `THUMBNAIL_CACHE_MAX_MB` | `2000` | Max thumbnail cache size in MB (0 = unlimited) |
| `MIN_FREE_DISK_MB` | `500` | Minimum free disk space before aggressive eviction |
| `CORS_ALLOW_ORIGINS` | `http://localhost:5173,http://127.0.0.1:5173` | Comma-separated CORS origin allowlist |

Where `{data_dir}` = `$XDG_DATA_HOME/imageviz` (Linux), `~/Library/Application Support/imageviz` (macOS), or `./data` (fallback).

## API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/v1/health` | Health check |
| `GET` | `/api/v1/media` | List media items (cursor-based pagination) |
| `GET` | `/api/v1/media/{id}` | Get single media item |
| `GET` | `/api/v1/media/{id}/file` | Get original file (Range requests, ETag/304) |
| `GET` | `/api/v1/media/{id}/thumbnail` | Get WebP thumbnail (`?width=100..500`) |
| `GET` | `/api/v1/media/{id}/metadata` | Get structured metadata |
| `GET` | `/api/v1/search` | Full-text search (`?q=...&limit=..&cursor=...`) |
| `GET` | `/api/v1/config` | Get watched folder configuration |
| `PUT` | `/api/v1/config` | Update watched folders |
| `GET` | `/api/v1/config/suggest` | Folder path autocomplete |
| `GET` | `/api/v1/stats` | Index statistics |
| `GET` | `/api/v1/events` | SSE real-time event stream |

Full API contract: [documents/plans/development-plan.md§3](documents/plans/development-plan.md#3-api-contract)

## Project Structure

```
imageviz/
├── backend/                    # Rust backend (Axum, SQLite, Tantivy)
│   ├── src/
│   │   ├── main.rs             # Server entry point
│   │   ├── lib.rs              # Health router factory
│   │   ├── config/             # App configuration (watched folders)
│   │   ├── db/                 # SQLite schema, migrations, queries
│   │   ├── indexer/            # File scan + index orchestration
│   │   ├── metadata/           # PNG tEXt/iTXt, video (ffmpeg), MIME detection
│   │   ├── routes/             # HTTP handlers (media, search, config, events, stats)
│   │   ├── scanner/            # Directory walker and file hasher
│   │   ├── search/             # Tantivy schema, indexer, searcher
│   │   ├── thumbnails/         # WebP thumbnail generation and caching
│   │   └── watcher/            # File system watcher (notify + debouncer)
│   └── tests/                  # Integration tests
├── frontend/                   # React + TypeScript frontend (Vite)
│   ├── src/
│   │   ├── App.tsx             # Root component with routing
│   │   ├── api/                # Typed API client (media, search, config)
│   │   ├── components/
│   │   │   ├── config/         # Config panel (watched folders)
│   │   │   ├── layout/         # App shell + header
│   │   │   ├── media/          # Thumbnail grid, card, drag source
│   │   │   ├── search/         # Search bar
│   │   │   ├── shared/         # EmptyState, ErrorBoundary, ErrorState, Skeleton, ShortcutsPanel
│   │   │   └── viewer/         # Detail view, image viewer, video viewer, metadata panel
│   │   ├── hooks/              # useInfiniteMedia, useSearch, useSse, useKeyboardNav, useScrollRestore
│   │   ├── store/              # Jotai atoms (media, search, SSE, UI)
│   │   ├── types/              # TypeScript type definitions
│   │   └── utils/              # Formatting helpers
│   ├── e2e/                    # Playwright E2E tests
│   └── playwright.config.ts
├── test-fixtures/              # Sample media files for testing
└── documents/                  # Development plans and tickets
```

## Development

### Commands

| Action | Backend | Frontend |
|--------|---------|----------|
| Run dev | `cargo run` | `npm run dev` |
| Run tests | `cargo test` | `npm test` |
| Single test | `cargo test test_name` | `npx vitest run -t "test name"` |
| Lint | `cargo clippy -D warnings` | `npm run lint` |
| Format check | `cargo fmt --check` | `npm run format:check` |
| Type check | `cargo check` | `npm run typecheck` |
| Build | `cargo build --release` | `npm run build` |
| E2E tests | — | `npx playwright test` |

### Running E2E Tests

```bash
cd frontend
npx playwright install chromium
npx playwright test
```

### Generating Test Fixtures

```bash
./scripts/generate-fixtures.sh
```

Requires **ffmpeg** on PATH. Generates PNGs with ComfyUI-style tEXt chunks and short video files.

## Contributing

1. **Read the plan** — Start with [documents/plans/development-plan.md](documents/plans/development-plan.md) for architecture, API contract, and task breakdown
2. **TDD workflow** — Write a failing test first, implement the minimum code, refactor, verify with `cargo test` / `npm test`
3. **Code style** — Run `cargo fmt && cargo clippy -D warnings` (backend) and `npx prettier --check . && npx eslint .` (frontend) before committing
4. **Commits** — Use descriptive commit messages. Each task generates at least one commit
5. **Tests** — All tests must pass before opening a PR. New features require tests

See [documents/plans/development-plan.md§7](documents/plans/development-plan.md#7-testing-strategy-tdd-first) for the full testing strategy.

## Tech Stack

### Backend

| Component | Choice |
|-----------|--------|
| Web framework | **Axum** 0.8 |
| Async runtime | **Tokio** 1.x |
| Metadata store | **SQLite** (rusqlite, WAL mode) |
| Full-text search | **Tantivy** 0.26 |
| File watching | **notify** + notify-debouncer-mini |
| Image processing | **image** crate (WebP thumbnails) |
| Video thumbnails | **ffmpeg** (sidecar subprocess) |

### Frontend

| Component | Choice |
|-----------|--------|
| Build tool | **Vite** 8 |
| UI library | **React** 19 |
| Virtual scroll | **react-virtuoso** |
| Data fetching | **TanStack Query** 5 |
| State management | **Jotai** 2 |
| Styling | **Tailwind CSS** 4 |
| Drag & drop | **react-dnd** |
| E2E testing | **Playwright** |

## License

MIT
