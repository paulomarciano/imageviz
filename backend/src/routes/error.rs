//! Shared route-layer error type — `AppError`.
//!
//! Every fallible handler returns `Result<_, AppError>` so error responses are
//! built in exactly one place (wave-8.16 / review D6): the pool-acquisition
//! block and the 500 tuple are no longer hand-written per handler.
//!
//! # Contract
//!
//! Each variant renders the exact status + JSON body its call sites produced
//! before the migration — message strings are asserted verbatim by
//! [`error_test`] and by the untouched integration suite.
//!
//! # Logging
//!
//! The generic `Pool`/`Db` variants log their source error once, here in
//! [`IntoResponse::into_response`]. Variants wrapping non-generic sources
//! (files, Tantivy, thumbnails) are logged at their call site where the
//! context is known, so every error path logs exactly once and nothing is
//! double-logged.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

use crate::middleware::validation::ValidationError;

/// Route-layer error: one variant per distinct response shape in the API.
#[derive(Debug)]
pub enum AppError {
    /// Database pool acquisition failed → 503 "Service temporarily unavailable".
    Pool(r2d2::Error),
    /// A SQLite query failed → 500 "Internal server error".
    Db(rusqlite::Error),
    /// 404 with a specific message ("Media not found", "File not found on disk").
    NotFound(&'static str),
    /// 500 with a non-default message ("Search failed", "Failed to save
    /// configuration", …).
    Internal(&'static str),
    /// 400 with a dynamic message (invalid search query syntax).
    BadRequest(String),
    /// 503 with a non-pool message (thumbnail generation limiter).
    ServiceUnavailable(&'static str),
    /// 416 whose body carries a `content_range` hint: the byte size the
    /// client's Range must fall within.
    RangeNotSatisfiable(u64),
    /// 400 body produced by the wave-7.6 validators, passed through verbatim
    /// (including the optional `details` array — the shape differs from a
    /// plain `{"error": …}` body, so it is not flattened into a string).
    Validation(ValidationError),
}

impl From<r2d2::Error> for AppError {
    fn from(e: r2d2::Error) -> Self {
        Self::Pool(e)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e)
    }
}

impl From<ValidationError> for AppError {
    fn from(e: ValidationError) -> Self {
        Self::Validation(e)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, body): (StatusCode, serde_json::Value) = match &self {
            AppError::Pool(e) => {
                tracing::error!(error = %e, "Failed to acquire database connection");
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    json!({ "error": "Service temporarily unavailable" }),
                )
            }
            AppError::Db(e) => {
                tracing::error!(error = %e, "Database error");
                (StatusCode::INTERNAL_SERVER_ERROR, json!({ "error": "Internal server error" }))
            }
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, json!({ "error": msg })),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, json!({ "error": msg })),
            // Client-driven 400s are not logged (same as the validators before
            // the migration).
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, json!({ "error": msg })),
            AppError::ServiceUnavailable(msg) => {
                tracing::error!(error = %msg, "Request rejected by resource limiter");
                (StatusCode::SERVICE_UNAVAILABLE, json!({ "error": msg }))
            }
            AppError::RangeNotSatisfiable(file_size) => (
                StatusCode::RANGE_NOT_SATISFIABLE,
                json!({
                    "error": "Range not satisfiable",
                    "content_range": format!("bytes */{file_size}"),
                }),
            ),
            AppError::Validation(e) => (StatusCode::BAD_REQUEST, json!(e)),
        };
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
#[path = "error_test.rs"]
mod error_test;
