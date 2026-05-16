use std::sync::Arc;

use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

use imageviz_backend::routes::config::ConfigState;
use imageviz_backend::routes::media::MediaState;
use imageviz_backend::routes::search::SearchState;
use imageviz_backend::routes::stats::StatsState;
use imageviz_backend::search::IndexManager;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Load settings from environment
    let settings = imageviz_backend::config::settings::Settings::from_env();

    // Ensure data directories exist before opening the database
    if let Some(parent) = settings.database_path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create database directory");
    }
    if let Some(parent) = settings.thumbnail_cache_dir.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create thumbnail cache directory");
    }
    if let Some(parent) = settings.tantivy_index_dir.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create Tantivy index directory");
    }

    // Open database and run migrations
    let conn =
        imageviz_backend::db::open(&settings.database_path).expect("Failed to open database");
    let mut conn_mut = conn;
    imageviz_backend::db::migrations::run_migrations(&mut conn_mut)
        .expect("Failed to run database migrations");

    // Wrap DB connection in Arc + Mutex so it can be shared across state structs
    let db = Arc::new(Mutex::new(conn_mut));

    // Initialize Tantivy search index
    let index_manager = Arc::new(
        IndexManager::open_or_create(&settings.tantivy_index_dir)
            .expect("Failed to open Tantivy index"),
    );

    // Initialize progress tracker
    let progress = Arc::new(imageviz_backend::indexer::progress::ProgressTracker::new());

    // Create broadcast channel for SSE events
    let (_sse_tx, _) = tokio::sync::broadcast::channel::<
        imageviz_backend::watcher::handler::SseEvent,
    >(256);

    // Build shared states
    let config_state = Arc::new(ConfigState { db: Arc::clone(&db) });
    let media_state = Arc::new(MediaState {
        db: Arc::clone(&db),
        thumbnail_cache_dir: settings.thumbnail_cache_dir.clone(),
    });
    let search_state = Arc::new(SearchState {
        index_manager: Arc::clone(&index_manager),
        db: Arc::clone(&db),
    });
    let stats_state = Arc::new(StatsState {
        db: Arc::clone(&db),
        progress: Arc::clone(&progress),
    });

    // Build application router with all stateful routes
    let app = imageviz_backend::app()
        .nest("/api/v1", imageviz_backend::routes::config::routes().with_state(config_state))
        .nest("/api/v1", imageviz_backend::routes::media::routes().with_state(media_state))
        .nest("/api/v1", imageviz_backend::routes::search::routes().with_state(search_state))
        .nest("/api/v1", imageviz_backend::routes::stats::routes().with_state(stats_state))
        .layer(CorsLayer::permissive());

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], settings.port));
    tracing::info!("Server running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
