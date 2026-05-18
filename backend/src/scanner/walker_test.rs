use crate::scanner::walker::WalkerError;
use crate::scanner::walker::*;
use std::io::Write;
use tempfile::TempDir;

fn create_temp_dir() -> TempDir {
    // Use a non-dot prefix so is_hidden_path does not filter the temp root.
    tempfile::Builder::new().prefix("imgviz_").tempdir().unwrap()
}

fn create_test_file(dir: &std::path::Path, name: &str, content: &[u8]) {
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content).unwrap();
}

#[test]
fn test_scan_temp_directory() {
    let dir = create_temp_dir();
    for i in 0..5 {
        create_test_file(dir.path(), &format!("image_{}.png", i), b"fake png data");
    }
    create_test_file(dir.path(), "readme.txt", b"not an image");

    let entries = scan_folder(dir.path()).unwrap();
    assert_eq!(entries.len(), 5);
}

#[test]
fn test_skips_hidden_directories() {
    let dir = create_temp_dir();
    let hidden = dir.path().join(".hidden");
    std::fs::create_dir(&hidden).unwrap();
    create_test_file(&hidden, "secret.png", b"hidden");
    create_test_file(dir.path(), "visible.png", b"visible");

    let entries = scan_folder(dir.path()).unwrap();
    assert_eq!(entries.len(), 1);
}

#[test]
fn test_handles_nonexistent_path() {
    let result = scan_folder(std::path::Path::new("/nonexistent/path_xyzzy"));
    assert!(result.is_err());
    match result {
        Err(WalkerError::PathNotFound(_)) => {}
        _ => panic!("Expected WalkerError::PathNotFound"),
    }
}

#[test]
fn test_empty_directory() {
    let dir = create_temp_dir();
    let entries = scan_folder(dir.path()).unwrap();
    assert!(entries.is_empty());
}

#[test]
fn test_supported_extensions() {
    let dir = create_temp_dir();
    let extensions = ["png", "jpg", "jpeg", "webp", "gif", "mp4", "webm", "mov"];
    for ext in &extensions {
        create_test_file(dir.path(), &format!("file.{}", ext), b"data");
    }
    create_test_file(dir.path(), "file.txt", b"text");

    let entries = scan_folder(dir.path()).unwrap();
    assert_eq!(entries.len(), extensions.len());
}

#[test]
fn test_file_entry_metadata_populated() {
    let dir = create_temp_dir();
    let path = dir.path().join("test.png");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"some content").unwrap();
    drop(f);

    let entries = scan_folder(dir.path()).unwrap();
    assert_eq!(entries.len(), 1);

    let entry = &entries[0];
    assert_eq!(entry.filename, "test.png");
    assert_eq!(entry.relative_path, "test.png");
    assert_eq!(entry.file_size, 12);
    assert!(!entry.created_at.is_empty());
    assert!(!entry.modified_at.is_empty());
}
