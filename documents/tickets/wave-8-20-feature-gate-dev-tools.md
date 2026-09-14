# Wave 8.20 — Gate tokio-console and pprof Behind a Feature Flag

| Field | Value |
|-------|-------|
| **Wave** | 8 — Post-Audit: Performance, Resources & Hygiene |
| **Seq** | 20 |
| **Estimate** | 1 hour |
| **Depends on** | — |
| **Parallel** | Yes (8.24/8.26 also touch main.rs — land this first) |
| **Source** | Code review §1 K3 (🟡) |

---

## Overview

Dev tooling is unconditionally wired into the production binary:

- `ConsoleLayer::new()` is constructed and the console server task spawned on **every** start (`main.rs:33-46,209`), regardless of `TOKIO_CONSOLE_ADDR` — instrumenting every task spawned afterward (the reason the build needs `--cfg tokio_unstable`).
- `/debug/pprof` is mounted on every start (`main.rs`), although `profiler.rs`'s own doc comment says it "should not be exposed in production" and suggests "a compile-time feature flag" — never done.

Fix: gate both behind a `dev-tools` cargo feature. Release binaries get smaller, `tokio_unstable` becomes unnecessary for releases, and the runtime loses per-task instrumentation overhead.

## Prerequisites

- None (v0.7.0 baseline)

## Reference Files

- `documents/code-review-kiss-dry-performance-resources.md` — §1 K3
- `backend/src/main.rs:33-46,209`, `backend/src/profiler.rs`
- `backend/Cargo.toml`, `.cargo/config.toml` (`--cfg tokio_unstable` for linux-gnu), `AGENTS.md` notes
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/Cargo.toml            # [features] dev-tools = ["console-subscriber", "pprof"] (adjust names)
backend/src/main.rs           # cfg(feature = "dev-tools") around console layer + pprof mount
backend/src/profiler.rs       # module compiled only under the feature
.github/workflows/ci.yml      # backend-test job enables dev-tools if tests reference it
AGENTS.md / README            # document the flag + RUSTFLAGS note update
```

## Acceptance Criteria (Pass/Fail)

- [ ] `cargo build --release` (no features): binary contains no pprof route (`/debug/pprof` → 404) and no console server; builds **without** `--cfg tokio_unstable`
- [ ] `cargo run --features dev-tools`: tokio-console connects (when `TOKIO_CONSOLE_ADDR` set) and `/debug/pprof/profile` serves a flamegraph
- [ ] `cargo test` green both with and without `--features dev-tools`
- [ ] `cargo clippy -- -D warnings` green in both configurations
- [ ] CI updated so the `frontend-e2e` job's `RUSTFLAGS`/feature combination still boots the backend
- [ ] Docs updated (AGENTS.md "Debug tooling" section)

## Implementation Notes

```rust
#[cfg(feature = "dev-tools")]
{
    // ConsoleLayer + console_subscriber::init + pprof router mount
}
```

- `.cargo/config.toml` currently forces `--cfg tokio_unstable` for linux-gnu — keep it for dev convenience or scope it via an alias (`cargo build --features dev-tools`); the acceptance is that a **plain release build works without it**. Prefer removing it from config and documenting `RUSTFLAGS="--cfg tokio_unstable" cargo run --features dev-tools` for console sessions.
- `console-subscriber` and `pprof` become optional dependencies (`optional = true`) tied to the feature.

## Test Strategy

- Compile-matrix script or CI matrix: `cargo check` for `{default, dev-tools}` × debug/release.
- Smoke (dev-tools on): curl `/debug/pprof/profile?seconds=1` → 200 SVG.
- Smoke (default): route 404s; no tokio console port listening.
