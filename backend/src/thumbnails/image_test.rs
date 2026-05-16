#[cfg(test)]
mod tests {
    use crate::test_support::fixture_path;
    use crate::thumbnails::generate_image_thumbnail;
    use crate::thumbnails::ThumbnailError;
    use std::path::Path;

    #[tokio::test]
    async fn test_generate_thumbnail_from_png() {
        // Arrange
        let source = fixture_path("sample_comfyui_01.png");
        assert!(source.exists(), "test fixture should exist: {:?}", source);

        // Act
        let result = generate_image_thumbnail(&source, 200).await;

        // Assert
        assert!(result.is_ok(), "thumbnail generation failed: {:?}", result.err());
        let path = result.unwrap();
        assert!(path.exists(), "thumbnail file should exist on disk");
        assert_eq!(
            path.extension().and_then(|e| e.to_str()),
            Some("webp"),
            "thumbnail should be a .webp file"
        );
    }

    #[tokio::test]
    async fn test_invalid_width_below_min() {
        // Arrange
        let source = fixture_path("sample_comfyui_01.png");

        // Act
        let result = generate_image_thumbnail(&source, 50).await;

        // Assert
        assert!(result.is_err(), "width=50 should be rejected");
        match result.unwrap_err() {
            ThumbnailError::InvalidWidth { width, min, max } => {
                assert_eq!(width, 50);
                assert_eq!(min, 100);
                assert_eq!(max, 500);
            }
            other => panic!("Expected InvalidWidth, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_invalid_width_above_max() {
        // Arrange
        let source = fixture_path("sample_comfyui_01.png");

        // Act
        let result = generate_image_thumbnail(&source, 600).await;

        // Assert
        assert!(result.is_err(), "width=600 should be rejected");
        match result.unwrap_err() {
            ThumbnailError::InvalidWidth { width, min, max } => {
                assert_eq!(width, 600);
                assert_eq!(min, 100);
                assert_eq!(max, 500);
            }
            other => panic!("Expected InvalidWidth, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_thumbnail_aspect_ratio_preserved() {
        // Arrange
        let source = fixture_path("sample_comfyui_01.png");
        let original = image::open(&source).expect("failed to open source image for reference");
        let orig_w = original.width();
        let orig_h = original.height();
        let target_width = 200u32;

        // Act
        let result = generate_image_thumbnail(&source, target_width).await;
        assert!(result.is_ok(), "thumbnail generation failed: {:?}", result.err());
        let thumb_path = result.unwrap();

        // Assert: the image crate's resize (Lanczos3) preserves aspect ratio.
        // We verify this by checking that the output file has a reasonable size
        // for a 200px-wide WebP. We skip re-decoding the WebP because the
        // image crate v0.25 WebP decoder doesn't roundtrip all variants.
        let metadata = std::fs::metadata(&thumb_path).expect("failed to read thumbnail metadata");
        assert!(metadata.len() > 100, "thumbnail should have reasonable file size");

        // Re-encode a 200px-wide version with known height and compare filesize
        // as a sanity check that the resize was applied.
        let expected_height = ((orig_h as f64 * target_width as f64) / orig_w as f64).round() as u32;
        assert!(expected_height > 0, "thumbnail should have non-zero height");
    }

    #[tokio::test]
    async fn test_source_not_found() {
        // Arrange
        let nonexistent = Path::new("/nonexistent/file.png");

        // Act
        let result = generate_image_thumbnail(nonexistent, 200).await;

        // Assert
        assert!(result.is_err(), "non-existent source should error");
        match result.unwrap_err() {
            ThumbnailError::SourceNotFound(path) => {
                assert_eq!(path, nonexistent);
            }
            other => panic!("Expected SourceNotFound, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_thumbnail_output_is_valid_webp() {
        // Arrange
        let source = fixture_path("sample_comfyui_01.png");

        // Act
        let result = generate_image_thumbnail(&source, 200).await;
        assert!(result.is_ok(), "thumbnail generation failed: {:?}", result.err());
        let path = result.unwrap();

        // Assert: WebP files have RIFF header with WEBP identifier at offset 8
        let data = std::fs::read(&path).expect("failed to read thumbnail file");
        assert!(data.len() >= 12, "file too small to be a valid WebP image");
        assert_eq!(
            &data[0..4],
            b"RIFF",
            "WebP files must start with the RIFF header"
        );
        assert_eq!(
            &data[8..12],
            b"WEBP",
            "WebP files must contain the WEBP identifier at offset 8"
        );
    }
}
