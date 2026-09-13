//! CORS middleware — explicit origin allowlist (wave 8.26, review §4 R9).
//!
//! The API is same-origin in production and proxied by the Vite dev server
//! during development, so cross-origin access is only expected from Vite.
//! `CorsLayer::permissive()` used to let any website the user visits read the
//! local API (enumerate the library, trigger thumbnail generation); origins
//! are now allow-listed instead.
//!
//! # Env-var configuration
//!
//! `CORS_ALLOW_ORIGINS` — comma-separated allowlist
//! (default: `http://localhost:5173,http://127.0.0.1:5173`).

use axum::http::{HeaderValue, Method, header};
use tower_http::cors::CorsLayer;

use crate::config::settings;

/// Build a CORS layer allowing only the given origins.
///
/// KISS by design (review §4 R9): a static allowlist — no regex origins, no
/// credentials (the app never uses cookies cross-origin). Only the methods
/// the API actually serves are preflightable, plus `Content-Type` because
/// the frontend PUTs JSON bodies.
pub fn cors_layer(origins: Vec<HeaderValue>) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::PUT, Method::POST, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE])
}

/// Build a CORS layer from the given origin strings, skipping (with a
/// warning) entries that are not valid HTTP header values instead of
/// panicking at startup.
pub fn cors_layer_from_origins(origins: Vec<String>) -> CorsLayer {
    let values: Vec<HeaderValue> = origins
        .into_iter()
        .filter_map(|origin| match HeaderValue::from_str(&origin) {
            Ok(value) => Some(value),
            Err(e) => {
                tracing::warn!(origin = %origin, error = %e, "Ignoring invalid CORS_ALLOW_ORIGINS entry");
                None
            }
        })
        .collect();
    cors_layer(values)
}

/// Build the CORS layer from the `CORS_ALLOW_ORIGINS` environment variable.
pub fn cors_layer_from_env() -> CorsLayer {
    cors_layer_from_origins(settings::cors_allow_origins())
}

#[cfg(test)]
#[path = "cors_test.rs"]
mod tests;
