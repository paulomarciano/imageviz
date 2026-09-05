use crate::metadata::video::*;
use crate::test_support::fixture_path;
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

#[tokio::test]
#[ignore = "requires test-fixtures/sample_video.webm (run scripts/generate-fixtures.sh)"]
async fn test_parse_webm_metadata_success() {
    let path = fixture_path("sample_video.webm");
    assert!(path.exists(), "Fixture not found: {}", path.display());

    let meta = parse_video_metadata(&path).await.unwrap();
    assert!(meta.width > 0, "webm should have width > 0");
    assert!(meta.height > 0, "webm should have height > 0");
    assert!(meta.duration_ms.is_some(), "webm should have duration");
    assert!(meta.duration_ms.unwrap() > 0, "duration should be > 0ms");
}

#[tokio::test]
#[ignore = "requires test-fixtures/sample_video.mp4 (run scripts/generate-fixtures.sh)"]
async fn test_parse_mp4_metadata_success() {
    let path = fixture_path("sample_video.mp4");
    assert!(path.exists(), "Fixture not found: {}", path.display());

    let meta = parse_video_metadata(&path).await.unwrap();
    assert!(meta.width > 0, "mp4 should have width > 0");
    assert!(meta.height > 0, "mp4 should have height > 0");
    assert!(meta.duration_ms.is_some(), "mp4 should have duration");
    assert!(meta.duration_ms.unwrap() > 0, "duration should be > 0ms");
}
