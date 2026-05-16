mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;

#[tokio::test]
async fn health_check_returns_ok() {
    // Arrange
    let app = common::create_test_app();

    // Act
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Assert
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn health_check_returns_json_with_status_and_version() {
    // Arrange
    let app = common::create_test_app();

    // Act
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Assert — status code
    assert_eq!(response.status(), StatusCode::OK);

    // Assert — response body contains expected fields
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string());
    assert!(!body["version"].as_str().unwrap().is_empty());
}
