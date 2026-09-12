use super::*;
use crate::config::WatchedFolder;
use crate::db::SqliteConnectionManager;
use crate::db::migrations::run_migrations;
use r2d2::Pool;

// ---------------------------------------------------------------------------
// Parameterization over index modes (ticket 8.13)
// ---------------------------------------------------------------------------

/// Which pipeline entry point a parameterized test exercises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndexMode {
    /// [`full_index`] — nothing skips Phase 1 before the checksum check.
    Full,
    /// [`incremental_index`] — unchanged files skip Phase 1 via size+mtime.
    Incremental,
}

/// Every behavioral test runs against both modes so the shared [`run_index`]
/// core cannot drift between them.
const ALL_MODES: [IndexMode; 2] = [IndexMode::Full, IndexMode::Incremental];

/// Run one index mode with an injected Phase-1 concurrency.
///
/// Test seam for the former `full_index_with_concurrency` /
/// `incremental_index_with_concurrency` pair: both modes funnel through the
/// same [`run_index`] core, differing only in the skip predicate.
async fn run_mode_with_concurrency(
    mode: IndexMode,
    pool: &Pool<SqliteConnectionManager>,
    config: &AppConfig,
    progress: &progress::ProgressTracker,
    concurrency: usize,
) -> Result<IndexStats, IndexError> {
    match mode {
        IndexMode::Full => run_index(pool, config, progress, concurrency, |_, _| false).await,
        IndexMode::Incremental => {
            run_index(pool, config, progress, concurrency, is_unchanged).await
        }
    }
}

/// Fresh fixtures for one parameterized index run: a temp watched folder
/// (kept alive), a migrated in-memory pool, and the matching config.
struct Case {
    mode: IndexMode,
    dir: tempfile::TempDir,
    pool: Pool<SqliteConnectionManager>,
    config: AppConfig,
}

impl Case {
    /// Watched folder = the (initially empty) temp dir.
    fn new(mode: IndexMode) -> Self {
        let dir = tempfile::Builder::new().prefix("imgviz_case_").tempdir().unwrap();
        let config = AppConfig {
            watched_folders: vec![WatchedFolder {
                path: dir.path().to_string_lossy().to_string(),
                label: None,
                id: None,
            }],
        };
        Self { mode, dir, pool: setup_pool(), config }
    }

    /// Path of the watched folder — create test files here before running.
    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }

    /// Run one index pass with deterministic sequential Phase-1 concurrency.
    async fn run(&self) -> IndexStats {
        run_mode_with_concurrency(self.mode, &self.pool, &self.config, &setup_progress(), 1)
            .await
            .unwrap()
    }

    /// Number of rows currently in `media_items`.
    fn db_count(&self) -> i32 {
        let conn = self.pool.get().unwrap();
        conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0)).unwrap()
    }
}

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
    for mode in ALL_MODES {
        let pool = setup_pool();

        let stats =
            run_mode_with_concurrency(mode, &pool, &AppConfig::default(), &setup_progress(), 1)
                .await
                .unwrap();

        assert_eq!(stats, IndexStats::default(), "{mode:?}");
    }
}

#[tokio::test]
async fn test_index_creates_entries_for_new_files() {
    for mode in ALL_MODES {
        let case = Case::new(mode);
        create_minimal_png(&case.path().join("test.png"));
        std::fs::write(case.path().join("test.jpg"), b"fake jpeg data").unwrap();

        let stats = case.run().await;

        assert_eq!(stats.created, 1, "{mode:?}: only the valid PNG is created");
        assert_eq!(stats.errors, 1, "{mode:?}: the fake JPG fails detection");
        assert_eq!(case.db_count(), 1, "{mode:?}: exactly one media item stored");
    }
}

#[tokio::test]
async fn test_index_is_idempotent() {
    for mode in ALL_MODES {
        let case = Case::new(mode);
        create_minimal_png(&case.path().join("test.png"));

        let first = case.run().await;
        assert_eq!(first.created, 1, "{mode:?}: first run creates the entry");

        for _ in 0..2 {
            let stats = case.run().await;
            assert_eq!(stats.created, 0, "{mode:?}: reruns create nothing");
            assert_eq!(stats.skipped, 1, "{mode:?}: reruns skip the unchanged file");
        }
        assert_eq!(case.db_count(), 1, "{mode:?}: still exactly one entry");
    }
}

#[tokio::test]
async fn test_index_updates_modified_files() {
    for mode in ALL_MODES {
        let case = Case::new(mode);
        let png_path = case.path().join("test.png");
        create_minimal_png(&png_path);
        assert_eq!(case.run().await.created, 1, "{mode:?}");

        // Rewrite with different content AND size, so the incremental
        // size+mtime gate cannot skip the file even if the filesystem's
        // mtime granularity is coarse.
        create_png_with_text_chunks(&png_path, &[("revision", "second-longer-value")]);

        let stats = case.run().await;

        assert_eq!(stats.updated, 1, "{mode:?}: modified file must be updated");
        assert_eq!(stats.created, 0, "{mode:?}");
        assert_eq!(stats.skipped, 0, "{mode:?}");
    }
}

#[tokio::test]
async fn test_index_skips_unchanged_files() {
    for mode in ALL_MODES {
        let case = Case::new(mode);
        create_minimal_png(&case.path().join("test.png"));

        assert_eq!(case.run().await.created, 1, "{mode:?}");
        let stats = case.run().await;

        assert_eq!(stats.created, 0, "{mode:?}");
        assert_eq!(stats.updated, 0, "{mode:?}");
        assert_eq!(stats.skipped, 1, "{mode:?}: unchanged file must be skipped");
    }
}

#[tokio::test]
async fn test_index_removes_deleted_files() {
    for mode in ALL_MODES {
        let case = Case::new(mode);
        let png_path = case.path().join("test.png");
        create_minimal_png(&png_path);

        assert_eq!(case.run().await.created, 1, "{mode:?}");

        std::fs::remove_file(&png_path).unwrap();
        let stats = case.run().await;

        assert_eq!(stats.deleted, 1, "{mode:?}: deletion must be detected");
        assert_eq!(case.db_count(), 0, "{mode:?}: DB empty after cleanup");
    }
}

#[tokio::test]
#[ignore = "requires test-fixtures/sample_video.mov (run scripts/generate-fixtures.sh)"]
async fn test_index_indexes_mov_files_without_errors() {
    for mode in ALL_MODES {
        let case = Case::new(mode);

        // Copy the generated .mov fixture into the watched folder
        let mov_src = crate::test_support::fixture_path("sample_video.mov");
        assert!(mov_src.exists(), "Fixture not found: {}", mov_src.display());
        std::fs::copy(&mov_src, case.path().join("clip.mov")).unwrap();

        let stats = case.run().await;

        assert_eq!(stats.created, 1, "{mode:?}: .mov file should be indexed");
        assert_eq!(stats.errors, 0, "{mode:?}: .mov must not increment stats.errors");

        let conn = case.pool.get().unwrap();
        let (mime_type, width, height): (String, Option<u32>, Option<u32>) = conn
            .query_row(
                "SELECT mime_type, width, height FROM media_items WHERE relative_path = 'clip.mov'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(mime_type, "video/quicktime", "{mode:?}");
        assert!(width.is_some_and(|w| w > 0), "{mode:?}: indexed .mov should have width");
        assert!(height.is_some_and(|h| h > 0), "{mode:?}: indexed .mov should have height");
    }
}

/// Drift guard (ticket 8.13): both modes must converge to identical stats and
/// identical DB state — over a fresh tree and over an unchanged tree.
///
/// On the unchanged rerun, full mode re-hashes everything and skips at the
/// checksum comparison while incremental mode skips at the size+mtime gate
/// without hashing; the observable outcome must be the same.
#[tokio::test]
async fn test_modes_agree_on_fresh_and_unchanged_trees() {
    let dir = tempfile::Builder::new().prefix("imgviz_drift_").tempdir().unwrap();
    create_png_batch(dir.path(), 120);
    let config = config_for(dir.path());

    // Fresh tree: each mode starts from an empty DB.
    let pool_full = setup_pool();
    let pool_incr = setup_pool();
    let stats_full = full_index(&pool_full, &config, &setup_progress()).await.unwrap();
    let stats_incr = incremental_index(&pool_incr, &config, &setup_progress()).await.unwrap();

    assert_eq!(stats_full, stats_incr, "fresh-tree stats must match across modes");
    assert_eq!(stats_full.created, 120);
    assert_eq!(fetch_indexed_rows(&pool_full), fetch_indexed_rows(&pool_incr));

    // Unchanged tree: rerun each mode over the same files.
    let stats_full = full_index(&pool_full, &config, &setup_progress()).await.unwrap();
    let stats_incr = incremental_index(&pool_incr, &config, &setup_progress()).await.unwrap();

    assert_eq!(stats_full, stats_incr, "unchanged-tree stats must match across modes");
    assert_eq!(stats_full, IndexStats { skipped: 120, ..IndexStats::default() });
    assert_eq!(fetch_indexed_rows(&pool_full), fetch_indexed_rows(&pool_incr));
}

/// Guard against seam ↔ wrapper wiring drift: `run_mode_with_concurrency`
/// re-states each mode's skip predicate (the public wrappers hard-code their
/// own), so pin the seam to the real entry points on an identical fixture.
///
/// Phase 3 makes predicate drift on the full wrapper observable: a full mode
/// that grew the incremental size+mtime gate would skip a same-size,
/// same-mtime content change and leave a stale row, while the real
/// never-skip predicate must re-hash and update it.
///
/// Phase 4 pins the *public* incremental wrapper from over-skipping (the
/// production-breaking direction: modified files never re-indexed). Lenient
/// drift there — processing more than needed — stays outcome-invisible by
/// design (it only costs extra hashing) and is guarded by review, as is
/// incremental-predicate drift on unmodified trees generally.
///
/// The concurrency asymmetry (seam at 4 vs the wrappers' env-derived value)
/// is intentional: outcomes are pinned concurrency-invariant by the
/// parallel-equivalence tests below.
#[tokio::test]
async fn test_seam_matches_public_wrappers() {
    let dir = tempfile::Builder::new().prefix("imgviz_seam_").tempdir().unwrap();
    create_png_batch(dir.path(), 5);
    let config = config_for(dir.path());

    // Fresh tree: seam (concurrency 4) vs public wrapper (env-derived).
    let pool_seam = setup_pool();
    let pool_api = setup_pool();
    let seam_full =
        run_mode_with_concurrency(IndexMode::Full, &pool_seam, &config, &setup_progress(), 4)
            .await
            .unwrap();
    let api_full = full_index(&pool_api, &config, &setup_progress()).await.unwrap();

    assert_eq!(seam_full, api_full, "seam must match full_index");
    assert_eq!(fetch_indexed_rows(&pool_seam), fetch_indexed_rows(&pool_api));

    // Unchanged tree: same for the incremental mode.
    let seam_incr = run_mode_with_concurrency(
        IndexMode::Incremental,
        &pool_seam,
        &config,
        &setup_progress(),
        4,
    )
    .await
    .unwrap();
    let api_incr = incremental_index(&pool_api, &config, &setup_progress()).await.unwrap();

    assert_eq!(seam_incr, api_incr, "seam must match incremental_index");
    assert_eq!(fetch_indexed_rows(&pool_seam), fetch_indexed_rows(&pool_api));

    // Phase 3: same-size, same-mtime content change. The real full
    // predicate (never skip) must re-hash and update; a full wrapper that
    // drifted into the incremental size+mtime gate would skip the file and
    // leave a stale row — this phase makes that drift fail the test.
    let path = dir.path().join("img_0000.png");
    let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
    // Same-length text value → same file size; mtime restored below.
    create_png_with_text_chunks(&path, &[("index", "Z")]);
    std::fs::File::options().write(true).open(&path).unwrap().set_modified(mtime).unwrap();

    let seam_full =
        run_mode_with_concurrency(IndexMode::Full, &pool_seam, &config, &setup_progress(), 4)
            .await
            .unwrap();
    let api_full = full_index(&pool_api, &config, &setup_progress()).await.unwrap();

    assert_eq!(seam_full, api_full, "seam must match full_index after content change");
    assert_eq!(api_full.updated, 1, "full mode must catch same-size/same-mtime change");
    assert_eq!(api_full.skipped, 4, "full mode skips only the untouched files, at checksum");
    assert_eq!(fetch_indexed_rows(&pool_seam), fetch_indexed_rows(&pool_api));

    // Phase 4: a genuinely modified (size-changed) file through the *public*
    // incremental wrapper. Over-skip predicate drift (e.g. `|_, _| true`)
    // would leave the modified file permanently stale; the real predicate
    // must re-process it. Phase 3 covers the full wrapper's direction; this
    // covers incremental's, since no other test drives the public wrapper
    // against a modification.
    let path = dir.path().join("img_0001.png");
    create_png_with_text_chunks(&path, &[("index", "longer-value-1")]); // size differs

    let api_incr = incremental_index(&pool_api, &config, &setup_progress()).await.unwrap();

    assert_eq!(api_incr.updated, 1, "incremental must catch a size-changed edit");
    assert_eq!(api_incr.skipped, 4, "incremental skips only the genuinely unchanged files");
}

/// Full column set of a freshly indexed `media_items` row (all 12 columns).
type FullyIndexedRow = (
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
);

#[tokio::test]
async fn test_indexed_item_has_all_required_fields() {
    for mode in ALL_MODES {
        let case = Case::new(mode);
        create_minimal_png(&case.path().join("test.png"));
        case.run().await;

        // Verify all required columns are populated
        let conn = case.pool.get().unwrap();
        let row: FullyIndexedRow = conn
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

        assert!(!row.0.is_empty(), "{mode:?}: id should be non-empty (UUID)");
        assert_eq!(row.1, "test.png", "{mode:?}: filename should match");
        assert_eq!(row.2, "test.png", "{mode:?}: relative_path should match");
        assert_eq!(row.3, "image/png", "{mode:?}: mime_type should be image/png");
        assert!(row.4.is_some(), "{mode:?}: width should be present");
        assert!(row.5.is_some(), "{mode:?}: height should be present");
        assert!(row.6 > 0, "{mode:?}: file_size should be > 0");
        assert!(!row.7.is_empty(), "{mode:?}: file_created_at should be non-empty");
        assert!(!row.8.is_empty(), "{mode:?}: file_modified_at should be non-empty");
        assert!(!row.9.is_empty(), "{mode:?}: indexed_at should be non-empty");
        assert!(
            row.10.is_none(),
            "{mode:?}: metadata_json should be None for a minimal PNG without text chunks"
        );
        assert!(row.11.is_some(), "{mode:?}: checksum should be present");
        assert_eq!(
            row.11.as_ref().unwrap().len(),
            64,
            "{mode:?}: checksum should be SHA-256 (64 hex chars)"
        );
    }
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
    for mode in ALL_MODES {
        let case = Case::new(mode);
        // Create a PNG with a non-standard text chunk (no prompt/workflow)
        create_png_with_text_chunks(
            &case.path().join("no_prompt.png"),
            &[("Description", "@michiking's image")],
        );

        case.run().await;

        // Verify metadata_json was populated even without prompt/workflow
        let conn = case.pool.get().unwrap();
        let row: (Option<String>,) = conn
            .query_row(
                "SELECT metadata_json FROM media_items WHERE filename = 'no_prompt.png'",
                [],
                |r| Ok((r.get(0)?,)),
            )
            .unwrap();

        let metadata_str =
            row.0.expect("metadata_json should be Some even without prompt/workflow");
        let parsed: serde_json::Value =
            serde_json::from_str(&metadata_str).expect("metadata_json should be valid JSON");

        // Verify the raw_text_entries contain the Description
        assert_eq!(parsed["raw_text_entries"]["Description"], "@michiking's image", "{mode:?}");
    }
}

#[tokio::test]
async fn test_index_extracts_png_metadata_content() {
    for mode in ALL_MODES {
        let case = Case::new(mode);
        // Create a PNG with ComfyUI-style prompt + workflow metadata
        let prompt_json = r#"{"3":{"inputs":{"seed":12345,"steps":20}}}"#;
        let workflow_json = r#"{"nodes":[{"id":3,"type":"KSampler"}]}"#;
        create_png_with_text_chunks(
            &case.path().join("with_metadata.png"),
            &[("prompt", prompt_json), ("workflow", workflow_json)],
        );

        case.run().await;

        // Verify metadata_json was populated correctly
        let conn = case.pool.get().unwrap();
        let row: (Option<String>,) = conn
            .query_row(
                "SELECT metadata_json FROM media_items WHERE filename = 'with_metadata.png'",
                [],
                |r| Ok((r.get(0)?,)),
            )
            .unwrap();

        let metadata_str =
            row.0.expect("metadata_json should be Some for a PNG with prompt+workflow");
        let parsed: serde_json::Value =
            serde_json::from_str(&metadata_str).expect("metadata_json should be valid JSON");

        // Verify the parsed metadata contains expected fields
        assert_eq!(parsed["prompt"]["3"]["inputs"]["seed"], 12345, "{mode:?}");
        assert_eq!(parsed["prompt"]["3"]["inputs"]["steps"], 20, "{mode:?}");
        assert_eq!(parsed["workflow"]["nodes"][0]["type"], "KSampler", "{mode:?}");
    }
}

// ---------------------------------------------------------------------------
// Concurrency (ticket 8.2)
// ---------------------------------------------------------------------------

// Concurrency resolution is tested through the pure `resolve_concurrency`
// function (no env mutation: `set_var`/`remove_var` race concurrent
// `getenv` readers on other test threads). The `run_mode_with_concurrency`
// seam lets the index-run tests inject a concurrency value the same way.

#[test]
fn test_resolve_concurrency_honors_explicit_value() {
    assert_eq!(resolve_concurrency(Some("2"), 4), 2);
    assert_eq!(resolve_concurrency(Some("16"), 4), 16, "explicit value is not capped");
    assert_eq!(resolve_concurrency(Some("1"), 8), 1);
}

#[test]
fn test_resolve_concurrency_default_caps_available_parallelism() {
    assert_eq!(resolve_concurrency(None, 16), 8, "default capped at 8");
    assert_eq!(resolve_concurrency(None, 4), 4, "below cap: cores used as-is");
    assert_eq!(resolve_concurrency(None, 1), 1);
}

#[test]
fn test_resolve_concurrency_invalid_falls_back_to_default() {
    for raw in [None, Some("not-a-number"), Some(""), Some("0"), Some("-1")] {
        assert_eq!(resolve_concurrency(raw, 16), 8, "{raw:?} must fall back to the capped default");
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
    let stats_seq = run_mode_with_concurrency(
        IndexMode::Full,
        &pool_seq,
        &config_for(dir.path()),
        &setup_progress(),
        1,
    )
    .await
    .unwrap();

    // Run 2: concurrent (concurrency 4), same files on disk
    let pool_par = setup_pool();
    let stats_par = run_mode_with_concurrency(
        IndexMode::Full,
        &pool_par,
        &config_for(dir.path()),
        &setup_progress(),
        4,
    )
    .await
    .unwrap();

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
    full_index_with_seed(&pool_seq, dir.path()).await;
    let pool_par = setup_pool();
    full_index_with_seed(&pool_par, dir.path()).await;

    // Modify 20 files (different text-chunk content → different size + checksum,
    // so the incremental size/mtime skip check cannot accidentally skip them).
    for i in 0..20 {
        let path = dir.path().join(format!("img_{:04}.png", i));
        create_png_with_text_chunks(&path, &[("index", &format!("modified-{}", i))]);
    }

    let stats_seq = run_mode_with_concurrency(
        IndexMode::Incremental,
        &pool_seq,
        &config_for(dir.path()),
        &setup_progress(),
        1,
    )
    .await
    .unwrap();
    let stats_par = run_mode_with_concurrency(
        IndexMode::Incremental,
        &pool_par,
        &config_for(dir.path()),
        &setup_progress(),
        4,
    )
    .await
    .unwrap();

    assert_eq!(stats_seq.updated, 20, "modified files must be updated");
    assert_eq!(stats_seq.skipped, 100, "unchanged files must be skipped");
    assert_eq!(stats_seq, stats_par, "stats must be identical regardless of concurrency");
    assert_eq!(fetch_indexed_rows(&pool_seq), fetch_indexed_rows(&pool_par));
}

/// Seed `pool` with a full sequential index of `dir`'s contents.
async fn full_index_with_seed(pool: &Pool<SqliteConnectionManager>, dir: &std::path::Path) {
    run_mode_with_concurrency(IndexMode::Full, pool, &config_for(dir), &setup_progress(), 1)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_stats_accurate_with_failures_under_concurrency() {
    for mode in ALL_MODES {
        let dir = tempfile::Builder::new().prefix("imgviz_err_").tempdir().unwrap();
        create_png_batch(dir.path(), 4);
        for name in ["broken_1.jpg", "broken_2.jpg"] {
            std::fs::write(dir.path().join(name), b"not a real jpeg").unwrap();
        }

        let pool = setup_pool();
        let stats =
            run_mode_with_concurrency(mode, &pool, &config_for(dir.path()), &setup_progress(), 4)
                .await
                .unwrap();

        assert_eq!(stats.created, 4, "{mode:?}: only valid PNGs created");
        assert_eq!(stats.errors, 2, "{mode:?}: both invalid files counted exactly once");
        assert_eq!(stats.updated, 0, "{mode:?}");
        assert_eq!(stats.skipped, 0, "{mode:?}");

        let conn = pool.get().unwrap();
        let count: i32 =
            conn.query_row("SELECT COUNT(*) FROM media_items", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 4, "{mode:?}: only valid files stored");
    }
}

#[tokio::test]
async fn test_progress_converges_under_concurrency() {
    for mode in ALL_MODES {
        let dir = tempfile::Builder::new().prefix("imgviz_prog_").tempdir().unwrap();
        create_png_batch(dir.path(), 150); // spans 2 chunks

        let pool = setup_pool();
        let progress = setup_progress();
        let stats = run_mode_with_concurrency(mode, &pool, &config_for(dir.path()), &progress, 4)
            .await
            .unwrap();

        let snap = progress.snapshot();
        assert_eq!(snap.status, progress::IndexStatus::Complete, "{mode:?}");
        assert_eq!(snap.total, 150, "{mode:?}");
        assert_eq!(snap.processed, 150, "{mode:?}: processed count must converge to total");
        assert_eq!(snap.errors.len(), 0, "{mode:?}: no errors recorded");
        assert_eq!(stats.errors, 0, "{mode:?}");
    }
}

// ---------------------------------------------------------------------------
// Removal epilogue — in-memory diff (ticket 8.18)
// ---------------------------------------------------------------------------

/// Insert a `media_items` row directly, bypassing the scan/hash pipeline.
///
/// Only NOT NULL columns are populated; everything else stays at its default.
/// The referenced `watched_folders` row is upserted first (FK enforced).
fn seed_media_item(conn: &Connection, folder_id: Option<&str>, relative_path: &str) {
    if let Some(fid) = folder_id {
        conn.execute(
            "INSERT OR IGNORE INTO watched_folders (id, path, label) VALUES (?1, ?2, NULL)",
            params![fid, format!("/{fid}")],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO media_items
            (id, filename, relative_path, mime_type, file_size,
             file_created_at, file_modified_at, indexed_at, folder_id)
         VALUES (?1, ?2, ?3, 'image/png', 1,
                 '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z',
                 '2026-01-01T00:00:00Z', ?4)",
        params![Uuid::new_v4().to_string(), relative_path, relative_path, folder_id],
    )
    .unwrap();
}

/// All `(folder_id, relative_path)` rows, ordered by relative path — the
/// surviving DB state after a removal run.
fn fetch_folder_path_rows(conn: &Connection) -> Vec<(Option<String>, String)> {
    let mut stmt = conn
        .prepare(
            "SELECT folder_id, relative_path FROM media_items ORDER BY relative_path, folder_id",
        )
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(|r| r.unwrap()).collect()
}

/// Build a scan snapshot in the `folder → relative paths` form the epilogue
/// diffs against.
fn scanned_of(entries: &[(&str, &[&str])]) -> ScannedFiles {
    entries
        .iter()
        .map(|(folder, paths)| {
            ((*folder).to_string(), paths.iter().map(|p| (*p).to_string()).collect::<HashSet<_>>())
        })
        .collect()
}

#[test]
fn test_remove_deleted_items_diffs_db_against_scanned_set() {
    let pool = setup_pool();
    let conn = pool.get().unwrap();
    seed_media_item(&conn, Some("f1"), "A.png");
    seed_media_item(&conn, Some("f1"), "B.png");
    seed_media_item(&conn, Some("f1"), "C.png");

    // The scan saw A and C on disk; B was deleted from the filesystem.
    let scanned = scanned_of(&[("f1", &["A.png", "C.png"])]);

    let removed = remove_deleted_items(&conn, &scanned).unwrap();

    assert_eq!(removed, 1, "only B.png is absent from the scan");
    assert_eq!(
        fetch_folder_path_rows(&conn),
        vec![
            (Some("f1".to_string()), "A.png".to_string()),
            (Some("f1".to_string()), "C.png".to_string()),
        ]
    );
}

#[test]
fn test_remove_deleted_items_empty_scanned_set_deletes_everything() {
    let pool = setup_pool();
    let conn = pool.get().unwrap();
    seed_media_item(&conn, Some("f1"), "A.png");
    seed_media_item(&conn, Some("f2"), "B.png");

    let removed = remove_deleted_items(&conn, &ScannedFiles::new()).unwrap();

    assert_eq!(removed, 2, "nothing on disk → nothing kept");
    assert!(fetch_folder_path_rows(&conn).is_empty());
}

#[test]
fn test_remove_deleted_items_empty_db_is_a_noop() {
    let pool = setup_pool();
    let conn = pool.get().unwrap();
    let scanned = scanned_of(&[("f1", &["A.png"])]);

    let removed = remove_deleted_items(&conn, &scanned).unwrap();

    assert_eq!(removed, 0);
    assert!(fetch_folder_path_rows(&conn).is_empty());
}

#[test]
fn test_remove_deleted_items_same_path_in_two_folders_is_per_folder() {
    let pool = setup_pool();
    let conn = pool.get().unwrap();
    seed_media_item(&conn, Some("f1"), "same.png");
    seed_media_item(&conn, Some("f2"), "same.png");

    // The scan still sees f1's copy; f2's was removed from disk.
    let scanned = scanned_of(&[("f1", &["same.png"])]);

    let removed = remove_deleted_items(&conn, &scanned).unwrap();

    assert_eq!(removed, 1, "f2's copy is gone; f1's must survive");
    assert_eq!(
        fetch_folder_path_rows(&conn),
        vec![(Some("f1".to_string()), "same.png".to_string())]
    );
}

#[test]
fn test_remove_deleted_items_drops_rows_of_unconfigured_folders() {
    let pool = setup_pool();
    let conn = pool.get().unwrap();
    seed_media_item(&conn, Some("f_gone"), "x.png");
    seed_media_item(&conn, Some("f1"), "y.png");

    // f_gone is no longer watched, so the scan only contains f1.
    let scanned = scanned_of(&[("f1", &["y.png"])]);

    let removed = remove_deleted_items(&conn, &scanned).unwrap();

    assert_eq!(removed, 1, "rows of de-configured folders have nothing to match");
    assert_eq!(fetch_folder_path_rows(&conn), vec![(Some("f1".to_string()), "y.png".to_string())]);
}

#[test]
fn test_remove_deleted_items_always_deletes_legacy_null_folder_rows() {
    let pool = setup_pool();
    let conn = pool.get().unwrap();
    seed_media_item(&conn, None, "legacy.png");
    seed_media_item(&conn, Some("f1"), "kept.png");

    let scanned = scanned_of(&[("f1", &["kept.png"])]);

    let removed = remove_deleted_items(&conn, &scanned).unwrap();

    assert_eq!(removed, 1, "NULL folder_id rows are always removed (prior semantics)");
    assert_eq!(
        fetch_folder_path_rows(&conn),
        vec![(Some("f1".to_string()), "kept.png".to_string())]
    );
}

/// The deletes must share a single transaction: a mid-run failure (simulated
/// by a `RAISE(ABORT)` trigger on the second target row) rolls back the first
/// row's delete, leaving the DB exactly as it was.
#[test]
fn test_remove_deleted_items_deletes_run_in_one_transaction() {
    let pool = setup_pool();
    let conn = pool.get().unwrap();
    seed_media_item(&conn, Some("f1"), "kept.png");
    seed_media_item(&conn, Some("f1"), "first_to_go.png");
    seed_media_item(&conn, Some("f1"), "abort_here.png");
    conn.execute(
        "CREATE TRIGGER abort_delete BEFORE DELETE ON media_items
         WHEN OLD.relative_path = 'abort_here.png'
         BEGIN
             SELECT RAISE(ABORT, 'simulated delete failure');
         END",
        [],
    )
    .unwrap();

    let scanned = scanned_of(&[("f1", &["kept.png"])]);

    let result = remove_deleted_items(&conn, &scanned);

    assert!(result.is_err(), "the aborted delete must propagate");
    assert_eq!(
        fetch_folder_path_rows(&conn),
        vec![
            (Some("f1".to_string()), "abort_here.png".to_string()),
            (Some("f1".to_string()), "first_to_go.png".to_string()),
            (Some("f1".to_string()), "kept.png".to_string()),
        ],
        "first_to_go.png's delete must have been rolled back with the transaction"
    );
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
