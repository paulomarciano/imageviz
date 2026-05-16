# Wave 3.7 — Implement SSE Endpoint for Real-Time Updates

| Field | Value |
|-------|-------|
| **Wave** | 3 — Backend: Search, Cursor Pagination & Real-time SSE |
| **Seq** | 07 |
| **Estimate** | 2 hours |
| **Depends on** | 3.6 (watcher → broadcast) |
| **Parallel** | No |

---

## Overview

Add the `GET /events` SSE (Server-Sent Events) endpoint. Clients connect and receive real-time file system events (created, modified, deleted, indexing_complete) as a stream. SSE is chosen over WebSocket for simplicity — unidirectional server→client push is sufficient for this use case.

## Prerequisites

- Broadcast channel from 3.6 (`tokio::sync::broadcast::Sender<SseEvent>`)
- Axum routes structure
- `tokio-stream` in Cargo.toml (for converting broadcast to stream)

## Reference Files

- `documents/plans/development-plan.md` — §3.2 Endpoints (GET /events), §3.3 SSE Event Format, §8.1 Performance Targets (SSE latency <500ms)
- `.opencode/context/development/principles/api-design.md`

## Deliverables

```
backend/src/routes/
└── events.rs                    # GET /events handler (SSE)
```

## Acceptance Criteria (Pass/Fail)

- [ ] `GET /api/v1/events` returns `Content-Type: text/event-stream`
- [ ] Connection stays open (persistent HTTP connection)
- [ ] Events are streamed in SSE format:
  ```
  event: file_created
  data: {"id":"uuid",...}

  ```
- [ ] Events include: `file_created`, `file_modified`, `file_deleted`, `indexing_complete`
- [ ] Client disconnects gracefully (cleanup on connection close)
- [ ] Client can reconnect and receive future events (broadcast is fire-and-forget — missed events are lost, which is acceptable)
- [ ] Connection sends a `connected` event on initial connection (with server timestamp)
- [ ] Multiple concurrent SSE clients are supported
- [ ] CORS headers present on SSE responses

## Implementation Notes

**SSE handler using broadcast + Axum:**
```rust
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Router,
};
use futures_util::stream::Stream;
use tokio_stream::wrappers::BroadcastStream;
use std::convert::Infallible;
use std::time::Duration;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/events", get(sse_handler))
}

async fn sse_handler(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Subscribe to broadcast channel
    let rx = state.sse_tx.subscribe();
    let stream = BroadcastStream::new(rx);
    
    let event_stream = stream.filter_map(|result| {
        match result {
            Ok(sse_event) => {
                let axum_event = Event::default()
                    .event(sse_event.event_type)
                    .data(sse_event.data.to_string());
                Some(Ok(axum_event))
            }
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                // Client missed N messages — send a resync event
                eprintln!("SSE client lagged by {} messages", n);
                Some(Ok(Event::default()
                    .event("lagged")
                    .data(format!(r#"{{"skipped":{}}}"#, n))))
            }
        }
    });
    
    Sse::new(event_stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(30))
                .text("keep-alive")
        )
}
```

**Keep-alive** — SSE connections can be silently dropped by proxies or load balancers. The `keep_alive` sends a comment (`: keep-alive\n\n`) every 30 seconds to maintain the connection.

**Connected event** — Send immediately on connection:
```rust
let connected_event = Event::default()
    .event("connected")
    .data(json!({"timestamp": chrono::Utc::now().to_rfc3339()}).to_string());

let stream_with_connected = futures_util::stream::once(async move {
    Ok(connected_event)
}).chain(event_stream);
```

**Event format compliance:**
```
event: file_created
data: {"id":"uuid-v4","filename":"ComfyUI_99999_.png","path":"2026-05-15/ComfyUI_99999_.png","mime_type":"image/png","thumbnail_url":"...","width":896,"height":1216}

event: file_deleted
data: {"id":"uuid-v4","path":"2025-08-05/ComfyUI_old.png"}

event: file_modified
data: {"id":"uuid-v4","filename":"ComfyUI_23767_.png","metadata_updated":true}

event: indexing_complete
data: {"total":14433,"duration_ms":2340}
```

Each event is terminated by a blank line (double newline).

**Error handling:**
- If the broadcast channel has no active subscribers, new subscribers connect normally
- If a subscriber is too slow (lagged), they receive a `lagged` event and should refresh their data
- If the connection drops, the client should reconnect with exponential backoff (handled by frontend in Wave 6.1)

## Test Strategy

```rust
#[tokio::test]
async fn test_sse_connection_established() {
    let app = setup_test_app_with_sse().await;
    
    let response = app.get("/api/v1/events").send().await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/event-stream"
    );
}

#[tokio::test]
async fn test_sse_receives_file_created_event() {
    let app = setup_test_app_with_sse().await;
    
    // Connect SSE client
    let mut stream = app.get("/api/v1/events").send().await.bytes_stream();
    
    // Trigger a file event by creating a file
    create_test_file_and_trigger_watcher(&app).await; // Helper that sends event to broadcast
    
    // Read first SSE event from stream
    let chunk = tokio::time::timeout(Duration::from_secs(2), stream.next()).await.unwrap();
    let text = String::from_utf8(chunk.unwrap().to_vec()).unwrap();
    
    assert!(text.contains("event: file_created"));
}

#[tokio::test]
async fn test_sse_multiple_clients() {
    let app = setup_test_app_with_sse().await;
    
    // Connect two clients
    let stream1 = app.get("/api/v1/events").send().await;
    let stream2 = app.get("/api/v1/events").send().await;
    
    // Both should receive the same events
}
```
