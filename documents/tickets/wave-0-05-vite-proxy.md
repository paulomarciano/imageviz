# Wave 0.5 — Set Up Vite Proxy to Backend

| Field | Value |
|-------|-------|
| **Wave** | 0 — Project Scaffolding & CI |
| **Seq** | 05 |
| **Estimate** | 15 minutes |
| **Depends on** | 0.4 (health endpoints) |
| **Parallel** | No |

---

## Overview

Configure Vite's dev server to proxy `/api` requests to the Rust backend running on port 3001. This eliminates CORS issues during development and allows the frontend to call backend endpoints using relative URLs.

## Prerequisites

- Backend running on port 3001 (from 0.2/0.4)
- Frontend Vite config exists (from 0.3)

## Reference Files

- `documents/plans/development-plan.md` — §3.1 Base URL: `http://localhost:3001/api/v1`
- `frontend/vite.config.ts` — existing file to modify

## Deliverables

```
frontend/vite.config.ts   # Updated with proxy configuration
```

## Acceptance Criteria (Pass/Fail)

- [ ] `npm run dev` proxies `GET /api/v1/health` to `http://localhost:3001/api/v1/health`
- [ ] Frontend can fetch `/api/v1/health` without CORS errors
- [ ] `useHealth` hook (from 0.4) returns `status: "ok"` when both servers are running
- [ ] WebSocket/SSE proxying is configured for future `/events` endpoint (optional at this stage)

## Implementation Notes

Add to `vite.config.ts`:
```typescript
export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://localhost:3001',
        changeOrigin: true,
      },
      '/events': {
        target: 'http://localhost:3001',
        changeOrigin: true,
        ws: true,  // For SSE keep-alive
      },
    },
  },
});
```

## Test Strategy

- Start backend: `cargo run` (in `backend/`)
- Start frontend: `npm run dev` (in `frontend/`)
- Open browser DevTools → Network → verify `/api/v1/health` returns 200
- No automated test needed — validated by manual integration check
