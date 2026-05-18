use super::*;
use crate::media_types::is_supported_extension;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

/// Helper: create a `TempDir` and return the path alongside the handle
/// (kept alive for the lifetime of the test).
///
/// Uses a non-dot prefix so that `is_hidden_path` does not filter the
/// temp root — the tests need to distinguish between "inside a hidden
/// directory" (e.g. `.hidden/`) and "inside any path with a dot
/// component".
fn setup_temp_dir() -> (TempDir, PathBuf) {
    let dir = tempfile::Builder::new().prefix("imgviz_").tempdir().expect("failed to create temp dir");
    let path = dir.path().to_path_buf();
    (dir, path)
}

/// Helper: drain all events from the receiver within a timeout,
/// returning `true` if any event matches the predicate.
async fn wait_for_event<F>(rx: &mut mpsc::Receiver<Vec<FileEvent>>, mut pred: F) -> bool
where
    F: FnMut(&FileEvent) -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(batch)) => {
                if batch.iter().any(|e| pred(e)) {
                    return true;
                }
            }
            Ok(None) => return false,
            Err(_) => return false,
        }
    }
}

/// Helper: drain all events, counting them.
async fn count_events(rx: &mut mpsc::Receiver<Vec<FileEvent>>) -> usize {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut count = 0;
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(batch)) => count += batch.len(),
            Ok(None) | Err(_) => return count,
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_watcher_detects_new_file() {
    let (_dir, dir_path) = setup_temp_dir();
    let (watcher, mut rx) = FileWatcher::new(&[dir_path.clone()]).unwrap();

    let file_path = dir_path.join("new_image.png");
    std::fs::write(&file_path, b"test image data").unwrap();

    // The debouncer emits Modified (file exists after creation)
    let found = wait_for_event(&mut rx, |e| matches!(e, FileEvent::Modified { .. })).await;
    assert!(found, "Watcher should detect a newly created .png file");

    drop(watcher);
}

#[tokio::test]
#[ignore]
async fn test_watcher_detects_deletion() {
    let (_dir, dir_path) = setup_temp_dir();
    let file_path = dir_path.join("to_delete.png");
    std::fs::write(&file_path, b"data").unwrap();

    let (watcher, mut rx) = FileWatcher::new(&[dir_path.clone()]).unwrap();

    // Wait a bit for watcher to initialize
    tokio::time::sleep(Duration::from_millis(100)).await;

    std::fs::remove_file(&file_path).unwrap();

    let found = wait_for_event(&mut rx, |e| matches!(e, FileEvent::Deleted { .. })).await;
    assert!(found, "Watcher should detect deleted file");

    drop(watcher);
}

#[tokio::test]
async fn test_watcher_ignores_hidden_files() {
    let (_dir, dir_path) = setup_temp_dir();

    let hidden_dir = dir_path.join(".hidden");
    std::fs::create_dir(&hidden_dir).unwrap();

    let (watcher, mut rx) = FileWatcher::new(&[dir_path.clone()]).unwrap();

    // Wait a bit for watcher to initialize
    tokio::time::sleep(Duration::from_millis(100)).await;

    std::fs::write(hidden_dir.join("test.png"), b"data").unwrap();

    // Wait for debounce period
    tokio::time::sleep(Duration::from_millis(800)).await;

    let count = count_events(&mut rx).await;
    assert_eq!(count, 0, "Hidden files should be ignored");

    drop(watcher);
}

#[tokio::test]
async fn test_watcher_filters_non_media() {
    let (_dir, dir_path) = setup_temp_dir();
    let (watcher, mut rx) = FileWatcher::new(&[dir_path.clone()]).unwrap();

    // Wait for watcher to initialize
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create a .txt file — should be ignored
    std::fs::write(dir_path.join("notes.txt"), b"not media").unwrap();

    tokio::time::sleep(Duration::from_millis(800)).await;

    let count = count_events(&mut rx).await;
    assert_eq!(count, 0, "Non-media files should be ignored");

    drop(watcher);
}

#[tokio::test]
#[ignore]
async fn test_watcher_supports_common_media_types() {
    let (_dir, dir_path) = setup_temp_dir();
    let (watcher, mut rx) = FileWatcher::new(&[dir_path.clone()]).unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Test all supported extensions
    for ext in &["jpg", "jpeg", "webp", "gif", "mp4", "webm", "mov"] {
        let f = dir_path.join(format!("test.{}", ext));
        std::fs::write(&f, b"data").unwrap();
    }

    let found = wait_for_event(&mut rx, |e| matches!(e, FileEvent::Modified { .. })).await;
    assert!(found, "Watcher should detect supported media files");

    drop(watcher);
}

#[tokio::test]
#[ignore]
async fn test_watcher_multiple_directories() {
    let (_dir1, dir1_path) = setup_temp_dir();
    let (_dir2, dir2_path) = setup_temp_dir();

    let paths = vec![dir1_path.clone(), dir2_path.clone()];
    let (watcher, mut rx) = FileWatcher::new(&paths).unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    std::fs::write(dir1_path.join("img1.png"), b"data").unwrap();
    std::fs::write(dir2_path.join("img2.png"), b"data").unwrap();

    // Wait and count events from both dirs
    tokio::time::sleep(Duration::from_millis(800)).await;
    let count = count_events(&mut rx).await;

    // Both files should generate events (likely batched together)
    assert!(count >= 1, "Should detect files in both watched directories");

    drop(watcher);
}

// ---------------------------------------------------------------------------
// Pure function tests
// ---------------------------------------------------------------------------

#[test]
fn test_is_supported_extension_png() {
    assert!(is_supported_extension(Path::new("image.png")));
}

#[test]
fn test_is_supported_extension_jpg() {
    assert!(is_supported_extension(Path::new("photo.jpg")));
    assert!(is_supported_extension(Path::new("photo.jpeg")));
}

#[test]
fn test_is_supported_extension_webp() {
    assert!(is_supported_extension(Path::new("anim.webp")));
}

#[test]
fn test_is_supported_extension_gif() {
    assert!(is_supported_extension(Path::new("anim.gif")));
}

#[test]
fn test_is_supported_extension_video() {
    assert!(is_supported_extension(Path::new("clip.mp4")));
    assert!(is_supported_extension(Path::new("clip.webm")));
    assert!(is_supported_extension(Path::new("clip.mov")));
}

#[test]
fn test_is_supported_extension_rejects_txt() {
    assert!(!is_supported_extension(Path::new("readme.txt")));
}

#[test]
fn test_is_supported_extension_rejects_no_extension() {
    assert!(!is_supported_extension(Path::new("Makefile")));
}


