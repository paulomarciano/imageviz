//! Bridge between file system watcher events and the indexing/broadcast pipeline.
//!
//! Listens for [`FileEvent`] batches from the watcher on an `mpsc` channel, updates
//! SQLite and Tantivy, and broadcasts SSE events via a `tokio::sync::broadcast` channel.
//!
//! # Architecture
//!
//! The event handler is an infinite async loop spawned as a Tokio task. It processes
//! debounced file events in batches:
//!
//! 1. **Async I/O phase** (no DB lock): compute SHA-256 hash, detect media type and
//!    dimensions, extract PNG metadata.
//! 2. **Blocking phase** (`spawn_blocking`): lock SQLite, upsert/delete rows, update
//!    Tantivy index.
//! 3. **Broadcast phase**: fan out an SSE event to all connected clients (silently
//!    dropping if none are subscribed).
//!
//! Tantivy `commit()` is called once per batch, not per event, to amortise write
//! overhead. Individual file errors are logged but never crash the loop.

use crate::metadata::detect::detect_media;
use crate::metadata::png::parse_png_metadata;
use crate::scanner::hasher::compute_file_hash;
use crate::search::IndexManager;
use crate::watcher::FileEvent;
use rusqlite::OptionalExtension;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tantivy::doc;
use std::time::SystemTime;
use tokio::sync::{broadcast, mpsc, Mutex};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// An event to broadcast to SSE (Server-Sent Events) clients.
///
/// The `event_type` field identifies the kind of event (e.g. `"file_created"`,
/// `"file_deleted"`), and `data` carries the event-specific payload as JSON.
///
/// # Send / Sync
///
/// `SseEvent: Clone + Send` because `tokio::sync::broadcast::Sender::send`
/// requires `T: Clone + Send`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SseEvent {
    pub event_type: String,
    pub data: Value,
}

/// Whether a file was created, updated, or unchanged during event processing.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ChangeType {
    Created,
    Updated,
    Skipped,
}

/// Result of processing a single file event in the blocking phase.
struct Outcome {
    id: String,
    change: ChangeType,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Run the event handler loop, processing file events as they arrive.
///
/// Events are received from the file watcher in debounced batches. Each event
/// updates SQLite, the Tantivy search index, and broadcasts an SSE notification.
/// After processing all events in a batch, the Tantivy index is committed so
/// new documents are immediately visible to search queries.
///
/// # Errors
///
/// Individual event errors are logged but do not crash the loop. Tantivy commit
/// failures are also logged. The loop runs indefinitely until the `mpsc` channel
/// is closed (i.e., the watcher is dropped).
pub async fn run_event_handler(
    mut file_events_rx: mpsc::Receiver<Vec<FileEvent>>,
    db: Arc<Mutex<Connection>>,
    index_manager: Arc<IndexManager>,
    sse_tx: broadcast::Sender<SseEvent>,
) {
    while let Some(events) = file_events_rx.recv().await {
        for event in &events {
            if let Err(e) = handle_single_event(event, &db, &index_manager, &sse_tx).await {
                tracing::error!("Error handling event {:?}: {}", event.path(), e);
            }
        }
        // Commit Tantivy after each batch so new documents are searchable.
        if let Err(e) = index_manager.commit() {
            tracing::error!("Error committing Tantivy index: {}", e);
        }
    }
    tracing::info!("File event channel closed — event handler shutting down");
}

// ---------------------------------------------------------------------------
// Event processing
// ---------------------------------------------------------------------------

/// Process a single [`FileEvent`], updating SQLite, Tantivy, and broadcasting.
///
/// For `Modified` events the file existence is checked on disk: if the file
/// still exists it is treated as a create-or-modify; if it has been removed
/// it is treated as a deletion for robustness.
#[allow(clippy::type_complexity)]
async fn handle_single_event(
    event: &FileEvent,
    db: &Arc<Mutex<Connection>>,
    index_manager: &Arc<IndexManager>,
    sse_tx: &broadcast::Sender<SseEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    match event {
        FileEvent::Modified { path } | FileEvent::Created { path } => {
            if path.exists() {
                handle_file_created_or_modified(path, db, index_manager, sse_tx).await?;
            } else {
                tracing::debug!(
                    "Modified event for non-existent file — treating as delete: {}",
                    path.display()
                );
                handle_file_deleted(path, db, index_manager, sse_tx).await?;
            }
        }
        FileEvent::Deleted { path } => {
            handle_file_deleted(path, db, index_manager, sse_tx).await?;
        }
    }
    Ok(())
}

/// Handle a file that was created or modified on disk.
///
/// # Pipeline
///
/// 1. Compute SHA-256 hash (async, via `spawn_blocking` internally).
/// 2. Detect media type, dimensions, file size (async).
/// 3. Extract PNG metadata if applicable.
/// 4. Resolve relative path against configured watched folders.
/// 5. `spawn_blocking`: lock DB, compare hash, upsert SQLite + Tantivy.
/// 6. Broadcast `"file_created"` or `"file_modified"` event.
async fn handle_file_created_or_modified(
    path: &Path,
    db: &Arc<Mutex<Connection>>,
    index_manager: &Arc<IndexManager>,
    sse_tx: &broadcast::Sender<SseEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    // -- Phase 1: Async I/O (no DB lock held) ------------------------------

    let hash = compute_file_hash(path).await?;
    let media_info = detect_media(path).await?;

    // Extract ComfyUI metadata for PNG files.
    let metadata_json = if media_info.mime_type == "image/png" {
        parse_png_metadata(path)
            .ok()
            .and_then(|meta| {
                if meta.prompt.is_some() || meta.workflow.is_some() {
                    serde_json::to_string(&meta).ok()
                } else {
                    None
                }
            })
    } else {
        None
    };

    // Read file timestamps from the filesystem.
    let disk_metadata = tokio::fs::metadata(path).await?;
    let created_at_iso = disk_metadata
        .created()
        .or_else(|_| disk_metadata.modified())
        .map(system_time_to_iso)
        .unwrap_or_else(|_| chrono::Utc::now().to_rfc3339());
    let modified_at_iso = disk_metadata
        .modified()
        .map(system_time_to_iso)
        .unwrap_or_else(|_| chrono::Utc::now().to_rfc3339());

    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    // Resolve relative path by stripping the watched folder prefix.
    // This requires a brief DB lock to read the config.
    let relative_path = {
        let conn = db.lock().await;
        let watched = load_watched_folders(&conn)?;
        resolve_relative_path(path, &watched).ok_or_else(|| {
            format!(
                "File {} is not inside any configured watched folder",
                path.display()
            )
        })?
    };

    // Extract Copy values from media_info before the move closure, since the
    // struct itself must remain available for broadcast after spawn_blocking.
    let img_width = media_info.width;
    let img_height = media_info.height;
    let img_file_size = media_info.file_size;

    // Clone values needed inside the blocking closure.
    let db_clone = Arc::clone(db);
    let im_clone = Arc::clone(index_manager);
    let path_rel = relative_path.clone();
    let fname = filename.clone();
    let mime = media_info.mime_type.clone();
    let h = hash.clone();
    let meta = metadata_json.clone();
    let created = created_at_iso.clone();
    let modified = modified_at_iso.clone();

    // -- Phase 2: Blocking DB + Tantivy operations -------------------------

    let outcome = tokio::task::spawn_blocking(move || -> Result<Outcome, String> {
        let conn = db_clone.blocking_lock();

        // Check whether this file is already tracked in SQLite.
        let existing: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT id, checksum FROM media_items WHERE relative_path = ?1",
                params![path_rel],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| format!("DB query for existing file: {}", e))?;

        // If the file is known AND its checksum matches, skip entirely.
        if let Some((ref existing_id, Some(ref existing_hash))) = existing {
            if existing_hash == &h {
                return Ok(Outcome {
                    id: existing_id.clone(),
                    change: ChangeType::Skipped,
                });
            }
        }

        // Generate a new UUID for new files; reuse the existing one for updates.
        let (id, change) = match existing {
            Some((existing_id, _)) => (existing_id, ChangeType::Updated),
            None => (Uuid::new_v4().to_string(), ChangeType::Created),
        };

        let indexed_at = chrono::Utc::now().to_rfc3339();

        // Upsert the media item into SQLite (INSERT OR REPLACE is idempotent).
        conn.execute(
            "INSERT OR REPLACE INTO media_items
                (id, filename, relative_path, mime_type, width, height, file_size,
                 file_created_at, file_modified_at, indexed_at, metadata_json, checksum)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id,
                fname,
                path_rel,
                mime,
                img_width,
                img_height,
                img_file_size as i64,
                created,
                modified,
                indexed_at,
                meta,
                h,
            ],
        ).map_err(|e| format!("DB insert/update: {}", e))?;

        // Update the Tantivy search index:
        //   1. Delete the previous document for this file (if any).
        //   2. Add a new document with the latest data.
        im_clone.delete_document_by_field("id", &id)
            .map_err(|e| format!("Tantivy delete: {}", e))?;

        let schema = im_clone.schema();
        let id_field = schema.get_field("id").map_err(|e| format!("Schema field id: {}", e))?;
        let filename_field = schema.get_field("filename").map_err(|e| format!("Schema field filename: {}", e))?;
        let mime_type_field = schema.get_field("mime_type").map_err(|e| format!("Schema field mime_type: {}", e))?;
        let metadata_json_field = schema.get_field("metadata_json").map_err(|e| format!("Schema field metadata_json: {}", e))?;
        let created_at_field = schema.get_field("created_at").map_err(|e| format!("Schema field created_at: {}", e))?;
        let file_size_field = schema.get_field("file_size").map_err(|e| format!("Schema field file_size: {}", e))?;
        let width_field = schema.get_field("width").map_err(|e| format!("Schema field width: {}", e))?;
        let height_field = schema.get_field("height").map_err(|e| format!("Schema field height: {}", e))?;

        // Parse created_at to Tantivy DateTime for the date field.
        let created_ts = created
            .parse::<chrono::DateTime<chrono::Utc>>()
            .map(|dt| tantivy::DateTime::from_timestamp_secs(dt.timestamp()))
            .unwrap_or_else(|_| tantivy::DateTime::from_timestamp_secs(chrono::Utc::now().timestamp()));

        let doc = tantivy::doc!(
            id_field => id.as_str(),
            filename_field => fname.as_str(),
            mime_type_field => mime.as_str(),
            metadata_json_field => meta.unwrap_or_default().as_str(),
            created_at_field => created_ts,
            file_size_field => img_file_size,
            width_field => img_width.unwrap_or(0) as u64,
            height_field => img_height.unwrap_or(0) as u64,
        );

        im_clone.add_document(doc)
            .map_err(|e| format!("Tantivy add_document: {}", e))?;

        Ok(Outcome { id, change })
    })
    .await
    .map_err(|e| format!("Blocking task join error: {}", e))?
    .map_err(|e| format!("Handler error: {}", e))?;

    // -- Phase 3: Broadcast SSE notification --------------------------------
    // media_info is still available here (not moved into the closure).

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
            tracing::debug!(
                "Broadcasted file_created for {} (id={})",
                relative_path,
                outcome.id
            );
        }
        ChangeType::Updated => {
            let event = SseEvent {
                event_type: "file_modified".into(),
                data: json!({
                    "id": outcome.id,
                    "filename": filename,
                }),
            };
            let _ = sse_tx.send(event);
            tracing::debug!(
                "Broadcasted file_modified for {} (id={})",
                relative_path,
                outcome.id
            );
        }
        ChangeType::Skipped => {
            tracing::debug!(
                "File {} unchanged (same hash) — skipping broadcast",
                path.display()
            );
        }
    }

    Ok(())
}

/// Handle a file deletion.
///
/// Looks up the file by relative path in SQLite, removes the row and the
/// corresponding Tantivy document, and broadcasts a `"file_deleted"` event.
async fn handle_file_deleted(
    path: &Path,
    db: &Arc<Mutex<Connection>>,
    index_manager: &Arc<IndexManager>,
    sse_tx: &broadcast::Sender<SseEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    // Resolve relative path.
    let relative_path = {
        let conn = db.lock().await;
        let watched = load_watched_folders(&conn)?;
        match resolve_relative_path(path, &watched) {
            Some(rel) => rel,
            None => {
                tracing::warn!(
                    "Deleted file {} is not inside any watched folder — ignoring",
                    path.display()
                );
                return Ok(());
            }
        }
    };

    let db_clone = Arc::clone(db);
    let im_clone = Arc::clone(index_manager);
    let rel = relative_path.clone();

    // Blocking phase: delete from SQLite and Tantivy.
    let outcome = tokio::task::spawn_blocking(move || -> Result<Option<String>, String> {
        let conn = db_clone.blocking_lock();

        // Find the media item by relative path.
        let row: Option<(String,)> = conn
            .query_row(
                "SELECT id FROM media_items WHERE relative_path = ?1",
                params![rel],
                |r| Ok((r.get(0)?,)),
            )
            .optional()
            .map_err(|e| format!("DB query for existing file on delete: {}", e))?;

        let id = match row {
            Some((id,)) => id,
            None => return Ok(None), // Already deleted — nothing to do.
        };

        // Remove from SQLite.
        conn.execute("DELETE FROM media_items WHERE id = ?1", params![id])
            .map_err(|e| format!("DB delete: {}", e))?;

        // Remove from Tantivy index.
        im_clone.delete_document_by_field("id", &id)
            .map_err(|e| format!("Tantivy delete: {}", e))?;

        Ok(Some(id))
    })
    .await
    .map_err(|e| format!("Blocking task join error: {}", e))?
    .map_err(|e| format!("Delete handler error: {}", e))?;

    // Broadcast if the item was actually deleted.
    if let Some(id) = outcome {
        let event = SseEvent {
            event_type: "file_deleted".into(),
            data: json!({
                "id": id,
                "path": relative_path,
            }),
        };
        let _ = sse_tx.send(event);
        tracing::debug!("Broadcasted file_deleted for {} (id={})", relative_path, id);
    } else {
        tracing::debug!(
            "Delete event for unknown file {} — already removed",
            path.display()
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a `SystemTime` to an RFC 3339 / ISO 8601 string with sub-second
/// precision, matching the format used by the scanner walker.
fn system_time_to_iso(time: SystemTime) -> String {
    let duration = time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs() as i64;
    let nsecs = duration.subsec_nanos();
    chrono::DateTime::from_timestamp(secs, nsecs)
        .unwrap_or_default()
        .to_rfc3339()
}

/// Load watched folder paths from the database `config` table.
///
/// Reads the `watched_folders` JSON blob and returns a list of absolute paths.
/// Returns an empty list when no config entry exists.
fn load_watched_folders(conn: &Connection) -> Result<Vec<PathBuf>, Box<dyn std::error::Error + Send + Sync + 'static>> {
    let config = crate::config::load_config(conn)?;
    Ok(config
        .watched_folders
        .iter()
        .map(|f| PathBuf::from(&f.path))
        .collect())
}

/// Resolve a file's relative path by stripping the longest-matching watched
/// folder prefix.
///
/// Returns `None` if the absolute path does not reside inside any configured
/// watched folder.
fn resolve_relative_path(absolute_path: &Path, watched_folders: &[PathBuf]) -> Option<String> {
    watched_folders
        .iter()
        .find_map(|folder| absolute_path.strip_prefix(folder).ok())
        .map(|rel| rel.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::search::IndexManager;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    // -----------------------------------------------------------------------
    // Helper: create a real temp SQLite DB + Tantivy index for integration
    // -----------------------------------------------------------------------

    struct TestContext {
        _tantivy_dir: tempfile::TempDir,
        db: Arc<Mutex<Connection>>,
        index_manager: Arc<IndexManager>,
        sse_tx: broadcast::Sender<SseEvent>,
        sse_rx: broadcast::Receiver<SseEvent>,
        watched_path: tempfile::TempDir,
    }

    fn test_context() -> TestContext {
        let mut conn = db::open_in_memory().expect("in-memory DB");
        db::migrations::run_migrations(&mut conn).expect("migrations");

        // Seed watched folders config.
        let watched = tempfile::tempdir().expect("tempdir");
        let config = json!({
            "watched_folders": [
                {"path": watched.path().to_str().unwrap()}
            ]
        });
        conn.execute(
            "INSERT INTO config (key, value) VALUES ('watched_folders', ?1)",
            params![config.to_string()],
        )
        .expect("seed config");

        let tantivy_dir = tempfile::tempdir().expect("tempdir");
        let im =
            IndexManager::open_or_create(&tantivy_dir.path().join("tantivy")).expect("IndexManager");

        let (sse_tx, sse_rx) = broadcast::channel(256);

        TestContext {
            _tantivy_dir: tantivy_dir,
            db: Arc::new(Mutex::new(conn)),
            index_manager: Arc::new(im),
            sse_tx,
            sse_rx,
            watched_path: watched,
        }
    }

    fn create_test_png(path: &Path) {
        let img = image::RgbaImage::new(64, 48);
        img.save_with_format(path, image::ImageFormat::Png)
            .expect("create test PNG");
    }

    // -----------------------------------------------------------------------
    // load_watched_folders / resolve_relative_path tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_watched_folders_from_config() {
        let mut conn = db::open_in_memory().expect("in-memory DB");
        db::migrations::run_migrations(&mut conn).expect("migrations");

        let config = json!({
            "watched_folders": [
                {"path": "/tmp/a"},
                {"path": "/tmp/b"}
            ]
        });
        conn.execute(
            "INSERT INTO config (key, value) VALUES ('watched_folders', ?1)",
            params![config.to_string()],
        )
        .expect("seed");

        let folders = load_watched_folders(&conn).expect("load");
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0], PathBuf::from("/tmp/a"));
        assert_eq!(folders[1], PathBuf::from("/tmp/b"));
    }

    #[test]
    fn test_load_watched_folders_empty_when_no_config() {
        let mut conn = db::open_in_memory().expect("in-memory DB");
        db::migrations::run_migrations(&mut conn).expect("migrations");

        let folders = load_watched_folders(&conn).expect("load");
        assert!(folders.is_empty(), "no config row should return empty list");
    }

    #[test]
    fn test_resolve_relative_path_matches() {
        let folders = vec![PathBuf::from("/media/photos")];
        let result = resolve_relative_path(Path::new("/media/photos/vacation/beach.png"), &folders);
        assert_eq!(result.as_deref(), Some("vacation/beach.png"));
    }

    #[test]
    fn test_resolve_relative_path_no_match() {
        let folders = vec![PathBuf::from("/media/photos")];
        let result = resolve_relative_path(Path::new("/other/vacation/beach.png"), &folders);
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_relative_path_exact_match() {
        let folders = vec![PathBuf::from("/media/photos")];
        let result = resolve_relative_path(Path::new("/media/photos"), &folders);
        assert_eq!(result.as_deref(), Some(""));
    }

    #[test]
    fn test_resolve_relative_path_first_matching_folder_wins() {
        let folders = vec![
            PathBuf::from("/media/photos"),
            PathBuf::from("/media/photos/vacation"), // more specific
        ];
        let result =
            resolve_relative_path(Path::new("/media/photos/vacation/beach.png"), &folders);
        // The first folder in the list matches first.
        assert_eq!(result.as_deref(), Some("vacation/beach.png"));
    }

    // -----------------------------------------------------------------------
    // SseEvent broadcast tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_sse_event_serialization() {
        let event = SseEvent {
            event_type: "file_created".into(),
            data: json!({
                "id": "abc-123",
                "filename": "test.png",
                "path": "subdir/test.png",
            }),
        };

        let json_str = serde_json::to_string(&event).expect("serialize");
        assert!(json_str.contains("file_created"));
        assert!(json_str.contains("abc-123"));
        assert!(json_str.contains("test.png"));
    }

    // -----------------------------------------------------------------------
    // Integration tests (tokio)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_handle_file_created() {
        let mut ctx = test_context();

        // Create a test PNG in the watched folder.
        let file_path = ctx.watched_path.path().join("created.png");
        create_test_png(&file_path);

        let event = FileEvent::Modified {
            path: file_path.clone(),
        };

        handle_single_event(
            &event,
            &ctx.db,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("handle single event");

        // Verify the DB has an entry.
        let conn = ctx.db.lock().await;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 1, "should have one media item");

        let (id, filename, mime): (String, String, String) = conn
            .query_row(
                "SELECT id, filename, mime_type FROM media_items LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("query item");
        assert_eq!(filename, "created.png");
        assert_eq!(mime, "image/png");
        drop(conn);

        // Verify the Tantivy index has a document (must commit first since
        // handle_single_event adds documents but commit happens in run_event_handler).
        ctx.index_manager.commit().expect("commit Tantivy");
        let reader = ctx.index_manager.reader();
        let searcher = reader.searcher();
        let num_docs: u64 = searcher.segment_readers().iter().map(|sr| sr.num_docs() as u64).sum();
        assert_eq!(num_docs, 1, "Tantivy should have one document");

        // Verify SSE event was broadcast.
        let sse = ctx.sse_rx.recv().await.expect("SSE event");
        assert_eq!(sse.event_type, "file_created");
        assert_eq!(sse.data["id"], id);
        assert_eq!(sse.data["filename"], "created.png");
        assert!(sse.data["thumbnail_url"].as_str().unwrap().contains(&id));
    }

    #[tokio::test]
    async fn test_handle_file_modified() {
        let mut ctx = test_context();

        // First: create a file and index it.
        let file_path = ctx.watched_path.path().join("modified.png");
        create_test_png(&file_path);

        handle_single_event(
            &FileEvent::Modified { path: file_path.clone() },
            &ctx.db,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("first handle (create)");

        // Read back the ID and clear the SSE channel.
        let first_id: String = {
            let conn = ctx.db.lock().await;
            conn.query_row("SELECT id FROM media_items", [], |r| r.get(0))
                .expect("query id")
        };
        let _ = ctx.sse_rx.try_recv().ok(); // drain the file_created event

        // Second: modify the file (write different content) and re-index.
        // Create a new image of different size so the hash changes.
        {
            let img = image::RgbaImage::new(128, 96);
            img.save_with_format(&file_path, image::ImageFormat::Png)
                .expect("modify PNG");
        }

        handle_single_event(
            &FileEvent::Modified { path: file_path.clone() },
            &ctx.db,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("second handle (modify)");

        // Verify the same ID was reused.
        let conn = ctx.db.lock().await;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 1, "should still have only one media item");

        let second_id: String = conn
            .query_row("SELECT id FROM media_items", [], |r| r.get(0))
            .expect("query id");
        assert_eq!(second_id, first_id, "ID should be preserved across modify");
        drop(conn);

        // Verify SSE event was file_modified.
        let sse = ctx.sse_rx.try_recv().expect("SSE event");
        assert_eq!(sse.event_type, "file_modified");
        assert_eq!(sse.data["id"], first_id);
    }

    #[tokio::test]
    async fn test_handle_file_unchanged_skips() {
        let mut ctx = test_context();

        let file_path = ctx.watched_path.path().join("unchanged.png");
        create_test_png(&file_path);

        // First index.
        handle_single_event(
            &FileEvent::Modified { path: file_path.clone() },
            &ctx.db,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("first handle");
        let _ = ctx.sse_rx.try_recv().ok(); // drain

        // Re-index same file (content unchanged).
        handle_single_event(
            &FileEvent::Modified { path: file_path.clone() },
            &ctx.db,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("second handle (no change)");

        // No SSE event should be emitted for unchanged files.
        let result = ctx.sse_rx.try_recv();
        assert!(
            result.is_err(),
            "unchanged file should not broadcast an SSE event: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_handle_file_deleted() {
        let mut ctx = test_context();

        // Create and index a file.
        let file_path = ctx.watched_path.path().join("delete_me.png");
        create_test_png(&file_path);

        handle_single_event(
            &FileEvent::Modified { path: file_path.clone() },
            &ctx.db,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("create");
        let _ = ctx.sse_rx.try_recv().ok(); // drain

        // Delete the file.
        std::fs::remove_file(&file_path).expect("remove file");

        handle_single_event(
            &FileEvent::Deleted {
                path: file_path.clone(),
            },
            &ctx.db,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("delete");

        // DB should be empty.
        let conn = ctx.db.lock().await;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 0, "DB should have no items after delete");
        drop(conn);

        // Tantivy should be empty (commit first to flush buffered deletes).
        ctx.index_manager.commit().expect("commit Tantivy");
        let reader = ctx.index_manager.reader();
        let searcher = reader.searcher();
        let num_docs: u64 = searcher.segment_readers().iter().map(|sr| sr.num_docs() as u64).sum();
        assert_eq!(num_docs, 0, "Tantivy should have no documents after delete");

        // SSE event should be file_deleted.
        let sse = ctx.sse_rx.try_recv().expect("SSE event");
        assert_eq!(sse.event_type, "file_deleted");
        assert!(sse.data["id"].as_str().unwrap().len() > 0);
        assert!(sse.data["path"].as_str().unwrap().contains("delete_me.png"));
    }

    #[tokio::test]
    async fn test_handle_delete_unknown_file_silently_ignored() {
        let mut ctx = test_context();

        let fake_path = ctx.watched_path.path().join("never_indexed.png");

        handle_single_event(
            &FileEvent::Deleted {
                path: fake_path.clone(),
            },
            &ctx.db,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("delete unknown");

        // No SSE event should be emitted.
        let result = ctx.sse_rx.try_recv();
        assert!(
            result.is_err(),
            "delete of unknown file should not broadcast: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_modified_event_for_removed_file_triggers_delete() {
        let mut ctx = test_context();

        // Create and index a file.
        let file_path = ctx.watched_path.path().join("removed.png");
        create_test_png(&file_path);

        handle_single_event(
            &FileEvent::Modified { path: file_path.clone() },
            &ctx.db,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("create");
        let _ = ctx.sse_rx.try_recv().ok(); // drain

        // Remove the file, then send a Modified event (file no longer exists).
        std::fs::remove_file(&file_path).expect("remove");

        handle_single_event(
            &FileEvent::Modified { path: file_path.clone() },
            &ctx.db,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("modified-without-file");

        // DB should be empty (treated as delete).
        let conn = ctx.db.lock().await;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 0, "modified event for missing file should act as delete");
    }

    #[tokio::test]
    async fn test_event_outside_watched_folder_is_skipped() {
        let ctx = test_context();

        // Create a file outside the watched folder.
        let outside = tempfile::tempdir().expect("tempdir");
        let file_path = outside.path().join("outside.png");
        create_test_png(&file_path);

        let result = handle_single_event(
            &FileEvent::Modified {
                path: file_path.clone(),
            },
            &ctx.db,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await;

        assert!(result.is_err(), "file outside watched folder should error");
    }
}
