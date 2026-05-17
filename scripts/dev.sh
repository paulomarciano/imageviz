#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

PORT="${PORT:-3001}"

cleanup() {
    echo ""
    echo -e "${BLUE}Shutting down...${NC}"
    kill "$BACKEND_PID" 2>/dev/null || true
    kill "$FRONTEND_PID" 2>/dev/null || true
    wait "$BACKEND_PID" 2>/dev/null || true
    wait "$FRONTEND_PID" 2>/dev/null || true
    echo -e "${GREEN}All servers stopped.${NC}"
}

trap cleanup EXIT INT TERM

echo -e "${GREEN}Starting ImageViz development servers...${NC}"
echo ""

# Start backend
echo -e "${YELLOW}[backend]${NC} Starting on port ${PORT}..."
cd "$ROOT_DIR/backend"
cargo run &
BACKEND_PID=$!

# Wait for backend health check
echo -e "${YELLOW}[backend]${NC} Waiting for backend to be ready..."
for i in $(seq 1 30); do
    if curl -s "http://localhost:${PORT}/api/v1/health" > /dev/null 2>&1; then
        echo -e "${GREEN}[backend]${NC} Ready!"
        break
    fi
    if ! kill -0 "$BACKEND_PID" 2>/dev/null; then
        echo -e "${BLUE}[backend]${NC} Process exited unexpectedly"
        exit 1
    fi
    sleep 1
done

# Start frontend
echo -e "${YELLOW}[frontend]${NC} Starting on port 5173..."
cd "$ROOT_DIR/frontend"
npm run dev &
FRONTEND_PID=$!

# Wait for any process to exit
wait
