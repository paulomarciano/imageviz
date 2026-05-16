# Wave 1.7 — Implement File Hash Computation (Change Detection)

| Field | Value |
|-------|-------|
| **Wave** | 1 — Backend: File System Scanner & Metadata Extraction |
| **Seq** | 07 |
| **Estimate** | 1 hour |
| **Depends on** | None (independent utility) |
| **Parallel** | Yes — can run in parallel with 1.4, 1.5 |

---

## Overview

Implement SHA-256 hash computation for media files. This hash is used for change detection: when a file is modified, its hash changes, allowing the indexer to detect updates. The implementation must be efficient (streaming, not loading entire files into memory).

## Prerequisites

- `sha2` crate in Cargo.toml (from §13 appendix)
- `tokio::fs::File` for async file reading

## Reference Files

- `documents/plans/development-plan.md` — §4 media_items table (checksum column), §8.2 streaming file serving
- `.opencode/context/core/standards/code-quality.md`

## Deliverables

```
backend/src/scanner/
├── mod.rs                       # Updated: re-export hasher
├── hasher.rs                    # SHA-256 file hash computation
└── hasher_test.rs               # Co-located tests
```

## Acceptance Criteria (Pass/Fail)

- [ ] `compute_file_hash(path)` returns hex-encoded SHA-256 hash string
- [ ] Same file → same hash (idempotent)
- [ ] Different file content → different hash
- [ ] Streaming implementation: uses 4MB buffer, never loads entire file into memory
- [ ] Handles large files (>1GB) without memory issues
- [ ] Async: uses `tokio::fs::File` for non-blocking I/O
- [ ] Unit test: hash of known content matches expected value

## Implementation Notes

**Streaming hash with 4MB buffer:**
```rust
use sha2::{Sha256, Digest};
use tokio::io::{AsyncReadExt, BufReader};

pub async fn compute_file_hash(path: &Path) -> Result<String, Error> {
    let file = tokio::fs::File::open(path).await?;
    let mut reader = BufReader::with_capacity(4 * 1024 * 1024, file); // 4MB buffer
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 8192]; // 8KB read chunks

    loop {
        let n = reader.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    let hash = hasher.finalize();
    Ok(hex::encode(hash))
}
```

**Note:** `hex` crate is needed for encoding. Add to Cargo.toml if not already present.

**Alternative — synchronous with spawn_blocking:** For large files, `tokio::fs` isn't always faster than sync I/O. Consider using `spawn_blocking` with `std::fs::File` for the actual hashing:
```rust
pub async fn compute_file_hash(path: &Path) -> Result<String, Error> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        compute_file_hash_sync(&path)
    }).await?
}

fn compute_file_hash_sync(path: &Path) -> Result<String, Error> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}
```

The `spawn_blocking` approach is recommended since SHA-256 computation is CPU-bound and file I/O on local disk is I/O-bound but not async-friendly in all cases.

**Change detection** — The hash is stored in the `checksum` column. During re-indexing, if the hash matches, skip metadata re-extraction (optimization for future waves).

## Test Strategy

```rust
#[tokio::test]
async fn test_hash_same_content_same_hash() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, b"hello world").unwrap();
    
    let hash1 = compute_file_hash(&path).await.unwrap();
    let hash2 = compute_file_hash(&path).await.unwrap();
    assert_eq!(hash1, hash2);
}

#[tokio::test]
async fn test_hash_different_content_different_hash() {
    let dir = tempfile::tempdir().unwrap();
    let path1 = dir.path().join("a.txt");
    let path2 = dir.path().join("b.txt");
    std::fs::write(&path1, b"hello").unwrap();
    std::fs::write(&path2, b"world").unwrap();
    
    let hash1 = compute_file_hash(&path1).await.unwrap();
    let hash2 = compute_file_hash(&path2).await.unwrap();
    assert_ne!(hash1, hash2);
}

#[tokio::test]
async fn test_hash_known_value() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, b"abc").unwrap();
    
    let hash = compute_file_hash(&path).await.unwrap();
    // SHA-256 of "abc" = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
    assert_eq!(hash, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}
```

## External Docs

Use **ExternalScout** to fetch current docs for:
- `sha2` crate — `Sha256`, `Digest` trait, `update`/`finalize` API
