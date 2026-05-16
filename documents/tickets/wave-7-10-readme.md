# Wave 7.10 — Write Project README

| Field | Value |
|-------|-------|
| **Wave** | 7 — Production Readiness & Hardening |
| **Seq** | 10 |
| **Estimate** | 2 hours |
| **Depends on** | All waves (complete project) |
| **Parallel** | No (final documentation) |

---

## Overview

Write a comprehensive README.md for the project. This is the primary entry point for developers and users. It covers project overview, setup instructions, development workflow, architecture overview, and contribution guidelines.

## Prerequisites

- All features implemented (Waves 0–7)
- All tests passing
- Build scripts created (7.8)

## Reference Files

- `documents/plans/development-plan.md` — §1 Project Overview, §2 Tech Stack, §3 API Contract, §12 Project Structure
- `.opencode/context/core/standards/documentation.md` — README structure template
- `.opencode/context/openagents-repo/guides/github-issues-workflow.md` — optional

## Deliverables

```
README.md                        # Updated from minimal version (0.1)
```

## Acceptance Criteria (Pass/Fail)

- [ ] README includes all standard sections:
  - **Project name + one-line description**
  - **Features** (bullet list of key capabilities)
  - **Screenshot** (optional — a screenshot of the app running)
  - **Prerequisites** (Rust, Node.js, ffmpeg)
  - **Quick Start** (clone, install, run)
  - **Development** (commands for dev, test, lint, build)
  - **Architecture** (high-level diagram or description)
  - **API Reference** (link to full API contract or summary table)
  - **Configuration** (environment variables, watched folders)
  - **Testing** (how to run tests)
  - **Contributing** (link to contributing guide or brief guidelines)
  - **License**
- [ ] Quick start section: user can clone, install, and run in < 5 minutes
- [ ] Development commands table matches the AGENTS.md commands table
- [ ] Architecture diagram (can be ASCII art or a link to a diagram)
- [ ] Links to documents/plans/development-plan.md for full details
- [ ] Clear, concise language — scannable in < 2 minutes

## Implementation Notes

**README structure:**
```markdown
# ImageViz

Browser-based image and video visualization for large datasets (100K–1M media files).  
Built for exploring ComfyUI output with real-time folder watching, full-text metadata search, and infinite scroll.

## Features

- 🖼️ **Infinite scroll grid** — Browse 100K+ media files smoothly with virtualized rendering
- 🔍 **Full-text search** — Search across filenames and embedded metadata (ComfyUI prompts/workflows)
- 📂 **Folder watching** — Configure watched folders; new files appear in real-time via SSE
- 👁️ **Detail viewer** — Click-to-preview with zoom/pan for images, playback for videos
- 🖱️ **Drag-and-drop** — Drag files directly to external applications (file explorer, editors)
- ⚡ **Fast** — Cursor-based pagination (O(log n)), WebP thumbnails, lazy loading
- 🎨 **Desktop-first** — Optimized for vertical (1080×1920) and horizontal (1920×1080) screens

## Prerequisites

- **Rust** (stable, edition 2024) — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Node.js** 22+ — `https://nodejs.org/`
- **ffmpeg** — For video metadata and thumbnail extraction
  - macOS: `brew install ffmpeg`
  - Linux: `apt install ffmpeg` or `dnf install ffmpeg`

## Quick Start

```bash
# Clone
git clone git@github.com:paulomarciano/imageviz.git
cd imageviz

# Install dependencies
cd backend && cargo build
cd ../frontend && npm install
cd ..

# Run (both servers)
./scripts/dev.sh
```

Open http://localhost:5173 — configure a watched folder in Settings.

## Development

### Commands

| Action | Backend | Frontend |
|--------|---------|----------|
| Run dev | `cargo run` (port 3001) | `npm run dev` (Vite proxies to :3001) |
| Run tests | `cargo test` | `npx vitest run` |
| Lint | `cargo clippy` + `cargo fmt --check` | `npx eslint .` + `npx prettier --check .` |
| Type check | `cargo check` | `npx tsc --noEmit` |
| Build | `cargo build --release` | `npm run build` |

### Project Structure

```
imageviz/
├── backend/           # Rust + Axum + SQLite + Tantivy
├── frontend/          # React 19 + Vite 8 + TypeScript + Tailwind
├── test-fixtures/     # Sample media files for testing
├── documents/         # Planning documents, tickets
└── scripts/           # dev.sh, build.sh
```

## Architecture

```
Browser (React) ←→ Rust Backend (Axum)
                      ├── SQLite (metadata)
                      ├── Tantivy (full-text search)
                      ├── File Watcher (notify)
                      └── Thumbnail Generator (image + ffmpeg)
```

## API

See [API Contract](http://localhost:3001/api/v1) — base URL. Key endpoints:

- `GET /api/v1/media` — List media (cursor pagination)
- `GET /api/v1/search?q=...` — Full-text search
- `GET /api/v1/events` — SSE real-time updates
- `GET /api/v1/config` — Watched folder configuration

Full API contract: [documents/plans/development-plan.md §3](documents/plans/development-plan.md#3-api-contract)

## Configuration

Environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | 3001 | Backend server port |
| `IMAGEVIZ_CACHE_DIR` | `.data/thumbnails` | Thumbnail cache directory |
| `THUMBNAIL_CONCURRENCY` | 4 | Max concurrent thumbnail generations |
| `THUMBNAIL_CACHE_MAX_MB` | 2000 | Max thumbnail cache size in MB |

## Testing

```bash
# Backend
cd backend && cargo test

# Frontend
cd frontend && npx vitest run

# E2E
cd frontend && npx playwright test
```

## License

MIT
```

## Test Strategy

- Manual review: verify all links work, commands are correct
- Ask another developer to follow the Quick Start instructions
- Verify the README renders correctly on GitHub (preview)
