# Contributing to ImageViz

## Development Environment Setup

- **Rust** (latest stable) — Install via [rustup](https://rustup.rs/)
- **Node.js** (latest LTS, >= 22) — Install via [nvm](https://nvm.sh/) or your package manager
- **ffmpeg** on PATH — Required for video metadata extraction and thumbnail generation (`apt install ffmpeg` / `brew install ffmpeg`)

Frontend package manager is **npm** (not pnpm/yarn).

## Quick Start

```bash
git clone git@github.com:paulomarciano/imageviz.git
cd imageviz

# Backend
cd backend && cargo build && cd ..
cargo test

# Frontend
cd frontend
npm install
npm test
```

### Running Development Servers

Use the convenience script (starts both servers with one command):

```bash
./scripts/dev.sh
```

Or run them manually:

```bash
# Terminal 1: Backend (Rust, port 3001)
cd backend && cargo run

# Terminal 2: Frontend (Vite, port 5173, proxies /api → :3001)
cd frontend && npm run dev
```

Open **http://localhost:5173** in your browser.

## TDD Workflow

Per task, in order:

1. **Write a failing test** that defines expected behavior
2. **Implement minimum code** to make it pass
3. **Refactor** while keeping tests green
4. **Add edge case tests** (empty inputs, nulls, errors, boundaries)
5. **Verify** with `cargo test` / `npm test` before marking complete

## Code Style

### Backend (Rust)

| Check | Command |
|-------|---------|
| Format | `cargo fmt` (max_width=100, tab_spaces=4, edition=2024) |
| Lint | `cargo clippy -- -D warnings` |
| Type check | `cargo check` |

- No warnings allowed in CI
- Co-located unit tests via `#[path = "..._test.rs"]` (e.g., `src/metadata/png_test.rs` beside `png.rs`)
- Integration tests in `backend/tests/` using `tower::ServiceExt::oneshot` on the `Router` (in-process, no real server socket)
- Larger tests use `TestApp` from `tests/common/mod.rs` with in-memory SQLite + tempdir Tantivy

### Frontend (TypeScript/React)

| Check | Command |
|-------|---------|
| Format | `npx prettier --check .` (singleQuote, trailingComma all, semi, printWidth 100, tabWidth 2) |
| Lint | `npm run lint` (ESLint with typescript-eslint; `no-unused-vars` is error, `no-explicit-any` is warn) |
| Type check | `npm run typecheck` (`tsc --noEmit`) |

- Co-located tests in `__tests__/` directories next to components
- Vitest with jsdom, `@testing-library/react`, `@testing-library/jest-dom`
- Integration tests use **MSW** (Mock Service Worker) — handlers at `src/test-utils/msw-handlers.ts`
- Path alias: `@/` maps to `./src/`
- Test setup at `src/setup-tests.ts`: mocks `EventSource` and `VirtuosoGrid` from react-virtuoso

### Generating Test Fixtures

```bash
./scripts/generate-fixtures.sh
```

Requires **ffmpeg** on PATH. Generates PNGs with ComfyUI-style tEXt chunks and short video files.

### Running E2E Tests (Playwright)

```bash
cd frontend
npx playwright install chromium
npx playwright test
```

Playwright auto-starts both servers via `webServer` config in `playwright.config.ts`.

## Commit Message Conventions

```
feat: new feature
fix: bug fix
refactor: code change without feature/fix
docs: documentation only
perf: performance improvement
test: test additions/changes
chore: tooling, CI, dependencies
```

- Commit often and early with helpful messages.
- **After every commit**, run the `CodeReviewer` subagent to review the change.

## PR Workflow

1. Create a branch from `main`
2. Make changes with TDD workflow
3. Ensure CI passes (4 parallel jobs, 15-min timeout each):
   - **backend-lint**: `cargo fmt --check` → `cargo clippy -- -D warnings`
   - **backend-test**: `cargo test`
   - **frontend-lint**: `npm ci` → `npx prettier --check .` → `npx eslint .`
   - **frontend-test**: `npm ci` → `npx tsc --noEmit` → `npx vitest run`
4. Create PR with description of changes
5. Request review
6. Merge after approval

## Project Conventions

- **TDD-first**: Tests before implementation
- **Co-located tests**: Tests live next to source files
- **AAA pattern**: Arrange → Act → Assert
- **Modular code**: Small functions (< 50 lines), single responsibility
- **Functional style**: Pure functions, immutability, composition
- **Cursor-based pagination**: Use `WHERE (created_at, id) < (?, ?)` (O(log n)), never offset-based
- **Thumbnail generation**: Uses `spawn_blocking` for CPU-bound image work — never blocks the async runtime
- **File serving**: `tokio::fs::File` + streaming — never loads a full file into memory
- **No auth** for v1. Desktop-first, not mobile.

## AI-Assisted Development

See [AGENTS.md](AGENTS.md) for AI context files and subagent configuration.

The `.opencode/context/` directory contains coding standards, test patterns, and workflows used by AI agents during development:

- `core/standards/code-quality.md` — modular/functional patterns
- `core/standards/test-coverage.md` — AAA pattern, coverage targets
- `core/workflows/component-planning.md` — how to decompose features
- `core/workflows/external-libraries-faq.md` — handling external deps

## Where to Get Help

- **GitHub Issues**: [https://github.com/paulomarciano/imageviz/issues](https://github.com/paulomarciano/imageviz/issues)
- **Project README**: [README.md](README.md)
- **Architecture & Plan**: [documents/plans/development-plan.md](documents/plans/development-plan.md)
