#[cfg(test)]
mod tests {
    use crate::metadata::detect::*;
    use crate::test_support::fixture_path;
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
    #[ignore = "requires test-fixtures/sample_video.webm (run scripts/generate-fixtures.sh)"]
    async fn test_detect_webm_video() {
        let path = fixture_path("sample_video.webm");
        assert!(path.exists(), "Fixture not found: {}", path.display());

        let info = detect_media(&path).await.unwrap();
        assert_eq!(info.mime_type, "video/webm");
        assert!(info.width.is_some_and(|w| w > 0), "webm should have width > 0");
        assert!(info.height.is_some_and(|h| h > 0), "webm should have height > 0");
        assert!(info.file_size > 0);
    }

    #[tokio::test]
    #[ignore = "requires test-fixtures/sample_video.mp4 (run scripts/generate-fixtures.sh)"]
    async fn test_detect_mp4_video() {
        let path = fixture_path("sample_video.mp4");
        assert!(path.exists(), "Fixture not found: {}", path.display());

        let info = detect_media(&path).await.unwrap();
        assert_eq!(info.mime_type, "video/mp4");
        assert!(info.width.is_some_and(|w| w > 0), "mp4 should have width > 0");
        assert!(info.height.is_some_and(|h| h > 0), "mp4 should have height > 0");
        assert!(info.file_size > 0);
    }

    #[tokio::test]
    async fn test_detect_video_dimensions_error_on_bad_file() {
        // A file with video extension but invalid content hits the ffprobe
        // error branch in detect_media, returning (None, None) for dimensions.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.mp4");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"this is not a real mp4 file").unwrap();
        drop(f);

        let info = detect_media(&path).await.unwrap();
        assert_eq!(info.mime_type, "video/mp4");
        // ffprobe fails on invalid content → dimensions are None
        assert!(info.width.is_none(), "width should be None when ffprobe fails");
        assert!(info.height.is_none(), "height should be None when ffprobe fails");
        assert!(info.file_size > 0);
    }

    #[tokio::test]
    async fn test_extract_png_metadata_from_file_with_text_chunks() {
        use crate::metadata::png::Metadata;
        use std::io::BufWriter;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("with_text.png");

        // Create a PNG with known tEXt chunks using the png encoder
        let file = std::fs::File::create(&path).unwrap();
        let w = BufWriter::new(file);
        let mut encoder = png::Encoder::new(w, 2, 2);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.add_text_chunk("prompt".to_string(), r#"{"text":"a test"}"#.to_string()).unwrap();
        let mut writer = encoder.write_header().unwrap();
        let data: Vec<u8> = vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
        writer.write_image_data(&data).unwrap();
        drop(writer);

        // This wrapper function lives in the detect module
        let metadata: Metadata = extract_png_metadata(&path).unwrap();
        assert!(metadata.prompt.is_some(), "prompt metadata should be extracted");
        assert_eq!(metadata.prompt.unwrap()["text"], "a test");
    }

    #[tokio::test]
    async fn test_extract_png_metadata_from_file_no_text() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("clean.png");
        create_test_png(&path, 4, 4);

        // Clean PNG without text chunks should return default empty metadata
        let metadata = extract_png_metadata(&path).unwrap();
        assert!(metadata.prompt.is_none(), "no prompt expected for clean PNG");
        assert!(metadata.workflow.is_none(), "no workflow expected for clean PNG");
        assert!(metadata.raw_text_entries.is_empty(), "no text entries expected");
    }

    #[tokio::test]
    async fn test_extract_png_metadata_nonexistent_file() {
        let result = extract_png_metadata(std::path::Path::new("/nonexistent/file.png"));
        assert!(result.is_err(), "nonexistent file should return error");
    }
}
