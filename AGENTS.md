# ImageViz — Agent Instructions

## Project Status

v0.8.0 — all 8 development waves complete (Wave 8: post-audit performance/resource/hygiene pass; follow-up tickets 8.29/8.30 pending). Browser-based image/video viewer for large datasets (100K–1M files), designed for ComfyUI output with embedded PNG metadata.

## Source of Truth

**`documents/plans/development-plan.md`** (v2.0, 946 lines) — architecture, API contract, DB schema, Tantivy schema, all 8 waves. Read before any implementation.

## Key Commands

| Action | Backend | Frontend |
|--------|---------|----------|
| Dev server | `cargo run` (port 3001) | `npm run dev` (port 5173, proxies `/api` → :3001) |
| Both at once | `./scripts/dev.sh` | |
| Test all | `cargo test` | `npm test` (= `vitest run`) |
| Single test | `cargo test test_name` | `npx vitest run -t "test name"` |
| Lint | `cargo clippy -- -D warnings` | `npm run lint` (= `eslint .`) |
| Format check | `cargo fmt --check` | `npm run format:check` (= `prettier --check .`) |
| Format fix | `cargo fmt` | `npm run format` (= `prettier --write .`) |
| Type check | `cargo check` | `npm run typecheck` (= `tsc --noEmit`) |
| Build | `cargo build --release` | `npm run build` (= `tsc -b && vite build`) |

Frontend package manager is **npm**. Path alias `@/` → `./src/`.

Production: `./scripts/build.sh` (release-build both sides) · `./scripts/start.sh` (build + run backend on :3001 + `vite preview` on :4173 — preview inherits the `/api` proxy from `server.proxy`).

## Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `PORT` | `3001` | HTTP server port |
| `REQUEST_TIMEOUT_SECS` | `60` | Default request timeout (media: 120s, SSE: 3600s) |
| `THUMBNAIL_CONCURRENCY` | `4` | Max concurrent thumbnail generations |
| `THUMBNAIL_CACHE_MAX_MB` | `2000` | Max cache size (0 = unlimited) |
| `INDEX_CONCURRENCY` | CPU cores capped at `8` | Max concurrent Phase-1 indexing files (hash/ffprobe) |
| `MIN_FREE_DISK_MB` | `500` | Min free disk before aggressive eviction |
| `CORS_ALLOW_ORIGINS` | `http://localhost:5173,http://127.0.0.1:5173` | Comma-separated CORS origin allowlist (`backend/src/middleware/cors.rs`) |
| `IMAGEVIZ_DB_PATH` | `{data_dir}/imageviz.db` | SQLite database location |
| `IMAGEVIZ_CACHE_DIR` | `{data_dir}/thumbnails` | Thumbnail cache location |
| `IMAGEVIZ_TANTIVY_DIR` | `{data_dir}/tantivy` | Tantivy index directory |

`{data_dir}` = `$XDG_DATA_HOME/imageviz` (Linux, falling back to `~/.local/share/imageviz`), `~/Library/Application Support/imageviz` (macOS), or `./data` (fallback).

## Architecture Notes

- **Route assembly**: `backend/src/lib.rs::health_router()` builds a minimal health-only router. `main.rs` nests stateful routes (config, media, search, events, stats) on top, all under `/api/v1`. Integration tests reuse `health_router()` + same nesting.
- **DB**: `r2d2` pool (max 10 conns, WAL-compatible, 5s busy timeout). Tantivy reindex opens a **separate read-only connection** during startup (WAL permits concurrent readers). Watched folders live **only** in the `watched_folders` table (`AppConfig` is derived from it; the legacy `config` JSON blob was imported once by migration v004 and is never read).
- **Indexing**: startup runs the **incremental** path — files whose size+mtime are unchanged are skipped before hashing (`run_index` core with a skip predicate shared by full/incremental wrappers). Phase 1 (hash/ffprobe) runs with bounded concurrency (`INDEX_CONCURRENCY`); deletion cleanup diffs in memory and deletes in one transaction.
- **Thumbnails**: Content-addressed WebP cache (key = `{sha256[:16]}_{width}.webp`), generated directly into the cache dir (`{key}.tmp` + atomic rename; no `/tmp` staging). `spawn_blocking` for CPU-bound work. Per-checksum `Mutex`es in a `DashMap` prevent duplicate generation; entries are `Weak`-valued and evicted when the last holder releases. The `ThumbnailLimiter` semaphore (env `THUMBNAIL_CONCURRENCY`) is acquired **only on cache misses** — hits bypass it.
- **File serving**: `tokio::fs::File` + streaming (never loads full file into memory). Range requests for video seeking. ETag/304 for caching.
- **File watcher**: `notify` + `notify-debouncer-mini` (500ms debounce). `mpsc` channel decouples watcher from indexer. Watcher is held in `Arc<Mutex<FileWatcher>>`; `_watcher_guard` in `main.rs:132` keeps an `Arc::clone()` alive for server lifetime.
- **Security**: CSP, X-Frame-Options, X-Content-Type-Options, Referrer-Policy, Permissions-Policy applied as outermost Axum middleware layer.
- **Input validation at route boundary**: limit [1–500], cursor ISO 8601, cursor_id UUID v4, search max 1000 chars, path max 4096 chars, no `..` traversal.
- **`backend/.cargo/config.toml`**: sets `-D warnings` for `x86_64-unknown-linux-gnu` **only** (not macOS). Default and release builds need no `--cfg tokio_unstable` (dev tooling is feature-gated, see below). The CI `frontend-e2e` job runs the default backend build.
- **Supported media extensions**: PNG, JPG/JPEG, WebP, GIF, MP4, WebM, MOV (shared constant in `backend/src/media_types.rs`).
- **Hidden files/dirs**: `is_hidden_path()` in `media_types.rs` ignores any path whose component starts with `.` — applied consistently by both scanner and watcher.
- **Debug tooling** (feature-gated, off by default): `cargo run --features dev-tools` enables tokio-console plus `GET /debug/pprof/profile?seconds=5&format=svg` (mounted **outside** `/api/v1`, via `backend/src/profiler.rs`, compiled only under the feature). Running the dev-tools binary requires tokio built with the unstable cfg: `RUSTFLAGS="--cfg tokio_unstable" cargo run --features dev-tools` (note `RUSTFLAGS` overrides `.cargo/config.toml`). Backend logging is filtered via `RUST_LOG` (`EnvFilter::try_from_default_env()`); the request middleware emits exactly **one** line per request (the response line with duration).

## Test Conventions

- **Rust unit tests**: co-located via `#[path = "filename_test.rs"]` (e.g., `src/metadata/png_test.rs` beside `png.rs`).
- **Rust integration tests**: in `backend/tests/` using `tower::ServiceExt::oneshot` (in-process, no socket). Larger tests use `TestApp` from `tests/common/mod.rs` with in-memory SQLite + tempdir Tantivy + real `reqwest` client.
- **Test support**: `backend/src/test_support.rs` provides `fixture_path(name)` → `test-fixtures/{name}` (only under `cfg(test)`). Use `crate::test_support::fixture_path` in unit tests, `imageviz_backend::test_support::fixture_path` in integration tests.
- **Fixture-dependent Rust tests** are `#[ignore]` — run with `cargo test -- --ignored` after `./scripts/generate-fixtures.sh` (requires **ffmpeg** on PATH). Fixture files are **gitignored** — only `.gitkeep` committed.
- **Frontend tests**: co-located `__tests__/` dirs. Vitest + jsdom + @testing-library/react. `setup-tests.ts` globally mocks `EventSource` (jsdom doesn't implement it) and `VirtuosoGrid` — any test rendering `<App />` or `ThumbnailGrid` needs these.
- **Frontend integration tests**: MSW handlers at `src/test-utils/msw-handlers.ts`. Render helpers at `src/test-utils/render-utils.tsx` wrap QueryClient + Jotai Provider.
- **E2E** (Playwright): `npx playwright test` from `frontend/`. Requires `npx playwright install chromium` once. Auto-starts both servers via `webServer` config. `workers: 1` (serial, shared backend state), `retries: 1`.

## TDD Workflow (Mandatory)

Per task: write failing test → implement minimum → refactor while green → add edge cases → verify with `cargo test` / `npm test`.

## CI Pipeline (`.github/workflows/ci.yml`)

Triggers on **every push** (any branch) and PRs to `main`. Five parallel jobs, cancel-in-progress:
1. **backend-lint** (15m): `cargo fmt --check` → `cargo clippy -- -D warnings` → `cargo clippy --features dev-tools -- -D warnings`
2. **backend-test** (15m): ffmpeg via apt → `cargo test` → `cargo test --features dev-tools` (profiler.rs tests exist only under the feature)
3. **frontend-lint** (15m): `npm ci` → `prettier --check .` → `eslint .`
4. **frontend-test** (15m): `npm ci` → `tsc --noEmit` → `vitest run`
5. **frontend-e2e** (30m): `npm ci` → `generate-fixtures.sh` → `playwright install chromium` → `playwright test`

## Release Process

1. Bump the version in **both** manifests: `backend/Cargo.toml` and `frontend/package.json` (keep in sync).
2. Add a `CHANGELOG.md` entry (Keep a Changelog 1.1.0 format) plus the compare link at the bottom of the file.
3. A release build must compile and test clean **without** `--cfg tokio_unstable` or the `dev-tools` feature (CI enforces the default-feature build).
4. Documentation to keep in step on release: `README.md` (badge, versions), `ARCHITECTURE.md` (schema/module claims), `CHANGELOG.md`.

## Git Conventions

- Remote: `git@github.com:paulomarciano/imageviz.git`.
- **After every commit**, spawn a `CodeReviewer` subagent (new invocation, not resumed). Pass session context path. Address critical/warning issues before additional commits.

## Formatter & Lint Settings

- **Rustfmt** (`backend/rustfmt.toml`): `max_width = 100`, `tab_spaces = 4`, `edition = "2024"`, `newline_style = "Unix"`, `use_small_heuristics = "Max"`.
- **Prettier** (`frontend/.prettierrc`): `singleQuote`, `trailingComma: "all"`, `semi`, `printWidth: 100`, `tabWidth: 2`.
- **ESLint** (`frontend/eslint.config.mjs`): TS recommended. `no-unused-vars` is **error**, `no-explicit-any` is **warn**. Ignores `dist/`, `node_modules/`, `*.config.*`.
- **TypeScript** (`frontend/tsconfig.json`): `strict: true`, `noUncheckedIndexedAccess` (array access → `T | undefined`), `noUnusedLocals`, `noUnusedParameters`.
