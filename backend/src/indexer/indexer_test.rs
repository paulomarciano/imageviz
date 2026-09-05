use super::*;
use crate::config::WatchedFolder;
use crate::db::SqliteConnectionManager;
use crate::db::migrations::run_migrations;
use r2d2::Pool;

/// Create a test pool with schema applied.
fn setup_pool() -> Pool<SqliteConnectionManager> {
    let pool = crate::db::pool::create_in_memory_pool();
    {
        let mut conn = pool.get().unwrap();
        run_migrations(&mut conn).unwrap();
    }
    pool
}

/// Create a progress tracker for testing.
fn setup_progress() -> progress::ProgressTracker {
    progress::ProgressTracker::new()
}

#[tokio::test]
async fn test_empty_config_returns_empty_stats() {
    let pool = setup_pool();
    let config = AppConfig::default();
    let progress = setup_progress();

    let stats = full_index(&pool, &config, &progress).await.unwrap();

    assert_eq!(stats, IndexStats { created: 0, updated: 0, skipped: 0, deleted: 0, errors: 0 });
}

#[tokio::test]
async fn test_incremental_index_processes_files_and_skips_unchanged() {
    let dir = tempfile::Builder::new().prefix("imgviz_").tempdir().unwrap();
    let png_path = dir.path().join("test.png");
    create_minimal_png(&png_path);

    let pool = setup_pool();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: None,
            id: None,
        }],
    };

    // incremental_index delegates to full_index internally
    let stats = incremental_index(&pool, &config, &setup_progress()).await.unwrap();
    assert_eq!(stats.created, 1, "incremental_index should create entries for new files");

    // Second call should skip unchanged files
    let stats = incremental_index(&pool, &config, &setup_progress()).await.unwrap();
    assert_eq!(stats.skipped, 1, "incremental_index should skip unchanged files");
}

#[tokio::test]
async fn test_full_index_creates_entries_for_new_files() {
    let dir = tempfile::Builder::new().prefix("imgviz_").tempdir().unwrap();

    // Create a minimal valid PNG file
    let png_path = dir.path().join("test.png");
    create_minimal_png(&png_path);

    // Create a JPG file (just a copy won't work — create a minimal valid one)
    let jpg_path = dir.path().join("test.jpg");
    std::fs::write(&jpg_path, b"fake jpeg data").unwrap();

    let pool = setup_pool();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: None,
            id: None,
        }],
    };
    let progress = setup_progress();

    let stats = full_index(&pool, &config, &progress).await.unwrap();

    // PNG should be indexed; JPG will fail detection (invalid format)
    assert_eq!(stats.created, 1, "Only valid PNG should be created");
    assert_eq!(stats.errors, 1, "JPG should fail detection");

    // Verify entry in DB
    let conn = pool.get().unwrap();
    let count: i32 = conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1, "Only one media item in DB");
}

#[tokio::test]
async fn test_incremental_index_skips_unchanged_files() {
    let dir = tempfile::Builder::new().prefix("imgviz_").tempdir().unwrap();
    let png_path = dir.path().join("test.png");
    create_minimal_png(&png_path);

    let pool = setup_pool();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: None,
            id: None,
        }],
    };

    // First index — should create
    let stats1 = full_index(&pool, &config, &setup_progress()).await.unwrap();
    assert_eq!(stats1.created, 1);

    // Second index with no changes — should skip
    let stats2 = full_index(&pool, &config, &setup_progress()).await.unwrap();
    assert_eq!(stats2.created, 0);
    assert_eq!(stats2.skipped, 1);
}

#[tokio::test]
async fn test_incremental_index_updates_modified_files() {
    let dir = tempfile::Builder::new().prefix("imgviz_").tempdir().unwrap();
    let png_path = dir.path().join("test.png");
    create_minimal_png(&png_path);

    let pool = setup_pool();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: None,
            id: None,
        }],
    };

    // First index
    full_index(&pool, &config, &setup_progress()).await.unwrap();

    // Modify file (change a byte)
    let mut data = std::fs::read(&png_path).unwrap();
    if let Some(byte) = data.last_mut() {
        *byte = byte.wrapping_add(1);
    }
    std::fs::write(&png_path, &data).unwrap();

    // Second index — should update
    let stats = full_index(&pool, &config, &setup_progress()).await.unwrap();
    assert_eq!(stats.updated, 1);
    assert_eq!(stats.created, 0);
    assert_eq!(stats.skipped, 0);
}

#[tokio::test]
async fn test_remove_deleted_files_cleans_up_db() {
    let dir = tempfile::Builder::new().prefix("imgviz_").tempdir().unwrap();
    let png_path = dir.path().join("test.png");
    create_minimal_png(&png_path);

    let pool = setup_pool();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: None,
            id: None,
        }],
    };

    // Index the file
    full_index(&pool, &config, &setup_progress()).await.unwrap();

    // Delete the file from disk
    std::fs::remove_file(&png_path).unwrap();

    // Re-index — should detect deletion
    let stats = full_index(&pool, &config, &setup_progress()).await.unwrap();
    assert_eq!(stats.deleted, 1);

    // DB should be empty
    let conn = pool.get().unwrap();
    let count: i32 = conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_full_index_is_idempotent() {
    let dir = tempfile::Builder::new().prefix("imgviz_").tempdir().unwrap();
    let png_path = dir.path().join("test.png");
    create_minimal_png(&png_path);

    let pool = setup_pool();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: None,
            id: None,
        }],
    };

    // Index 3 times — should be idempotent (no duplicate entries)
    for _ in 0..3 {
        full_index(&pool, &config, &setup_progress()).await.unwrap();
    }

    let conn = pool.get().unwrap();
    let count: i32 = conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1, "Should have exactly one entry after 3 index runs");
}

#[tokio::test]
async fn test_indexed_item_has_all_required_fields() {
    let dir = tempfile::Builder::new().prefix("imgviz_").tempdir().unwrap();
    let png_path = dir.path().join("test.png");
    create_minimal_png(&png_path);

    let pool = setup_pool();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: None,
            id: None,
        }],
    };

    full_index(&pool, &config, &setup_progress()).await.unwrap();

    // Verify all required columns are populated
    let conn = pool.get().unwrap();
    let row: (
        String,
        String,
        String,
        String,
        Option<u32>,
        Option<u32>,
        i64,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT id, filename, relative_path, mime_type, width, height, file_size,
                    file_created_at, file_modified_at, indexed_at, metadata_json, checksum
             FROM media_items LIMIT 1",
            [],
            |r| {
                Ok((
                    r.get(0)?,  // id
                    r.get(1)?,  // filename
                    r.get(2)?,  // relative_path
                    r.get(3)?,  // mime_type
                    r.get(4)?,  // width
                    r.get(5)?,  // height
                    r.get(6)?,  // file_size
                    r.get(7)?,  // file_created_at
                    r.get(8)?,  // file_modified_at
                    r.get(9)?,  // indexed_at
                    r.get(10)?, // metadata_json
                    r.get(11)?, // checksum
                ))
            },
        )
        .unwrap();

    assert!(!row.0.is_empty(), "id should be non-empty (UUID)");
    assert_eq!(row.1, "test.png", "filename should match");
    assert_eq!(row.2, "test.png", "relative_path should match");
    assert_eq!(row.3, "image/png", "mime_type should be image/png");
    assert!(row.4.is_some(), "width should be present");
    assert!(row.5.is_some(), "height should be present");
    assert!(row.6 > 0, "file_size should be > 0");
    assert!(!row.7.is_empty(), "file_created_at should be non-empty");
    assert!(!row.8.is_empty(), "file_modified_at should be non-empty");
    assert!(!row.9.is_empty(), "indexed_at should be non-empty");
    assert!(row.10.is_none(), "metadata_json should be None for a minimal PNG without text chunks");
    assert!(row.11.is_some(), "checksum should be present");
    assert_eq!(row.11.as_ref().unwrap().len(), 64, "checksum should be SHA-256 (64 hex chars)");
}

/// Create a minimal valid PNG file for testing.
///
/// Uses the `png` crate to encode a 2×2 RGB image. This produces a real PNG
/// with valid header, IHDR, IDAT, and IEND chunks that passes detection.
fn create_minimal_png(path: &std::path::Path) {
    create_png_with_text_chunks(path, &[])
}

/// Create a minimal PNG with embedded tEXt metadata chunks.
///
/// Each entry in `chunks` is a `(keyword, value)` pair that becomes a tEXt
/// chunk in the PNG file, simulating ComfyUI-style embedded metadata.
fn create_png_with_text_chunks(path: &std::path::Path, chunks: &[(&str, &str)]) {
    let file = std::fs::File::create(path).unwrap();
    let w = std::io::BufWriter::new(file);

    let mut encoder = png::Encoder::new(w, 2, 2);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);

    for (keyword, value) in chunks {
        encoder
            .add_text_chunk(keyword.to_string(), value.to_string())
            .expect("Failed to add text chunk to PNG");
    }

    let mut writer = encoder.write_header().unwrap();

    // 2×2 RGB pixels: red, green, blue, white
    let data: Vec<u8> = vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
    writer.write_image_data(&data).unwrap();
}

#[tokio::test]
async fn test_index_stores_raw_text_entries_without_prompt() {
    let dir = tempfile::Builder::new().prefix("imgviz_").tempdir().unwrap();
    let png_path = dir.path().join("no_prompt.png");

    // Create a PNG with a non-standard text chunk (no prompt/workflow)
    create_png_with_text_chunks(&png_path, &[("Description", "@michiking's image")]);

    let pool = setup_pool();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: None,
            id: None,
        }],
    };

    full_index(&pool, &config, &setup_progress()).await.unwrap();

    // Verify metadata_json was populated even without prompt/workflow
    let conn = pool.get().unwrap();
    let row: (Option<String>,) = conn
        .query_row(
            "SELECT metadata_json FROM media_items WHERE filename = 'no_prompt.png'",
            [],
            |r| Ok((r.get(0)?,)),
        )
        .unwrap();

    let metadata_str = row.0.expect("metadata_json should be Some even without prompt/workflow");
    let parsed: serde_json::Value =
        serde_json::from_str(&metadata_str).expect("metadata_json should be valid JSON");

    // Verify the raw_text_entries contain the Description
    assert_eq!(parsed["raw_text_entries"]["Description"], "@michiking's image");
}

#[tokio::test]
async fn test_index_extracts_png_metadata_content() {
    let dir = tempfile::Builder::new().prefix("imgviz_").tempdir().unwrap();
    let png_path = dir.path().join("with_metadata.png");

    // Create a PNG with ComfyUI-style prompt + workflow metadata
    let prompt_json = r#"{"3":{"inputs":{"seed":12345,"steps":20}}}"#;
    let workflow_json = r#"{"nodes":[{"id":3,"type":"KSampler"}]}"#;
    create_png_with_text_chunks(&png_path, &[("prompt", prompt_json), ("workflow", workflow_json)]);

    let pool = setup_pool();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: None,
            id: None,
        }],
    };

    full_index(&pool, &config, &setup_progress()).await.unwrap();

    // Verify metadata_json was populated correctly
    let conn = pool.get().unwrap();
    let row: (Option<String>,) = conn
        .query_row(
            "SELECT metadata_json FROM media_items WHERE filename = 'with_metadata.png'",
            [],
            |r| Ok((r.get(0)?,)),
        )
        .unwrap();

    let metadata_str = row.0.expect("metadata_json should be Some for a PNG with prompt+workflow");
    let parsed: serde_json::Value =
        serde_json::from_str(&metadata_str).expect("metadata_json should be valid JSON");

    // Verify the parsed metadata contains expected fields
    assert_eq!(parsed["prompt"]["3"]["inputs"]["seed"], 12345);
    assert_eq!(parsed["prompt"]["3"]["inputs"]["steps"], 20);
    assert_eq!(parsed["workflow"]["nodes"][0]["type"], "KSampler");
}

// ---------------------------------------------------------------------------
// Wave 8.2 — Parallel Phase-1 processing
// ---------------------------------------------------------------------------

/// Serializes tests that mutate `INDEX_CONCURRENCY` (env is process-global).
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// RAII guard holding `INDEX_CONCURRENCY` at `value`; restores the previous
/// environment on drop. Holding the `ENV_LOCK` guard prevents concurrent env
/// tests from racing each other.
///
/// Safe to hold across `.await` in tests: `#[tokio::test]` uses a
/// current-thread runtime, so the test future has no `Send` requirement and no
/// other task contends on the lock.
fn set_index_concurrency(value: Option<&str>) -> EnvGuard {
    let lock = ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let prev = std::env::var("INDEX_CONCURRENCY").ok();
    match value {
        Some(v) => unsafe { std::env::set_var("INDEX_CONCURRENCY", v) },
        None => unsafe { std::env::remove_var("INDEX_CONCURRENCY") },
    }
    EnvGuard { _lock: lock, prev }
}

struct EnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    prev: Option<String>,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => unsafe { std::env::set_var("INDEX_CONCURRENCY", v) },
            None => unsafe { std::env::remove_var("INDEX_CONCURRENCY") },
        }
    }
}

#[test]
fn test_index_concurrency_from_env() {
    {
        let _guard = set_index_concurrency(Some("2"));
        assert_eq!(index_concurrency(), 2);
    }
    {
        let _guard = set_index_concurrency(Some("16"));
        assert_eq!(index_concurrency(), 16, "explicit env override is not capped");
    }
}

#[test]
fn test_index_concurrency_default_when_unset() {
    let _guard = set_index_concurrency(None);
    let n = index_concurrency();
    assert!(n >= 1, "default must be at least 1, got {}", n);
    assert!(n <= 8, "default must be capped at 8, got {}", n);
}

#[test]
fn test_index_concurrency_invalid_env_falls_back_to_default() {
    let expected = {
        let _guard = set_index_concurrency(None);
        index_concurrency()
    };
    {
        let _guard = set_index_concurrency(Some("not-a-number"));
        assert_eq!(index_concurrency(), expected, "invalid value must fall back");
    }
    {
        let _guard = set_index_concurrency(Some("0"));
        assert_eq!(
            index_concurrency(),
            expected,
            "zero must fall back (buffer_unordered(0) is unbounded)"
        );
    }
}

/// Create `count` distinct minimal PNGs in `dir` (unique text-chunk content so
/// every file has a distinct checksum).
fn create_png_batch(dir: &std::path::Path, count: usize) {
    for i in 0..count {
        let path = dir.join(format!("img_{:04}.png", i));
        create_png_with_text_chunks(&path, &[("index", &i.to_string())]);
    }
}

/// Build an `AppConfig` watching a single directory.
fn config_for(dir: &std::path::Path) -> AppConfig {
    AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.to_string_lossy().to_string(),
            label: None,
            id: None,
        }],
    }
}

/// Fetch comparable DB state — `(relative_path, checksum, mime_type, width,
/// height, file_size)` sorted by `relative_path`. Excludes `id` (random UUID)
/// and `indexed_at` (wall-clock timestamp) which legitimately differ between
/// independent index runs.
type IndexedRow = (String, Option<String>, String, Option<u32>, Option<u32>, i64);

fn fetch_indexed_rows(pool: &Pool<SqliteConnectionManager>) -> Vec<IndexedRow> {
    let conn = pool.get().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT relative_path, checksum, mime_type, width, height, file_size
             FROM media_items ORDER BY relative_path",
        )
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

#[tokio::test]
async fn test_full_index_parallel_matches_sequential_state() {
    let dir = tempfile::Builder::new().prefix("imgviz_det_").tempdir().unwrap();
    create_png_batch(dir.path(), 250); // spans 3 chunks: 100 + 100 + 50

    // Run 1: sequential (concurrency 1)
    let pool_seq = setup_pool();
    let stats_seq = {
        let _guard = set_index_concurrency(Some("1"));
        full_index(&pool_seq, &config_for(dir.path()), &setup_progress()).await.unwrap()
    };

    // Run 2: concurrent (concurrency 4), same files on disk
    let pool_par = setup_pool();
    let stats_par = {
        let _guard = set_index_concurrency(Some("4"));
        full_index(&pool_par, &config_for(dir.path()), &setup_progress()).await.unwrap()
    };

    assert_eq!(stats_seq.created, 250, "all files created in sequential run");
    assert_eq!(stats_seq, stats_par, "stats must be identical regardless of concurrency");
    assert_eq!(fetch_indexed_rows(&pool_seq), fetch_indexed_rows(&pool_par));
    assert_eq!(fetch_indexed_rows(&pool_par).len(), 250);
}

#[tokio::test]
async fn test_incremental_index_parallel_matches_sequential_state() {
    let dir = tempfile::Builder::new().prefix("imgviz_det_").tempdir().unwrap();
    create_png_batch(dir.path(), 120);

    // Seed both pools with a full index of the same on-disk fixture.
    let pool_seq = setup_pool();
    {
        let _guard = set_index_concurrency(Some("1"));
        full_index(&pool_seq, &config_for(dir.path()), &setup_progress()).await.unwrap();
    }
    let pool_par = setup_pool();
    {
        let _guard = set_index_concurrency(Some("4"));
        full_index(&pool_par, &config_for(dir.path()), &setup_progress()).await.unwrap();
    }

    // Modify 20 files (different text-chunk content → different size + checksum,
    // so the incremental size/mtime skip check cannot accidentally skip them).
    for i in 0..20 {
        let path = dir.path().join(format!("img_{:04}.png", i));
        create_png_with_text_chunks(&path, &[("index", &format!("modified-{}", i))]);
    }

    let stats_seq = {
        let _guard = set_index_concurrency(Some("1"));
        incremental_index(&pool_seq, &config_for(dir.path()), &setup_progress()).await.unwrap()
    };
    let stats_par = {
        let _guard = set_index_concurrency(Some("4"));
        incremental_index(&pool_par, &config_for(dir.path()), &setup_progress()).await.unwrap()
    };

    assert_eq!(stats_seq.updated, 20, "modified files must be updated");
    assert_eq!(stats_seq.skipped, 100, "unchanged files must be skipped");
    assert_eq!(stats_seq, stats_par, "stats must be identical regardless of concurrency");
    assert_eq!(fetch_indexed_rows(&pool_seq), fetch_indexed_rows(&pool_par));
}

#[tokio::test]
async fn test_stats_accurate_with_failures_under_concurrency() {
    let dir = tempfile::Builder::new().prefix("imgviz_err_").tempdir().unwrap();
    create_png_batch(dir.path(), 4);
    for name in ["broken_1.jpg", "broken_2.jpg"] {
        std::fs::write(dir.path().join(name), b"not a real jpeg").unwrap();
    }

    let pool = setup_pool();
    let stats = {
        let _guard = set_index_concurrency(Some("4"));
        full_index(&pool, &config_for(dir.path()), &setup_progress()).await.unwrap()
    };

    assert_eq!(stats.created, 4, "only valid PNGs created");
    assert_eq!(stats.errors, 2, "both invalid files counted exactly once");
    assert_eq!(stats.updated, 0);
    assert_eq!(stats.skipped, 0);

    let conn = pool.get().unwrap();
    let count: i32 = conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 4, "only valid files stored");
}

#[tokio::test]
async fn test_progress_converges_under_concurrency() {
    let dir = tempfile::Builder::new().prefix("imgviz_prog_").tempdir().unwrap();
    create_png_batch(dir.path(), 150); // spans 2 chunks

    let pool = setup_pool();
    let progress = setup_progress();
    let stats = {
        let _guard = set_index_concurrency(Some("4"));
        full_index(&pool, &config_for(dir.path()), &progress).await.unwrap()
    };

    let snap = progress.snapshot();
    assert_eq!(snap.status, progress::IndexStatus::Complete);
    assert_eq!(snap.total, 150);
    assert_eq!(snap.processed, 150, "processed count must converge to total");
    assert_eq!(snap.errors.len(), 0, "no errors recorded");
    assert_eq!(stats.errors, 0);
}

#[tokio::test]
async fn test_process_chunk_concurrent_pairs_results_with_entries() {
    let dir = tempfile::Builder::new().prefix("imgviz_pair_").tempdir().unwrap();
    create_minimal_png(&dir.path().join("a.png"));
    create_minimal_png(&dir.path().join("b.png"));
    std::fs::write(dir.path().join("broken.jpg"), b"not a real jpeg").unwrap();

    let entries: Vec<FileEntry> = ["a.png", "b.png", "broken.jpg"]
        .iter()
        .map(|name| {
            let path = dir.path().join(name);
            let meta = std::fs::metadata(&path).unwrap();
            FileEntry {
                filename: name.to_string(),
                relative_path: name.to_string(),
                absolute_path: path,
                file_size: meta.len(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                modified_at: "2026-01-01T00:00:00Z".to_string(),
            }
        })
        .collect();
    let pairs: Vec<FolderFileEntry> = entries
        .iter()
        .map(|e| FolderFileEntry { folder_id: "test-folder".to_string(), file: e.clone() })
        .collect();

    let results = process_chunk_concurrent(pairs, 2).await;

    assert_eq!(results.len(), 3, "one result per entry");
    for (entry, result) in results {
        match entry.file.relative_path.as_str() {
            "broken.jpg" => assert!(result.is_err(), "invalid file must produce Err"),
            name => {
                let processed = result
                    .unwrap_or_else(|e| panic!("{} should process successfully: {}", name, e));
                assert_eq!(
                    processed.file.relative_path, name,
                    "result must pair with its own entry"
                );
                assert_eq!(processed.new_hash.len(), 64, "SHA-256 hex length");
                assert_eq!(processed.folder_id, "test-folder");
            }
        }
    }
}
