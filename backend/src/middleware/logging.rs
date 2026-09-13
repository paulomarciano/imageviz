//! Structured request logging middleware.
//!
//! Logs each HTTP request with a unique request ID, method, path, status code,
//! and duration. Request IDs are generated server-side (UUID v4) and logged
//! in the tracing span for correlation across logs.
//!
//! # One log line per request
//!
//! Only the response line (`← status (duration_ms)`) is rendered at INFO, so
//! a thumbnail-grid load produces one line per request, not two (wave 8.24).
//! Request-start logging is left to `tower_http`'s `DefaultOnRequest`, which
//! emits `started processing request` at DEBUG — still available when
//! troubleshooting with `RUST_LOG=debug`.
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
use tower_http::trace::{MakeSpan, OnResponse, TraceLayer};
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
    tower_http::trace::DefaultOnRequest,
    LogOnResponse,
    tower_http::trace::DefaultOnBodyChunk,
    tower_http::trace::DefaultOnEos,
    tower_http::trace::DefaultOnFailure,
> {
    TraceLayer::new_for_http().make_span_with(MakeRequestSpan).on_response(LogOnResponse)
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
    use std::io::Write as IoWrite;
    use std::sync::{Arc, Mutex as StdMutex};
    use tower::ServiceExt;
    use tracing_subscriber::filter::LevelFilter;
    use tracing_subscriber::layer::SubscriberExt;

    /// `io::Write` adapter over a shared in-memory buffer, used to capture
    /// rendered log output from a fmt layer.
    #[derive(Clone, Default)]
    struct SharedBuffer(Arc<StdMutex<Vec<u8>>>);

    impl IoWrite for SharedBuffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().write(buf)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.0.lock().unwrap().flush()
        }
    }

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

    /// Run one request through the logging layer with fmt output captured
    /// into an in-memory buffer, filtered at `level`. Returns the rendered
    /// log text.
    ///
    /// `#[tokio::test]` runs on a current-thread runtime, so the
    /// thread-local default-subscriber guard set here stays effective
    /// across the `.await` points of `oneshot`.
    async fn captured_log(level: LevelFilter) -> String {
        let buffer = SharedBuffer::default();
        let writer_buffer = buffer.clone();
        let subscriber = tracing_subscriber::registry().with(level).with(
            tracing_subscriber::fmt::layer()
                .with_writer(move || writer_buffer.clone())
                .with_ansi(false)
                .without_time()
                .compact(),
        );

        let _guard = tracing::subscriber::set_default(subscriber);

        let app = Router::new().route("/", get(dummy_handler)).layer(logging_layer());
        let response =
            app.oneshot(Request::builder().uri("/").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), 200);

        let logs = buffer.0.lock().unwrap();
        String::from_utf8_lossy(&logs).into_owned()
    }

    #[tokio::test]
    async fn test_info_level_renders_single_response_line_per_request() {
        let logs = captured_log(LevelFilter::INFO).await;

        let lines: Vec<&str> = logs.lines().collect();
        assert_eq!(lines.len(), 1, "expected exactly one log line at INFO, got: {lines:?}");
        assert!(lines.first().is_some_and(|l| l.contains("←")), "expected the response line");
        assert!(!logs.contains("→ request"), "request-start line must not render at INFO");
    }

    #[tokio::test]
    async fn test_debug_level_still_logs_request_start() {
        let logs = captured_log(LevelFilter::DEBUG).await;

        assert!(
            logs.contains("started processing request"),
            "request-start line must be available at DEBUG, got: {logs:?}"
        );
        assert!(logs.contains("←"), "response line must also render at DEBUG");
    }
}
