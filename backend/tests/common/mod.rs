use axum::Router;

/// Create a test app with all routes mounted for integration testing.
/// Uses the same route definitions as the production server.
pub fn create_test_app() -> Router {
    Router::new().nest("/api/v1", imageviz_backend::routes::health::routes())
}
