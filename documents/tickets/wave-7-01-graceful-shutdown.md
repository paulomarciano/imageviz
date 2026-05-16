# Wave 7.1 — Implement Graceful Shutdown (In-Flight Requests)

| Field | Value |
|-------|-------|
| **Wave** | 7 — Production Readiness & Hardening |
| **Seq** | 01 |
| **Estimate** | 1 hour |
| **Depends on** | 0.2 (backend scaffold) |
| **Parallel** | No |

---

## Overview

Implement graceful shutdown for the Axum server. When `Ctrl+C` (SIGINT) or SIGTERM is received, the server stops accepting new connections but drains in-flight requests before exiting. This prevents interrupted downloads/uploads and database corruption.

## Prerequisites

- Backend running with Axum + Tokio (0.2)
- `tokio::signal` for OS signal handling

## Reference Files

- `documents/plans/development-plan.md` — §5 Wave 7 task 7.1
- `.opencode/context/development/principles/clean-code.md`

## Deliverables

```
backend/src/main.rs              # Updated: graceful shutdown signal handling
```

## Acceptance Criteria (Pass/Fail)

- [ ] `Ctrl+C` initiates graceful shutdown (not immediate kill)
- [ ] Server stops accepting new connections (TCP listener closed)
- [ ] In-flight HTTP requests complete before exit (up to a timeout of 30s)
- [ ] SSE connections are closed gracefully
- [ ] Database connections are closed (SQLite file properly flushed)
- [ ] Tantivy index is committed before exit
- [ ] Log message: "Shutting down gracefully..." and "Shutdown complete"
- [ ] SIGTERM (e.g., from `kill` command) also triggers graceful shutdown

## Implementation Notes

**Axum graceful shutdown:**
```rust
use tokio::signal;
use axum::serve;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = build_app().await;
    let listener = TcpListener::bind("127.0.0.1:3001").await.unwrap();
    
    tracing::info!("Server running on http://{}", listener.local_addr().unwrap());

    // Graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    // Cleanup
    cleanup_resources().await;
    tracing::info!("Shutdown complete");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        tracing::info!("Received Ctrl+C, shutting down...");
    };

    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
        tracing::info!("Received SIGTERM, shutting down...");
    };

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

async fn cleanup_resources() {
    // Commit Tantivy index
    if let Some(ref index_manager) = APP_STATE.search_index {
        let _ = index_manager.commit();
        tracing::info!("Tantivy index committed");
    }
    
    // Close SQLite connection (dropped automatically when Arc is dropped)
    
    // Close file watcher
    tracing::info!("Resources cleaned up");
}
```

**Graceful shutdown with timeout:**
```rust
axum::serve(listener, app)
    .with_graceful_shutdown(async {
        shutdown_signal().await;
    })
    .await
    .unwrap();

// After axum::serve returns, give remaining tasks time to finish
let cleanup_timeout = tokio::time::timeout(
    Duration::from_secs(30),
    cleanup_resources(),
).await;

if cleanup_timeout.is_err() {
    tracing::warn!("Cleanup timed out after 30s, forcing exit");
}
```

**Signal handling:**
- `SIGINT` (Ctrl+C) — standard terminal interrupt
- `SIGTERM` — standard termination signal (used by process managers, Docker, systemd)
- `SIGKILL` — can't be caught; not relevant (the OS kills the process immediately)

## Test Strategy

- Manual: start server, make a slow request (e.g., large file download), press Ctrl+C, verify request completes before server exits
- Manual: send SIGTERM via `kill -TERM <pid>`, verify graceful shutdown
- Log inspection: verify "Shutting down gracefully" and "Shutdown complete" messages
