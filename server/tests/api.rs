use axum::body::Body;
use axum::http::{Request, StatusCode};
use cazt::config::Config;
use cazt::state::AppState;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

fn test_config() -> Config {
    Config {
        bind: "127.0.0.1".into(),
        port: 8787,
        public_host: Some("192.168.1.10".into()),
        log_level: "error".into(),
        log_format: cazt::config::LogFormat::Pretty,
        web_root: std::path::PathBuf::from("/tmp/cazt-missing-ui"),
        discovery_timeout: std::time::Duration::from_millis(10),
        discovery_interval: std::time::Duration::from_secs(30),
        request_timeout: std::time::Duration::from_secs(5),
        media_session_ttl: std::time::Duration::from_secs(60),
        known_cast_hosts: vec![],
        known_dlna_locations: vec![],
        cinejoy_catalog_url: None,
        allow_direct_urls: true,
    }
}

fn app() -> axum::Router {
    cazt::api::router(AppState::new(test_config()))
}

async fn json(app: axum::Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

#[tokio::test]
async fn health_ok() {
    let (status, body) = json(app(), "/api/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["name"], "cazt");
}

#[tokio::test]
async fn lists_providers_including_samples() {
    let (status, body) = json(app(), "/api/media/providers").await;
    assert_eq!(status, StatusCode::OK);
    let ids: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"samples"));
    assert!(ids.contains(&"cinejoy"));
}

#[tokio::test]
async fn search_returns_sample_library() {
    let (status, body) = json(app(), "/api/media/search?q=bunny").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["id"] == "bbb"));
}

#[tokio::test]
async fn cinejoy_item_is_unavailable_without_catalog() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cast")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"device_id":"missing","provider":"cinejoy","media_id":"x"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn proxy_rejects_unknown_token() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/media/not-a-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn devices_endpoint_lists_array() {
    let (status, body) = json(app(), "/api/devices").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["devices"].is_array());
}

#[tokio::test]
async fn playback_status_idle_by_default() {
    let (status, body) = json(app(), "/api/playback/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active"], false);
}
