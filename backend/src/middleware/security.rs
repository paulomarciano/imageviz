//! Security headers middleware for the Axum server.
//!
//! Applies standard security HTTP headers to all responses, similar to
//! the `helmet` middleware in Express.js.  Headers are applied via
//! [`tower_http::set_header::SetResponseHeaderLayer`].

use axum::http::header;
use axum::http::{HeaderName, HeaderValue};
use tower_http::set_header::SetResponseHeaderLayer;

/// `Permissions-Policy` header name (not yet in the `http` crate constants).
const PERMISSIONS_POLICY: HeaderName = HeaderName::from_static("permissions-policy");

/// Apply all security headers to a router.
///
/// This function chains [`SetResponseHeaderLayer`] layers for each security
/// header and applies them to the given router.  The headers applied are:
///
/// | Header | Value |
/// |--------|-------|
/// | `X-Content-Type-Options` | `nosniff` |
/// | `X-Frame-Options` | `SAMEORIGIN` |
/// | `X-XSS-Protection` | `0` (deprecated but harmless) |
/// | `Referrer-Policy` | `strict-origin-when-cross-origin` |
/// | `Permissions-Policy` | `camera=(), microphone=(), geolocation=()` |
/// | `Content-Security-Policy` | Permissive for local development |
pub fn apply_security_headers(router: axum::Router) -> axum::Router {
    router
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("SAMEORIGIN"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_XSS_PROTECTION,
            HeaderValue::from_static("0"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            PERMISSIONS_POLICY,
            HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; \
                 img-src 'self' data: blob:; \
                 media-src 'self' blob:; \
                 style-src 'self' 'unsafe-inline'; \
                 script-src 'self' 'unsafe-inline'",
            ),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt;

    async fn dummy_handler() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn test_security_headers_present() {
        let app = apply_security_headers(Router::new().route("/", get(dummy_handler)));

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        let headers = response.headers();

        assert_eq!(
            headers.get("x-content-type-options").and_then(|v| v.to_str().ok()),
            Some("nosniff"),
        );
        assert_eq!(
            headers.get("x-frame-options").and_then(|v| v.to_str().ok()),
            Some("SAMEORIGIN"),
        );
        assert_eq!(
            headers.get("x-xss-protection").and_then(|v| v.to_str().ok()),
            Some("0"),
        );
        assert_eq!(
            headers.get("referrer-policy").and_then(|v| v.to_str().ok()),
            Some("strict-origin-when-cross-origin"),
        );
        assert_eq!(
            headers.get("permissions-policy").and_then(|v| v.to_str().ok()),
            Some("camera=(), microphone=(), geolocation=()"),
        );
        assert!(
            headers.get("content-security-policy").is_some(),
            "Content-Security-Policy should be set",
        );
    }

    #[tokio::test]
    async fn test_security_headers_on_404() {
        let app = apply_security_headers(Router::new().route("/", get(dummy_handler)));

        let response = app
            .oneshot(Request::builder().uri("/nonexistent").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), 404);
        assert!(
            response.headers().get("x-content-type-options").is_some(),
            "Security headers should appear on error responses too",
        );
    }
}
