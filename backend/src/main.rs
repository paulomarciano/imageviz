use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use console_subscriber::ConsoleLayer;
use tokio::signal;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, prelude::*};

use r2d2::Pool;

use imageviz_backend::db::SqliteConnectionManager;

use imageviz_backend::config::AppConfig;
use imageviz_backend::indexer::progress::ProgressTracker;
use imageviz_backend::middleware::logging::logging_layer;
use imageviz_backend::middleware::security::apply_security_headers;
use imageviz_backend::middleware::timeout;
use imageviz_backend::routes::config::ConfigState;
use imageviz_backend::routes::events::EventsState;
use imageviz_backend::routes::media::MediaState;
use imageviz_backend::routes::search::SearchState;
use imageviz_backend::routes::stats::StatsState;
use imageviz_backend::search::IndexManager;
use imageviz_backend::thumbnails::limiter::ThumbnailLimiter;
use imageviz_backend::watcher::FileEvent;
use imageviz_backend::watcher::FileWatcher;
use imageviz_backend::watcher::handler::SseEvent;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    // Register tokio-console subscriber before the runtime starts so that
    // every task spawned from this point on is instrumented.
    // This is a no-op when TOKIO_CONSOLE_ADDR is not set (zero overhead at rest).
    let (console_layer, console_server) = ConsoleLayer::new();

    // The console server must be kept alive for the duration of the program.
    // We spawn it on the runtime so it runs independently of the main task.
    tokio::spawn(async move {
        if let Err(e) = console_server.serve().await {
            tracing::warn!(error = %e, "tokio-console server error");
        }
    });

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "imageviz_backend=info,tower_http=info".into());

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(true)
        .with_target(false)
        .compact();

    tracing_subscriber::registry().with(env_filter).with(fmt_layer).with(console_layer).init();

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

    let pool = imageviz_backend::db::pool::create_pool(&settings.database_path)
        .expect("Failed to create database pool");
    {
        let mut conn = pool.get().expect("Failed to get connection for migrations");
        imageviz_backend::db::migrations::run_migrations(&mut conn)
            .expect("Failed to run database migrations");
    }

    let index_manager = Arc::new(
        IndexManager::open_or_create(&settings.tantivy_index_dir, 200_000_000)
            .expect("Failed to open Tantivy index"),
    );

    let thumbnail_limiter = Arc::new(ThumbnailLimiter::new(
        imageviz_backend::thumbnails::limiter::max_thumbnail_concurrency(),
    ));

    let progress = Arc::new(ProgressTracker::new());

    // Spawn a background timer that periodically evicts old thumbnails
    // from the content-addressed cache (every 5 minutes).  This keeps the
    // cache size within the configured limit without adding latency to
    // thumbnail request paths.
    spawn_cache_eviction_timer(settings.thumbnail_cache_dir.clone());

    let (sse_tx, _) = tokio::sync::broadcast::channel::<SseEvent>(256);

    // Load startup config before creating the watcher and background
    // indexer so that both see the same configuration.
    let config = {
        let conn = pool.get().expect("Failed to get connection for config");
        imageviz_backend::config::load_config(&conn).unwrap_or_default()
    };

    // Start file system watcher.  This always creates a watcher + event
    // handler so that the config route can add watches dynamically at
    // runtime when the user adds a new watched folder.
    let config_clone = config.clone();
    let (watcher, mut file_events_rx) = start_file_watcher(&config_clone);
    let watcher = Arc::new(Mutex::new(watcher));

    // Keep a reference alive for the server lifetime — the watcher must not
    // be dropped while the server is running.  `config_state.watcher` holds
    // another reference, so `_watcher_guard` alone is not strictly required,
    // but the binding documents the intent explicitly.
    let _watcher_guard = Arc::clone(&watcher);

    let config_state = Arc::new(ConfigState {
        db: pool.clone(),
        watcher: Arc::clone(&watcher),
        index_manager: Arc::clone(&index_manager),
        progress: Arc::clone(&progress),
        db_path: settings.database_path.clone(),
    });
    let media_state = Arc::new(MediaState {
        db: pool.clone(),
        thumbnail_cache_dir: settings.thumbnail_cache_dir.clone(),
        thumbnail_limiter: Arc::clone(&thumbnail_limiter),
        total_count_cache: Arc::new(StdMutex::new(None)),
    });
    let search_state =
        Arc::new(SearchState { index_manager: Arc::clone(&index_manager), db: pool.clone() });
    let stats_state = Arc::new(StatsState { db: pool.clone(), progress: Arc::clone(&progress) });
    let events_state = Arc::new(EventsState { sse_tx: sse_tx.clone() });

    // Start background indexing and capture the handle so we can wait for
    // it to finish before activating the file watcher event handler.
    let indexing_handle = spawn_background_indexing(
        pool.clone(),
        Arc::clone(&index_manager),
        Arc::clone(&progress),
        sse_tx.clone(),
        config,
        &settings.database_path,
    );

    // Spawn a task that waits for initial indexing to complete, then
    // activates the file watcher event handler.  Events that arrive during
    // indexing are stale because full_reindex captures all files from
    // SQLite — processing them would be wasted work, so we drain the
    // channel before starting the handler.
    let handler_pool = pool.clone();
    let handler_im = Arc::clone(&index_manager);
    let handler_sse_tx = sse_tx.clone();
    tokio::spawn(async move {
        let _ = indexing_handle.await;
        // Discard any events that accumulated while indexing was in
        // progress — they refer to files already captured by the full
        // Tantivy reindex.
        while file_events_rx.try_recv().is_ok() {}
        imageviz_backend::watcher::handler::run_event_handler(
            file_events_rx,
            handler_pool,
            handler_im,
            handler_sse_tx,
        )
        .await;
    });

    // Build route groups with per-group timeout middleware.
    //
    // Most routes use the default timeout (60s, or as configured via
    // `REQUEST_TIMEOUT_SECS`).  Media routes get 120s because thumbnail
    // generation is CPU-bound.  SSE events get 3600s (1 h) because the
    // connection is long-lived.
    let app = timeout::apply_default_timeout(imageviz_backend::health_router())
        .nest(
            "/api/v1",
            timeout::apply_default_timeout(
                imageviz_backend::routes::config::routes().with_state(config_state),
            ),
        )
        .nest(
            "/api/v1",
            timeout::apply_timeout(
                imageviz_backend::routes::media::routes().with_state(media_state),
                120,
            ),
        )
        .nest(
            "/api/v1",
            timeout::apply_default_timeout(
                imageviz_backend::routes::search::routes().with_state(search_state),
            ),
        )
        .nest(
            "/api/v1",
            timeout::apply_default_timeout(
                imageviz_backend::routes::stats::routes().with_state(stats_state),
            ),
        )
        .nest(
            "/api/v1",
            timeout::apply_timeout(
                imageviz_backend::routes::events::routes().with_state(events_state),
                3600,
            ),
        )
        .layer(logging_layer())
        .layer(CorsLayer::permissive());

    // Security headers are the outermost layer so they appear on every
    // response, including those from inner middleware (timeout, CORS,
    // error handlers).
    let app = apply_security_headers(app);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], settings.port));
    tracing::info!("Server running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await.unwrap();

    tracing::info!("Shutting down gracefully...");

    let cleanup_timeout =
        tokio::time::timeout(std::time::Duration::from_secs(30), cleanup_resources(&index_manager))
            .await;

    if cleanup_timeout.is_err() {
        tracing::warn!("Cleanup timed out after 30s, forcing exit");
    }

    tracing::info!("Shutdown complete");
}

/// Listen for SIGINT (Ctrl+C) or SIGTERM and return when either is received.
///
/// This triggers [`axum::serve::with_graceful_shutdown`] to stop accepting
/// new connections and drain in-flight requests.
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
        tracing::info!("Received Ctrl+C, shutting down gracefully...");
    };

    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
        tracing::info!("Received SIGTERM, shutting down gracefully...");
    };

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

/// Commit Tantivy index and flush any pending writes before exit.
///
/// This runs after the HTTP server has stopped accepting new connections
/// and drained in-flight requests. The 30-second timeout in `main`
/// prevents the process from hanging indefinitely.
async fn cleanup_resources(index_manager: &Arc<IndexManager>) {
    if let Err(e) = index_manager.commit() {
        tracing::warn!(error = %e, "Failed to commit Tantivy index");
    } else {
        tracing::info!("Tantivy index committed");
    }
    tracing::info!("Resources cleaned up");
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
/// # Concurrency
///
/// Phase 2 uses a **separate read-only SQLite connection** (WAL mode allows
/// concurrent readers) so that the connection pool remains available for API
/// requests during the Tantivy reindex. Without this, every `GET /api/v1/media`
/// or `/search` request would block until the entire Tantivy index was rebuilt.
/// Returns a `JoinHandle` that completes when the initial index finishes.
/// Callers can await this handle before starting the file watcher event
/// handler so that events arriving during the CPU-heavy initial scan are
/// not processed wastefully.
fn spawn_background_indexing(
    pool: Pool<SqliteConnectionManager>,
    index_manager: Arc<IndexManager>,
    progress: Arc<ProgressTracker>,
    sse_tx: tokio::sync::broadcast::Sender<SseEvent>,
    config: AppConfig,
    database_path: &std::path::Path,
) -> tokio::task::JoinHandle<()> {
    if config.watched_folders.is_empty() {
        tracing::info!("No watched folders configured — skipping initial index");
        return tokio::spawn(async {});
    }

    let db_path = database_path.to_path_buf();

    tokio::spawn(async move {
        let index_start = std::time::Instant::now();

        tracing::info!(
            folders = %config.watched_folders.iter().map(|f| f.path.as_str()).collect::<Vec<_>>().join(", "),
            "Starting initial file scan and indexing"
        );

        // Phase 1: scan files and populate SQLite
        let stats =
            match imageviz_backend::indexer::full_index(&pool, &config, progress.as_ref()).await {
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

        // Phase 2 is CPU-bound (iterating all SQLite rows, indexing into
        // Tantivy), so it must run on a blocking thread to avoid starving
        // the async runtime.
        // `full_reindex` returns `Result<_, Box<dyn Error>>` which is not `Send`,
        // so we convert to `Option` inside the closure for `spawn_blocking`.
        let im = Arc::clone(&index_manager);
        let tantivy_ok = match tokio::task::spawn_blocking(move || {
            imageviz_backend::search::indexer::full_reindex(&read_conn, &im).ok()
        })
        .await
        {
            Ok(Some(search_stats)) => {
                tracing::info!(
                    indexed = search_stats.indexed_count,
                    errors = search_stats.errors,
                    "Tantivy search index populated"
                );
                true
            }
            Ok(None) => {
                tracing::error!("Failed to populate Tantivy index");
                false
            }
            Err(join_e) => {
                tracing::error!(error = %join_e, "Tantivy reindex task panicked");
                false
            }
        };

        let duration_ms = index_start.elapsed().as_millis() as u64;

        if tantivy_ok
            && let Err(e) = sse_tx.send(SseEvent {
                event_type: "indexing_complete".into(),
                data: serde_json::json!({
                    "total": stats.created + stats.updated + stats.skipped,
                    "duration_ms": duration_ms,
                }),
            })
        {
            tracing::warn!(error = %e, "Failed to broadcast indexing_complete");
        }
    })
}

/// Spawn a background task that periodically evicts old thumbnails.
///
/// Runs every 5 minutes and calls [`evict_if_needed`] with the configured
/// cache size and free-disk-space limits.  This is the primary eviction
/// mechanism; the inline fire-and-forget spawn in `get_or_generate_thumbnail`
/// is an additional safety net for cache bursts.
fn spawn_cache_eviction_timer(cache_dir: PathBuf) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            if let Err(e) = imageviz_backend::thumbnails::cache::evict_if_needed(
                &cache_dir,
                imageviz_backend::thumbnails::cache::max_cache_size(),
                imageviz_backend::thumbnails::cache::min_free_disk_space(),
            ) {
                tracing::warn!(error = %e, "Background cache eviction failed");
            }
        }
    });
}

/// Create a file system watcher and return both the watcher guard and the
/// event receiver.
///
/// The returned **watcher must be kept alive** — dropping it stops all
/// monitoring.  Callers typically bind the first element to
/// `let _watcher = ...` so it lives for the duration of `main`.
///
/// Unlike the earlier design, this function always creates a watcher and
/// event channel, even when the config has no folders.  This allows the
/// [`routes::config::update_config`] handler to dynamically add watches
/// at runtime via [`FileWatcher::watch`].
///
/// **The event handler is NOT spawned here.**  Callers are responsible
/// for starting the handler (via
/// [`handler::run_event_handler`](imageviz_backend::watcher::handler::run_event_handler))
/// after initial indexing completes, so that file-watch events arriving
/// during the CPU-heavy initial scan are not processed wastefully.
fn start_file_watcher(config: &AppConfig) -> (FileWatcher, mpsc::Receiver<Vec<FileEvent>>) {
    let paths: Vec<PathBuf> =
        config.watched_folders.iter().map(|f| PathBuf::from(&f.path)).collect();

    let (watcher, rx) = FileWatcher::new(&paths).expect("Failed to create file watcher");

    tracing::info!(
        paths = %paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join(", "),
        "File watcher started ({} paths)",
        paths.len(),
    );

    (watcher, rx)
}
