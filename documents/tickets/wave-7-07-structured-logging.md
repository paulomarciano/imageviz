# Wave 7.7 — Add Structured Logging (Request ID, Duration, Status)

| Field | Value |
|-------|-------|
| **Wave** | 7 — Production Readiness & Hardening |
| **Seq** | 07 |
| **Estimate** | 1 hour |
| **Depends on** | 0.2 (backend scaffold) |
| **Parallel** | Can run in parallel with other Wave 7 tasks |

---

## Overview

Add structured logging middleware that logs each HTTP request with a unique request ID, method, path, status code, and duration. This enables debugging and monitoring in production.

## Prerequisites

- Backend with tracing crate (already in Cargo.toml from 0.2)
- `tower-http` trace feature

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 7 task 7.7
- `.opencode/context/development/principles/api-design.md`

## Deliverables

```
backend/src/middleware/
├── mod.rs                       # Updated
└── logging.rs                   # Logging middleware
```

## Acceptance Criteria (Pass/Fail)

- [ ] Each request is logged with:
  - Unique request ID (UUID v4 or ULID)
  - HTTP method and path
  - Response status code
  - Request duration in milliseconds
  - Optional: user agent, IP address
- [ ] Request ID is generated on the server (not trusted from client)
- [ ] Request ID is returned to the client in `X-Request-Id` response header
- [ ] Structured (JSON) logging format in production, human-readable in development
- [ ] Log level: INFO for normal requests, WARN for 4xx, ERROR for 5xx
- [ ] Sensitive data is NOT logged (request bodies, query strings with secrets)

## Implementation Notes

**Using `tower_http::trace`:**
```rust
use tower_http::trace::{TraceLayer, DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse};
use tracing::Level;
use uuid::Uuid;
use axum::http::{Request, HeaderMap};
use std::time::Instant;

pub fn logging_layer() -> TraceLayer<
    impl tower_http::classify::MakeClassifier,
    impl Fn(&Request<Body>) -> tracing::Span + Clone,
    impl Fn(&Request<Body>, &tracing::Span) + Clone,
    impl Fn(&Response<Body>, Duration, &tracing::Span) + Clone,
> {
    TraceLayer::new_for_http()
        .make_span_with(|request: &Request<Body>| {
            let request_id = Uuid::new_v4().to_string();
            
            tracing::info_span!(
                "request",
                method = %request.method(),
                uri = %request.uri().path(),
                request_id = %request_id,
            )
        })
        .on_request(|request: &Request<Body>, _span: &tracing::Span| {
            tracing::info!("→ {} {}", request.method(), request.uri().path());
        })
        .on_response(|response: &Response<Body>, latency: Duration, _span: &tracing::Span| {
            let status = response.status();
            let level = if status.is_server_error() {
                Level::ERROR
            } else if status.is_client_error() {
                Level::WARN
            } else {
                Level::INFO
            };
            
            tracing::event!(
                level,
                "← {} {} {}ms",
                status.as_u16(),
                status.canonical_reason().unwrap_or(""),
                latency.as_millis(),
            );
        })
}
```

**Adding request ID to response header:**
```rust
use tower_http::request_id::{MakeRequestId, SetRequestIdLayer};
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct MakeRequestUuid;

impl MakeRequestId for MakeRequestUuid {
    fn make_request_id<B>(&mut self, _request: &Request<B>) -> Option<http::HeaderValue> {
        let id = Uuid::new_v4().to_string();
        http::HeaderValue::from_str(&id).ok()
    }
}

// In router assembly:
let app = Router::new()
    .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
    .layer(logging_layer());
```

**Structured (JSON) logging for production:**
```rust
use tracing_subscriber::fmt::format::FmtSpan;

// In main.rs
tracing_subscriber::fmt()
    .with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "imageviz_backend=info,tower_http=info".into()),
    )
    .json() // JSON format for structured logging
    .flatten_event(true)
    .init();
```

## Test Strategy

- Manual: start server, make requests, verify structured log output
- Verify `X-Request-Id` header in response
- Verify duration is logged for each request
- Verify error responses log at ERROR level
