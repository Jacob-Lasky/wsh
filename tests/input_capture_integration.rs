//! Integration tests for input capture API endpoints.
//!
//! These tests verify the input mode switching flow:
//! - Verify default mode is passthrough (GET /input/mode)
//! - Capture with POST /input/capture
//! - Verify mode is capture
//! - Release with POST /input/release
//! - Verify mode is passthrough

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use wsh::api::router;

#[tokio::test]
async fn test_input_capture_flow() {
    let (state, _, _) = common::create_test_state();
    let app = router(state, None);

    // Step 1: Verify default mode is passthrough
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/sessions/test/input/mode")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["mode"], "passthrough", "default mode should be passthrough");

    // Step 2: Capture with POST /input/capture
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions/test/input/capture")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Step 3: Verify mode is capture
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/sessions/test/input/mode")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["mode"], "capture", "mode should be capture after /input/capture");

    // Step 4: Release with POST /input/release
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions/test/input/release")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Step 5: Verify mode is passthrough
    let response = app
        .oneshot(
            Request::builder()
                .uri("/sessions/test/input/mode")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["mode"], "passthrough", "mode should be passthrough after /input/release");
}

#[tokio::test]
async fn test_input_capture_idempotent() {
    let (state, _, _) = common::create_test_state();
    let app = router(state, None);

    // Capture multiple times should be idempotent
    for _ in 0..3 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sessions/test/input/capture")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    // Should still be in capture mode
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/sessions/test/input/mode")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["mode"], "capture");

    // Release multiple times should be idempotent
    for _ in 0..3 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sessions/test/input/release")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    // Should be in passthrough mode
    let response = app
        .oneshot(
            Request::builder()
                .uri("/sessions/test/input/mode")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["mode"], "passthrough");
}

#[tokio::test]
async fn test_input_mode_wrong_method() {
    let (state, _, _) = common::create_test_state();
    let app = router(state, None);

    // POST on /input/mode should fail (only GET is allowed)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions/test/input/mode")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    // GET on /input/capture should fail (only POST is allowed)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/sessions/test/input/capture")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    // GET on /input/release should fail (only POST is allowed)
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/sessions/test/input/release")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_input_mode_state_shared_across_requests() {
    // Test that state is properly shared across multiple requests
    let (state, _, _) = common::create_test_state();
    let app = router(state, None);

    // Capture mode
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions/test/input/capture")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Make multiple GET requests to verify state persists
    for _ in 0..5 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/sessions/test/input/mode")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["mode"], "capture", "mode should persist across requests");
    }
}
