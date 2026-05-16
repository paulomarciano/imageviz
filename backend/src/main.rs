use std::sync::Arc;

use axum::Router;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

use imageviz_backend::routes::config::ConfigState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Load settings from environment
    let settings = imageviz_backend::config::settings::Settings::from_env();

    // Open database and run migrations
    let conn = imageviz_backend::db::open(&settings.database_path)
        .expect("Failed to open database");
    let mut conn_mut = conn;
    imageviz_backend::db::migrations::run_migrations(&mut conn_mut)
        .expect("Failed to run database migrations");

    // Build shared config state (Arc + Mutex for thread-safe access)
    let config_state = Arc::new(ConfigState {
        db: Mutex::new(conn_mut),
    });

    // Build application router
    // Config routes require Arc<ConfigState>; health routes require no state.
    // We apply .with_state() to config routes before nesting them into the
    // top-level Router<()>.
    let app = Router::new()
        .nest("/api/v1", imageviz_backend::routes::health::routes())
        .nest(
            "/api/v1",
            imageviz_backend::routes::config::routes().with_state(config_state),
        )
        .layer(CorsLayer::permissive());

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], settings.port));
    println!("Server running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
