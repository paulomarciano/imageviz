//! Stage 3: Format and broadcast SSE events to connected clients.
//!
//! Constructs the appropriate [`SseEvent`](crate::watcher::handler::SseEvent)
//! based on the [`ChangeType`](crate::watcher::handler::ChangeType) and sends
//! it via the broadcast channel.  Events are silently dropped if no SSE clients
//! are subscribed.

use crate::metadata::detect::MediaInfo;
use crate::watcher::handler::ChangeType;
use crate::watcher::handler::SseEvent;
use crate::watcher::stages::store::StoreOutcome;
use serde_json::json;
use tokio::sync::broadcast;

/// Broadcast an SSE notification for a stored media item.
///
/// * `Created` → `"file_created"` event with full media info.
/// * `Updated` → `"file_modified"` event with minimal info.
/// * `Skipped` → no event; only a debug log line.
pub fn broadcast_change(
    sse_tx: &broadcast::Sender<SseEvent>,
    outcome: &StoreOutcome,
    media_info: &MediaInfo,
    filename: &str,
    relative_path: &str,
) {
    match outcome.change {
        ChangeType::Created => {
            let event = SseEvent {
                event_type: "file_created".into(),
                data: json!({
                    "id": outcome.id,
                    "filename": filename,
                    "path": relative_path,
                    "mime_type": media_info.mime_type,
                    "thumbnail_url": format!("/api/v1/media/{}/thumbnail", outcome.id),
                    "width": media_info.width,
                    "height": media_info.height,
                }),
            };
            let _ = sse_tx.send(event);
            tracing::debug!("Broadcasted file_created for {} (id={})", relative_path, outcome.id);
        }
        ChangeType::Updated => {
            let event = SseEvent {
                event_type: "file_modified".into(),
                data: json!({
                    "id": outcome.id,
                    "filename": filename,
                    "metadata_updated": true,
                }),
            };
            let _ = sse_tx.send(event);
            tracing::debug!("Broadcasted file_modified for {} (id={})", relative_path, outcome.id);
        }
        ChangeType::Skipped => {
            tracing::debug!("File {} unchanged (same hash) — skipping broadcast", relative_path);
        }
    }
}
