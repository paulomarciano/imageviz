//! Indexer orchestration — scan → detect → hash → store pipeline.
//!
//! Wires together the scanner, file-type detector, hasher, and metadata extractor
//! into a single "index" operation that upserts media items into SQLite.
//!
//! # Architecture
//!
//! The indexer separates async I/O (hashing, ffprobe) from synchronous DB writes.
//! Async work runs without holding a database connection from the pool; DB
//! operations are batched in transactions of [`BATCH_SIZE`] files for write
//! throughput.

use crate::config::AppConfig;
use crate::config::folder_id_map;
use crate::metadata::detect::{MediaInfo, detect_media};
use crate::metadata::png::parse_png_metadata;
use crate::scanner::hasher::compute_file_hash;
use crate::scanner::walker::{FileEntry, scan_folder};
use r2d2::Pool;

use crate::db::SqliteConnectionManager;
use rusqlite::OptionalExtension;
use rusqlite::{Connection, params};
use std::collections::HashMap;
use std::path::Path;
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
    pool: &Pool<SqliteConnectionManager>,
    config: &AppConfig,
    progress: &progress::ProgressTracker,
) -> Result<IndexStats, IndexError> {
    // Ensure all watched folders have stable UUIDs before scanning.
    let mut config = config.clone();
    {
        let conn = pool.get()?;
        crate::config::assign_folder_ids(&conn, &mut config)?;
    }

    let fid_map = folder_id_map(&config);
    let all_files = scan_all_folders(&config)?;
    progress.set_total(all_files.len());

    progress.set_status(progress::IndexStatus::Indexing);
    let mut stats = IndexStats::default();

    if all_files.is_empty() {
        // Still need to clean up deleted items even when no files to index
        let conn = pool.get()?;
        let removed = remove_deleted_items(&conn, &config)?;
        stats.deleted = removed;
        progress.set_status(progress::IndexStatus::Complete);
        return Ok(stats);
    }

    // Build a list of (folder_id, file) pairs by looking up each file's
    // watched folder from the config.
    let folder_file_pairs = resolve_folder_file_pairs(&all_files, &fid_map);

    // Process files in batches to limit transaction size
    for chunk in folder_file_pairs.chunks(BATCH_SIZE) {
        // Phase 1: Async I/O — compute hashes and metadata without DB lock
        let mut batch_results: Vec<ProcessedFile> = Vec::with_capacity(chunk.len());
        for ff_entry in chunk {
            progress.increment_processed();
            match process_file_metadata(ff_entry.file, &ff_entry.folder_id).await {
                Ok(processed) => batch_results.push(processed),
                Err(e) => {
                    stats.errors += 1;
                    progress.add_error(format!("{}: {}", ff_entry.file.relative_path, e));
                }
            }
        }

        // Phase 2: DB writes — acquire connection from pool, batch-transact, upsert
        let conn = pool.get()?;
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
    // Runs with its own connection from the pool.
    {
        let conn = pool.get()?;
        let removed = remove_deleted_items(&conn, &config)?;
        stats.deleted = removed;
    }

    progress.set_status(progress::IndexStatus::Complete);
    Ok(stats)
}

/// Run an incremental index — only processes new or modified files.
///
/// Before scanning, loads all existing DB entries to build a map of
/// `(folder_id, relative_path) → (file_size, file_modified_at)`.  During
/// scanning, files whose size AND modification time match the DB entry
/// are skipped entirely (no SHA-256 hashing, no media detection, no DB
/// write).  New or modified files go through the full pipeline.
///
/// This avoids the O(n) hash + detect cost of a full re-scan for the
/// common case where most files are unchanged.
pub async fn incremental_index(
    pool: &Pool<SqliteConnectionManager>,
    config: &AppConfig,
    progress: &progress::ProgressTracker,
) -> Result<IndexStats, IndexError> {
    // Ensure all watched folders have stable UUIDs before scanning.
    let mut config = config.clone();
    {
        let conn = pool.get()?;
        crate::config::assign_folder_ids(&conn, &mut config)?;
    }

    // Load existing entries from DB so we can skip unchanged files.
    let existing: HashMap<(String, String), (i64, String)> = {
        let conn = pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT folder_id, relative_path, file_size, file_modified_at FROM media_items",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                (row.get::<_, Option<String>>(0)?.unwrap_or_default(), row.get::<_, String>(1)?),
                (row.get::<_, i64>(2)?, row.get::<_, String>(3)?),
            ))
        })?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let fid_map = folder_id_map(&config);
    let all_files = scan_all_folders(&config)?;
    progress.set_total(all_files.len());

    progress.set_status(progress::IndexStatus::Indexing);
    let mut stats = IndexStats::default();

    if all_files.is_empty() {
        let conn = pool.get()?;
        let removed = remove_deleted_items(&conn, &config)?;
        stats.deleted = removed;
        progress.set_status(progress::IndexStatus::Complete);
        return Ok(stats);
    }

    let folder_file_pairs = resolve_folder_file_pairs(&all_files, &fid_map);

    for chunk in folder_file_pairs.chunks(BATCH_SIZE) {
        // Phase 1: Async I/O — only for files that appear new or modified
        let mut batch_results: Vec<ProcessedFile> = Vec::with_capacity(chunk.len());
        for ff_entry in chunk {
            progress.increment_processed();

            // Quick check against existing DB metadata — skip if size AND
            // mtime match (file is extremely likely to be unchanged).
            let key = (ff_entry.folder_id.clone(), ff_entry.file.relative_path.clone());
            if let Some((existing_size, existing_mtime)) = existing.get(&key)
                && *existing_size == ff_entry.file.file_size as i64
                && *existing_mtime == ff_entry.file.modified_at
            {
                stats.skipped += 1;
                continue;
            }

            // File is new or modified — run the full pipeline.
            match process_file_metadata(ff_entry.file, &ff_entry.folder_id).await {
                Ok(processed) => batch_results.push(processed),
                Err(e) => {
                    stats.errors += 1;
                    progress.add_error(format!("{}: {}", ff_entry.file.relative_path, e));
                }
            }
        }

        // Phase 2: DB writes — batch-transact only if there are changes
        if !batch_results.is_empty() {
            let conn = pool.get()?;
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
        }
    }

    // Clean up: remove DB entries for files no longer on disk.
    {
        let conn = pool.get()?;
        let removed = remove_deleted_items(&conn, &config)?;
        stats.deleted = removed;
    }

    progress.set_status(progress::IndexStatus::Complete);
    Ok(stats)
}

// ---------------------------------------------------------------------------
// Pipeline stages
// ---------------------------------------------------------------------------

/// Pair a file entry with its watched folder ID.
#[derive(Debug)]
struct FolderFileEntry<'a> {
    folder_id: String,
    file: &'a FileEntry,
}

/// Intermediate result from the async processing phase of a single file.
struct ProcessedFile<'a> {
    file: &'a FileEntry,
    new_hash: String,
    media_info: MediaInfo,
    metadata_json: Option<String>,
    folder_id: String,
}

/// Resolve folder IDs for all scanned files by matching their absolute path
/// prefix against watched folder paths.
fn resolve_folder_file_pairs<'a>(
    files: &'a [FileEntry],
    fid_map: &HashMap<String, String>,
) -> Vec<FolderFileEntry<'a>> {
    let mut result = Vec::with_capacity(files.len());
    for file in files {
        let abs_path = Path::new(&file.absolute_path);
        if let Some(folder_id) = fid_map.iter().find_map(|(folder_path, fid)| {
            abs_path.strip_prefix(Path::new(folder_path)).ok().map(|_| fid.clone())
        }) {
            result.push(FolderFileEntry { folder_id, file });
        }
    }
    result
}

/// Phase 1: Compute hash, detect media, and extract PNG metadata for a file.
///
/// Runs asynchronously without holding the database lock. This phase handles
/// all I/O-bound work (SHA-256 via spawn_blocking, ffprobe for videos).
async fn process_file_metadata<'a>(
    file: &'a FileEntry,
    folder_id: &'a str,
) -> Result<ProcessedFile<'a>, IndexError> {
    let abs_path = Path::new(&file.absolute_path);

    let new_hash = compute_file_hash(abs_path).await?;
    let media_info = detect_media(abs_path).await?;

    // Extract PNG metadata (tEXt/iTXt chunks) — prompt/workflow parsed as JSON, rest in raw_text_entries
    let metadata_json = if media_info.mime_type == "image/png" {
        parse_png_metadata(abs_path).ok().and_then(|meta| {
            if meta.prompt.is_some() || meta.workflow.is_some() || !meta.raw_text_entries.is_empty()
            {
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
        folder_id: folder_id.to_string(),
    })
}

/// Phase 2: Store a processed file's data in the database.
///
/// Synchronous — must be called while holding a database connection.
/// Uses `INSERT OR REPLACE` for idempotent upserts. The `(folder_id, relative_path)`
/// compound unique index prevents duplicates across multiple watched folders.
fn store_file(conn: &Connection, processed: &ProcessedFile<'_>) -> Result<IndexChange, IndexError> {
    // Check if file already indexed with same hash (skip if unchanged)
    let existing: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT id, checksum FROM media_items WHERE folder_id = ?1 AND relative_path = ?2",
            params![processed.folder_id, processed.file.relative_path],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    if let Some((_, Some(ref existing_hash))) = existing
        && existing_hash == &processed.new_hash
    {
        return Ok(IndexChange::Skipped);
    }

    // Determine change type and reuse existing UUID or generate new one
    let (id, change) = match existing {
        Some((existing_id, _)) => (existing_id, IndexChange::Updated),
        None => (Uuid::new_v4().to_string(), IndexChange::Created),
    };

    // Upsert into DB — INSERT OR REPLACE is idempotent
    let indexed_at = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO media_items
            (id, filename, relative_path, mime_type, width, height, file_size,
             file_created_at, file_modified_at, indexed_at, metadata_json, checksum,
             folder_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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
            indexed_at,
            processed.metadata_json,
            processed.new_hash,
            processed.folder_id,
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
/// Iterates over all items in the DB (with folder_id + relative_path) and
/// checks whether the corresponding file still exists in its watched folder.
/// Deleted items are removed to keep the database in sync with the filesystem.
fn remove_deleted_items(conn: &Connection, config: &AppConfig) -> Result<usize, IndexError> {
    let mut stmt = conn.prepare("SELECT folder_id, relative_path FROM media_items")?;
    let db_items: Vec<(Option<String>, String)> =
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.filter_map(|r| r.ok()).collect();

    let fid_map = folder_id_map(config);

    let mut removed = 0;
    for (db_folder_id, db_path) in &db_items {
        // Check if this item's watched folder still has the file on disk
        let exists = db_folder_id
            .as_ref()
            .and_then(|fid| {
                fid_map.iter().find_map(|(folder_path, folder_id)| {
                    if folder_id == fid {
                        let full_path = Path::new(folder_path).join(db_path);
                        if full_path.exists() { Some(true) } else { None }
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(false);

        if !exists {
            if let Some(fid) = db_folder_id {
                conn.execute(
                    "DELETE FROM media_items WHERE folder_id = ?1 AND relative_path = ?2",
                    params![fid.as_str(), db_path],
                )?;
            } else {
                conn.execute("DELETE FROM media_items WHERE relative_path = ?1", params![db_path])?;
            }
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
    /// Connection pool error (r2d2).
    Pool(r2d2::Error),
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexError::Scanner(e) => write!(f, "Scanner error: {}", e),
            IndexError::Hash(e) => write!(f, "Hash error: {}", e),
            IndexError::Detection(e) => write!(f, "Detection error: {}", e),
            IndexError::Png(e) => write!(f, "PNG error: {}", e),
            IndexError::Db(e) => write!(f, "DB error: {}", e),
            IndexError::Pool(e) => write!(f, "Pool error: {}", e),
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

impl From<r2d2::Error> for IndexError {
    fn from(e: r2d2::Error) -> Self {
        IndexError::Pool(e)
    }
}

#[cfg(test)]
#[path = "indexer_test.rs"]
mod tests;
