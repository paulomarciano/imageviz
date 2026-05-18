use axum::Router;
use r2d2::Pool;

use imageviz_backend::db::SqliteConnectionManager;
use std::sync::Arc;
use tokio::sync::broadcast;

/// A test application bundling the router with all internal state handles
/// so that integration tests can seed data and drive the broadcast channel.
///
/// Fields are `pub` so that test functions can access them for seeding
/// and broadcasting.  `#[allow(dead_code)]` suppresses warnings when only
/// a subset of fields is used by a particular test file.
#[allow(dead_code)]
pub struct TestApp {
    pub router: Router,
    pub pool: Pool<SqliteConnectionManager>,
    pub index_manager: Arc<imageviz_backend::search::IndexManager>,
    pub sse_tx: broadcast::Sender<imageviz_backend::watcher::handler::SseEvent>,
    /// Kept alive for the duration of the test — holds the Tantivy index directory.
    pub _tantivy_dir: tempfile::TempDir,
    /// Kept alive for the duration of the test — holds the thumbnail cache directory.
    pub _cache_dir: tempfile::TempDir,
}

/// Create a test app with all routes mounted for integration testing.
/// Uses the same route definitions as the production server via the app factory.
///
/// `#[allow(dead_code)]` because only a subset of test files calls this
/// when compiled individually (e.g. `health_test` uses it, `search_test`
/// uses `create_test_app_with_search`).
#[allow(dead_code)]
pub fn create_test_app() -> Router {
    imageviz_backend::health_router()
}

/// Create a full test application with all Wave 3 state wired together:
///
///   - In-memory SQLite with the full schema migrated
///   - A real Tantivy index in a temporary directory
///   - A `broadcast::Sender` for SSE events
///   - A `ProgressTracker` for stats
///   - All stateful route trees (config, media, search, events, stats) mounted
///     under `/api/v1` alongside the stateless health route.
///
/// Returns a [`TestApp`] whose fields can be used to seed data, index
/// documents, and broadcast events before exercising the router.
///
/// `#[allow(dead_code)]` because only a subset of test files calls this
/// when compiled individually.
#[allow(dead_code)]
pub fn create_test_app_with_search() -> TestApp {
    // 1. In-memory SQLite pool with migrations
    let pool = imageviz_backend::db::pool::create_in_memory_pool();
    {
        let mut conn = pool.get().expect("get conn for migrations");
        imageviz_backend::db::migrations::run_migrations(&mut conn).expect("migrations");
    }

    // 2. Temporary Tantivy index
    let tantivy_dir = tempfile::tempdir().expect("tempdir for tantivy");
    let index_manager = Arc::new(
        imageviz_backend::search::IndexManager::open_or_create(
            &tantivy_dir.path().join("index"),
            50_000_000,
        )
        .expect("IndexManager"),
    );

    // 3. Broadcast channel for SSE
    let (sse_tx, _) = broadcast::channel(256);

    // 4. Progress tracker for stats
    let progress = Arc::new(imageviz_backend::indexer::progress::ProgressTracker::new());

    // 5. File watcher (watches nothing — used only to satisfy ConfigState)
    let (watcher, _watcher_rx) =
        imageviz_backend::watcher::FileWatcher::new(&[]).expect("FileWatcher");

    // 7. Build state structs
    let config_state = Arc::new(imageviz_backend::routes::config::ConfigState {
        db: pool.clone(),
        watcher: Arc::new(tokio::sync::Mutex::new(watcher)),
        index_manager: Arc::clone(&index_manager),
        progress: Arc::clone(&progress),
        db_path: tantivy_dir.path().join("imageviz.db"),
    });
    let cache_dir = tempfile::tempdir().expect("tempdir for thumbnail cache");
    let media_state = Arc::new(imageviz_backend::routes::media::MediaState {
        db: pool.clone(),
        thumbnail_cache_dir: cache_dir.path().to_path_buf(),
        thumbnail_limiter: Arc::new(imageviz_backend::thumbnails::limiter::ThumbnailLimiter::new(
            16,
        )),
        total_count_cache: Arc::new(std::sync::Mutex::new(None)),
    });
    let search_state = Arc::new(imageviz_backend::routes::search::SearchState {
        index_manager: Arc::clone(&index_manager),
        db: pool.clone(),
    });
    let events_state =
        Arc::new(imageviz_backend::routes::events::EventsState { sse_tx: sse_tx.clone() });
    let stats_state = Arc::new(imageviz_backend::routes::stats::StatsState {
        db: pool.clone(),
        progress: Arc::clone(&progress),
    });

    // 6. Assemble the full router under `/api/v1`
    let router = imageviz_backend::health_router()
        .nest("/api/v1", imageviz_backend::routes::config::routes().with_state(config_state))
        .nest("/api/v1", imageviz_backend::routes::media::routes().with_state(media_state))
        .nest("/api/v1", imageviz_backend::routes::search::routes().with_state(search_state))
        .nest("/api/v1", imageviz_backend::routes::events::routes().with_state(events_state))
        .nest("/api/v1", imageviz_backend::routes::stats::routes().with_state(stats_state));

    TestApp {
        router,
        pool,
        index_manager,
        sse_tx,
        _tantivy_dir: tantivy_dir,
        _cache_dir: cache_dir,
    }
}
