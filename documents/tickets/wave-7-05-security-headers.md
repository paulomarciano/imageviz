# Wave 7.5 — Add Security Headers (Content-Security-Policy, etc.)

| Field | Value |
|-------|-------|
| **Wave** | 7 — Production Readiness & Hardening |
| **Seq** | 05 |
| **Estimate** | 45 minutes |
| **Depends on** | 0.2 (backend scaffold) |
| **Parallel** | Can run in parallel with other Wave 7 tasks |

---

## Overview

Add security HTTP headers to all responses, similar to what `helmet` provides in Express.js. This includes Content-Security-Policy, X-Content-Type-Options, X-Frame-Options, and other standard security headers.

## Prerequisites

- Backend with Axum (0.2)
- `tower-http` in Cargo.toml

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 7 task 7.5
- `.opencode/context/development/principles/api-design.md`

## Deliverables

```
backend/src/middleware/
├── mod.rs                       # Updated
└── security.rs                  # Security headers layer
```

## Acceptance Criteria (Pass/Fail)

- [ ] All responses include these security headers:
  - `X-Content-Type-Options: nosniff`
  - `X-Frame-Options: DENY` (or `SAMEORIGIN` for local tool)
  - `X-XSS-Protection: 0` (deprecated but harmless)
  - `Referrer-Policy: strict-origin-when-cross-origin`
  - `Permissions-Policy: camera=(), microphone=(), geolocation=()`
- [ ] `Content-Security-Policy` is reasonable for a local tool (not overly restrictive)
- [ ] Headers are applied via Tower middleware layer
- [ ] CORS headers are already handled by tower-http CORS layer (from 0.5)
- [ ] Security headers appear on error responses too (including 404, 500)

## Implementation Notes

**Security headers layer:**
```rust
use axum::{
    http::{HeaderMap, header},
    response::Response,
};
use tower::{Layer, Service};
use std::task::{Context, Poll};

#[derive(Clone)]
pub struct SecurityHeadersLayer;

impl<S> Layer<S> for SecurityHeadersLayer {
    type Service = SecurityHeadersService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SecurityHeadersService { inner }
    }
}

#[derive(Clone)]
pub struct SecurityHeadersService<S> {
    inner: S,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for SecurityHeadersService<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let future = self.inner.call(req);
        // We need to add headers to the response
        // This requires a map on the future
        // For simplicity, use tower_http::set_header::SetResponseHeaderLayer
        future
    }
}
```

**Simpler approach — use `tower_http::set_header`:**
```rust
use tower_http::set_header::SetResponseHeaderLayer;
use axum::http::header;

pub fn security_headers_layer() -> (
    SetResponseHeaderLayer<header::XContentTypeOptions>,
    SetResponseHeaderLayer<header::XFrameOptions>,
    SetResponseHeaderLayer<header::ReferrerPolicy>,
) {
    (
        SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            header::HeaderValue::from_static("nosniff"),
        ),
        SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            header::HeaderValue::from_static("SAMEORIGIN"),
        ),
        SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            header::HeaderValue::from_static("strict-origin-when-cross-origin"),
        ),
    )
}
```

**Or use axum's `Router::layer` with multiple layers:**
```rust
let app = Router::new()
    .layer(SetResponseHeaderLayer::overriding(
        header::X_CONTENT_TYPE_OPTIONS,
        "nosniff".parse().unwrap(),
    ))
    .layer(SetResponseHeaderLayer::overriding(
        header::X_FRAME_OPTIONS,
        "SAMEORIGIN".parse().unwrap(),
    ))
    .layer(SetResponseHeaderLayer::overriding(
        header::REFERRER_POLICY,
        "strict-origin-when-cross-origin".parse().unwrap(),
    ));
```

**Recommended CSP for a local tool (permissive — localhost only):**
```rust
SetResponseHeaderLayer::overriding(
    header::CONTENT_SECURITY_POLICY,
    "default-src 'self'; img-src 'self' data: blob:; media-src 'self' blob:; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'".parse().unwrap(),
)
```

## Test Strategy

```rust
#[tokio::test]
async fn test_security_headers_present() {
    let app = test_app().await;
    
    let response = app.get("/api/v1/health").send().await;
    
    let headers = response.headers();
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    assert!(headers.get("x-frame-options").is_some());
    assert!(headers.get("referrer-policy").is_some());
}
```
