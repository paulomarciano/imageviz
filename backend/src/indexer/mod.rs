//! Indexer orchestration — scan → detect → hash → store pipeline.
//!
//! Wires together the scanner, file-type detector, hasher, and metadata extractor
//! into a single "index" operation that upserts media items into SQLite.
//!
//! # Architecture
//!
//! The indexer separates async I/O (hashing, ffprobe) from synchronous DB writes.
//! Async work runs without holding the database lock; DB operations are batched
//! in transactions of [`BATCH_SIZE`] files for write throughput.

use crate::config::AppConfig;
use crate::metadata::detect::{detect_media, MediaInfo};
use crate::metadata::png::parse_png_metadata;
use crate::scanner::hasher::compute_file_hash;
use crate::scanner::walker::{scan_folder, FileEntry};
use rusqlite::{params, Connection};
use rusqlite::OptionalExtension;
use std::path::Path;
use tokio::sync::Mutex;
use uuid::Uuid;

pub mod progress;

/// Batch size for transaction commits during indexing.
///
/// Committing every 100 files balances write throughput with memory usage,
/// preventing a single long-running transaction from holding the WAL checkpoint
/// for too long.
const BATCH_SIZE: usize = 100;

/// Run a full index of all watched folders.
///
/// Scans folders, detects file types, computes hashes, extracts metadata,
/// and stores everything in SQLite with upsert semantics.
///
/// The pipeline processes files in two phases per batch:
/// 1. **Async phase** (no DB lock): compute hash, detect media, extract PNG metadata
/// 2. **Sync phase** (DB lock held): query existing, compare checksum, upsert
///
/// Transactions are committed every [`BATCH_SIZE`] files for performance.
pub async fn full_index(
    db: &Mutex<Connection>,
    config: &AppConfig,
    progress: &progress::ProgressTracker,
) -> Result<IndexStats, IndexError> {
    progress.set_status(progress::IndexStatus::Scanning);

    let all_files = scan_all_folders(config)?;
    progress.set_total(all_files.len());

    progress.set_status(progress::IndexStatus::Indexing);
    let mut stats = IndexStats::default();

    if all_files.is_empty() {
        // Still need to clean up deleted items even when no files to index
        let conn = db.lock().await;
        let removed = remove_deleted_items(&conn, config)?;
        stats.deleted = removed;
        progress.set_status(progress::IndexStatus::Complete);
        return Ok(stats);
    }

    // Process files in batches to limit transaction size
    for chunk in all_files.chunks(BATCH_SIZE) {
        // Phase 1: Async I/O — compute hashes and metadata without DB lock
        let mut batch_results: Vec<ProcessedFile> = Vec::with_capacity(chunk.len());
        for file in chunk {
            progress.increment_processed(&file.relative_path);
            match process_file_metadata(file).await {
                Ok(processed) => batch_results.push(processed),
                Err(e) => {
                    stats.errors += 1;
                    progress.add_error(format!("{}: {}", file.relative_path, e));
                }
            }
        }

        // Phase 2: DB writes — lock, batch-transact, upsert
        let conn = db.lock().await;
        conn.execute_batch("BEGIN")?;
        for processed in &batch_results {
            match store_file(&conn, processed) {
                Ok(change) => match change {
                    IndexChange::Created => stats.created += 1,
                    IndexChange::Updated => stats.updated += 1,
                    IndexChange::Skipped => stats.skipped += 1,
                },
                Err(e) => {
                    stats.errors += 1;
                    progress.add_error(format!("{}: {}", processed.file.relative_path, e));
                }
            }
        }
        conn.execute_batch("COMMIT")?;
        // DB lock released here (conn drops)
    }

    // Clean up: remove DB entries for files no longer on disk.
    // Runs in its own lock cycle to avoid blocking the write path.
    {
        let conn = db.lock().await;
        let removed = remove_deleted_items(&conn, config)?;
        stats.deleted = removed;
    }

    progress.set_status(progress::IndexStatus::Complete);
    Ok(stats)
}

/// Run an incremental index (only processes new or modified files).
///
/// Currently delegates to [`full_index`]. A future optimization will skip
/// files with matching checksums at the scanner level once the initial
/// index is populated.
pub async fn incremental_index(
    db: &Mutex<Connection>,
    config: &AppConfig,
    progress: &progress::ProgressTracker,
) -> Result<IndexStats, IndexError> {
    full_index(db, config, progress).await
}

// ---------------------------------------------------------------------------
// Pipeline stages
// ---------------------------------------------------------------------------

/// Intermediate result from the async processing phase of a single file.
struct ProcessedFile<'a> {
    file: &'a FileEntry,
    new_hash: String,
    media_info: MediaInfo,
    metadata_json: Option<String>,
}

/// Phase 1: Compute hash, detect media, and extract PNG metadata for a file.
///
/// Runs asynchronously without holding the database lock. This phase handles
/// all I/O-bound work (SHA-256 via spawn_blocking, ffprobe for videos).
async fn process_file_metadata(file: &FileEntry) -> Result<ProcessedFile<'_>, IndexError> {
    let abs_path = Path::new(&file.absolute_path);

    let new_hash = compute_file_hash(abs_path).await?;
    let media_info = detect_media(abs_path).await?;

    // Extract PNG metadata (ComfyUI prompt/workflow) if applicable
    let metadata_json = if media_info.mime_type == "image/png" {
        parse_png_metadata(abs_path)
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

    Ok(ProcessedFile {
        file,
        new_hash,
        media_info,
        metadata_json,
    })
}

/// Phase 2: Store a processed file's data in the database.
///
/// Synchronous — must be called while holding the database lock.
/// Uses `INSERT OR REPLACE` for idempotent upserts.
fn store_file(conn: &Connection, processed: &ProcessedFile<'_>) -> Result<IndexChange, IndexError> {
    // Check if file already indexed with same hash (skip if unchanged)
    let existing: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT id, checksum FROM media_items WHERE relative_path = ?1",
            params![processed.file.relative_path],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    if let Some((_, Some(ref existing_hash))) = existing {
        if existing_hash == &processed.new_hash {
            return Ok(IndexChange::Skipped);
        }
    }

    // Determine change type and reuse existing UUID or generate new one
    let (id, change) = match existing {
        Some((existing_id, _)) => (existing_id, IndexChange::Updated),
        None => (Uuid::new_v4().to_string(), IndexChange::Created),
    };

    // Upsert into DB — INSERT OR REPLACE is idempotent
    conn.execute(
        "INSERT OR REPLACE INTO media_items
            (id, filename, relative_path, mime_type, width, height, file_size,
             file_created_at, file_modified_at, indexed_at, metadata_json, checksum)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'), ?10, ?11)",
        params![
            id,
            processed.file.filename,
            processed.file.relative_path,
            processed.media_info.mime_type,
            processed.media_info.width,
            processed.media_info.height,
            processed.media_info.file_size,
            processed.file.created_at,
            processed.file.modified_at,
            processed.metadata_json,
            processed.new_hash,
        ],
    )?;

    Ok(change)
}

/// Scan all watched folders and return merged file entries.
fn scan_all_folders(config: &AppConfig) -> Result<Vec<FileEntry>, IndexError> {
    let mut all = Vec::new();
    for folder in &config.watched_folders {
        let path = Path::new(&folder.path);
        let entries = scan_folder(path)?;
        all.extend(entries);
    }
    Ok(all)
}

/// Remove items from DB that no longer exist on disk.
///
/// Iterates over all `relative_path` values in the DB and checks whether
/// the corresponding file still exists in any watched folder. Deleted items
/// are removed to keep the database in sync with the filesystem.
fn remove_deleted_items(conn: &Connection, config: &AppConfig) -> Result<usize, IndexError> {
    let mut stmt = conn.prepare("SELECT relative_path FROM media_items")?;
    let db_paths: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    let mut removed = 0;
    for db_path in &db_paths {
        let exists = config.watched_folders.iter().any(|f| {
            let full_path = Path::new(&f.path).join(db_path);
            full_path.exists()
        });

        if !exists {
            conn.execute(
                "DELETE FROM media_items WHERE relative_path = ?1",
                params![db_path],
            )?;
            removed += 1;
        }
    }

    Ok(removed)
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Statistics from a single index run.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IndexStats {
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub deleted: usize,
    pub errors: usize,
}

/// Classification of what happened to a single file during indexing.
enum IndexChange {
    Created,
    Updated,
    Skipped,
}

/// Errors that can occur during indexing.
///
/// Wraps all sub-module errors to allow `?` propagation throughout the
/// orchestrator. Each variant preserves the original error context.
#[derive(Debug)]
pub enum IndexError {
    Scanner(crate::scanner::walker::WalkerError),
    Hash(crate::scanner::hasher::HashError),
    Detection(crate::metadata::detect::DetectionError),
    Png(crate::metadata::png::PngParseError),
    Db(rusqlite::Error),
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexError::Scanner(e) => write!(f, "Scanner error: {}", e),
            IndexError::Hash(e) => write!(f, "Hash error: {}", e),
            IndexError::Detection(e) => write!(f, "Detection error: {}", e),
            IndexError::Png(e) => write!(f, "PNG error: {}", e),
            IndexError::Db(e) => write!(f, "DB error: {}", e),
        }
    }
}

impl std::error::Error for IndexError {}

impl From<crate::scanner::walker::WalkerError> for IndexError {
    fn from(e: crate::scanner::walker::WalkerError) -> Self {
        IndexError::Scanner(e)
    }
}

impl From<crate::scanner::hasher::HashError> for IndexError {
    fn from(e: crate::scanner::hasher::HashError) -> Self {
        IndexError::Hash(e)
    }
}

impl From<crate::metadata::detect::DetectionError> for IndexError {
    fn from(e: crate::metadata::detect::DetectionError) -> Self {
        IndexError::Detection(e)
    }
}

impl From<crate::metadata::png::PngParseError> for IndexError {
    fn from(e: crate::metadata::png::PngParseError) -> Self {
        IndexError::Png(e)
    }
}

impl From<rusqlite::Error> for IndexError {
    fn from(e: rusqlite::Error) -> Self {
        IndexError::Db(e)
    }
}

#[cfg(test)]
#[path = "indexer_test.rs"]
mod tests;
