#[cfg(test)]
mod tests {
    use crate::metadata::video::*;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    /// Return the absolute path to a file in test-fixtures/.
    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("CARGO_MANIFEST_DIR should have a parent")
            .join("test-fixtures")
            .join(name)
    }

    #[tokio::test]
    async fn test_nonexistent_file() {
        let result = parse_video_metadata(Path::new("/nonexistent/video.mp4")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invalid_path_returns_error() {
        let result = parse_video_metadata(Path::new("")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_empty_file_returns_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("empty.mp4");
        std::fs::write(&path, b"").unwrap();

        let result = parse_video_metadata(&path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parse_webm_metadata_success() {
        let path = fixture_path("sample_video.webm");
        if !path.exists() {
            eprintln!(
                "Skipping test: fixture {} not found (run generate-fixtures.sh)",
                path.display()
            );
            return;
        }

        let meta = parse_video_metadata(&path).await.unwrap();
        assert!(meta.width > 0, "webm should have width > 0");
        assert!(meta.height > 0, "webm should have height > 0");
        assert!(meta.duration_ms.is_some(), "webm should have duration");
        assert!(meta.duration_ms.unwrap() > 0, "duration should be > 0ms");
    }

    #[tokio::test]
    async fn test_parse_mp4_metadata_success() {
        let path = fixture_path("sample_video.mp4");
        if !path.exists() {
            eprintln!(
                "Skipping test: fixture {} not found (run generate-fixtures.sh)",
                path.display()
            );
            return;
        }

        let meta = parse_video_metadata(&path).await.unwrap();
        assert!(meta.width > 0, "mp4 should have width > 0");
        assert!(meta.height > 0, "mp4 should have height > 0");
        assert!(meta.duration_ms.is_some(), "mp4 should have duration");
        assert!(meta.duration_ms.unwrap() > 0, "duration should be > 0ms");
    }
}
