# Wave 0.2 — Scaffold Rust Backend with Axum Hello-World

| Field | Value |
|-------|-------|
| **Wave** | 0 — Project Scaffolding & CI |
| **Seq** | 02 |
| **Estimate** | 45 minutes |
| **Depends on** | 0.1 (monorepo structure) |
| **Parallel** | No |

---

## Overview

Initialize the Rust backend project with Axum web framework. Create a minimal "hello world" HTTP server that responds on port 3001. This establishes the Rust toolchain, dependency management, and project layout for all backend waves.

## Prerequisites

- Rust toolchain installed (`rustc`, `cargo`)
- `backend/` directory exists (from 0.1)

## Reference Files

- `documents/plans/development-plan.md` — §2 (Tech Stack), §13 Appendix (Cargo.toml dependencies), §12 (project structure)
- `.opencode/context/development/principles/clean-code.md` — Rust-specific patterns (embrace ownership, pattern matching, Result types)

## Deliverables

```
backend/
├── Cargo.toml                        # With all dependencies from §13
├── rustfmt.toml                      # Rust formatting config
├── .cargo/
│   └── config.toml                   # Cargo configuration
└── src/
    └── main.rs                       # Axum hello-world server
```

## Acceptance Criteria (Pass/Fail)

- [ ] `cargo build` compiles without errors
- [ ] `cargo run` starts server on port 3001 (or configurable via `PORT` env var)
- [ ] `curl http://localhost:3001/` returns a 200 response with a JSON body (`{"status":"ok"}` or similar)
- [ ] `Cargo.toml` includes all dependencies listed in §13 appendix:
  - `axum = "0.8"`, `tokio = { version = "1", features = ["full"] }`
  - `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`
  - `tower = "0.5"`, `tower-http = { version = "0.6", features = ["cors", "trace", "compression-gzip", "limit"] }`
  - `uuid = { version = "1", features = ["v4"] }`
  - `tracing = "0.1"`, `tracing-subscriber = { version = "0.3", features = ["env-filter"] }`
- [ ] `rustfmt.toml` exists with project formatting preferences

## Implementation Notes

1. **main.rs structure**:
   ```rust
   use axum::{Router, routing::get};
   use std::net::SocketAddr;

   #[tokio::main]
   async fn main() {
       // Initialize tracing
       tracing_subscriber::fmt::init();

       let app = Router::new()
           .route("/", get(root_handler));

       let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
       println!("Server running on http://{}", addr);
       
       let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
       axum::serve(listener, app).await.unwrap();
   }

   async fn root_handler() -> &'static str {
       "Hello from ImageViz backend!"
   }
   ```
2. **`.cargo/config.toml`** — set compiler flags if needed (e.g., `[build] rustflags = ["-D", "warnings"]` for CI).
3. Use Rust edition `2024` as specified in §13.
4. Keep `main.rs` minimal — route extraction will happen in Wave 0.4.

## Test Strategy

- Manual: `cargo run` then `curl http://localhost:3001/`
- Manual: `cargo build --release` to verify release compilation
- No automated tests yet — Wave 0.7 adds the first backend tests
