# Wave 1.10 — Write Integration Test: Index Sample Dataset

| Field | Value |
|-------|-------|
| **Wave** | 1 — Backend: File System Scanner & Metadata Extraction |
| **Seq** | 10 |
| **Estimate** | 1.5 hours |
| **Depends on** | 1.8 (indexer orchestrator) |
| **Parallel** | No (verifies entire Wave 1 pipeline) |

---

## Overview

Write a comprehensive integration test that verifies the entire Wave 1 pipeline end-to-end: configure watched folders → scan → extract metadata → store in SQLite. Uses real ComfyUI PNGs from `test-fixtures/` and programmatically generated files for edge cases.

## Prerequisites

- All Wave 1 modules complete (1.1–1.9)
- Test fixtures in `test-fixtures/`:
  - `sample_comfyui.png` — real ComfyUI PNG with prompt + workflow
  - `sample_no_metadata.png` — clean PNG, no text chunks
  - (Optional) `sample_video.mp4` / `sample_video.webm` — if ffmpeg available

## Reference Files

- `documents/plans/development-plan.md` — §7.2 Backend Testing (integration tests in `tests/`), §7.5 Test Data Strategy
- `.opencode/context/core/standards/test-coverage.md` — integration test patterns

## Deliverables

```
backend/tests/
├── common/
│   └── mod.rs                   # Test helpers (create_test_db, temp_dir_with_files, etc.)
└── indexer_test.rs              # Integration tests for Wave 1
```

## Acceptance Criteria (Pass/Fail)

- [ ] Test: `full_index_with_real_comfyui_png` — indexes a real ComfyUI PNG, verifies metadata extracted
- [ ] Test: `full_index_with_clean_png` — indexes PNG with no metadata, verifies `metadata_json` is NULL
- [ ] Test: `full_index_with_mixed_files` — indexes multiple file types (PNG, JPG), verifies all present
- [ ] Test: `incremental_index_skips_unchanged` — index once, index again, verify no unnecessary updates
- [ ] Test: `incremental_index_updates_modified` — index, modify a file, re-index, verify updated
- [ ] Test: `index_respects_watched_folders` — files outside watched folders are not indexed
- [ ] Test: `index_handles_nonexistent_path` — configuring a nonexistent folder doesn't crash
- [ ] Test: `cursor_pagination_order` — indexed files are returned in `file_created_at DESC, id` order (preview of Wave 3.4)
- [ ] All tests use a temp directory (via `tempfile` crate) — no permanent files created
- [ ] `cargo test --test indexer_test` passes

## Implementation Notes

**Test helpers (`common/mod.rs`):**
```rust
use rusqlite::Connection;
use tempfile::TempDir;
use std::path::PathBuf;

pub fn create_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::migrations::run_migrations(&conn).unwrap();
    conn
}

pub fn create_test_dir_with_files(files: &[(&str, &[u8])]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (name, content) in files {
        std::fs::write(dir.path().join(name), content).unwrap();
    }
    dir
}

pub fn test_config(dir: &TempDir) -> AppConfig {
    AppConfig {
        watched_folders: vec![WatchedFolder {
            path: dir.path().to_string_lossy().to_string(),
            label: Some("test".to_string()),
        }],
    }
}

/// Copy a real fixture file into a temp directory
pub fn copy_fixture(fixture_name: &str, dest_dir: &Path) -> PathBuf {
    let src = Path::new("../test-fixtures").join(fixture_name);
    let dest = dest_dir.join(fixture_name);
    std::fs::copy(&src, &dest).unwrap();
    dest
}
```

**Integration test example:**
```rust
// tests/indexer_test.rs
mod common;

use common::{create_test_db, create_test_dir_with_files, copy_fixture, test_config};

#[tokio::test]
async fn test_full_index_with_real_comfyui_png() {
    let dir = tempfile::tempdir().unwrap();
    copy_fixture("sample_comfyui.png", dir.path());
    
    let db = create_test_db();
    let config = test_config(&dir);
    
    let (tracker, _rx) = ProgressTracker::new();
    full_index(&db, &config, &tracker).await.unwrap();
    
    let count: i32 = db.query_row(
        "SELECT COUNT(*) FROM media_items", [], |r| r.get(0)
    ).unwrap();
    assert_eq!(count, 1);
    
    let metadata: Option<String> = db.query_row(
        "SELECT metadata_json FROM media_items WHERE filename = 'sample_comfyui.png'",
        [],
        |r| r.get(0),
    ).unwrap();
    assert!(metadata.is_some(), "ComfyUI PNG should have metadata");
}

#[tokio::test]
async fn test_incremental_index_skips_unchanged() {
    let dir = create_test_dir_with_files(&[("test.png", &[0u8; 100])]);
    let db = create_test_db();
    let config = test_config(&dir);
    let (tracker, _rx) = ProgressTracker::new();
    
    // First index
    full_index(&db, &config, &tracker).await.unwrap();
    assert_eq!(count_media(&db), 1);
    
    // Second index (no changes)
    let (tracker2, _rx2) = ProgressTracker::new();
    full_index(&db, &config, &tracker2).await.unwrap();
    assert_eq!(count_media(&db), 1); // Still 1, not duplicated
}
```

## Test Strategy

- Run all tests with `cargo test --test indexer_test`
- Tests marked `#[ignore]` if they require ffmpeg (video tests)
- Each test should independently set up its own temp directory and DB
- Share setup logic via `common/mod.rs` but keep tests isolated
