# Wave 7.8 — Create Production Build Scripts

| Field | Value |
|-------|-------|
| **Wave** | 7 — Production Readiness & Hardening |
| **Seq** | 08 |
| **Estimate** | 1 hour |
| **Depends on** | 0.2 (backend), 0.3 (frontend) |
| **Parallel** | Can run in parallel with other Wave 7 tasks |

---

## Overview

Create shell scripts for common development and build operations: starting both dev servers, building for production, and running all tests. These scripts simplify the developer experience and CI configuration.

## Prerequisites

- Backend scaffolded (0.2) with `cargo build` working
- Frontend scaffolded (0.3) with `npm run build` working
- `scripts/` directory exists (0.1)

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 7 task 7.8, §Commands table, §12 scripts directory
- `.opencode/context/core/standards/documentation.md`

## Deliverables

```
scripts/
├── dev.sh                        # Start both dev servers
└── build.sh                      # Production build
```

## Acceptance Criteria (Pass/Fail)

**dev.sh:**
- [ ] Starts backend dev server (`cargo run`) in background
- [ ] Waits for backend to be ready (health check)
- [ ] Starts frontend dev server (`npm run dev`) in foreground
- [ ] `Ctrl+C` kills both processes (trap EXIT)
- [ ] Output from both servers is interleaved with prefixes (`[backend]`, `[frontend]`)

**build.sh:**
- [ ] Runs `cargo build --release` for backend
- [ ] Runs `npm run build` for frontend
- [ ] Fails early if any step fails (`set -e`)
- [ ] Prints build summary (artifact sizes, locations)

## Implementation Notes

**dev.sh:**
```bash
#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

cleanup() {
    echo -e "\n${BLUE}Shutting down...${NC}"
    kill $BACKEND_PID 2>/dev/null || true
    wait $BACKEND_PID 2>/dev/null || true
    echo "All servers stopped."
}

trap cleanup EXIT INT TERM

echo -e "${GREEN}Starting ImageViz development servers...${NC}"

# Start backend
echo "Starting backend on port 3001..."
cd "$ROOT_DIR/backend"
cargo run &
BACKEND_PID=$!

# Wait for backend health check
echo "Waiting for backend..."
for i in {1..30}; do
    if curl -s http://localhost:3001/api/v1/health > /dev/null 2>&1; then
        echo -e "${GREEN}Backend ready!${NC}"
        break
    fi
    sleep 1
done

# Start frontend
echo "Starting frontend on port 5173..."
cd "$ROOT_DIR/frontend"
npm run dev

# If frontend exits, kill backend
kill $BACKEND_PID 2>/dev/null || true
```

**build.sh:**
```bash
#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

GREEN='\033[0;32m'
NC='\033[0m'

echo -e "${GREEN}=== Building ImageViz ===${NC}"

# Build backend
echo ""
echo "Building backend (release)..."
cd "$ROOT_DIR/backend"
cargo build --release
BACKEND_SIZE=$(du -sh target/release/imageviz-backend | cut -f1)
echo -e "${GREEN}Backend built: target/release/imageviz-backend (${BACKEND_SIZE})${NC}"

# Build frontend
echo ""
echo "Building frontend..."
cd "$ROOT_DIR/frontend"
npm ci
npm run build
FRONTEND_SIZE=$(du -sh dist | cut -f1)
echo -e "${GREEN}Frontend built: dist/ (${FRONTEND_SIZE})${NC}"

# Summary
echo ""
echo -e "${GREEN}=== Build Complete ===${NC}"
echo "Backend:  backend/target/release/imageviz-backend (${BACKEND_SIZE})"
echo "Frontend: frontend/dist/ (${FRONTEND_SIZE})"
echo ""
echo "To run:"
echo "  ./backend/target/release/imageviz-backend"
echo "  (frontend is static — serve via nginx or backend)"
```

## Test Strategy

- Manual: run `./scripts/dev.sh`, verify both servers start
- Manual: press Ctrl+C, verify both servers stop
- Manual: run `./scripts/build.sh`, verify artifacts produced
- CI: verify scripts exit with 0
