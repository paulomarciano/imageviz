//! Tests for the media path resolver (wave 8.17).
//!
//! Pins the one-statement-per-resolve contract (`RESOLVE_STATEMENTS` counter),
//! the 404 matrix (missing row / NULL folder / dangling folder / missing file),
//! and the `ResolvedMedia` field contract consumed by the file and thumbnail
//! routes.

use std::cell::Cell;
use std::path::Path;

use axum::http::StatusCode;
use rusqlite::Connection;

use super::{RESOLVE_STATEMENTS, resolve_media_row, verify_on_disk};
use crate::db::migrations::run_migrations;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Fresh in-memory database with the full schema applied.
fn test_db() -> Connection {
    let mut conn = Connection::open_in_memory().expect("open in-memory db");
    run_migrations(&mut conn).expect("run migrations");
    conn
}

/// Seed a watched folder row (`watched_folders` — the single source of truth).
fn seed_folder(conn: &Connection, id: &str, path: &Path) {
    conn.execute(
        "INSERT INTO watched_folders (id, path) VALUES (?1, ?2)",
        rusqlite::params![id, path.to_str().unwrap()],
    )
    .expect("seed watched folder");
}

/// Seed a media item with fixed filename/mime/checksum and the given folder id.
fn seed_item(conn: &Connection, id: &str, relative_path: &str, folder_id: Option<&str>) {
    conn.execute(
        "INSERT INTO media_items (id, filename, relative_path, mime_type, file_size, \
         file_created_at, file_modified_at, checksum, folder_id) \
         VALUES (?1, 'a.png', ?2, 'image/png', 10, '2025-01-01T00:00:00Z', \
         '2025-01-02T03:04:05Z', 'cksum123', ?3)",
        rusqlite::params![id, relative_path, folder_id],
    )
    .expect("seed media item");
}

fn resolve_count() -> usize {
    RESOLVE_STATEMENTS.with(Cell::get)
}

fn assert_not_found_with(err: (StatusCode, axum::Json<serde_json::Value>), message: &str) {
    assert_eq!(err.0, StatusCode::NOT_FOUND);
    assert_eq!(err.1.0["error"], message);
}

// ---------------------------------------------------------------------------
// Query half (resolve_media_row) — one statement, joined metadata, 404s
// ---------------------------------------------------------------------------

#[test]
fn resolve_media_row_returns_joined_metadata_in_one_statement() {
    let conn = test_db();
    seed_folder(&conn, "fid", Path::new("/tmp/watched"));
    seed_item(&conn, "item-1", "sub/a.png", Some("fid"));

    let before = resolve_count();
    let resolved = resolve_media_row(&conn, "item-1").expect("resolve succeeds");
    let after = resolve_count();

    assert_eq!(after - before, 1, "resolver must execute exactly one SQL statement");
    assert_eq!(
        resolved.full_path,
        Path::new("/tmp/watched").join("sub/a.png"),
        "full path joins folder path + relative path"
    );
    assert_eq!(resolved.mime_type, "image/png");
    assert_eq!(resolved.filename, "a.png");
    assert_eq!(resolved.checksum, "cksum123");
    assert_eq!(resolved.modified_at, "2025-01-02T03:04:05Z");
}

#[test]
fn resolve_media_row_missing_item_yields_media_not_found() {
    let conn = test_db();

    let err = resolve_media_row(&conn, "no-such-item").expect_err("missing row must 404");
    assert_not_found_with(err, "Media not found");
}

#[test]
fn resolve_media_row_null_folder_id_yields_file_not_found() {
    let conn = test_db();
    seed_item(&conn, "item-1", "a.png", None);

    let err = resolve_media_row(&conn, "item-1").expect_err("NULL folder must 404");
    assert_not_found_with(err, "File not found on disk");
}

#[test]
fn resolve_media_row_dangling_folder_id_yields_file_not_found() {
    let conn = test_db();
    // Simulate drift: media referencing a folder row that no longer exists.
    // FK enforcement is on, so relax it just for the seed.
    conn.execute_batch("PRAGMA foreign_keys = OFF").expect("relax FK for seed");
    seed_item(&conn, "item-1", "a.png", Some("gone"));

    let err = resolve_media_row(&conn, "item-1").expect_err("dangling folder must 404");
    assert_not_found_with(err, "File not found on disk");
}

// ---------------------------------------------------------------------------
// Full resolver (resolve_media_path) — async existence check
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolve_media_path_returns_resolved_media_when_file_on_disk() {
    let watched = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(watched.path().join("sub")).unwrap();
    std::fs::write(watched.path().join("sub/a.png"), b"png").unwrap();

    let conn = test_db();
    seed_folder(&conn, "fid", watched.path());
    seed_item(&conn, "item-1", "sub/a.png", Some("fid"));

    let resolved = resolve_media_row(&conn, "item-1").expect("resolve succeeds");
    let resolved = verify_on_disk(resolved).await.expect("file on disk");
    assert_eq!(resolved.full_path, watched.path().join("sub/a.png"));
    assert_eq!(resolved.checksum, "cksum123");
}

#[tokio::test]
async fn resolve_media_path_missing_file_yields_file_not_found() {
    let watched = tempfile::tempdir().unwrap();

    let conn = test_db();
    seed_folder(&conn, "fid", watched.path());
    seed_item(&conn, "item-1", "a.png", Some("fid"));

    let resolved = resolve_media_row(&conn, "item-1").expect("resolve succeeds");
    let err = verify_on_disk(resolved).await.expect_err("missing file on disk must 404");
    assert_not_found_with(err, "File not found on disk");
}
