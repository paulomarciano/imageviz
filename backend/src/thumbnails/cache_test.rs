use crate::thumbnails::cache::{CacheError, get_or_generate_thumbnail};
use sha2::Digest;
use std::path::Path;
use tempfile::tempdir;

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

/// Create a small solid-colour PNG file for testing.
fn create_test_png(path: &Path, width: u32, height: u32) {
    let img = image::RgbaImage::new(width, height);
    img.save_with_format(path, image::ImageFormat::Png).expect("failed to create test PNG");
}

/// A 64-character hex string simulating a SHA-256 checksum.
const TEST_CHECKSUM: &str = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_cache_miss_generates_and_caches() {
    // Arrange
    let cache_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    let source_path = source_dir.path().join("test.png");
    create_test_png(&source_path, 100, 100);

    // Act
    let result =
        get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 200, cache_dir.path(), "image/png")
            .await;

    // Assert
    assert!(result.is_ok(), "cache miss should generate: {:?}", result.err());
    let cached = result.unwrap();
    assert!(cached.exists(), "cached thumbnail file should exist on disk");

    let expected_name = format!("{}_200.webp", &TEST_CHECKSUM[..16]);
    assert_eq!(
        cached.file_name().and_then(|n| n.to_str()),
        Some(expected_name.as_str()),
        "cache filename should match the content-addressed key"
    );
}

#[tokio::test]
async fn test_cache_hit_returns_cached_file() {
    // Arrange
    let cache_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    let source_path = source_dir.path().join("test.png");
    create_test_png(&source_path, 100, 100);

    // Act — first call (cache miss)
    let first =
        get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 200, cache_dir.path(), "image/png")
            .await;
    assert!(first.is_ok(), "first call should generate: {:?}", first.err());

    // Act — second call (cache hit)
    let second =
        get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 200, cache_dir.path(), "image/png")
            .await;

    // Assert
    assert!(second.is_ok(), "second call should succeed: {:?}", second.err());
    assert_eq!(first.unwrap(), second.unwrap(), "both calls should return the same cache path");
}

#[tokio::test]
async fn test_cache_key_is_deterministic() {
    // Arrange
    let cache_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    let source_path = source_dir.path().join("test.png");
    create_test_png(&source_path, 100, 100);

    // Act
    let result =
        get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 200, cache_dir.path(), "image/png")
            .await;
    assert!(result.is_ok(), "generation should succeed: {:?}", result.err());
    let path = result.unwrap();

    // Assert — the cache file name is purely derived from checksum[:16] + width
    let expected_name = format!("{}_200.webp", &TEST_CHECKSUM[..16]);
    assert_eq!(
        path.file_name().and_then(|n| n.to_str()),
        Some(expected_name.as_str()),
        "cache key must be deterministic from checksum and width alone"
    );
}

#[tokio::test]
async fn test_cache_creates_directory() {
    // Arrange — use a deeply nested subdirectory that does not yet exist
    let base = tempdir().unwrap();
    let cache_subdir = base.path().join("nested").join("cache");
    assert!(!cache_subdir.exists(), "cache directory must not exist yet");

    let source_dir = tempdir().unwrap();
    let source_path = source_dir.path().join("test.png");
    create_test_png(&source_path, 100, 100);

    // Act
    let result =
        get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 200, &cache_subdir, "image/png")
            .await;

    // Assert
    assert!(result.is_ok(), "should create missing cache directory: {:?}", result.err());
    assert!(cache_subdir.exists(), "cache directory should now exist");
    assert!(result.unwrap().exists(), "cached file should exist inside the new directory");
}

#[tokio::test]
async fn test_cache_source_not_found() {
    // Arrange
    let cache_dir = tempdir().unwrap();
    let nonexistent = Path::new("/nonexistent/file.png");

    // Act
    let result =
        get_or_generate_thumbnail(nonexistent, TEST_CHECKSUM, 200, cache_dir.path(), "image/png")
            .await;

    // Assert
    assert!(result.is_err(), "non-existent source should produce an error");
    match result.unwrap_err() {
        CacheError::SourceNotFound(path) => {
            assert_eq!(path, nonexistent, "should return the exact path that was requested");
        }
        other => panic!("Expected CacheError::SourceNotFound, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_invalid_width_rejected() {
    // Arrange
    let cache_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    let source_path = source_dir.path().join("test.png");
    create_test_png(&source_path, 100, 100);

    // Act — width below minimum
    let result =
        get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 50, cache_dir.path(), "image/png")
            .await;

    // Assert
    assert!(result.is_err(), "width=50 should be rejected");
    assert!(
        matches!(result.unwrap_err(), CacheError::InvalidWidth { width: 50, .. }),
        "expected InvalidWidth for width=50"
    );
}

#[tokio::test]
async fn test_generation_failure_returns_error() {
    // Arrange
    let cache_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    let bad_path = source_dir.path().join("corrupt.bin");
    std::fs::write(&bad_path, b"not an image").unwrap();

    // Act — generate from corrupt/non-image file
    let result =
        get_or_generate_thumbnail(&bad_path, TEST_CHECKSUM, 200, cache_dir.path(), "image/png")
            .await;

    // Assert
    assert!(result.is_err(), "corrupt source should produce an error");
    assert!(
        matches!(result.unwrap_err(), CacheError::Generation(_)),
        "expected CacheError::Generation for corrupt source"
    );
}

#[tokio::test]
async fn test_cache_with_different_widths() {
    // Arrange
    let cache_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    let source_path = source_dir.path().join("test.png");
    create_test_png(&source_path, 100, 100);

    // Act
    let w200 =
        get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 200, cache_dir.path(), "image/png")
            .await;
    let w300 =
        get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 300, cache_dir.path(), "image/png")
            .await;

    // Assert
    assert!(w200.is_ok(), "200px thumbnail should succeed: {:?}", w200.err());
    assert!(w300.is_ok(), "300px thumbnail should succeed: {:?}", w300.err());

    let path200 = w200.unwrap();
    let path300 = w300.unwrap();

    assert_ne!(path200, path300, "different widths must produce different cache entries");
    assert!(path200.exists(), "200px cached file should exist");
    assert!(path300.exists(), "300px cached file should exist");
}

// -----------------------------------------------------------------------
// /tmp-free generation (wave 8.3 / review finding R2)
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_generation_does_not_touch_os_temp_dir() {
    // Arrange
    let cache_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    let source_path = source_dir.path().join("test.png");
    create_test_png(&source_path, 100, 100);

    // Act
    let result =
        get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 200, cache_dir.path(), "image/png")
            .await;
    assert!(result.is_ok(), "generation should succeed: {:?}", result.err());

    // Assert — the legacy path-keyed temp artifact must NOT exist in the
    // OS temp directory (`{temp_dir}/{sha256(abs_path:width)}.webp` was the
    // pre-8.3 staging location that was never cleaned).
    let source_abs = source_path.canonicalize().unwrap();
    let mut hasher = sha2::Sha256::new();
    hasher.update(source_abs.to_string_lossy().as_bytes());
    hasher.update(b":");
    hasher.update("200".as_bytes());
    let legacy_temp = std::env::temp_dir().join(format!("{}.webp", hex::encode(hasher.finalize())));
    assert!(
        !legacy_temp.exists(),
        "path-keyed temp cache must not be written: {}",
        legacy_temp.display()
    );

    // Assert — cache dir contains exactly the final thumbnail (no .tmp leftovers).
    let mut entries: Vec<String> = std::fs::read_dir(cache_dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();
    let expected = vec![format!("{}_200.webp", &TEST_CHECKSUM[..16])];
    assert_eq!(entries, expected, "cache dir must contain only the final thumbnail");
}

#[tokio::test]
async fn test_pre_existing_tmp_file_does_not_break_generation() {
    // Arrange — a stale `{key}.tmp` from a crashed generation.
    // (`{key}` already ends in `.webp`, so the temp file is `{key}.tmp`.)
    let cache_dir = tempdir().unwrap();
    let stale_tmp = cache_dir.path().join(format!("{}_200.webp.tmp", &TEST_CHECKSUM[..16]));
    std::fs::write(&stale_tmp, b"stale").unwrap();

    let source_dir = tempdir().unwrap();
    let source_path = source_dir.path().join("test.png");
    create_test_png(&source_path, 100, 100);

    // Act
    let result =
        get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 200, cache_dir.path(), "image/png")
            .await;

    // Assert — generation succeeds and publishes over the stale tmp.
    assert!(result.is_ok(), "stale .tmp must not break generation: {:?}", result.err());
    let cached = result.unwrap();
    assert!(cached.exists(), "final thumbnail should exist");

    let data = std::fs::read(&cached).unwrap();
    assert!(
        data.len() > 12 && &data[0..4] == b"RIFF",
        "final file must be the freshly generated WebP, not the stale tmp content"
    );
    assert!(!stale_tmp.exists(), ".tmp must be consumed by the atomic rename");
}

#[tokio::test]
#[ignore = "requires test-fixtures/sample_video.webm (run scripts/generate-fixtures.sh)"]
async fn test_video_generation_leaves_no_temp_or_intermediate_files() {
    // Arrange
    let cache_dir = tempdir().unwrap();
    let source = crate::test_support::fixture_path("sample_video.webm");
    assert!(source.exists(), "test fixture should exist: {:?}", source);

    // Act
    let result =
        get_or_generate_thumbnail(&source, TEST_CHECKSUM, 200, cache_dir.path(), "video/webm")
            .await;
    assert!(result.is_ok(), "video thumbnail generation failed: {:?}", result.err());
    let cached = result.unwrap();
    assert!(cached.exists(), "final thumbnail should exist");

    // Assert — exactly one file in the cache dir: the final WebP. No
    // `{key}.frame.png` / `{key}.webp.tmp` residue may remain.
    let mut entries: Vec<String> = std::fs::read_dir(cache_dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();
    let expected = vec![format!("{}_200.webp", &TEST_CHECKSUM[..16])];
    assert_eq!(entries, expected, "cache dir must contain only the final thumbnail");

    // Assert — the legacy OS-temp video frame dir is no longer created.
    let legacy_video_temp = std::env::temp_dir().join("imageviz-video-thumbs");
    assert!(
        !legacy_video_temp.exists(),
        "legacy video temp dir must not be created: {}",
        legacy_video_temp.display()
    );
}

#[tokio::test]
async fn test_concurrent_first_requests_generate_once() {
    // Arrange
    let cache_dir = tempdir().unwrap();
    let source_dir = tempdir().unwrap();
    let source_path = source_dir.path().join("test.png");
    create_test_png(&source_path, 100, 100);

    // Act — two concurrent first-requests for the same cache key.
    let (first, second) = tokio::join!(
        get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 200, cache_dir.path(), "image/png"),
        get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 200, cache_dir.path(), "image/png")
    );

    // Assert — both succeed with the same path; deduplicated per-key locking
    // leaves exactly one file in the cache dir.
    assert!(first.is_ok(), "first concurrent request failed: {:?}", first.err());
    assert!(second.is_ok(), "second concurrent request failed: {:?}", second.err());
    assert_eq!(
        first.unwrap(),
        second.unwrap(),
        "both requests must resolve to the same cache entry"
    );

    let count = std::fs::read_dir(cache_dir.path()).unwrap().count();
    assert_eq!(count, 1, "exactly one cache file must exist after concurrent misses");
}

// -----------------------------------------------------------------------
// Eviction tests
// -----------------------------------------------------------------------

#[test]
fn test_eviction_when_over_limit() {
    use crate::thumbnails::cache::{dir_size, evict_if_needed};
    use std::fs;

    let dir = tempfile::tempdir().unwrap();

    // Create 10 files of 10 MB each = 100 MB total.
    for i in 0..10 {
        let path = dir.path().join(format!("thumb_{i}.webp"));
        let file = fs::File::create(&path).unwrap();
        // sparse file: allocate 10 MB without writing actual bytes
        file.set_len(10 * 1024 * 1024).unwrap();
    }

    // Set max to 50 MB — should evict down to ~40 MB (80% of 50 MB).
    let stats = evict_if_needed(dir.path(), 50_000_000, 1_000_000_000).unwrap();

    assert!(stats.evicted > 0, "Should evict some files when over limit");

    let remaining = dir_size(dir.path()).unwrap();
    assert!(remaining <= 50_000_000, "Remaining size {remaining} should be under max 50 MB");
}

#[test]
fn test_no_eviction_when_under_limit() {
    use crate::thumbnails::cache::evict_if_needed;
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();

    // Create small files totaling ~few hundred bytes.
    for i in 0..3 {
        let path = dir.path().join(format!("small_{i}.webp"));
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, "not a real webp").unwrap();
    }

    let stats = evict_if_needed(dir.path(), 1_000_000_000, 100_000_000).unwrap();
    assert_eq!(stats.evicted, 0, "Should not evict when under limit");
}

#[test]
fn test_eviction_empty_cache_does_not_crash() {
    use crate::thumbnails::cache::evict_if_needed;
    let dir = tempfile::tempdir().unwrap();
    let stats = evict_if_needed(dir.path(), 1_000_000, 100_000_000).unwrap();
    assert_eq!(stats.evicted, 0);
}

#[test]
fn test_dir_size_empty_directory() {
    use crate::thumbnails::cache::dir_size;
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(dir_size(dir.path()).unwrap(), 0);
}

#[test]
fn test_free_disk_space_returns_reasonable_value() {
    use crate::thumbnails::cache::free_disk_space;

    // Should return a reasonable free-space value for the temp directory
    // (or u64::MAX if the platform call fails).  It should never panic or
    // return 0 on a typical filesystem.
    let dir = tempfile::tempdir().unwrap();
    let space = free_disk_space(dir.path());
    assert!(
        space > 0 || space == u64::MAX,
        "free_disk_space should return >0 or u64::MAX on failure, got: {space}"
    );
}

#[test]
fn test_free_disk_space_existing_path_is_reasonable() {
    use crate::thumbnails::cache::free_disk_space;

    // Even a non-existent path should resolve via the parent directory.
    let space = free_disk_space(Path::new("/"));
    assert!(
        space > 0 || space == u64::MAX,
        "free_disk_space for root should return something, got: {space}"
    );
}

#[test]
fn test_dir_size_counts_only_direct_files() {
    use crate::thumbnails::cache::dir_size;

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.webp"), b"hello").unwrap();
    std::fs::create_dir(dir.path().join("subdir")).unwrap();
    // File inside subdir — dir_size is shallow so this should not count.
    std::fs::write(dir.path().join("subdir").join("b.webp"), b"world").unwrap();
    assert_eq!(dir_size(dir.path()).unwrap(), 5, "Should only count direct files");
}
