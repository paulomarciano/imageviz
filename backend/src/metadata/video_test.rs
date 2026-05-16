#[cfg(test)]
mod tests {
    use crate::metadata::video::*;
    use std::path::Path;
    use tempfile::TempDir;

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
}
