# Wave 0.7 — Write Backend Health Endpoint Unit Tests

| Field | Value |
|-------|-------|
| **Wave** | 0 — Project Scaffolding & CI |
| **Seq** | 07 |
| **Estimate** | 30 minutes |
| **Depends on** | 0.4 (health endpoint) |
| **Parallel** | Can run in parallel with 0.5, 0.6, 0.8 |

---

## Overview

Write unit tests for the health-check endpoint using Axum's built-in test utilities. This establishes the backend testing pattern (AAA: Arrange → Act → Assert) and verifies the health route responds correctly.

## Prerequisites

- `backend/src/routes/health.rs` exists (from 0.4)
- `backend/src/routes/mod.rs` exists (from 0.4)

## Reference Files

- `documents/plans/development-plan.md` — §7 Testing Strategy, §7.2 Backend Testing (co-located tests)
- `.opencode/context/core/standards/test-coverage.md` — AAA pattern, coverage goals

## Deliverables

```
backend/
├── src/routes/health.rs        # Updated: make routes() testable
└── tests/
    ├── common/
    │   └── mod.rs               # Test helpers (shared)
    └── health_test.rs           # Health endpoint integration test
```

## Acceptance Criteria (Pass/Fail)

- [ ] `cargo test` passes — all health tests green
- [ ] Test covers: GET `/api/v1/health` returns 200 with correct JSON body
- [ ] Test covers: response includes `"status": "ok"` and `"version"` fields
- [ ] Test uses AAA pattern (Arrange → Act → Assert)
- [ ] No actual network connections — uses Axum's `Router::oneshot` or test client

## Implementation Notes

**Using Axum's test utilities:**
```rust
// tests/health_test.rs
use axum::{body::Body, http::{Request, StatusCode}};
use tower::ServiceExt;
use imageviz_backend::app; // or wherever the router is assembled

#[tokio::test]
async fn health_check_returns_ok() {
    // Arrange
    let app = app::create_app(); // Router factory function (extract from main.rs)

    // Act
    let response = app
        .oneshot(Request::builder()
            .uri("/api/v1/health")
            .body(Body::empty())
            .unwrap())
        .await
        .unwrap();

    // Assert
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn health_check_body_contains_status() {
    // ... test JSON body contains expected fields
}
```

**Important:** You'll need to extract the Router assembly into a reusable function (e.g., `app::create_app()`) so tests can build the router independently of `main.rs`.

**Test naming convention** (from test-coverage.md):
- `health_check_returns_ok` — describes behavior, not implementation
- `health_check_returns_json_with_status_and_version`

## Test Strategy

- Unit/integration test using Axum test utilities (no external HTTP server)
- `cargo test` must pass
- This pattern will be reused for all subsequent endpoint tests
