# Wave 0.4 — Add Health-Check Endpoints

| Field | Value |
|-------|-------|
| **Wave** | 0 — Project Scaffolding & CI |
| **Seq** | 04 |
| **Estimate** | 30 minutes |
| **Depends on** | 0.2 (Rust backend), 0.3 (React frontend) |
| **Parallel** | No |

---

## Overview

Add a `/api/v1/health` endpoint on the backend and a corresponding hook on the frontend to verify connectivity between the two services. This is the first integration point between backend and frontend.

## Prerequisites

- Backend running on port 3001 (from 0.2)
- Frontend dev server with Vite proxy (will be configured in 0.5)

## Reference Files

- `documents/plans/development-plan.md` — §3 API Contract (base URL `http://localhost:3001/api/v1`), §12 project structure
- `.opencode/context/development/principles/api-design.md` — REST patterns, status codes

## Deliverables (Backend)

```
backend/src/
├── main.rs                   # Updated: mount health route
└── routes/
    ├── mod.rs                # Route module declarations
    └── health.rs             # Health endpoint handler
```

## Deliverables (Frontend)

```
frontend/src/
└── hooks/
    └── use-health.ts         # Hook to fetch health status
```

## Acceptance Criteria (Pass/Fail)

**Backend:**
- [ ] `GET http://localhost:3001/api/v1/health` returns HTTP 200
- [ ] Response body: `{"status":"ok","version":"0.1.0"}`
- [ ] Route is mounted under `/api/v1/` prefix (not root `/health`)

**Frontend:**
- [ ] `useHealth` hook exists and exports `{ status, isLoading, error }`
- [ ] Hook calls `GET /api/v1/health` (works through Vite proxy after 0.5)
- [ ] TypeScript types for the health response are defined

## Implementation Notes

**Backend — `health.rs`:**
```rust
use axum::{response::Json, routing::get, Router};
use serde::Serialize;

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
}

pub fn routes() -> Router {
    Router::new().route("/health", get(health_check))
}

async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}
```

**Backend — Update `main.rs`** to mount routes under `/api/v1`:
```rust
use axum::Router;

mod routes;

let app = Router::new()
    .nest("/api/v1", routes::health::routes());
```

**Frontend — `use-health.ts`:**
```typescript
import { useState, useEffect } from 'react';

interface HealthStatus {
  status: string;
  version: string;
}

export function useHealth() {
  const [status, setStatus] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  useEffect(() => {
    fetch('/api/v1/health')
      .then(res => res.json())
      .then((data: HealthStatus) => setStatus(data.status))
      .catch(setError)
      .finally(() => setIsLoading(false));
  }, []);

  return { status, isLoading, error };
}
```

## Test Strategy

- Backend unit tests will be written in Wave 0.7
- Frontend hook test will be covered in Wave 0.8 smoke test
- Manual verification: start backend, `curl localhost:3001/api/v1/health`
