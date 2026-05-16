use axum::Router;

/// Create a test app with all routes mounted for integration testing.
/// Uses the same route definitions as the production server via the app factory.
pub fn create_test_app() -> Router {
    imageviz_backend::app()
}
