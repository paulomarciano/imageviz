//! CPU profiling via pprof-rs with flamegraph output.
//!
//! Exposes `GET /debug/pprof/profile?seconds=5&format=svg` which runs CPU profiling
//! for the requested duration and returns a flamegraph SVG (viewable in a browser)
//! or `profile.pb` (for `go tool pprof`).
//!
//! The profiler uses `tokio::task::spawn_blocking` to run the sampling period so it
//! does not block the async runtime. Profiling runs for a fixed duration then stops
//! — this is on-demand, not continuous, so there is zero overhead between requests.
//!
//! # Security
//!
//! The `/debug/pprof` endpoint is intended for local development and debugging only.
//! It has no authentication and should not be exposed in production deployments.
//! Consider using a firewall, a compile-time feature flag, or a reverse-proxy
//! to restrict access to this endpoint.

use std::marker::PhantomData;
use std::time::Duration;

use axum::{
    Router,
    extract::Query,
    response::{IntoResponse, Response},
    routing::get,
};
use pprof::protos::Message;
use serde::Deserialize;

/// Query parameters for the profiling endpoint.
#[derive(Debug, Deserialize)]
pub struct ProfileParams {
    /// Number of seconds to profile. Defaults to 5. Max 60.
    #[serde(default = "default_seconds")]
    pub seconds: u64,
    /// Output format: "svg" (default) or "pb" (raw protobuf for go tool pprof).
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_seconds() -> u64 {
    5
}
fn default_format() -> String {
    "svg".to_owned()
}

/// Application state holding the profiler endpoint.
#[derive(Clone)]
pub struct ProfilerState {
    _phantom: PhantomData<()>,
}

impl ProfilerState {
    /// Create a new profiler state.
    pub fn new() -> Self {
        Self { _phantom: std::marker::PhantomData }
    }

    /// Build an Axum router with the profiler endpoint mounted at `/debug/pprof`.
    pub fn router(&self) -> Router {
        Router::new().route("/profile", get(profile_handler)).with_state(self.clone())
    }
}

impl Default for ProfilerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for `GET /debug/pprof/profile`.
///
/// Runs CPU profiling for `seconds` (default 5, max 60) and returns either:
/// - `text/svg` — flamegraph viewable in a browser
/// - `application/x-protobuf` — raw profile.proto for `go tool pprof`
async fn profile_handler(Query(params): Query<ProfileParams>) -> Response {
    let seconds = params.seconds.clamp(1, 60);
    let total_timeout = Duration::from_secs(seconds.saturating_add(10));

    let report = match tokio::time::timeout(
        total_timeout,
        tokio::task::spawn_blocking(move || {
            let guard = match pprof::ProfilerGuardBuilder::default()
                .frequency(1000)
                .blocklist(&["libc", "libgcc", "pthread", "vdso"])
                .build()
            {
                Ok(g) => g,
                Err(e) => {
                    return Err(format!("failed to start profiler: {e}"));
                }
            };
            std::thread::sleep(Duration::from_secs(seconds));
            guard.report().build().map_err(|e| format!("failed to build report: {e}"))
        }),
    )
    .await
    {
        Err(_) => {
            return (axum::http::StatusCode::REQUEST_TIMEOUT, "profiling timed out".to_owned())
                .into_response();
        }
        Ok(Err(_join_err)) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "profiling task panicked".to_owned(),
            )
                .into_response();
        }
        Ok(Ok(Err(msg))) => {
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, msg).into_response();
        }
        Ok(Ok(Ok(report))) => report,
    };

    match params.format.as_str() {
        "pb" => {
            let profile = match report.pprof() {
                Ok(p) => p,
                Err(e) => {
                    return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("{e}"))
                        .into_response();
                }
            };
            let mut buf = Vec::new();
            if let Err(e) = profile.encode(&mut buf) {
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to encode profile: {e}"),
                )
                    .into_response();
            }
            (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/x-protobuf")],
            )
                .into_response()
        }
        "svg" => {
            let mut buf = Vec::new();
            if let Err(e) = report.flamegraph(&mut buf) {
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to generate flamegraph: {e}"),
                )
                    .into_response();
            }
            (axum::http::StatusCode::OK, [(axum::http::header::CONTENT_TYPE, "image/svg+xml")])
                .into_response()
        }
        _ => (axum::http::StatusCode::BAD_REQUEST, "format must be 'svg' or 'pb'".to_owned())
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_params() {
        assert_eq!(default_seconds(), 5);
        assert_eq!(default_format(), "svg");
    }
}
