#[cfg(test)]
mod tests {
    use crate::metadata::detect::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_png(path: &std::path::Path, width: u32, height: u32) {
        // Create a minimal valid PNG with specific dimensions
        let mut img = image::DynamicImage::new_rgba8(width, height);
        // Fill with non-transparent pixels to ensure valid encoding
        let pixels = img.as_mut_rgba8().unwrap();
        for p in pixels.pixels_mut() {
            *p = image::Rgba([128, 128, 128, 255]);
        }
        img.save(path).unwrap();
    }

    #[tokio::test]
    async fn test_detect_png() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.png");
        create_test_png(&path, 400, 300);

        // Act
        let info = detect_media(&path).await.unwrap();

        // Assert
        assert_eq!(info.mime_type, "image/png");
        assert_eq!(info.width, Some(400));
        assert_eq!(info.height, Some(300));
        assert!(info.file_size > 0);
    }

    #[tokio::test]
    async fn test_detect_jpg() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.jpg");
        let img = image::DynamicImage::new_rgba8(800, 600);
        img.save(&path).unwrap();

        // Act
        let info = detect_media(&path).await.unwrap();

        // Assert
        assert_eq!(info.mime_type, "image/jpeg");
        assert_eq!(info.width, Some(800));
        assert_eq!(info.height, Some(600));
    }

    #[tokio::test]
    async fn test_detect_unsupported() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"text content").unwrap();
        drop(f);

        // Act
        let result = detect_media(&path).await;

        // Assert
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_detect_nonexistent_file() {
        // Act
        let result = detect_media(std::path::Path::new("/nonexistent/file.png")).await;

        // Assert
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_detect_webp() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.webp");
        let img = image::DynamicImage::new_rgba8(100, 100);
        img.save(&path).unwrap();

        let info = detect_media(&path).await.unwrap();
        assert_eq!(info.mime_type, "image/webp");
        assert_eq!(info.width, Some(100));
        assert_eq!(info.height, Some(100));
    }

    #[tokio::test]
    async fn test_detect_webm_video() {
        let path = fixture_path("sample_video.webm");
        if !path.exists() {
            eprintln!("Skipping test: fixture not found: {}", path.display());
            return;
        }

        let info = detect_media(&path).await.unwrap();
        assert_eq!(info.mime_type, "video/webm");
        assert!(info.width.is_some_and(|w| w > 0), "webm should have width > 0");
        assert!(info.height.is_some_and(|h| h > 0), "webm should have height > 0");
        assert!(info.file_size > 0);
    }

    #[tokio::test]
    async fn test_detect_mp4_video() {
        let path = fixture_path("sample_video.mp4");
        if !path.exists() {
            eprintln!("Skipping test: fixture not found: {}", path.display());
            return;
        }

        let info = detect_media(&path).await.unwrap();
        assert_eq!(info.mime_type, "video/mp4");
        assert!(info.width.is_some_and(|w| w > 0), "mp4 should have width > 0");
        assert!(info.height.is_some_and(|h| h > 0), "mp4 should have height > 0");
        assert!(info.file_size > 0);
    }

    /// Return the absolute path to a file in test-fixtures/.
    fn fixture_path(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("CARGO_MANIFEST_DIR should have a parent")
            .join("test-fixtures")
            .join(name)
    }
}
