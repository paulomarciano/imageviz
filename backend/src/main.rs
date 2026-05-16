use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

use imageviz_backend::config::AppConfig;
use imageviz_backend::indexer::progress::ProgressTracker;
use imageviz_backend::routes::config::ConfigState;
use imageviz_backend::routes::events::EventsState;
use imageviz_backend::routes::media::MediaState;
use imageviz_backend::routes::search::SearchState;
use imageviz_backend::routes::stats::StatsState;
use imageviz_backend::search::IndexManager;
use imageviz_backend::watcher::FileWatcher;
use imageviz_backend::watcher::handler::SseEvent;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let settings = imageviz_backend::config::settings::Settings::from_env();

    if let Some(parent) = settings.database_path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create database directory");
    }
    if let Some(parent) = settings.thumbnail_cache_dir.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create thumbnail cache directory");
    }
    if let Some(parent) = settings.tantivy_index_dir.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create Tantivy index directory");
    }

    let conn =
        imageviz_backend::db::open(&settings.database_path).expect("Failed to open database");
    let mut conn_mut = conn;
    imageviz_backend::db::migrations::run_migrations(&mut conn_mut)
        .expect("Failed to run database migrations");

    let db = Arc::new(Mutex::new(conn_mut));

    let index_manager = Arc::new(
        IndexManager::open_or_create(&settings.tantivy_index_dir)
            .expect("Failed to open Tantivy index"),
    );

    let progress = Arc::new(ProgressTracker::new());

    let (sse_tx, _) = tokio::sync::broadcast::channel::<SseEvent>(256);

    // Load startup config before creating the watcher and background
    // indexer so that both see the same configuration.
    let config = {
        let conn = db.lock().await;
        imageviz_backend::config::load_config(&conn).unwrap_or_default()
    };

    // Start file system watcher.  This always creates a watcher + event
    // handler so that the config route can add watches dynamically at
    // runtime when the user adds a new watched folder.
    let config_clone = config.clone();
    let watcher = Arc::new(Mutex::new(start_file_watcher(
        Arc::clone(&db),
        Arc::clone(&index_manager),
        sse_tx.clone(),
        &config_clone,
    )));

    // Keep a reference alive for the server lifetime — dropping the
    // _watcher_guard would stop file system monitoring.
    let _watcher_guard = Arc::clone(&watcher);

    let config_state = Arc::new(ConfigState {
        db: Arc::clone(&db),
        watcher: Arc::clone(&watcher),
        index_manager: Arc::clone(&index_manager),
        progress: Arc::clone(&progress),
        db_path: settings.database_path.clone(),
    });
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

    spawn_background_indexing(
        Arc::clone(&db),
        Arc::clone(&index_manager),
        Arc::clone(&progress),
        sse_tx.clone(),
        config,
        &settings.database_path,
    );

    let app = imageviz_backend::health_router()
        .nest("/api/v1", imageviz_backend::routes::config::routes().with_state(config_state))
        .nest("/api/v1", imageviz_backend::routes::media::routes().with_state(media_state))
        .nest("/api/v1", imageviz_backend::routes::search::routes().with_state(search_state))
        .nest("/api/v1", imageviz_backend::routes::stats::routes().with_state(stats_state))
        .nest("/api/v1", imageviz_backend::routes::events::routes().with_state(events_state))
        .layer(CorsLayer::permissive());

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], settings.port));
    tracing::info!("Server running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Spawn a background task that scans watched folders and populates indexes.
///
/// # Pipeline
///
/// 1. **Phase 1** — `full_index`: scan watched folders, compute hashes, detect
///    media types, extract PNG metadata, and upsert into SQLite.
/// 2. **Phase 2** — `full_reindex`: read all SQLite rows and rebuild the Tantivy
///    full-text search index.
///
/// # Lock safety
///
/// Phase 2 uses a **separate read-only SQLite connection** (WAL mode allows
/// concurrent readers) so that the shared `db` Mutex remains available for API
/// requests during the Tantivy reindex. Without this, every `GET /api/v1/media`
/// or `/search` request would block until the entire Tantivy index was rebuilt.
fn spawn_background_indexing(
    db: Arc<Mutex<rusqlite::Connection>>,
    index_manager: Arc<IndexManager>,
    progress: Arc<ProgressTracker>,
    sse_tx: tokio::sync::broadcast::Sender<SseEvent>,
    config: AppConfig,
    database_path: &std::path::Path,
) {
    if config.watched_folders.is_empty() {
        tracing::info!("No watched folders configured — skipping initial index");
        return;
    }

    let db_path = database_path.to_path_buf();

    tokio::spawn(async move {
        tracing::info!(
            folders = %config.watched_folders.iter().map(|f| f.path.as_str()).collect::<Vec<_>>().join(", "),
            "Starting initial file scan and indexing"
        );

        // Phase 1: scan files and populate SQLite
        let stats = match imageviz_backend::indexer::full_index(
            db.as_ref(),
            &config,
            progress.as_ref(),
        )
        .await
        {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "Initial file scan failed");
                return;
            }
        };

        tracing::info!(
            created = stats.created,
            updated = stats.updated,
            skipped = stats.skipped,
            deleted = stats.deleted,
            errors = stats.errors,
            "File scan complete"
        );

        // Phase 2: populate Tantivy full-text index from SQLite.
        //
        // We open a separate read‑only connection (WAL mode permits concurrent
        // readers) so that the shared db Mutex is never locked during the
        // Tantivy reindex.  This keeps the API responsive during startup.
        let read_conn = match imageviz_backend::db::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "Failed to open read‑only DB for Tantivy reindex");
                return;
            }
        };

        let tantivy_ok =
            match imageviz_backend::search::indexer::full_reindex(&read_conn, &index_manager) {
                Ok(search_stats) => {
                    tracing::info!(
                        indexed = search_stats.indexed_count,
                        errors = search_stats.errors,
                        "Tantivy search index populated"
                    );
                    true
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to populate Tantivy index");
                    false
                }
            };

        if tantivy_ok
            && let Err(e) = sse_tx.send(SseEvent {
                event_type: "indexing_complete".into(),
                data: serde_json::json!({
                    "total": stats.created + stats.updated + stats.skipped,
                }),
            })
        {
            tracing::warn!(error = %e, "Failed to broadcast indexing_complete");
        }
    });
}

/// Start a file system watcher that monitors watched folders for changes.
///
/// File events (create / modify / delete) are debounced (500 ms) and
/// forwarded to [`run_event_handler`], which updates SQLite, Tantivy, and
/// broadcasts an SSE event to all connected clients.
///
/// **The returned watcher must be kept alive** — dropping it stops all
/// monitoring.  Callers typically bind the return value to
/// `let _watcher = ...` so it lives for the duration of `main`.
///
/// Unlike the earlier design, this function always creates a watcher and
/// event handler, even when the config has no folders.  This allows the
/// [`routes::config::update_config`] handler to dynamically add watches
/// at runtime via [`FileWatcher::watch`].
fn start_file_watcher(
    db: Arc<Mutex<rusqlite::Connection>>,
    index_manager: Arc<IndexManager>,
    sse_tx: tokio::sync::broadcast::Sender<SseEvent>,
    config: &AppConfig,
) -> FileWatcher {
    let paths: Vec<PathBuf> = config
        .watched_folders
        .iter()
        .map(|f| PathBuf::from(&f.path))
        .collect();

    let (watcher, rx) =
        FileWatcher::new(&paths).expect("Failed to create file watcher");

    tokio::spawn(imageviz_backend::watcher::handler::run_event_handler(
        rx, db, index_manager, sse_tx,
    ));

    tracing::info!(
        paths = %paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join(", "),
        "File watcher started ({} paths)",
        paths.len(),
    );

    watcher
}
