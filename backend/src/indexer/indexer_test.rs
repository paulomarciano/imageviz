use super::*;
use crate::config::WatchedFolder;
use crate::db::migrations::run_migrations;
use crate::db::SqliteConnectionManager;
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
    let dir = tempfile::tempdir().unwrap();
    let png_path = dir.path().join("test.png");
    create_minimal_png(&png_path);

    let pool = setup_pool();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: None,
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
    let dir = tempfile::tempdir().unwrap();

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
        }],
    };
    let progress = setup_progress();

    let stats = full_index(&pool, &config, &progress).await.unwrap();

    // PNG should be indexed; JPG will fail detection (invalid format)
    assert_eq!(stats.created, 1, "Only valid PNG should be created");
    assert_eq!(stats.errors, 1, "JPG should fail detection");

    // Verify entry in DB
    let conn = pool.get().unwrap();
    let count: i32 =
        conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1, "Only one media item in DB");
}

#[tokio::test]
async fn test_incremental_index_skips_unchanged_files() {
    let dir = tempfile::tempdir().unwrap();
    let png_path = dir.path().join("test.png");
    create_minimal_png(&png_path);

    let pool = setup_pool();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: None,
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
    let dir = tempfile::tempdir().unwrap();
    let png_path = dir.path().join("test.png");
    create_minimal_png(&png_path);

    let pool = setup_pool();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: None,
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
    let dir = tempfile::tempdir().unwrap();
    let png_path = dir.path().join("test.png");
    create_minimal_png(&png_path);

    let pool = setup_pool();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: None,
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
    let count: i32 =
        conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_full_index_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let png_path = dir.path().join("test.png");
    create_minimal_png(&png_path);

    let pool = setup_pool();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: None,
        }],
    };

    // Index 3 times — should be idempotent (no duplicate entries)
    for _ in 0..3 {
        full_index(&pool, &config, &setup_progress()).await.unwrap();
    }

    let conn = pool.get().unwrap();
    let count: i32 =
        conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1, "Should have exactly one entry after 3 index runs");
}

#[tokio::test]
async fn test_indexed_item_has_all_required_fields() {
    let dir = tempfile::tempdir().unwrap();
    let png_path = dir.path().join("test.png");
    create_minimal_png(&png_path);

    let pool = setup_pool();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: None,
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
    let dir = tempfile::tempdir().unwrap();
    let png_path = dir.path().join("no_prompt.png");

    // Create a PNG with a non-standard text chunk (no prompt/workflow)
    create_png_with_text_chunks(&png_path, &[("Description", "@michiking's image")]);

    let pool = setup_pool();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: None,
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
    let dir = tempfile::tempdir().unwrap();
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
