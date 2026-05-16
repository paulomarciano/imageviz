pub mod config;
pub mod db;
pub mod indexer;
pub mod metadata;
pub mod routes;
pub mod scanner;

/// Build the base application router with stateless routes mounted under `/api/v1`.
///
/// This is the single source of truth for route assembly, used by both
/// the production binary (`main.rs`) and integration tests (`tests/`).
/// Stateful routes (e.g., config) are added by callers that provide state.
pub fn app() -> axum::Router {
    axum::Router::new().nest("/api/v1", routes::health::routes())
}
