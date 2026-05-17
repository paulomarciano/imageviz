//! Integration tests for the SSE event stream endpoint (`GET /api/v1/events`).
//!
//! These tests exercise the full pipeline: route mounting → SSE handler →
//! broadcast channel fan-out.  The co-located unit tests in `routes/events.rs`
//! cover handler logic; this suite validates integration with the production
//! router and real broadcast channels.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures_util::StreamExt;
use std::time::Duration;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Content-type & basic connectivity (via oneshot)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_sse_content_type_is_text_event_stream() {
    let app = common::create_test_app_with_search();

    let response = app
        .router
        .oneshot(Request::builder().uri("/api/v1/events").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap(), "text/event-stream");
}

// ---------------------------------------------------------------------------
// Connected event (via real server + reqwest)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_sse_receives_connected_event() {
    let app = common::create_test_app_with_search();

    // Bind to a random port so multiple test runs don't conflict.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app.router).await.unwrap();
    });

    // Give the server a moment to start.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap();

    let response = client.get(&format!("http://{}/api/v1/events", addr)).send().await.unwrap();

    assert_eq!(response.headers().get("content-type").unwrap(), "text/event-stream");

    // Read the first chunk — the "connected" event is sent immediately.
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

// ---------------------------------------------------------------------------
// Broadcast event propagation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_sse_receives_broadcast_event() {
    let app = common::create_test_app_with_search();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let sse_tx = app.sse_tx.clone();

    tokio::spawn(async move {
        axum::serve(listener, app.router).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap();

    let response = client.get(&format!("http://{}/api/v1/events", addr)).send().await.unwrap();

    let mut stream = response.bytes_stream();

    // Drain the "connected" event first.
    let _connected = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .expect("connected event should arrive quickly")
        .expect("stream should not end")
        .expect("connected event should not error");

    // Broadcast a file_created event.
    let sse_event = imageviz_backend::watcher::handler::SseEvent {
        event_type: "file_created".into(),
        data: serde_json::json!({
            "id": "test-uuid-1234",
            "filename": "test.png",
            "path": "subdir/test.png",
        }),
    };
    sse_tx.send(sse_event).unwrap();

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
async fn test_sse_multiple_event_types_propagate() {
    let app = common::create_test_app_with_search();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let sse_tx = app.sse_tx.clone();

    tokio::spawn(async move {
        axum::serve(listener, app.router).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap();

    let response = client.get(&format!("http://{}/api/v1/events", addr)).send().await.unwrap();

    let mut stream = response.bytes_stream();

    // Drain connected event.
    let _connected = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .expect("connected")
        .expect("stream open")
        .expect("no error");

    // Broadcast events of each relevant type.
    let event_types = ["file_created", "file_deleted", "file_modified", "indexing_complete"];
    for event_type in &event_types {
        let sse_event = imageviz_backend::watcher::handler::SseEvent {
            event_type: event_type.to_string(),
            data: serde_json::json!({"id": "test"}),
        };
        sse_tx.send(sse_event).unwrap();
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
            "Expected event type '{expected_type}', got chunk: {text}",
        );
    }
}

// ---------------------------------------------------------------------------
// Graceful disconnect
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_sse_client_disconnect_does_not_crash_server() {
    let app = common::create_test_app_with_search();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        axum::serve(listener, app.router).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap();

    let _response = client.get(&format!("http://{}/api/v1/events", addr)).send().await.unwrap();

    // Drop the connection immediately without reading.
    // The server should not panic — the stream drop is handled gracefully.
    drop(client);

    // Give the server a moment to process the disconnect.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // The server task should still be running (not panicked).
    assert!(!handle.is_finished(), "Server should survive client disconnect");
    handle.abort();
}
