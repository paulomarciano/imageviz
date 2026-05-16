#![allow(dead_code)]

use axum::Router;
use std::path::{Path, PathBuf};

/// Root directory of the project (parent of backend/).
///
/// Uses `CARGO_MANIFEST_DIR` which is set to `backend/` at compile time,
/// so we go one level up to reach the project root.
fn project_root() -> &'static Path {
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

/// Create a test app with all routes mounted for integration testing.
/// Uses the same route definitions as the production server via the app factory.
pub fn create_test_app() -> Router {
    imageviz_backend::app()
}
