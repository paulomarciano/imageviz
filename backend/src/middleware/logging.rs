//! Structured request logging middleware.
//!
//! Logs each HTTP request with a unique request ID, method, path, status code,
//! and duration. Request IDs are generated server-side (UUID v4) and logged
//! in the tracing span for correlation across logs.
//!
//! # Log levels by status code
//!
//! | Status range | Level |
//! |--------------|-------|
//! | 2xx / 3xx    | INFO  |
//! | 4xx          | WARN  |
//! | 5xx          | ERROR |

use axum::http::Request;
use std::time::Duration;
use tower_http::trace::{MakeSpan, OnRequest, OnResponse, TraceLayer};
use tracing::Span;

/// Build a logging layer that adds request ID, method, path, status, and
/// duration to every request span.
///
/// The layer generates a UUID v4 request ID on every request and attaches
/// it to the tracing span. After the response, it logs the status code and
/// duration at the appropriate level:
/// - INFO for 2xx/3xx
/// - WARN for 4xx
/// - ERROR for 5xx
pub fn logging_layer() -> TraceLayer<
    tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>,
    MakeRequestSpan,
    LogOnRequest,
    LogOnResponse,
    tower_http::trace::DefaultOnBodyChunk,
    tower_http::trace::DefaultOnEos,
    tower_http::trace::DefaultOnFailure,
> {
    TraceLayer::new_for_http()
        .make_span_with(MakeRequestSpan)
        .on_request(LogOnRequest)
        .on_response(LogOnResponse)
}

/// Creates the tracing span for each request with method, URI, and request ID.
#[derive(Clone)]
pub struct MakeRequestSpan;

impl<B> MakeSpan<B> for MakeRequestSpan {
    fn make_span(&mut self, request: &Request<B>) -> Span {
        let request_id = uuid::Uuid::new_v4().to_string();
        tracing::info_span!(
            "http_request",
            method = %request.method(),
            uri = %request.uri().path(),
            request_id = %request_id,
        )
    }
}

/// Logs when a request arrives.
#[derive(Clone)]
pub struct LogOnRequest;

impl<B> OnRequest<B> for LogOnRequest {
    fn on_request(&mut self, request: &Request<B>, _span: &Span) {
        tracing::info!(method = %request.method(), uri = %request.uri().path(), "→ request");
    }
}

/// Logs when a response is sent, including status code and duration.
#[derive(Clone)]
pub struct LogOnResponse;

impl<B> OnResponse<B> for LogOnResponse {
    fn on_response(self, response: &axum::http::Response<B>, latency: Duration, _span: &Span) {
        let status = response.status();
        let duration_ms = latency.as_millis();

        if status.is_server_error() {
            tracing::error!(
                status = status.as_u16(),
                duration_ms = duration_ms,
                "← {} ({}ms)",
                status.canonical_reason().unwrap_or("Unknown"),
                duration_ms,
            );
        } else if status.is_client_error() {
            tracing::warn!(
                status = status.as_u16(),
                duration_ms = duration_ms,
                "← {} ({}ms)",
                status.canonical_reason().unwrap_or("Unknown"),
                duration_ms,
            );
        } else {
            tracing::info!(
                status = status.as_u16(),
                duration_ms = duration_ms,
                "← {} ({}ms)",
                status.canonical_reason().unwrap_or("Unknown"),
                duration_ms,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt;

    async fn dummy_handler() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn test_logging_layer_applied() {
        let app = Router::new().route("/", get(dummy_handler)).layer(logging_layer());

        let response =
            app.oneshot(Request::builder().uri("/").body(Body::empty()).unwrap()).await.unwrap();

        assert_eq!(response.status(), 200);
    }
}
