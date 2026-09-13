//! Stage 2: Store file data in SQLite and update the Tantivy search index.
//!
//! This module is designed to run inside `tokio::task::spawn_blocking` — all
//! operations are synchronous and CPU-bound.  It receives the watched-folder
//! snapshot loaded once per event batch by the handler (R8), resolves the
//! relative path, checks for existing entries by checksum, upserts, and
//! re-indexes.

use crate::db::SqliteConnectionManager;
use crate::search::IndexManager;
use crate::watcher::handler::{ChangeType, WatchedFolderSnapshot, resolve_relative_path};
use crate::watcher::stages::extract::ExtractedData;
use r2d2::Pool;
use rusqlite::OptionalExtension;
use rusqlite::params;
use std::path::Path;
use std::sync::Arc;
use tantivy::doc;
use uuid::Uuid;

/// Outcome of storing a single media item.
#[derive(Debug, Clone)]
pub struct StoreOutcome {
    /// The SQLite / Tantivy document ID (newly generated or reused).
    pub id: String,
    /// What kind of change was detected.
    pub change: ChangeType,
    /// Path relative to the containing watched folder.
    pub relative_path: String,
}

/// Store or update a media item in the database and search index.
///
/// # Operations
///
/// 1. Resolves the file's relative path against the watched-folder snapshot
///    loaded once per batch by the handler (R8 — no per-event config reload).
/// 2. Checks for an existing row keyed by `(folder_id, relative_path)`.
/// 3. If the row exists and its checksum matches, returns `Skipped`.
/// 4. Otherwise, upserts the SQLite row and updates the Tantivy index.
///
/// This function is synchronous and intended to be called from within
/// `tokio::task::spawn_blocking`.
pub fn store_media(
    pool: &Pool<SqliteConnectionManager>,
    index_manager: &Arc<IndexManager>,
    data: &ExtractedData,
    path: &Path,
    watched: &WatchedFolderSnapshot,
) -> Result<StoreOutcome, String> {
    let conn = pool.get().map_err(|e| format!("Pool error: {}", e))?;

    // Resolve relative path and folder_id by stripping the watched folder prefix.
    let (relative_path, folder_id) = resolve_relative_path(path, watched).ok_or_else(|| {
        format!("File {} is not inside any configured watched folder", path.display())
    })?;

    // Check whether this file is already tracked in SQLite (by folder + path).
    let existing: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT id, checksum FROM media_items WHERE folder_id = ?1 AND relative_path = ?2",
            params![folder_id, relative_path],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| format!("DB query for existing file: {}", e))?;

    // If the file is known AND its checksum matches, skip entirely.
    if let Some((ref existing_id, Some(ref existing_hash))) = existing
        && existing_hash == &data.hash
    {
        return Ok(StoreOutcome {
            id: existing_id.clone(),
            change: ChangeType::Skipped,
            relative_path,
        });
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
             file_created_at, file_modified_at, indexed_at, metadata_json, checksum,
             folder_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            id,
            data.filename,
            relative_path,
            data.media_info.mime_type,
            data.media_info.width,
            data.media_info.height,
            data.media_info.file_size as i64,
            data.created_at_iso,
            data.modified_at_iso,
            indexed_at,
            data.metadata_json,
            data.hash,
            folder_id,
        ],
    )
    .map_err(|e| format!("DB insert/update: {}", e))?;

    // Update the Tantivy search index:
    //   1. Delete the previous document for this file (updates only — for new
    //      files the id is fresh so the delete would be a wasteful no-op).
    //   2. Add a new document with the latest data.
    if change == ChangeType::Updated {
        index_manager
            .delete_document_by_field("id", &id)
            .map_err(|e| format!("Tantivy delete: {}", e))?;
    }

    let schema = index_manager.schema();
    let id_field = schema.get_field("id").map_err(|e| format!("Schema field id: {}", e))?;
    let filename_field =
        schema.get_field("filename").map_err(|e| format!("Schema field filename: {}", e))?;
    let mime_type_field =
        schema.get_field("mime_type").map_err(|e| format!("Schema field mime_type: {}", e))?;
    let metadata_json_field = schema
        .get_field("metadata_json")
        .map_err(|e| format!("Schema field metadata_json: {}", e))?;
    let created_at_field =
        schema.get_field("created_at").map_err(|e| format!("Schema field created_at: {}", e))?;
    let file_size_field =
        schema.get_field("file_size").map_err(|e| format!("Schema field file_size: {}", e))?;
    let width_field =
        schema.get_field("width").map_err(|e| format!("Schema field width: {}", e))?;
    let height_field =
        schema.get_field("height").map_err(|e| format!("Schema field height: {}", e))?;

    // Parse created_at to Tantivy DateTime for the date field.
    let created_ts = data
        .created_at_iso
        .parse::<chrono::DateTime<chrono::Utc>>()
        .map(|dt| tantivy::DateTime::from_timestamp_secs(dt.timestamp()))
        .unwrap_or_else(|_| tantivy::DateTime::from_timestamp_secs(chrono::Utc::now().timestamp()));

    let doc = tantivy::doc!(
        id_field => id.as_str(),
        filename_field => data.filename.as_str(),
        mime_type_field => data.media_info.mime_type.as_str(),
        metadata_json_field => data.metadata_json.clone().unwrap_or_default().as_str(),
        created_at_field => created_ts,
        file_size_field => data.media_info.file_size,
        width_field => data.media_info.width.unwrap_or(0) as u64,
        height_field => data.media_info.height.unwrap_or(0) as u64,
    );

    index_manager.add_document(doc).map_err(|e| format!("Tantivy add_document: {}", e))?;

    Ok(StoreOutcome { id, change, relative_path })
}
