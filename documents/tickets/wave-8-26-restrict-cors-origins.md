# Wave 8.26 — Restrict CORS to the Vite Dev Origin

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 26 |
| **Estimate** | 30 minutes |
| **Depends on** | 8.20 (main.rs middleware stack settled) |
| **Parallel** | No |
| **Source** | Code review §4 R9 (🔵) |

---

## Overview

`CorsLayer::permissive()` (`backend/src/main.rs:211`) lets **any** website the user visits call `http://127.0.0.1:3001` and read responses — including triggering thumbnail generation (CPU) and enumerating the library. The frontend is same-origin in production; permissive CORS is only needed for the Vite dev server.

Fix: restrict allowed origins to `http://localhost:5173` (+ `127.0.0.1` variant), configurable via env for non-standard dev setups — or omit the layer in release builds.

## Prerequisites

- 8.20 (middleware/router assembly finalized)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §4 R9
- `backend/src/main.rs:211`
- tower-http docs — `CorsLayer`, `AllowOrigin::list`
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/main.rs            # explicit origin allowlist (env-overridable)
backend/src/config/settings.rs # CORS_ALLOW_ORIGINS env (optional)
```

## Acceptance Criteria (Pass/Fail)

- [ ] `GET /api/v1/...` from origin `http://localhost:5173` succeeds with CORS headers (Vite dev works unchanged: `./scripts/dev.sh` smoke)
- [ ] Cross-origin request from any other origin gets no `Access-Control-Allow-Origin` grant (browser blocks reads)
- [ ] Preflight `OPTIONS` for the methods actually used (GET, PUT, POST, DELETE) succeeds only for allowed origins
- [ ] Production/same-origin usage unaffected (no CORS headers required for same-origin; security headers from 7.5 intact)
- [ ] Env override `CORS_ALLOW_ORIGINS` (comma-separated) honored; default = `http://localhost:5173,http://127.0.0.1:5173`
- [ ] Env var documented in README/AGENTS env table
- [ ] `cargo test` green

## Implementation Notes

```rust
let origins: Vec<HeaderValue> = env_origins().iter().map(|o| o.parse().unwrap()).collect();
let cors = CorsLayer::new()
    .allow_origin(origins)
    .allow_methods([Method::GET, Method::PUT, Method::POST, Method::DELETE]);
```

- Keep it KISS: a static allowlist, no regex origins, no credentials support (the app doesn't use cookies cross-origin).
- If the e2e suite (Playwright) drives the app through the Vite proxy, same-origin applies — no CORS involvement; verify the e2e job stays green.

## Test Strategy

- Integration test: request with `Origin: http://evil.example` → no `access-control-allow-origin` in response; `Origin: http://localhost:5173` → header present.
- OPTIONS preflight test for PUT with allowed + disallowed origins.
