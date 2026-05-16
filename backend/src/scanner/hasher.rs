use sha2::{Digest, Sha256};
use std::io::{self, Read};
use std::path::Path;

/// Compute SHA-256 hash of a file using streaming reads (never loads entire file).
///
/// SHA-256 computation is CPU-bound, so this runs inside `spawn_blocking`
/// to avoid starving the async runtime. Uses a 4MB read buffer for efficient
/// disk I/O on large files.
pub async fn compute_file_hash(path: &Path) -> Result<String, HashError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || compute_file_hash_sync(&path))
        .await
        .map_err(|e| HashError::Join(e.to_string()))?
}

/// Synchronous file hashing (called inside spawn_blocking).
///
/// Reads the file in 8KB chunks through a 4MB buffered reader. This avoids
/// loading the entire file into memory regardless of file size.
fn compute_file_hash_sync(path: &Path) -> Result<String, HashError> {
    let file = std::fs::File::open(path)?;
    let mut reader = io::BufReader::with_capacity(4 * 1024 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192]; // 8KB read chunks

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    let hash = hasher.finalize();
    Ok(hex::encode(hash))
}

/// Compute SHA-256 hash synchronously (for use in blocking contexts).
///
/// Useful when already in a blocking context (e.g., inside another
/// `spawn_blocking` or during startup).
pub fn compute_file_hash_blocking(path: &Path) -> Result<String, HashError> {
    compute_file_hash_sync(path)
}

#[derive(Debug)]
pub enum HashError {
    /// Wraps standard I/O errors (file not found, permission denied, etc.)
    Io(io::Error),
    /// Wraps tokio task join errors (unlikely in practice, but handled)
    Join(String),
}

impl std::fmt::Display for HashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HashError::Io(e) => write!(f, "IO error: {}", e),
            HashError::Join(e) => write!(f, "Task join error: {}", e),
        }
    }
}

impl std::error::Error for HashError {}

impl From<io::Error> for HashError {
    fn from(e: io::Error) -> Self {
        HashError::Io(e)
    }
}

#[cfg(test)]
#[path = "hasher_test.rs"]
mod tests;
