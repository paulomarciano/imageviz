# ImageViz — Agent Instructions

## Project Status

This project is at **Wave 0** (scaffolding). The monorepo (`backend/`, `frontend/`) does not exist yet.
Only the development plan and `.gitignore` are committed.

## Source of Truth

**`documents/plans/development-plan.md`** — architecture, API contract, DB schema, Tantivy schema,
development waves (0–7), task breakdown, dependency graph, testing strategy, and performance budgets.
Read this before any implementation.

## Stack (Non-Obvious Choices)

- **Backend**: Rust + Axum + Tokio. SQLite in WAL mode for concurrency. Tantivy for full-text search.
  Cursor-based pagination (not offset — `WHERE (created_at, id) < (?, ?)` is O(log n)).
- **Frontend**: React 19 + Vite 8 + TypeScript strict. react-virtuoso for virtual scroll (not TanStack Virtual).
  react-dnd for OS-level drag (not dnd-kit, which is internal-only). Jotai for state. TanStack Query for data.
- **Video thumbnails**: ffmpeg invoked as a subprocess (not a Rust crate).
- **Thumbnail caching**: Content-addressed on-disk. Use `spawn_blocking` for CPU-bound image work.
- **No auth** for v1. Desktop-first (vertical + horizontal screens), not mobile.

## Monorepo Layout (Target)

```
backend/           # Rust, Cargo.toml at root
  src/             # Co-locate *_test.rs next to source
  tests/           # Integration tests (reqwest + real SQLite + temp files)
frontend/          # React + Vite
  src/
    __tests__/     # Integration tests
    components/**/__tests__/  # Co-located component tests
  e2e/             # Playwright E2E
test-fixtures/     # Sample ComfyUI PNGs + test videos
scripts/
  dev.sh           # Start both dev servers
  build.sh         # Production build
```

## Commands

| Action | Backend | Frontend |
|--------|---------|----------|
| Run dev | `cargo run` (port 3001) | `npm run dev` (Vite proxies to :3001) |
| Run tests | `cargo test` | `npm test` (Vitest) |
| Run single test | `cargo test test_name` | `npx vitest -t "test name"` |
| Lint | `cargo clippy` + `cargo fmt --check` | `npx eslint .` + `npx prettier --check .` |
| Type check | `cargo check` | `npx tsc --noEmit` |
| Build | `cargo build --release` | `npm run build` |

Package manager is `npm` (not pnpm/yarn).

## API (Key Facts)

Base URL: `http://localhost:3001/api/v1`

- Cursor pagination with `cursor` (ISO 8601 date) + `cursor_id` (UUID tiebreaker). Default limit: 100, max: 500.
- SSE at `GET /events` for real-time file system updates.
- Thumbnails served as WebP at `GET /media/:id/thumbnail`.
- Original files streamed at `GET /media/:id/file` (never loaded into memory; Range header supported for video seeking).

See development plan §3 for full API contract.

## TDD Workflow (Mandatory)

Per task, in order:
1. **Write a failing test** that defines expected behavior
2. **Implement minimum code** to make it pass
3. **Refactor** while keeping tests green
4. **Add edge case tests** (empty inputs, nulls, errors, boundaries)
5. **Verify** with `cargo test` / `npm test` before marking complete

Tests must pass before a ticket is marked done.

## Test Conventions

- **Rust unit tests**: co-located `*_test.rs` next to source (e.g., `src/metadata/png_test.rs` beside `png.rs`)
- **Rust integration tests**: in `tests/` directory (requires `reqwest` + tempfile + real SQLite)
- **Frontend unit/component tests**: `__tests__/` directories next to the code being tested
- **Frontend integration tests**: `__tests__/integration/` with MSW for API mocking
- **Test fixtures**: real ComfyUI PNGs (3–5 files) in `test-fixtures/`; helper modules at `backend/tests/common/mod.rs` and `frontend/src/test-utils/`

## Git Conventions

- Commit often and early with helpful messages
- All changes committed before marking work complete
- Remote: `git@github.com:paulomarciano/imageviz.git`
- **After every commit**, spawn a `CodeReviewer` subagent with a fresh context window to review the change.
  The reviewer reports findings back to the main agent; address any issues before proceeding.

## Reference: Context System

OpenCode context files live under `.opencode/context/`. Key files agents load:
- `core/standards/code-quality.md` — modular/functional patterns (loaded before any code work)
- `core/standards/test-coverage.md` — AAA pattern, coverage targets
- `core/workflows/component-planning.md` — how to decompose features
- `core/workflows/external-libraries-faq.md` — handling external deps
- `openagents-repo/guides/external-libraries-workflow.md` — fetching external library docs
