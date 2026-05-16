use std::sync::Arc;

use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

use imageviz_backend::routes::config::ConfigState;
use imageviz_backend::routes::events::EventsState;
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
    let (sse_tx, _) = tokio::sync::broadcast::channel::<
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

    let events_state = Arc::new(EventsState {
        sse_tx: sse_tx.clone(),
    });

    // Build application router with all stateful routes
    let app = imageviz_backend::app()
        .nest("/api/v1", imageviz_backend::routes::config::routes().with_state(config_state))
        .nest("/api/v1", imageviz_backend::routes::media::routes().with_state(media_state))
        .nest("/api/v1", imageviz_backend::routes::search::routes().with_state(search_state))
        .nest("/api/v1", imageviz_backend::routes::stats::routes().with_state(stats_state))
        .nest("/api/v1", imageviz_backend::routes::events::routes().with_state(events_state))
        .layer(CorsLayer::permissive());

    // ------------------------------------------------------------------
    // Background indexing: scan watched folders and populate search index
    // ------------------------------------------------------------------
    {
        let config = {
            let conn = db.lock().await;
            imageviz_backend::config::load_config(&conn).unwrap_or_default()
        };

        if !config.watched_folders.is_empty() {
            let db = Arc::clone(&db);
            let index_manager = Arc::clone(&index_manager);
            let progress = Arc::clone(&progress);
            let sse_tx = sse_tx.clone();
            let folders = config.watched_folders.iter().map(|f| f.path.clone()).collect::<Vec<_>>();

            tokio::spawn(async move {
                tracing::info!(
                    folders = ?folders,
                    "Starting initial file scan and indexing"
                );

                // Phase 1: scan files and populate SQLite
                match imageviz_backend::indexer::full_index(
                    db.as_ref(),
                    &config,
                    progress.as_ref(),
                )
                .await
                {
                    Ok(stats) => {
                        tracing::info!(
                            created = stats.created,
                            updated = stats.updated,
                            skipped = stats.skipped,
                            deleted = stats.deleted,
                            errors = stats.errors,
                            "File scan complete"
                        );

                        // Phase 2: populate Tantivy full-text index from SQLite
                        let conn = db.lock().await;
                        match imageviz_backend::search::indexer::full_reindex(&conn, &index_manager) {
                            Ok(search_stats) => {
                                tracing::info!(
                                    indexed = search_stats.indexed_count,
                                    errors = search_stats.errors,
                                    "Tantivy search index populated"
                                );
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "Failed to populate Tantivy index");
                            }
                        }
                        drop(conn);

                        // Broadcast indexing_complete event
                        let _ = sse_tx.send(
                            imageviz_backend::watcher::handler::SseEvent {
                                event_type: "indexing_complete".into(),
                                data: serde_json::json!({
                                    "total": stats.created + stats.updated + stats.skipped,
                                }),
                            },
                        );
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Initial indexing failed");
                    }
                }
            });
        } else {
            tracing::info!("No watched folders configured — skipping initial index");
        }
    }

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], settings.port));
    tracing::info!("Server running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
