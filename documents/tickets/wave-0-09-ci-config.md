# Wave 0.9 — Add CI Configuration (GitHub Actions)

| Field | Value |
|-------|-------|
| **Wave** | 0 — Project Scaffolding & CI |
| **Seq** | 09 |
| **Estimate** | 45 minutes |
| **Depends on** | 0.6 (lint), 0.7 (backend tests), 0.8 (frontend tests) |
| **Parallel** | No |

---

## Overview

Set up a GitHub Actions CI pipeline that runs on every push and pull request. The pipeline runs linting and tests for both backend and frontend. This ensures all future work is automatically validated.

## Prerequisites

- Linting configured for both projects (from 0.6)
- Tests passing for both projects (from 0.7, 0.8)
- `.github/workflows/` directory exists (from 0.1)

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 0 task table

## Deliverables

```
.github/workflows/
└── ci.yml                      # GitHub Actions workflow
```

## Acceptance Criteria (Pass/Fail)

- [ ] CI runs on push to any branch
- [ ] CI runs on pull requests
- [ ] CI workflow includes jobs for:
  - [ ] Backend: `cargo fmt --check`
  - [ ] Backend: `cargo clippy -- -D warnings`
  - [ ] Backend: `cargo test`
  - [ ] Frontend: `npx prettier --check .`
  - [ ] Frontend: `npx eslint .`
  - [ ] Frontend: `npx tsc --noEmit`
  - [ ] Frontend: `npx vitest run`
- [ ] Cache is configured for Rust `target/` and `~/.cargo`
- [ ] Cache is configured for `node_modules/`

## Implementation Notes

**ci.yml structure:**
```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  backend-lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - run: cargo fmt --check
        working-directory: backend
      - run: cargo clippy -- -D warnings
        working-directory: backend

  backend-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: backend
      - run: cargo test
        working-directory: backend

  frontend-lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '22'
          cache: 'npm'
          cache-dependency-path: frontend/package-lock.json
      - run: npm ci
        working-directory: frontend
      - run: npx prettier --check .
        working-directory: frontend
      - run: npx eslint .
        working-directory: frontend

  frontend-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '22'
          cache: 'npm'
          cache-dependency-path: frontend/package-lock.json
      - run: npm ci
        working-directory: frontend
      - run: npx tsc --noEmit
        working-directory: frontend
      - run: npx vitest run
        working-directory: frontend
```

**Important:** All four jobs (backend-lint, backend-test, frontend-lint, frontend-test) can run in parallel since they are independent.

**Adjust for project specifics:**
- If `frontend/package-lock.json` doesn't exist yet, use `npm install` instead of `npm ci`, or configure caching differently.
- If using `rust-cache` action doesn't work, fall back to manual caching of `~/.cargo/registry` and `backend/target/`.

## Test Strategy

- Push the workflow file and verify CI runs on GitHub
- Make a test PR to verify PR triggers work
- All jobs must pass on first run (lint and tests should already be passing from 0.6, 0.7, 0.8)
