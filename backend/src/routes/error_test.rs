//! Pins the exact status + JSON body each [`AppError`] variant renders.
//!
//! This is the wave-8.16 contract snapshot: handler error bodies may not
//! change when migrating to `AppError`, so each variant's rendering is
//! asserted message-string verbatim here.

use axum::{http::StatusCode, response::IntoResponse};
use http_body_util::BodyExt;
use serde_json::Value;

use super::AppError;
use crate::middleware::validation::{FieldError, ValidationError};

/// Render `err` through [`IntoResponse::into_response`] — exactly what axum
/// does when a handler returns `Err` — and collect `(status, body)`.
async fn rendered(err: AppError) -> (StatusCode, Value) {
    let response = err.into_response();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).expect("error body is valid JSON");
    (status, body)
}

/// Produce a real [`r2d2::Error`] (the type has no public constructor):
/// either the pool build fails eagerly or the first `get()` fails on connect —
/// both happen because the database path lives in a nonexistent directory.
fn any_pool_error() -> r2d2::Error {
    let missing_dir = std::env::temp_dir().join("imageviz-nonexistent-dir-for-tests");
    let manager = crate::db::SqliteConnectionManager::file(&missing_dir.join("x.db"));
    match r2d2::Pool::new(manager) {
        Ok(pool) => pool.get().expect_err("connect to nonexistent dir must fail"),
        Err(e) => e,
    }
}

#[tokio::test]
async fn pool_renders_503_service_temporarily_unavailable() {
    let (status, body) = rendered(AppError::Pool(any_pool_error())).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "Service temporarily unavailable");
    assert!(body.get("details").is_none(), "pool body must be the plain error shape");
}

#[tokio::test]
async fn db_renders_500_internal_server_error() {
    let e = rusqlite::Error::InvalidColumnName("nope".into());
    let (status, body) = rendered(AppError::Db(e)).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"], "Internal server error");
}

#[tokio::test]
async fn not_found_renders_404_with_message() {
    let (status, body) = rendered(AppError::NotFound("Media not found")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "Media not found");
}

#[tokio::test]
async fn internal_renders_500_with_custom_message() {
    let (status, body) = rendered(AppError::Internal("Search lookup failed")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"], "Search lookup failed");
}

#[tokio::test]
async fn bad_request_renders_400_with_dynamic_message() {
    let (status, body) = rendered(AppError::BadRequest("Invalid query: boom".into())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid query: boom");
}

#[tokio::test]
async fn service_unavailable_renders_503_with_message() {
    let (status, body) =
        rendered(AppError::ServiceUnavailable("Too many thumbnail requests. Try again later."))
            .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "Too many thumbnail requests. Try again later.");
}

#[tokio::test]
async fn range_not_satisfiable_renders_416_with_content_range() {
    let (status, body) = rendered(AppError::RangeNotSatisfiable(1234)).await;
    assert_eq!(status, StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(body["error"], "Range not satisfiable");
    assert_eq!(body["content_range"], "bytes */1234");
}

#[tokio::test]
async fn validation_renders_400_with_details_verbatim() {
    let err = ValidationError::with_details(
        "Invalid watched folder configuration",
        vec![FieldError {
            field: "watched_folders[0].path".into(),
            message: "Path must not be empty".into(),
        }],
    );
    let (status, body) = rendered(err.into()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "Invalid watched folder configuration");
    assert_eq!(body["details"][0]["field"], "watched_folders[0].path");
    assert_eq!(body["details"][0]["message"], "Path must not be empty");
}

#[tokio::test]
async fn validation_without_details_omits_the_key() {
    let (status, body) = rendered(ValidationError::new("limit must be positive").into()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "limit must be positive");
    assert!(body.get("details").is_none(), "details must stay skip_serializing_if None");
}

#[test]
fn from_impls_map_to_the_matching_variants() {
    let pool_err = any_pool_error();
    assert!(matches!(AppError::from(pool_err), AppError::Pool(_)));

    let db_err = AppError::from(rusqlite::Error::InvalidColumnName("x".into()));
    assert!(matches!(db_err, AppError::Db(_)));

    let validation_err = AppError::from(ValidationError::new("v"));
    assert!(matches!(validation_err, AppError::Validation(_)));
}
