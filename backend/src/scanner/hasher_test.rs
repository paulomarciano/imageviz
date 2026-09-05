use crate::scanner::hasher::HashError;
use crate::scanner::hasher::*;
use std::io::Write;
use tempfile::TempDir;

#[tokio::test]
async fn test_hash_same_content_same_hash() {
    // Arrange
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"hello world").unwrap();
    drop(f);

    // Act
    let hash1 = compute_file_hash(&path).await.unwrap();
    let hash2 = compute_file_hash(&path).await.unwrap();

    // Assert
    assert_eq!(hash1, hash2);
}

#[tokio::test]
async fn test_hash_different_content_different_hash() {
    // Arrange
    let dir = TempDir::new().unwrap();
    let path1 = dir.path().join("a.txt");
    let path2 = dir.path().join("b.txt");

    let mut f1 = std::fs::File::create(&path1).unwrap();
    f1.write_all(b"hello").unwrap();
    drop(f1);

    let mut f2 = std::fs::File::create(&path2).unwrap();
    f2.write_all(b"world").unwrap();
    drop(f2);

    // Act
    let hash1 = compute_file_hash(&path1).await.unwrap();
    let hash2 = compute_file_hash(&path2).await.unwrap();

    // Assert
    assert_ne!(hash1, hash2);
}

#[tokio::test]
async fn test_hash_known_value() {
    // Arrange
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"abc").unwrap();
    drop(f);

    // Act
    let hash = compute_file_hash(&path).await.unwrap();

    // Assert
    // SHA-256 of "abc"
    assert_eq!(hash, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}

#[tokio::test]
async fn test_hash_nonexistent_file() {
    // Act
    let result = compute_file_hash(std::path::Path::new("/nonexistent/file.txt")).await;

    // Assert
    assert!(result.is_err());
    match result {
        Err(HashError::Io(_)) => {} // Expected
        _ => panic!("Expected HashError::Io for nonexistent file"),
    }
}

#[tokio::test]
async fn test_blocking_api() {
    // Arrange
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.txt");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"test data").unwrap();
    drop(f);

    // Act
    let hash = compute_file_hash_blocking(&path).unwrap();

    // Assert
    assert!(!hash.is_empty());
    assert_eq!(hash.len(), 64); // SHA-256 hex = 64 chars
}
