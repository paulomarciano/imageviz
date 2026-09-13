//! Unit tests for the CORS middleware builder (wave 8.26, review §4 R9).
//!
//! All tests exercise the pure builders — no environment mutation — so they
//! are safe to run in parallel with any other test.

use axum::{
    Router,
    body::Body,
    http::{Request, Response, StatusCode, header},
    routing::get,
};
use tower::ServiceExt;

use crate::middleware::cors::cors_layer_from_origins;

const ALLOWED: &str = "http://localhost:5173";
const FOREIGN: &str = "http://evil.example";

fn app_with(cors: tower_http::cors::CorsLayer) -> Router {
    Router::new().route("/ping", get(|| async { "pong" })).layer(cors)
}

fn layer_for(origins: &[&str]) -> tower_http::cors::CorsLayer {
    cors_layer_from_origins(origins.iter().map(|s| s.to_string()).collect())
}

async fn send(app: Router, req: Request<Body>) -> Response<Body> {
    app.oneshot(req).await.expect("oneshot request")
}

fn get_request(origin: &str) -> Request<Body> {
    Request::get("/ping").header(header::ORIGIN, origin).body(Body::empty()).unwrap()
}

fn preflight_request(origin: &str) -> Request<Body> {
    Request::options("/ping")
        .header(header::ORIGIN, origin)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "PUT")
        .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn allowed_origin_gets_cors_grant() {
    let res = send(app_with(layer_for(&[ALLOWED])), get_request(ALLOWED)).await;

    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).expect("ACAO header"),
        ALLOWED,
        "allowed origin must be echoed in Access-Control-Allow-Origin"
    );
}

#[tokio::test]
async fn foreign_origin_gets_no_grant() {
    let res = send(app_with(layer_for(&[ALLOWED])), get_request(FOREIGN)).await;

    assert_eq!(res.status(), StatusCode::OK);
    assert!(
        res.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none(),
        "foreign origin must not receive an Access-Control-Allow-Origin grant"
    );
}

#[tokio::test]
async fn preflight_allowed_origin_succeeds_for_put_and_content_type() {
    let res = send(app_with(layer_for(&[ALLOWED])), preflight_request(ALLOWED)).await;

    assert_eq!(res.status(), StatusCode::OK, "preflight for allowed origin");
    let methods = res.headers().get(header::ACCESS_CONTROL_ALLOW_METHODS).expect("methods");
    assert!(methods.to_str().unwrap().contains("PUT"), "PUT must be preflightable");
    let headers = res.headers().get(header::ACCESS_CONTROL_ALLOW_HEADERS).expect("headers");
    assert!(
        headers.to_str().unwrap().contains("content-type"),
        "Content-Type must be allowed (frontend PUTs JSON)"
    );
    assert_eq!(res.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).expect("ACAO"), ALLOWED);
}

#[tokio::test]
async fn preflight_foreign_origin_rejected() {
    let res = send(app_with(layer_for(&[ALLOWED])), preflight_request(FOREIGN)).await;

    assert!(
        res.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none(),
        "rejected preflight must not grant the foreign origin"
    );
}

#[tokio::test]
async fn invalid_origin_entries_are_skipped_not_fatal() {
    // Control characters make an origin unparseable as a HeaderValue (e.g. a
    // header-injection attempt in CORS_ALLOW_ORIGINS). The builder must skip
    // it — with a warning — instead of panicking, and keep honoring the
    // remaining valid entries.
    let layer = layer_for(&["http://good.example:8080", "http://bad.example\r\nX-Injected: 1"]);

    let res = send(app_with(layer), get_request("http://good.example:8080")).await;
    assert_eq!(
        res.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).expect("ACAO header"),
        "http://good.example:8080",
        "valid origins must be honored alongside skipped invalid entries"
    );
}

#[tokio::test]
async fn wildcard_origin_entry_is_skipped_not_fatal() {
    // `*` parses as a valid HeaderValue, but `AllowOrigin::list` panics on a
    // wildcard entry — the builder must skip it (with a warning) instead of
    // crashing startup, and keep honoring the remaining valid entries.
    let layer = layer_for(&["*", "http://good.example:8080"]);

    let res = send(app_with(layer), get_request("http://good.example:8080")).await;
    assert_eq!(
        res.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).expect("ACAO header"),
        "http://good.example:8080",
        "valid origins must be honored alongside a skipped wildcard entry"
    );
}

#[tokio::test]
async fn preflight_allowed_origin_succeeds_for_all_api_methods() {
    // The API uses exactly GET/PUT/POST/DELETE (ticket AC3) — every one of
    // them must be preflightable for an allowed origin. A preflight is always
    // an OPTIONS request carrying `Access-Control-Request-Method`.
    for method in ["GET", "PUT", "POST", "DELETE"] {
        let req = Request::options("/ping")
            .header(header::ORIGIN, ALLOWED)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, method)
            .body(Body::empty())
            .unwrap();
        let res = send(app_with(layer_for(&[ALLOWED])), req).await;

        assert_eq!(res.status(), StatusCode::OK, "preflight for {method}");
        let methods = res.headers().get(header::ACCESS_CONTROL_ALLOW_METHODS).expect("methods");
        assert!(
            methods.to_str().unwrap().contains(method),
            "{method} must be listed in Access-Control-Allow-Methods"
        );
    }
}

#[tokio::test]
async fn empty_allowlist_grants_nothing() {
    // Edge case: an allowlist with no valid entries denies every cross-origin
    // read (fail closed).
    let res = send(app_with(layer_for(&[])), get_request(ALLOWED)).await;

    assert!(res.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
}

#[tokio::test]
async fn non_preflight_options_is_not_cors_handled() {
    // Sanity check: a plain OPTIONS without preflight headers is forwarded
    // untouched by the CORS layer (no grant headers either way).
    let req = Request::options("/ping").body(Body::empty()).unwrap();
    let res = send(app_with(layer_for(&[ALLOWED])), req).await;

    assert!(res.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
}
