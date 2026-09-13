//! Integration tests for CORS origin restriction (wave 8.26, review §4 R9).
//!
//! Verifies the ticket's acceptance criteria at the HTTP boundary:
//! - `Origin: http://localhost:5173` receives CORS grants (Vite dev works)
//! - Any other origin receives no `Access-Control-Allow-Origin` grant
//! - Preflight `OPTIONS` for `PUT` succeeds only for allowed origins
//!
//! The app mirrors `main.rs` assembly: `health_router()` + the env-configured
//! CORS layer. Preflights are answered by the CORS layer before routing, so
//! no route/state setup is required beyond the health endpoint.

use axum::{
    Router,
    body::Body,
    http::{Request, Response, StatusCode, header},
};
use tower::ServiceExt;

use imageviz_backend::middleware::cors;

const DEV_ORIGIN: &str = "http://localhost:5173";
const FOREIGN_ORIGIN: &str = "http://evil.example";

/// Production-like app assembly (logging/security layers omitted — they do
/// not participate in CORS decisions).
fn cors_app() -> Router {
    imageviz_backend::health_router().layer(cors::cors_layer_from_env())
}

async fn send(app: Router, req: Request<Body>) -> Response<Body> {
    app.oneshot(req).await.expect("oneshot request")
}

#[tokio::test]
async fn health_get_from_dev_origin_has_cors_grant() {
    let res = send(
        cors_app(),
        Request::get("/api/v1/health")
            .header(header::ORIGIN, DEV_ORIGIN)
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).expect("ACAO header"),
        DEV_ORIGIN,
        "Vite dev origin must be allowed"
    );
}

#[tokio::test]
async fn health_get_from_foreign_origin_has_no_grant() {
    let res = send(
        cors_app(),
        Request::get("/api/v1/health")
            .header(header::ORIGIN, FOREIGN_ORIGIN)
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(res.status(), StatusCode::OK);
    assert!(
        res.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none(),
        "browsers must be unable to read responses from foreign origins"
    );
}

#[tokio::test]
async fn preflight_put_from_dev_origin_succeeds() {
    let res = send(
        cors_app(),
        Request::options("/api/v1/config")
            .header(header::ORIGIN, DEV_ORIGIN)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "PUT")
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(res.status(), StatusCode::OK, "preflight for allowed origin must succeed");
    assert_eq!(
        res.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).expect("ACAO header"),
        DEV_ORIGIN
    );
    let methods = res.headers().get(header::ACCESS_CONTROL_ALLOW_METHODS).expect("methods");
    assert!(methods.to_str().unwrap().contains("PUT"));
}

#[tokio::test]
async fn preflight_put_from_foreign_origin_is_rejected() {
    let res = send(
        cors_app(),
        Request::options("/api/v1/config")
            .header(header::ORIGIN, FOREIGN_ORIGIN)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "PUT")
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert!(
        res.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none(),
        "foreign preflight must not be granted"
    );
}

#[tokio::test]
async fn same_origin_request_needs_no_cors_grant() {
    // Production/same-origin usage: browsers omit the Origin header on
    // same-origin GET navigations/fetches; the response must be served
    // normally without any CORS grant.
    let res = send(cors_app(), Request::get("/api/v1/health").body(Body::empty()).unwrap()).await;

    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
}
