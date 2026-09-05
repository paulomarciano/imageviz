//! Integration test for warm-start incremental indexing (Wave 8.1).
//!
//! The startup pipeline (`spawn_background_indexing` in `main.rs`) invokes
//! [`imageviz_backend::indexer::incremental_index`]. These tests validate the
//! warm-start contract that path must uphold: on a second index run over a
//! mutated library, unchanged files are skipped (no re-hash), new files are
//! indexed, modified files are re-hashed and updated, and deleted files are
//! removed — with progress tracking reaching `Complete`.

mod common;

use std::time::{Duration, SystemTime};

use common::create_test_app_with_search;
use imageviz_backend::config::{AppConfig, WatchedFolder};
use imageviz_backend::indexer::incremental_index;
use imageviz_backend::indexer::progress::{IndexStatus, ProgressTracker};

/// A warm start over a mutated library skips unchanged files, indexes new
/// files, re-hashes modified files, and removes deleted files.
#[tokio::test]
async fn warm_start_skips_unchanged_files() {
    let app = create_test_app_with_search();
    let media_dir = tempfile::Builder::new().prefix("imgviz_warm_").tempdir().unwrap();
    let config = AppConfig {
        watched_folders: vec![WatchedFolder {
            path: media_dir.path().to_string_lossy().to_string(),
            label: None,
            id: None,
        }],
    };

    // -----------------------------------------------------------------
    // 1. First startup index over a fresh library of 4 files.
    // -----------------------------------------------------------------
    let files = ["a.png", "x.png", "y.png", "c.png"];
    for (i, name) in files.iter().enumerate() {
        let path = media_dir.path().join(name);
        create_png(&path, (i + 1) as u8);
        pin_mtime(&path, 1_000_000 + i as u64);
    }

    let first_stats = incremental_index(&app.pool, &config, &ProgressTracker::new()).await.unwrap();
    assert_eq!(first_stats.created, 4, "fresh library: all files created");
    assert_eq!(first_stats.skipped, 0, "fresh library: nothing to skip");
    assert_eq!(first_stats.errors, 0);

    // Capture a.png's checksum so we can prove the modified file is re-hashed.
    let checksum_before: String = {
        let conn = app.pool.get().unwrap();
        conn.query_row("SELECT checksum FROM media_items WHERE filename = 'a.png'", [], |r| {
            r.get(0)
        })
        .unwrap()
    };

    // -----------------------------------------------------------------
    // 2. Mutate the library: modify a.png (content + mtime), add b.png,
    //    delete c.png. x.png and y.png stay untouched.
    // -----------------------------------------------------------------
    let modified = media_dir.path().join("a.png");
    create_png(&modified, 99);
    pin_mtime(&modified, 2_000_000);

    let added = media_dir.path().join("b.png");
    create_png(&added, 50);
    pin_mtime(&added, 1_500_000);

    std::fs::remove_file(media_dir.path().join("c.png")).unwrap();

    // -----------------------------------------------------------------
    // 3. Warm start: second startup index over the mutated library.
    // -----------------------------------------------------------------
    let warm_progress = ProgressTracker::new();
    let warm_stats = incremental_index(&app.pool, &config, &warm_progress).await.unwrap();

    assert_eq!(warm_stats.skipped, 2, "x.png + y.png unchanged → skipped without hashing");
    assert_eq!(warm_stats.created, 1, "b.png is new → created");
    assert_eq!(warm_stats.updated, 1, "a.png modified → re-hashed and updated");
    assert_eq!(warm_stats.deleted, 1, "c.png no longer on disk → removed from DB");
    assert_eq!(warm_stats.errors, 0);

    // Progress plumbing observable: run reached Complete with all files processed.
    let snapshot = warm_progress.snapshot();
    assert_eq!(snapshot.status, IndexStatus::Complete, "warm start must reach Complete");
    assert_eq!(snapshot.total, 4, "second run scans a, b, x, y (c is gone)");
    assert_eq!(snapshot.processed, 4);

    // -----------------------------------------------------------------
    // 4. DB rows reflect the mutations.
    // -----------------------------------------------------------------
    let conn = app.pool.get().unwrap();

    let count: i32 = conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 4, "4 original - 1 deleted + 1 added = 4 rows");

    let checksum_after: String = conn
        .query_row("SELECT checksum FROM media_items WHERE filename = 'a.png'", [], |r| r.get(0))
        .unwrap();
    assert_ne!(checksum_before, checksum_after, "modified file must be re-hashed, not skipped");

    let deleted_count: i32 = conn
        .query_row("SELECT COUNT(*) FROM media_items WHERE filename = 'c.png'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(deleted_count, 0, "deleted file must be removed from DB");

    let added_count: i32 = conn
        .query_row("SELECT COUNT(*) FROM media_items WHERE filename = 'b.png'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(added_count, 1, "new file must be indexed exactly once");
}

/// Create a minimal valid PNG with content derived from `seed`.
///
/// Pixel bytes and an embedded tEXt chunk vary with `seed`, so two calls with
/// different seeds produce files with different SHA-256 checksums.
fn create_png(path: &std::path::Path, seed: u8) {
    let file = std::fs::File::create(path).unwrap();
    let writer = std::io::BufWriter::new(file);

    let mut encoder = png::Encoder::new(writer, 2, 2);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .add_text_chunk(format!("seed-{seed}"), "warm-start-test".to_string())
        .expect("text chunk");

    let mut writer = encoder.write_header().unwrap();
    let data: Vec<u8> = vec![seed, 0, 0, 0, seed, 0, 0, 0, seed, 255, 255, 255];
    writer.write_image_data(&data).unwrap();
}

/// Pin a file's mtime to a fixed point in time.
///
/// The incremental skip gate compares stored `file_size` AND
/// `file_modified_at` against the on-disk stat. Pinning mtimes explicitly
/// keeps the test deterministic regardless of filesystem timestamp resolution.
fn pin_mtime(path: &std::path::Path, unix_secs: u64) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(unix_secs)).unwrap();
}
