//! Test support utilities shared across unit and integration tests.
//!
//! This module is only compiled when `cfg(test)` is active, which includes
//! both unit tests (co-located `*_test.rs` files) and integration tests
//! (the `tests/` directory crate).
//!
//! # Usage (unit tests in `src/`)
//! ```
//! use crate::test_support::fixture_path;
//! ```
//!
//! # Usage (integration tests in `tests/`)
//! ```
//! use imageviz_backend::test_support::fixture_path;
//! ```

use std::path::{Path, PathBuf};

/// Root directory of the project (parent of `backend/`).
///
/// Uses `CARGO_MANIFEST_DIR` which is set to `backend/` at compile time,
/// so we go one level up to reach the project root.
pub fn project_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CARGO_MANIFEST_DIR should have a parent (project root)")
}

/// Return the absolute path to a file in `test-fixtures/`.
///
/// # Example
/// ```
/// let path = fixture_path("sample_comfyui_01.png");
/// assert!(path.exists());
/// ```
pub fn fixture_path(name: &str) -> PathBuf {
    project_root().join("test-fixtures").join(name)
}
