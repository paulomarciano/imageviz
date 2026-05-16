#[cfg(test)]
mod tests {
    use crate::thumbnails::cache::{get_or_generate_thumbnail, CacheError};
    use std::path::Path;
    use tempfile::tempdir;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Create a small solid-colour PNG file for testing.
    fn create_test_png(path: &Path, width: u32, height: u32) {
        let img = image::RgbaImage::new(width, height);
        img.save_with_format(path, image::ImageFormat::Png)
            .expect("failed to create test PNG");
    }

    /// A 64-character hex string simulating a SHA-256 checksum.
    const TEST_CHECKSUM: &str =
        "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";

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
            get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 200, cache_dir.path()).await;

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
            get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 200, cache_dir.path()).await;
        assert!(first.is_ok(), "first call should generate: {:?}", first.err());

        // Act — second call (cache hit)
        let second =
            get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 200, cache_dir.path()).await;

        // Assert
        assert!(second.is_ok(), "second call should succeed: {:?}", second.err());
        assert_eq!(
            first.unwrap(),
            second.unwrap(),
            "both calls should return the same cache path"
        );
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
            get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 200, cache_dir.path()).await;
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
            get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 200, &cache_subdir).await;

        // Assert
        assert!(
            result.is_ok(),
            "should create missing cache directory: {:?}",
            result.err()
        );
        assert!(cache_subdir.exists(), "cache directory should now exist");
        assert!(result.unwrap().exists(), "cached file should exist inside the new directory");
    }

    #[tokio::test]
    async fn test_cache_source_not_found() {
        // Arrange
        let cache_dir = tempdir().unwrap();
        let nonexistent = Path::new("/nonexistent/file.png");

        // Act
        let result = get_or_generate_thumbnail(nonexistent, TEST_CHECKSUM, 200, cache_dir.path()).await;

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
    async fn test_cache_with_different_widths() {
        // Arrange
        let cache_dir = tempdir().unwrap();
        let source_dir = tempdir().unwrap();
        let source_path = source_dir.path().join("test.png");
        create_test_png(&source_path, 100, 100);

        // Act
        let w200 =
            get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 200, cache_dir.path()).await;
        let w300 =
            get_or_generate_thumbnail(&source_path, TEST_CHECKSUM, 300, cache_dir.path()).await;

        // Assert
        assert!(w200.is_ok(), "200px thumbnail should succeed: {:?}", w200.err());
        assert!(w300.is_ok(), "300px thumbnail should succeed: {:?}", w300.err());

        let path200 = w200.unwrap();
        let path300 = w300.unwrap();

        assert_ne!(
            path200, path300,
            "different widths must produce different cache entries"
        );
        assert!(path200.exists(), "200px cached file should exist");
        assert!(path300.exists(), "300px cached file should exist");
    }
}
