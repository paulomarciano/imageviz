use super::*;
use crate::media_types::is_supported_extension;
use notify::EventKind;
use notify::event::{
    AccessKind, AccessMode, CreateKind, DataChange, MetadataKind, ModifyKind, RemoveKind,
    RenameMode,
};
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
    let dir =
        tempfile::Builder::new().prefix("imgviz_").tempdir().expect("failed to create temp dir");
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
                if batch.iter().any(&mut pred) {
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

/// Regression test for the self-sustaining watcher feedback loop.
///
/// On Linux, notify's inotify backend watches `IN_OPEN`, so *reading* a
/// watched file emits `EventKind::Access(AccessKind::Open(_))`. The indexer
/// reads media files (hashing, metadata extraction), so forwarding those
/// events to the debouncer made the backend re-index forever. A pure
/// read-only open must now produce **no** `FileEvent` at all.
#[tokio::test]
async fn test_watcher_ignores_read_only_access() {
    let (_dir, dir_path) = setup_temp_dir();

    // Create the media file *before* the watcher starts so that creation
    // itself cannot contribute an event.
    let file_path = dir_path.join("existing.png");
    std::fs::write(&file_path, b"png data").unwrap();

    let (watcher, mut rx) = FileWatcher::new(std::slice::from_ref(&dir_path)).unwrap();

    // Let the debouncer settle; watches are registered synchronously by
    // `FileWatcher::new`, so this is just belt-and-braces.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Read-only open => IN_OPEN => Access(Open). This must be filtered out
    // before it reaches the debouncer.
    let bytes = std::fs::read(&file_path).unwrap();
    assert_eq!(bytes.as_slice(), b"png data".as_slice());

    // Wait for well over one debounce cycle (500ms) plus margin.
    tokio::time::sleep(Duration::from_millis(1200)).await;

    // Drop the watcher first so the channel closes and `count_events` returns
    // as soon as it drains (any already-sent event is still counted).
    drop(watcher);

    let count = count_events(&mut rx).await;
    assert_eq!(count, 0, "read-only access must not produce a FileEvent");
}

/// A genuine write must still reach the debouncer after access-event
/// filtering. A normal write emits CREATE/MODIFY (non-access) plus an
/// `IN_OPEN`/`IN_CLOSE_WRITE` pair, so the non-access kinds must survive.
#[tokio::test]
async fn test_watcher_detects_genuine_write() {
    let (_dir, dir_path) = setup_temp_dir();
    let (watcher, mut rx) = FileWatcher::new(std::slice::from_ref(&dir_path)).unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let file_path = dir_path.join("written.png");
    std::fs::write(&file_path, b"fresh bytes").unwrap();

    let found = wait_for_event(&mut rx, |e| matches!(e, FileEvent::Modified { .. })).await;
    assert!(found, "a genuine write must still produce a FileEvent");

    drop(watcher);
}

/// A genuine deletion must still reach the debouncer after access-event
/// filtering (the filter's blast radius includes `Access*`, so `Remove` is a
/// valuable complement to the read-only regression guard).
#[tokio::test]
async fn test_watcher_detects_genuine_deletion() {
    let (_dir, dir_path) = setup_temp_dir();

    // Create before the watcher starts so creation cannot contribute an event.
    let file_path = dir_path.join("to_delete.png");
    std::fs::write(&file_path, b"data").unwrap();

    let (watcher, mut rx) = FileWatcher::new(std::slice::from_ref(&dir_path)).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    std::fs::remove_file(&file_path).unwrap();

    let found = wait_for_event(&mut rx, |e| matches!(e, FileEvent::Deleted { .. })).await;
    assert!(found, "a genuine deletion must still produce a Deleted FileEvent");

    drop(watcher);
}

#[tokio::test]
async fn test_watcher_ignores_hidden_files() {
    let (_dir, dir_path) = setup_temp_dir();

    let hidden_dir = dir_path.join(".hidden");
    std::fs::create_dir(&hidden_dir).unwrap();

    let (watcher, mut rx) = FileWatcher::new(std::slice::from_ref(&dir_path)).unwrap();

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
    let (watcher, mut rx) = FileWatcher::new(std::slice::from_ref(&dir_path)).unwrap();

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
    let (watcher, mut rx) = FileWatcher::new(std::slice::from_ref(&dir_path)).unwrap();

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

// ---------------------------------------------------------------------------
// `should_forward` — platform-independent filter tests
//
// The watcher-level regression tests only exercise the filter on inotify
// (where reads emit `IN_OPEN`); these guard the predicate itself on every
// platform.
// ---------------------------------------------------------------------------

#[test]
fn test_should_forward_drops_access_open() {
    let kind = EventKind::Access(AccessKind::Open(AccessMode::Any));
    assert!(!should_forward(&kind), "IN_OPEN back-feed must be dropped");
}

#[test]
fn test_should_forward_drops_access_close() {
    for mode in [AccessMode::Write, AccessMode::Read] {
        let kind = EventKind::Access(AccessKind::Close(mode));
        assert!(!should_forward(&kind), "close access must be dropped");
    }
}

#[test]
fn test_should_forward_drops_metadata_only_modify() {
    for meta in [
        MetadataKind::Any,
        MetadataKind::AccessTime,
        MetadataKind::WriteTime,
        MetadataKind::Permissions,
        MetadataKind::Ownership,
        MetadataKind::Extended,
        MetadataKind::Other,
    ] {
        let kind = EventKind::Modify(ModifyKind::Metadata(meta));
        assert!(!should_forward(&kind), "metadata-only modify must be dropped: {kind:?}");
    }
}

#[test]
fn test_should_forward_keeps_content_changes() {
    let kept = [
        EventKind::Create(CreateKind::Any),
        EventKind::Create(CreateKind::File),
        EventKind::Create(CreateKind::Folder),
        EventKind::Modify(ModifyKind::Any),
        EventKind::Modify(ModifyKind::Data(DataChange::Any)),
        EventKind::Modify(ModifyKind::Data(DataChange::Size)),
        EventKind::Modify(ModifyKind::Data(DataChange::Content)),
        EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
        EventKind::Modify(ModifyKind::Other),
        EventKind::Remove(RemoveKind::Any),
        EventKind::Remove(RemoveKind::File),
        EventKind::Any,
        EventKind::Other,
    ];
    for kind in kept {
        assert!(should_forward(&kind), "content change must be forwarded: {kind:?}");
    }
}
