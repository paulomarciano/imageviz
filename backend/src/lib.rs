pub mod config;
pub mod db;
pub mod middleware;
pub mod indexer;
pub mod metadata;
pub mod routes;
pub mod scanner;
pub mod search;
pub mod thumbnails;
pub mod watcher;

#[cfg(test)]
pub mod test_support;

/// Build a minimal router with only the stateless health endpoint mounted.
///
/// Used as the base for both the production server and integration tests.
/// Stateful routes (config, media, search, events, stats) are added by
/// callers that provide the necessary state.
///
/// # Why not "app()"?
///
/// This function does NOT return a complete application — it only mounts
/// the health route. Naming it `health_router()` makes its limited scope
/// explicit and avoids confusion when reading code that nests additional
/// stateful routes on top of it.
pub fn health_router() -> axum::Router {
    axum::Router::new().nest("/api/v1", routes::health::routes())
}
