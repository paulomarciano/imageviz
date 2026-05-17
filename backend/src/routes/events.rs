//! Server-Sent Events (SSE) endpoint — `GET /api/v1/events`
//!
//! Provides a real-time event stream for file system changes and index
//! status updates. Clients connect via SSE and receive:
//!
//! - `connected` — initial event confirming connection
//! - `file_created`, `file_deleted`, `file_modified` — file system events
//! - `indexing_complete` — when a scan/indexing pass finishes (reserved,
//!   emitted by the indexer via the broadcast channel)
//!
//! The stream stays open indefinitely with keep-alive comments every 30s
//! to prevent proxy timeouts. Client disconnect is handled gracefully:
//! when the `broadcast::Receiver` is dropped, the stream ends and axum
//! closes the connection.

use axum::{
    Router,
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
};
use futures_util::stream::Stream;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::watcher::handler::SseEvent;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Shared state for the SSE endpoint — holds the broadcast sender used to
/// fan out file system events to all connected SSE clients.
///
/// A single `broadcast::Sender` is created in `main.rs` and cloned for each
/// subscriber via `subscribe()`. Dropping all senders shuts down the channel.
pub struct EventsState {
    pub sse_tx: tokio::sync::broadcast::Sender<SseEvent>,
}

// ---------------------------------------------------------------------------
// Route factory
// ---------------------------------------------------------------------------

/// Build the router for the SSE endpoint.
///
/// The returned `Router` expects `Arc<EventsState>` as its state type, which
/// is provided by `main.rs` at application startup.
pub fn routes() -> Router<Arc<EventsState>> {
    Router::new().route("/events", get(sse_handler))
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// SSE handler — returns an infinite stream of Server-Sent Events.
///
/// On connect, the handler:
/// 1. Sends an initial `connected` event with the current server timestamp.
/// 2. Subscribes to the broadcast channel and forwards all events to the
///    client as they arrive.
/// 3. Inserts keep-alive comment lines (`: keep-alive`) every 30 seconds
///    to prevent proxies from closing idle connections.
///
/// # Graceful disconnect
///
/// When the client disconnects, the `broadcast::Receiver` is dropped
/// because the response stream is dropped. This causes the BroadcastStream
/// to yield `None`, ending the SSE stream and triggering HTTP cleanup.
async fn sse_handler(
    State(state): State<Arc<EventsState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.sse_tx.subscribe();
    let stream = BroadcastStream::new(rx);

    // Map broadcast events into axum SSE Event objects.
    // BroadcastStream yields Result<T, BroadcastStreamRecvError> where
    // the error variant indicates the client lagged behind.
    let event_stream = stream.filter_map(|result| match result {
        Ok(sse_event) => {
            let axum_event =
                Event::default().event(sse_event.event_type).data(sse_event.data.to_string());
            Some(Ok(axum_event))
        }
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
            tracing::warn!("SSE client lagged by {} messages", n);
            Some(Ok(Event::default().event("lagged").data(format!(r#"{{"skipped":{}}}"#, n))))
        }
    });

    // Prepend a "connected" event so clients know the stream is alive.
    let connected_event = Event::default()
        .event("connected")
        .data(serde_json::json!({"timestamp": chrono::Utc::now().to_rfc3339()}).to_string());

    let stream_with_connected =
        futures_util::stream::once(async move { Ok(connected_event) }).chain(event_stream);

    Sse::new(stream_with_connected)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(30)).text("keep-alive"))
}

// ---------------------------------------------------------------------------
// Co-located tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::http::StatusCode;
    use futures_util::StreamExt;
    use std::time::Duration;
    use tower::ServiceExt;

    /// Build a test router with a dedicated broadcast channel.
    fn test_app() -> (Arc<EventsState>, axum::Router) {
        let (sse_tx, _) = tokio::sync::broadcast::channel(256);
        let state = Arc::new(EventsState { sse_tx: sse_tx.clone() });
        let app = routes().with_state(Arc::clone(&state));
        (state, app)
    }

    // -----------------------------------------------------------------------
    // Header / content-type checks
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_sse_content_type() {
        let (_state, app) = test_app();

        let response = app
            .oneshot(Request::builder().uri("/events").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get("content-type").unwrap(), "text/event-stream");
    }

    // -----------------------------------------------------------------------
    // Connected event
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_sse_connected_event() {
        let (_state, app) = test_app();

        // Bind to a random port so multiple test runs don't conflict.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // Give the server a moment to start accepting connections.
        tokio::time::sleep(Duration::from_millis(100)).await;

        let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap();
        let response = client.get(&format!("http://{}/events", addr)).send().await.unwrap();

        assert_eq!(response.headers().get("content-type").unwrap(), "text/event-stream");

        // Read the first chunk from the SSE stream. The connected event is
        // sent immediately upon connection, so it should arrive quickly.
        let mut stream = response.bytes_stream();
        let first_chunk = tokio::time::timeout(Duration::from_secs(3), stream.next()).await;

        match first_chunk {
            Ok(Some(Ok(bytes))) => {
                let text = String::from_utf8_lossy(&bytes);
                assert!(
                    text.contains("event: connected"),
                    "Expected 'event: connected' as first SSE event, got: {text}"
                );
                assert!(
                    text.contains("timestamp"),
                    "Connected event should include a 'timestamp' field, got: {text}"
                );
            }
            Ok(Some(Err(e))) => panic!("SSE stream error: {e}"),
            Ok(None) => panic!("SSE stream ended before 'connected' event"),
            Err(_elapsed) => {
                panic!("Timed out waiting for 'connected' SSE event");
            }
        }
    }

    // -----------------------------------------------------------------------
    // Event flow: broadcast → SSE
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_sse_receives_broadcast_event() {
        let (state, app) = test_app();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap();
        let response = client.get(&format!("http://{}/events", addr)).send().await.unwrap();

        let mut stream = response.bytes_stream();

        // Drain the "connected" event first.
        let _connected = tokio::time::timeout(Duration::from_secs(3), stream.next())
            .await
            .expect("connected event should arrive quickly")
            .expect("stream should not end")
            .expect("connected event should not error");

        // Broadcast a file_created event.
        let sse_event = SseEvent {
            event_type: "file_created".into(),
            data: serde_json::json!({
                "id": "test-uuid-1234",
                "filename": "test.png",
                "path": "subdir/test.png",
            }),
        };
        state.sse_tx.send(sse_event).unwrap();

        // Read the event from the SSE stream.
        let event_chunk = tokio::time::timeout(Duration::from_secs(3), stream.next()).await;

        match event_chunk {
            Ok(Some(Ok(bytes))) => {
                let text = String::from_utf8_lossy(&bytes);
                assert!(
                    text.contains("event: file_created"),
                    "Expected file_created event, got: {text}"
                );
                assert!(
                    text.contains("test-uuid-1234"),
                    "Expected event data to contain the id, got: {text}"
                );
            }
            Ok(Some(Err(e))) => panic!("SSE stream error: {e}"),
            Ok(None) => panic!("SSE stream ended before broadcast event"),
            Err(_) => panic!("Timed out waiting for broadcast event"),
        }
    }

    #[tokio::test]
    async fn test_sse_multiple_event_types() {
        let (state, app) = test_app();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap();
        let response = client.get(&format!("http://{}/events", addr)).send().await.unwrap();

        let mut stream = response.bytes_stream();

        // Drain connected event.
        let _connected = tokio::time::timeout(Duration::from_secs(3), stream.next())
            .await
            .expect("connected")
            .expect("stream open")
            .expect("no error");

        // Broadcast events of each type the handler produces.
        let event_types = ["file_created", "file_deleted", "file_modified", "indexing_complete"];
        for event_type in &event_types {
            let sse_event = SseEvent {
                event_type: event_type.to_string(),
                data: serde_json::json!({"id": "test"}),
            };
            state.sse_tx.send(sse_event).unwrap();
        }

        // Collect events from the stream and verify order.
        for expected_type in &event_types {
            let chunk = tokio::time::timeout(Duration::from_secs(3), stream.next())
                .await
                .expect("timeout")
                .expect("stream ended early")
                .expect("stream error");
            let text = String::from_utf8_lossy(&chunk);
            assert!(
                text.contains(&format!("event: {}", expected_type)),
                "Expected event type '{}', got chunk: {text}",
                expected_type
            );
        }
    }

    // -----------------------------------------------------------------------
    // Graceful disconnect: dropping the client should not panic the server
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_sse_client_disconnect_does_not_panic() {
        let (_state, app) = test_app();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap();
        let _response = client.get(&format!("http://{}/events", addr)).send().await.unwrap();

        // Drop the connection immediately without reading.
        // The server should not panic — the stream drop is handled gracefully.
        drop(client);

        // Give the server a moment to process the disconnect.
        tokio::time::sleep(Duration::from_millis(200)).await;

        // The server task should still be running (not panicked).
        assert!(!handle.is_finished(), "Server should survive client disconnect");
        handle.abort();
    }
}
