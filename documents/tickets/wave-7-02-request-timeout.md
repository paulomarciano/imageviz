# Wave 7.2 — Add Request Timeout Middleware

| Field | Value |
|-------|-------|
| **Wave** | 7 — Production Readiness & Hardening |
| **Seq** | 02 |
| **Estimate** | 30 minutes |
| **Depends on** | 0.2 (backend scaffold) |
| **Parallel** | Can run in parallel with other Wave 7 tasks |

---

## Overview

Add request timeout middleware to the Axum server. Requests that exceed the timeout (e.g., 60 seconds) are automatically cancelled and return HTTP 408 Request Timeout. This prevents resource leaks from hung connections.

## Prerequisites

- Backend with Axum (0.2)
- `tower-http` with `timeout` feature

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 7 task 7.2
- `.opencode/context/development/principles/api-design.md`

## Deliverables

```
backend/src/middleware/
├── mod.rs                       # Middleware module
└── timeout.rs                   # Timeout middleware layer
```

## Acceptance Criteria (Pass/Fail)

- [ ] All routes wrapped with timeout middleware (default: 60 seconds)
- [ ] Requests exceeding timeout return 408 Request Timeout
- [ ] Timeout is configurable per-route (longer for file uploads, shorter for health check)
- [ ] SSE endpoint (`/events`) uses a longer timeout (it's a persistent connection)
- [ ] Thumbnail generation endpoint uses longer timeout (CPU-bound work)
- [ ] Timeout value is configurable via environment variable

## Implementation Notes

```rust
use tower_http::timeout::TimeoutLayer;
use std::time::Duration;
use axum::{Router, error_handling::HandleErrorLayer};
use tower::ServiceBuilder;

pub fn timeout_layer() -> TimeoutLayer {
    TimeoutLayer::new(Duration::from_secs(60))
}

// Per-route configuration
pub fn build_router() -> Router {
    let default_timeout = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|_| async {
            (StatusCode::REQUEST_TIMEOUT, "Request timed out")
        }))
        .layer(TimeoutLayer::new(Duration::from_secs(60)));

    let sse_timeout = ServiceBuilder::new()
        .layer(TimeoutLayer::new(Duration::from_secs(3600))); // 1 hour for SSE

    let thumbnail_timeout = ServiceBuilder::new()
        .layer(TimeoutLayer::new(Duration::from_secs(120))); // 2 minutes for thumbnail gen

    Router::new()
        .route("/health", get(health_check))
        .route("/events", get(sse_handler).layer(sse_timeout))
        .route("/media/{id}/thumbnail", get(get_thumbnail).layer(thumbnail_timeout))
        .layer(default_timeout) // Default timeout for all routes
}
```

**Environment variable:**
```rust
use std::env;

fn get_timeout_seconds() -> u64 {
    env::var("REQUEST_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60)
}
```
