#[cfg(test)]
mod tests {
    use crate::test_support::fixture_path;
    use crate::thumbnails::video::{VideoThumbnailError, extract_video_thumbnail};
    use std::path::Path;
    use tempfile::tempdir;
    use tokio::process::Command;

    /// Quick check: is ffmpeg available on this system?
    async fn ffmpeg_is_available() -> bool {
        Command::new("ffmpeg")
            .arg("-version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Verify that a file on disk looks like a valid PNG (correct magic bytes).
    fn is_valid_png(path: &Path) -> bool {
        let Ok(bytes) = std::fs::read(path) else {
            return false;
        };
        let png_header: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.len() > 8 && bytes[..8] == png_header
    }

    // ------------------------------------------------------------------
    // Happy-path tests (require ffmpeg + fixture files)
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_extract_from_webm() {
        if !ffmpeg_is_available().await {
            eprintln!("Skipping test_extract_from_webm: ffmpeg not installed");
            return;
        }

        let source = fixture_path("sample_video.webm");
        if !source.exists() {
            eprintln!("Skipping test_extract_from_webm: fixture not found at {}", source.display());
            return;
        }

        let dir = tempdir().unwrap();
        let result: Result<std::path::PathBuf, VideoThumbnailError> =
            extract_video_thumbnail(&source, dir.path(), 1).await;
        assert!(result.is_ok(), "Failed to extract webm thumbnail: {:?}", result.err());

        let output = result.unwrap();
        assert!(output.exists(), "Output file does not exist");
        assert!(
            output.file_name().unwrap().to_string_lossy().contains("sample_video_frame_1"),
            "Unexpected output filename: {:?}",
            output.file_name()
        );
        assert!(is_valid_png(&output), "Output is not a valid PNG");
    }

    #[tokio::test]
    async fn test_extract_from_mp4() {
        if !ffmpeg_is_available().await {
            eprintln!("Skipping test_extract_from_mp4: ffmpeg not installed");
            return;
        }

        let source = fixture_path("sample_video.mp4");
        if !source.exists() {
            eprintln!("Skipping test_extract_from_mp4: fixture not found at {}", source.display());
            return;
        }

        let dir = tempdir().unwrap();
        let result: Result<std::path::PathBuf, VideoThumbnailError> =
            extract_video_thumbnail(&source, dir.path(), 1).await;
        assert!(result.is_ok(), "Failed to extract mp4 thumbnail: {:?}", result.err());

        let output = result.unwrap();
        assert!(output.exists(), "Output file does not exist");
        assert!(
            output.file_name().unwrap().to_string_lossy().contains("sample_video_frame_1"),
            "Unexpected output filename: {:?}",
            output.file_name()
        );
        assert!(is_valid_png(&output), "Output is not a valid PNG");
    }

    #[tokio::test]
    async fn test_extract_at_custom_timestamp() {
        if !ffmpeg_is_available().await {
            eprintln!("Skipping test_extract_at_custom_timestamp: ffmpeg not installed");
            return;
        }

        let source = fixture_path("sample_video.webm");
        if !source.exists() {
            eprintln!(
                "Skipping test_extract_at_custom_timestamp: fixture not found at {}",
                source.display()
            );
            return;
        }

        let dir = tempdir().unwrap();
        let result: Result<std::path::PathBuf, VideoThumbnailError> =
            extract_video_thumbnail(&source, dir.path(), 2).await;
        assert!(result.is_ok(), "Failed to extract at timestamp 2: {:?}", result.err());

        let output = result.unwrap();
        assert!(output.exists(), "Output file does not exist");
        assert!(
            output.file_name().unwrap().to_string_lossy().contains("_frame_2"),
            "Expected timestamp 2 in filename, got: {:?}",
            output.file_name()
        );
        assert!(is_valid_png(&output), "Output is not a valid PNG");
    }

    // ------------------------------------------------------------------
    // Error-path tests (no ffmpeg or fixture needed for most)
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_extract_from_nonexistent_file() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("nonexistent_video.mp4");
        let result: Result<std::path::PathBuf, VideoThumbnailError> =
            extract_video_thumbnail(&source, dir.path(), 1).await;

        assert!(
            matches!(result, Err(VideoThumbnailError::SourceNotFound(_))),
            "Expected SourceNotFound, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_ffmpeg_not_installed() {
        // Only run this test when ffmpeg is NOT on PATH.
        if ffmpeg_is_available().await {
            eprintln!("Skipping test_ffmpeg_not_installed: ffmpeg is installed");
            return;
        }

        // Create a dummy file that "exists" so we bypass SourceNotFound.
        let dir = tempdir().unwrap();
        let source = dir.path().join("dummy_video.webm");
        std::fs::write(&source, b"dummy content").unwrap();

        let result: Result<std::path::PathBuf, VideoThumbnailError> =
            extract_video_thumbnail(&source, dir.path(), 1).await;

        assert!(
            matches!(result, Err(VideoThumbnailError::FfmpegNotFound)),
            "Expected FfmpegNotFound, got: {:?}",
            result
        );
    }
}
