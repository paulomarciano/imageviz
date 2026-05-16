# Wave 1.3 — Implement File System Scanner (Walk Directory Tree)

| Field | Value |
|-------|-------|
| **Wave** | 1 — Backend: File System Scanner & Metadata Extraction |
| **Seq** | 03 |
| **Estimate** | 2 hours |
| **Depends on** | 1.2 (config management) |
| **Parallel** | No (needs config to know what to scan) |

---

## Overview

Implement a directory tree walker that recursively scans watched folders and produces a list of file paths with their metadata (filename, path, basic attributes). This is the first stage of the indexing pipeline.

## Prerequisites

- Config management (from 1.2) — scanner reads watched folders from config
- `tokio` with `fs` feature in Cargo.toml

## Reference Files

- `documents/plans/development-plan.md` — §8.1 Performance Targets (scan >500 files/sec), §12 scanner module structure
- `.opencode/context/development/principles/clean-code.md` — Rust iterators over loops

## Deliverables

```
backend/src/scanner/
├── mod.rs                       # Public interface
└── walker.rs                    # Directory walker implementation
```

## Acceptance Criteria (Pass/Fail)

- [ ] `scan_folder(path)` returns a `Vec<FileEntry>` with all files under that path (recursive)
- [ ] `FileEntry` struct contains: `filename`, `relative_path`, `absolute_path`, `file_size`, `created_at`, `modified_at`
- [ ] Scanner respects configured watched folders (scan all, handle duplicates)
- [ ] Scanner skips hidden files/directories (`.git`, `.DS_Store`, `Thumbs.db`)
- [ ] Scanner only includes supported media formats: `.png`, `.jpg`, `.jpeg`, `.webp`, `.gif`, `.mp4`, `.webm`
- [ ] Scanner handles permission errors gracefully (logs warning, continues)
- [ ] Scan of a 14K-file directory completes in < 5 seconds
- [ ] Unit test: scanning a temp directory with 100 generated files returns 100 entries

## Implementation Notes

**FileEntry struct:**
```rust
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub filename: String,
    pub relative_path: String,   // Relative to watched folder root
    pub absolute_path: PathBuf,
    pub file_size: u64,
    pub created_at: String,      // ISO 8601
    pub modified_at: String,     // ISO 8601
}
```

**Support function — supported extensions:**
```rust
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "gif", "mp4", "webm",
];
```

**Hidden file check** — On Unix, hidden files start with `.`. Skip any path component that starts with `.`.

**Parallel consideration** — For large directories, consider using `tokio::task::spawn_blocking` with `walkdir` crate (synchronous walking is faster than async walking for local filesystems). Use `walkdir` crate (add to Cargo.toml):
```rust
use walkdir::WalkDir;

pub fn scan_folder(root: &Path) -> Result<Vec<FileEntry>, Error> {
    let mut entries = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
    {
        let entry = entry?;
        if entry.file_type().is_file() && is_supported_media(entry.path()) {
            let metadata = entry.metadata()?;
            entries.push(FileEntry { ... });
        }
    }
    Ok(entries)
}
```

**Performance** — `walkdir` is synchronous and fast. Wrap in `spawn_blocking` when calling from async context.

## Test Strategy

```rust
#[test]
fn test_scan_temp_directory() {
    let dir = tempfile::tempdir().unwrap();
    // Create 10 test files (5 PNG, 5 TXT)
    for i in 0..5 {
        std::fs::write(dir.path().join(format!("image_{}.png", i)), b"fake").unwrap();
    }
    
    let entries = scan_folder(dir.path()).unwrap();
    assert_eq!(entries.len(), 5); // Only 5 PNGs, not TXTs
}

#[test]
fn test_skips_hidden_directories() {
    // Create .hidden/ directory with files — should be skipped
}

#[test]
fn test_handles_nonexistent_path() {
    let result = scan_folder(Path::new("/nonexistent/path"));
    assert!(result.is_err());
}
```

## External Docs

Use **ExternalScout** to fetch current docs for:
- `walkdir` crate — directory walking API (add to Cargo.toml)
