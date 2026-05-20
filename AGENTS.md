# ImageViz — Agent Instructions

## Project Status

**Backend (Waves 1–3 complete)** — scanner, metadata extraction (PNG tEXt/iTXt, video via ffmpeg), SQLite with WAL, Tantivy full-text search, thumbnail generation (WebP, content-addressed cache), media serving (Range requests, ETag/304), SSE real-time events, file watcher with debouncer.

**Frontend (Wave 0 complete)** — Vite 8 + React 19 + TypeScript strict scaffolded with Tailwind v4, Vitest, ESLint flat config, Prettier. No UI beyond the smoke-test shell.

## Source of Truth

**`documents/plans/development-plan.md`** — architecture, API contract, DB schema, Tantivy schema, development waves (0–7), task breakdown, dependency graph, testing strategy, performance budgets. Read this before any implementation.

## Stack (Non-Obvious Choices)

- **Backend**: Rust edition **2024**, Axum, Tokio. SQLite with **WAL mode** for concurrency. Tantivy for full-text search. Cursor-based pagination (not offset — `WHERE (created_at, id) < (?, ?)` is O(log n)).
- **Frontend**: React 19, Vite 8, TypeScript strict. react-virtuoso for virtual scroll (not TanStack Virtual). react-dnd for OS-level drag. Jotai for state. TanStack Query for data.
- **Video thumbnails**: ffmpeg invoked as a subprocess (not a Rust crate).
- **Thumbnail caching**: Content-addressed on-disk. Use `spawn_blocking` for CPU-bound image work (avoids blocking the async runtime).
- **No auth** for v1. Desktop-first (vertical + horizontal screens), not mobile.

## Key Commands

| Action | Backend | Frontend |
|--------|---------|----------|
| Run dev | `cargo run` (port 3001) | `npm run dev` (Vite proxies `/api` → :3001) |
| Run all tests | `cargo test` | `npm test` (Vitest) |
| Run single test | `cargo test test_name` | `npx vitest run -t "test name"` |
| Lint | `cargo clippy -- -D warnings` + `cargo fmt --check` | `npm run lint` + `npm run format:check` |
| Type check | `cargo check` | `npm run typecheck` (= `tsc --noEmit`) |
| Build | `cargo build --release` | `npm run build` (= `tsc -b && vite build`) |

Frontend package manager is **npm** (not pnpm/yarn).

**Rustfmt** (`backend/rustfmt.toml`): `max_width = 100`, `tab_spaces = 4`, `edition = "2024"`, `newline_style = "Unix"`, `use_small_heuristics = "Max"`. Backend builds with `-D warnings` via `.cargo/config.toml`.

**Prettier** (`frontend/.prettierrc`): `singleQuote`, `trailingComma: "all"`, `semi`, `printWidth: 100`, `tabWidth: 2`.

**ESLint** (`frontend/eslint.config.mjs`): `typescript-eslint` recommended. `no-unused-vars` is error, `no-explicit-any` is warn. Ignores `dist/`, `node_modules/`, `*.config.*`.
**TypeScript** (`frontend/tsconfig.json`): `strict: true` with `noUncheckedIndexedAccess` — array access returns `T | undefined`.

## Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `IMAGEVIZ_DB_PATH` | `{data_dir}/imageviz.db` | SQLite database location |
| `IMAGEVIZ_CACHE_DIR` | `{data_dir}/thumbnails` | On-disk thumbnail cache |
| `IMAGEVIZ_TANTIVY_DIR` | `{data_dir}/tantivy` | Tantivy index directory |
| `PORT` | `3001` | HTTP server port |

Where `{data_dir}` = `$XDG_DATA_HOME/imageviz` (Linux), `~/Library/Application Support/imageviz` (macOS), or `./data` (fallback).

## Architecture Notes

- **Route assembly**: `backend/src/lib.rs::health_router()` builds a minimal router with only the health endpoint. `main.rs` nests stateful routes (config, media, search, events, stats) on top of it. All routes are under `/api/v1`. Integration tests reuse `health_router()` + the same nesting pattern.
- **DB lock strategy**: Uses an `r2d2` connection pool (max 10 connections, WAL-compatible, 5s busy timeout). The Tantivy reindex opens a **separate read-only connection** (WAL allows concurrent readers) so the API stays responsive during startup.
- **Thumbnail generation**: `spawn_blocking` for CPU-bound image work. Never blocks the async runtime.
- **Thumbnail concurrency**: A `DashMap` of per-key mutexes prevents duplicate generation when the same thumbnail is requested concurrently.
- **Cache eviction**: Background timer runs every 5 minutes. An inline fire-and-forget spawn handles cache bursts.
- **File serving**: `tokio::fs::File` + streaming — never loads a full file into memory. Range requests supported for video seeking.
- **File watcher**: `notify` + `notify-debouncer-mini` with 500ms debounce. In-memory `mpsc` channel decouples watcher from indexer. Must be kept alive (bind to `let _watcher = ...`).

## Test Conventions

- **Rust unit tests**: co-located via `#[path = "filename_test.rs"]` (e.g., `src/metadata/png_test.rs` beside `png.rs`).
- **Rust integration tests**: in `backend/tests/` using `tower::ServiceExt::oneshot` on the `Router` (in-process, no real server socket). Larger tests use `TestApp` from `tests/common/mod.rs` with in-memory SQLite + tempdir Tantivy + real `reqwest` client. See `create_test_app_with_search()`.
- **Test support**: `backend/src/test_support.rs` provides `fixture_path(name)` resolving to `test-fixtures/{name}` (only compiled under `cfg(test)`). Use `crate::test_support::fixture_path` in unit tests, `imageviz_backend::test_support::fixture_path` in integration tests.
- **Frontend unit/component tests**: co-located `__tests__/` dirs next to components. Vitest with jsdom, `@testing-library/react`, `@testing-library/jest-dom`.
- **Frontend integration tests**: use **MSW** (Mock Service Worker) — handlers live at `frontend/src/test-utils/msw-handlers.ts`. Render helpers at `test-utils/render-utils.tsx` wrap QueryClient + Jotai Provider.
- **Frontend test setup** (`frontend/src/setup-tests.ts`): mocks `EventSource` (jsdom doesn't implement it) and `VirtuosoGrid` from react-virtuoso (no real virtual-scroll DOM measurements in jsdom). Any test rendering `<App />` or `ThumbnailGrid` needs these mocks.
- **Frontend path alias**: `@/` maps to `./src/` (configured in both `vite.config.ts` and `tsconfig.json`). Use `import { ... } from '@/...'`.
- **Fixture generation**: Run `./scripts/generate-fixtures.sh` before running fixture-dependent tests (`cargo test -- --ignored`). Requires **ffmpeg** on PATH. Generates PNGs with ComfyUI-style tEXt chunks and short video files.
- **Test fixture files** (`.png`, `.webm`, `.mp4`, `.jpg`) are gitignored — only `.gitkeep` is committed.
- **E2E tests** (Playwright): in `frontend/e2e/`. Run with `npx playwright test` from `frontend/`. Requires `npx playwright install chromium` once. Playwright auto-starts both servers via `webServer` config in `playwright.config.ts`. Uses `workers: 1` (serial execution, shared backend state) with `retries: 1`.

## TDD Workflow (Mandatory)

Per task, in order:
1. **Write a failing test** that defines expected behavior
2. **Implement minimum code** to make it pass
3. **Refactor** while keeping tests green
4. **Add edge case tests** (empty inputs, nulls, errors, boundaries)
5. **Verify** with `cargo test` / `npm test` before marking complete

## CI Pipeline (GitHub Actions)

Runs on push and PR to `main`. Five parallel jobs:
1. **backend-lint** (15m): `cargo fmt --check` → `cargo clippy -- -D warnings`
2. **backend-test** (15m): `cargo test` (ffmpeg installed via apt)
3. **frontend-lint** (15m): `npm ci` → `npx prettier --check .` → `npx eslint .`
4. **frontend-test** (15m): `npm ci` → `npx tsc --noEmit` → `npx vitest run`
5. **frontend-e2e** (30m): `npm ci` → `cd ../scripts && bash generate-fixtures.sh` → `npx playwright install chromium` → `npx playwright test` (Playwright auto-starts backend via webServer)

## Git Conventions

- Commit often and early with helpful messages.
- Remote: `git@github.com:paulomarciano/imageviz.git`.
- **After every commit**, spawn a `CodeReviewer` subagent (new invocation, not a resumed session) to review the change. Pass the session context path so the reviewer applies the same standards used during implementation. Address critical/warning-level issues before additional commits.

## Reference: OpenCode Context System

Context files live under `.opencode/context/`. Key files agents should load:
- `core/standards/code-quality.md` — modular/functional patterns (load before any code work)
- `core/standards/test-coverage.md` — AAA pattern, coverage targets
- `core/workflows/component-planning.md` — how to decompose features
- `core/workflows/external-libraries-faq.md` — handling external deps

Task tracking uses `.opencode/skills/task-management/SKILL.md` — task JSONs live under `.tmp/tasks/`.
