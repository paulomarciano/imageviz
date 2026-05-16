use axum::{Json, Router, routing::get};
use std::net::SocketAddr;

mod routes;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(root_handler))
        .nest("/api/v1", routes::health::routes());

    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    println!("Server running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Root handler — returns a simple JSON greeting for health check.
async fn root_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}
