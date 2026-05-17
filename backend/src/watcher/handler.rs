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

use crate::search::IndexManager;
use crate::watcher::FileEvent;
use crate::watcher::stages;
use r2d2::Pool;

use crate::db::SqliteConnectionManager;
use rusqlite::OptionalExtension;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{broadcast, mpsc};
#[cfg(test)]
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
pub enum ChangeType {
    Created,
    Updated,
    Skipped,
}

/// Result of processing a single file event in the blocking phase.
#[allow(dead_code)]
pub(crate) struct Outcome {
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
    pool: Pool<SqliteConnectionManager>,
    index_manager: Arc<IndexManager>,
    sse_tx: broadcast::Sender<SseEvent>,
) {
    while let Some(events) = file_events_rx.recv().await {
        for event in &events {
            if let Err(e) = handle_single_event(event, &pool, &index_manager, &sse_tx).await {
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
    pool: &Pool<SqliteConnectionManager>,
    index_manager: &Arc<IndexManager>,
    sse_tx: &broadcast::Sender<SseEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    match event {
        FileEvent::Modified { path } | FileEvent::Created { path } => {
            if path.exists() {
                handle_file_created_or_modified(path, pool, index_manager, sse_tx).await?;
            } else {
                tracing::debug!(
                    "Modified event for non-existent file — treating as delete: {}",
                    path.display()
                );
                handle_file_deleted(path, pool, index_manager, sse_tx).await?;
            }
        }
        FileEvent::Deleted { path } => {
            handle_file_deleted(path, pool, index_manager, sse_tx).await?;
        }
    }
    Ok(())
}

/// Handle a file that was created or modified on disk.
///
/// Delegates to the three-stage pipeline:
/// 1. [`stages::extract::extract_file_data`] — async disk I/O.
/// 2. [`stages::store::store_media`] — blocking DB + Tantivy operations.
/// 3. [`stages::broadcast::broadcast_change`] — SSE notification.
async fn handle_file_created_or_modified(
    path: &Path,
    pool: &Pool<SqliteConnectionManager>,
    index_manager: &Arc<IndexManager>,
    sse_tx: &broadcast::Sender<SseEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    // Phase 1: Extract file data from disk (async I/O, no DB lock held).
    let extracted = stages::extract::extract_file_data(path).await?;

    // Phase 2: Store in SQLite + Tantivy (blocking, spawned on a dedicated thread).
    let pool_clone = pool.clone();
    let im_clone = Arc::clone(index_manager);
    let path_buf = path.to_path_buf();
    let extracted_for_blocking = extracted.clone();

    let outcome = tokio::task::spawn_blocking(move || {
        stages::store::store_media(&pool_clone, &im_clone, &extracted_for_blocking, &path_buf)
    })
    .await
    .map_err(|e| format!("Blocking task join error: {}", e))?
    .map_err(|e| format!("Handler error: {}", e))?;

    // Phase 3: Broadcast SSE notification to connected clients.
    stages::broadcast::broadcast_change(
        sse_tx,
        &outcome,
        &extracted.media_info,
        &extracted.filename,
        &outcome.relative_path,
    );

    Ok(())
}

/// Handle a file deletion.
///
/// Looks up the file by relative path in SQLite, removes the row and the
/// corresponding Tantivy document, and broadcasts a `"file_deleted"` event.
async fn handle_file_deleted(
    path: &Path,
    pool: &Pool<SqliteConnectionManager>,
    index_manager: &Arc<IndexManager>,
    sse_tx: &broadcast::Sender<SseEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    // Resolve relative path and folder_id.
    let (relative_path, folder_id) = {
        let conn = pool.get()?;
        let watched = load_watched_folders(&conn)?;
        match resolve_relative_path(path, &watched) {
            Some(result) => result,
            None => {
                tracing::warn!(
                    "Deleted file {} is not inside any watched folder — ignoring",
                    path.display()
                );
                return Ok(());
            }
        }
    };

    let pool_clone = pool.clone();
    let im_clone = Arc::clone(index_manager);
    let rel = relative_path.clone();
    let fid = folder_id.clone();

    // Blocking phase: delete from SQLite and Tantivy.
    let outcome = tokio::task::spawn_blocking(move || -> Result<Option<String>, String> {
        let conn = pool_clone.get().map_err(|e| format!("Pool error: {}", e))?;

        // Find the media item by folder_id + relative_path.
        let row: Option<(String,)> = conn
            .query_row(
                "SELECT id FROM media_items WHERE folder_id = ?1 AND relative_path = ?2",
                params![fid, rel],
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
        im_clone
            .delete_document_by_field("id", &id)
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
        tracing::debug!("Delete event for unknown file {} — already removed", path.display());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a `SystemTime` to an RFC 3339 / ISO 8601 string with sub-second
/// precision, matching the format used by the scanner walker.
pub(crate) fn system_time_to_iso(time: SystemTime) -> String {
    let duration = time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs() as i64;
    let nsecs = duration.subsec_nanos();
    chrono::DateTime::from_timestamp(secs, nsecs).unwrap_or_default().to_rfc3339()
}

/// Load watched folder paths from the database `config` table.
///
/// Reads the `watched_folders` JSON blob and returns a list of absolute paths.
/// Returns an empty list when no config entry exists.
pub(crate) fn load_watched_folders(
    conn: &Connection,
) -> Result<Vec<(PathBuf, String)>, Box<dyn std::error::Error + Send + Sync + 'static>> {
    let config = crate::config::load_config(conn)?;
    Ok(config
        .watched_folders
        .iter()
        .filter_map(|f| f.id.as_ref().map(|id| (PathBuf::from(&f.path), id.clone())))
        .collect())
}

/// Resolve a file's relative path and folder_id by stripping the
/// longest-matching watched folder prefix.
///
/// Returns `(relative_path, folder_id)` if the absolute path resides inside
/// a configured watched folder, or `None` otherwise.
pub(crate) fn resolve_relative_path(
    absolute_path: &Path,
    watched_folders: &[(PathBuf, String)],
) -> Option<(String, String)> {
    watched_folders.iter().find_map(|(folder_path, folder_id)| {
        absolute_path
            .strip_prefix(folder_path)
            .ok()
            .map(|rel| (rel.to_string_lossy().into_owned(), folder_id.clone()))
    })
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

    // -----------------------------------------------------------------------
    // Helper: create a real temp SQLite DB + Tantivy index for integration
    // -----------------------------------------------------------------------

    struct TestContext {
        _tantivy_dir: tempfile::TempDir,
        pool: Pool<SqliteConnectionManager>,
        index_manager: Arc<IndexManager>,
        sse_tx: broadcast::Sender<SseEvent>,
        sse_rx: broadcast::Receiver<SseEvent>,
        watched_path: tempfile::TempDir,
    }

    fn test_context() -> TestContext {
        let pool = crate::db::pool::create_in_memory_pool();
        {
            let mut conn = pool.get().expect("in-memory conn");
            db::migrations::run_migrations(&mut conn).expect("migrations");
        }

        // Seed watched folders config with a stable folder ID.
        let watched = tempfile::tempdir().expect("tempdir");
        let watched_path = watched.path().to_str().unwrap().to_string();
        let fid = Uuid::new_v4().to_string();
        let config = json!({
            "watched_folders": [
                {"path": watched_path, "id": fid}
            ]
        });
        {
            let conn = pool.get().expect("get conn");
            conn.execute(
                "INSERT INTO config (key, value) VALUES ('watched_folders', ?1)",
                params![config.to_string()],
            )
            .expect("seed config");
            // Also seed the watched_folders registry table.
            conn.execute(
                "INSERT OR IGNORE INTO watched_folders (id, path) VALUES (?1, ?2)",
                params![fid, watched_path],
            )
            .expect("seed watched_folders");
        }

        let tantivy_dir = tempfile::tempdir().expect("tempdir");
        let im = IndexManager::open_or_create(&tantivy_dir.path().join("tantivy"), 50_000_000)
            .expect("IndexManager");

        let (sse_tx, sse_rx) = broadcast::channel(256);

        TestContext {
            _tantivy_dir: tantivy_dir,
            pool,
            index_manager: Arc::new(im),
            sse_tx,
            sse_rx,
            watched_path: watched,
        }
    }

    fn create_test_png(path: &Path) {
        let img = image::RgbaImage::new(64, 48);
        img.save_with_format(path, image::ImageFormat::Png).expect("create test PNG");
    }

    // -----------------------------------------------------------------------
    // load_watched_folders / resolve_relative_path tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_watched_folders_from_config() {
        let mut conn = db::open_in_memory().expect("in-memory DB");
        db::migrations::run_migrations(&mut conn).expect("migrations");

        // Seed watched_folders entries.
        conn.execute("INSERT INTO watched_folders (id, path) VALUES ('fid1', '/tmp/a')", [])
            .expect("seed watched_folders");
        conn.execute("INSERT INTO watched_folders (id, path) VALUES ('fid2', '/tmp/b')", [])
            .expect("seed watched_folders");

        let config = json!({
            "watched_folders": [
                {"path": "/tmp/a", "id": "fid1"},
                {"path": "/tmp/b", "id": "fid2"}
            ]
        });
        conn.execute(
            "INSERT INTO config (key, value) VALUES ('watched_folders', ?1)",
            params![config.to_string()],
        )
        .expect("seed");

        let folders = load_watched_folders(&conn).expect("load");
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].0, PathBuf::from("/tmp/a"));
        assert_eq!(folders[1].0, PathBuf::from("/tmp/b"));
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
        let folders = vec![(PathBuf::from("/media/photos"), "fid1".to_string())];
        let result = resolve_relative_path(Path::new("/media/photos/vacation/beach.png"), &folders);
        assert_eq!(result.as_ref().map(|(r, _)| r.as_str()), Some("vacation/beach.png"));
        assert_eq!(result.as_ref().map(|(_, f)| f.as_str()), Some("fid1"));
    }

    #[test]
    fn test_resolve_relative_path_no_match() {
        let folders = vec![(PathBuf::from("/media/photos"), "fid1".to_string())];
        let result = resolve_relative_path(Path::new("/other/vacation/beach.png"), &folders);
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_relative_path_exact_match() {
        let folders = vec![(PathBuf::from("/media/photos"), "fid1".to_string())];
        let result = resolve_relative_path(Path::new("/media/photos"), &folders);
        assert_eq!(result.as_ref().map(|(r, _)| r.as_str()), Some(""));
        assert_eq!(result.as_ref().map(|(_, f)| f.as_str()), Some("fid1"));
    }

    #[test]
    fn test_resolve_relative_path_first_matching_folder_wins() {
        let folders = vec![
            (PathBuf::from("/media/photos"), "fid1".to_string()),
            (PathBuf::from("/media/photos/vacation"), "fid2".to_string()),
        ];
        let result = resolve_relative_path(Path::new("/media/photos/vacation/beach.png"), &folders);
        // The first folder in the list matches first.
        assert_eq!(result.as_ref().map(|(r, _)| r.as_str()), Some("vacation/beach.png"));
        assert_eq!(result.as_ref().map(|(_, f)| f.as_str()), Some("fid1"));
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

    #[test]
    fn test_indexing_complete_event_includes_duration_ms() {
        let event = SseEvent {
            event_type: "indexing_complete".into(),
            data: json!({
                "total": 14433,
                "duration_ms": 2340,
            }),
        };

        let json_str = serde_json::to_string(&event).expect("serialize");
        assert!(json_str.contains("indexing_complete"), "event_type should be indexing_complete");
        assert!(
            json_str.contains("\"duration_ms\":2340"),
            "indexing_complete must include duration_ms in payload, got: {json_str}"
        );
        assert!(json_str.contains("\"total\":14433"), "indexing_complete must include total");
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

        let event = FileEvent::Modified { path: file_path.clone() };

        handle_single_event(&event, &ctx.pool, &ctx.index_manager, &ctx.sse_tx)
            .await
            .expect("handle single event");

        // Verify the DB has an entry.
        let conn = ctx.pool.get().expect("get conn");
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0)).expect("count");
        assert_eq!(count, 1, "should have one media item");

        let (id, filename, mime): (String, String, String) = conn
            .query_row("SELECT id, filename, mime_type FROM media_items LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
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
            &ctx.pool,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("first handle (create)");

        // Read back the ID and clear the SSE channel.
        let first_id: String = {
            let conn = ctx.pool.get().expect("get conn");
            conn.query_row("SELECT id FROM media_items", [], |r| r.get(0)).expect("query id")
        };
        let _ = ctx.sse_rx.try_recv().ok(); // drain the file_created event

        // Second: modify the file (write different content) and re-index.
        // Create a new image of different size so the hash changes.
        {
            let img = image::RgbaImage::new(128, 96);
            img.save_with_format(&file_path, image::ImageFormat::Png).expect("modify PNG");
        }

        handle_single_event(
            &FileEvent::Modified { path: file_path.clone() },
            &ctx.pool,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("second handle (modify)");

        // Verify the same ID was reused.
        let conn = ctx.pool.get().expect("get conn");
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0)).expect("count");
        assert_eq!(count, 1, "should still have only one media item");

        let second_id: String =
            conn.query_row("SELECT id FROM media_items", [], |r| r.get(0)).expect("query id");
        assert_eq!(second_id, first_id, "ID should be preserved across modify");
        drop(conn);

        // Verify SSE event was file_modified with all required fields.
        let sse = ctx.sse_rx.try_recv().expect("SSE event");
        assert_eq!(sse.event_type, "file_modified");
        assert_eq!(sse.data["id"], first_id);
        assert_eq!(
            sse.data["metadata_updated"], true,
            "file_modified must include metadata_updated"
        );
    }

    #[tokio::test]
    async fn test_handle_file_unchanged_skips() {
        let mut ctx = test_context();

        let file_path = ctx.watched_path.path().join("unchanged.png");
        create_test_png(&file_path);

        // First index.
        handle_single_event(
            &FileEvent::Modified { path: file_path.clone() },
            &ctx.pool,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("first handle");
        let _ = ctx.sse_rx.try_recv().ok(); // drain

        // Re-index same file (content unchanged).
        handle_single_event(
            &FileEvent::Modified { path: file_path.clone() },
            &ctx.pool,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("second handle (no change)");

        // No SSE event should be emitted for unchanged files.
        let result = ctx.sse_rx.try_recv();
        assert!(result.is_err(), "unchanged file should not broadcast an SSE event: {:?}", result);
    }

    #[tokio::test]
    async fn test_handle_file_deleted() {
        let mut ctx = test_context();

        // Create and index a file.
        let file_path = ctx.watched_path.path().join("delete_me.png");
        create_test_png(&file_path);

        handle_single_event(
            &FileEvent::Modified { path: file_path.clone() },
            &ctx.pool,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("create");
        let _ = ctx.sse_rx.try_recv().ok(); // drain

        // Delete the file.
        std::fs::remove_file(&file_path).expect("remove file");

        handle_single_event(
            &FileEvent::Deleted { path: file_path.clone() },
            &ctx.pool,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("delete");

        // DB should be empty.
        let conn = ctx.pool.get().expect("get conn");
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0)).expect("count");
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
            &FileEvent::Deleted { path: fake_path.clone() },
            &ctx.pool,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("delete unknown");

        // No SSE event should be emitted.
        let result = ctx.sse_rx.try_recv();
        assert!(result.is_err(), "delete of unknown file should not broadcast: {:?}", result);
    }

    #[tokio::test]
    async fn test_modified_event_for_removed_file_triggers_delete() {
        let mut ctx = test_context();

        // Create and index a file.
        let file_path = ctx.watched_path.path().join("removed.png");
        create_test_png(&file_path);

        handle_single_event(
            &FileEvent::Modified { path: file_path.clone() },
            &ctx.pool,
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
            &ctx.pool,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await
        .expect("modified-without-file");

        // DB should be empty (treated as delete).
        let conn = ctx.pool.get().expect("get conn");
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0)).expect("count");
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
            &FileEvent::Modified { path: file_path.clone() },
            &ctx.pool,
            &ctx.index_manager,
            &ctx.sse_tx,
        )
        .await;

        assert!(result.is_err(), "file outside watched folder should error");
    }
}
