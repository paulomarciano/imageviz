//! Request timeout middleware for the Axum server.
//!
//! Provides configurable timeout layers applied per-route-group, so that
//! long-lived endpoints (SSE, thumbnail generation) can use longer timeouts
//! while the rest of the API uses a sensible default.
//!
//! # Env-var configuration
//!
//! `REQUEST_TIMEOUT_SECS` — default timeout in seconds (default: 60).

use axum::http::StatusCode;
use std::time::Duration;
use tower_http::timeout::TimeoutLayer;

/// Get the default request timeout in seconds from the `REQUEST_TIMEOUT_SECS`
/// environment variable, falling back to 60 if unset or invalid.
pub fn default_timeout_secs() -> u64 {
    std::env::var("REQUEST_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60)
}

/// Apply a timeout middleware to a router.
///
/// Requests that exceed `duration_secs` are cancelled and return
/// HTTP 408 Request Timeout.
///
/// # Usage
///
/// ```rust,ignore
/// let wrapped = apply_timeout(my_router, 120);
/// app.nest("/api/v1", wrapped);
/// ```
///
/// `TimeoutLayer::with_status_code` from `tower-http` handles the timeout by
/// returning a response with the given status code directly, without changing
/// the error type. This avoids needing a `HandleErrorLayer`.
pub fn apply_timeout(router: axum::Router, duration_secs: u64) -> axum::Router {
    router.layer(TimeoutLayer::with_status_code(
        StatusCode::REQUEST_TIMEOUT,
        Duration::from_secs(duration_secs),
    ))
}

/// Alias for [`apply_timeout`] using [`default_timeout_secs`].
pub fn apply_default_timeout(router: axum::Router) -> axum::Router {
    apply_timeout(router, default_timeout_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use std::time::Duration;
    use tower::ServiceExt;

    /// A handler that sleeps longer than the timeout to trigger a 408.
    async fn slow_handler() -> &'static str {
        tokio::time::sleep(Duration::from_secs(5)).await;
        "done"
    }

    /// A handler that returns instantly (within the timeout).
    async fn fast_handler() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn test_timeout_triggers_408() {
        let router = apply_timeout(
            Router::new().route("/slow", get(slow_handler)),
            1, // 1 second timeout
        );

        let response = router
            .oneshot(Request::builder().uri("/slow").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::REQUEST_TIMEOUT,
            "slow handler should timeout and return 408"
        );
    }

    #[tokio::test]
    async fn test_fast_request_succeeds() {
        let router = apply_timeout(
            Router::new().route("/fast", get(fast_handler)),
            1, // 1 second timeout
        );

        let response = router
            .oneshot(Request::builder().uri("/fast").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "fast handler should complete before timeout"
        );
    }

    #[tokio::test]
    async fn test_default_timeout_secs_fallback() {
        // Temporarily remove the env var to test the fallback.
        // Note: env var tests are inherently racy when run in parallel;
        // this test assumes no other test concurrently sets the var.
        let previous = std::env::var("REQUEST_TIMEOUT_SECS").ok();
        unsafe {
            std::env::remove_var("REQUEST_TIMEOUT_SECS");
        }
        assert_eq!(default_timeout_secs(), 60, "should default to 60");
        // Restore the previous value.
        if let Some(ref val) = previous {
            unsafe { std::env::set_var("REQUEST_TIMEOUT_SECS", val); }
        }
    }
}
