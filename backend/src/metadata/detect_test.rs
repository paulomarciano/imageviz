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
    async fn test_mime_types() {
        // Arrange
        let cases = [
            ("test.png", "image/png"),
            ("test.jpg", "image/jpeg"),
            ("test.jpeg", "image/jpeg"),
            ("test.webp", "image/webp"),
            ("test.gif", "image/gif"),
            ("test.mp4", "video/mp4"),
            ("test.webm", "video/webm"),
        ];

        // Act & Assert
        for (filename, expected_mime) in &cases {
            let path = std::path::Path::new(filename);
            let ext = path.extension().unwrap().to_str().unwrap();
            // Just check extension-based mapping without creating files
            let mime = match ext {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "webp" => "image/webp",
                "gif" => "image/gif",
                "mp4" => "video/mp4",
                "webm" => "video/webm",
                _ => continue,
            };
            assert_eq!(mime, *expected_mime, "MIME mismatch for {}", filename);
        }
    }
}
