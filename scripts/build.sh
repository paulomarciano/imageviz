#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}=== Building ImageViz ===${NC}"
echo ""

# Build backend
echo -e "${YELLOW}[backend]${NC} Building release..."
cd "$ROOT_DIR/backend"
cargo build --release
BACKEND_SIZE=$(du -sh target/release/imageviz-backend 2>/dev/null | cut -f1 || echo "unknown")
echo -e "${GREEN}[backend]${NC} Built: target/release/imageviz-backend (${BACKEND_SIZE})"
echo ""

# Build frontend
echo -e "${YELLOW}[frontend]${NC} Building production bundle..."
cd "$ROOT_DIR/frontend"
npm ci
npm run build
FRONTEND_SIZE=$(du -sh dist 2>/dev/null | cut -f1 || echo "unknown")
echo -e "${GREEN}[frontend]${NC} Built: dist/ (${FRONTEND_SIZE})"
echo ""

# Summary
echo -e "${GREEN}=== Build Complete ===${NC}"
echo "  Backend:  backend/target/release/imageviz-backend (${BACKEND_SIZE})"
echo "  Frontend: frontend/dist/ (${FRONTEND_SIZE})"
echo ""
echo "To run:"
echo "  ./scripts/dev.sh"
echo "  # or directly:"
echo "  ./backend/target/release/imageviz-backend"
echo "  # (frontend is static — serve via backend or nginx)"
