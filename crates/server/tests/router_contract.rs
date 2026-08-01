//! HTTP boundary contract for the reduced Local IT Desk server.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;

use local_it_desk_server::config::Config;
use local_it_desk_server::{db, routes};

/// Builds an isolated migrated router and keeps its runtime directory alive.
async fn test_router() -> (Router, TempDir) {
    let temp = tempfile::tempdir().expect("temporary directory");
    let config = Config::for_test(temp.path().join("desk.db"), temp.path().join("uploads"));
    let pool = db::create_pool(&config.database_path);
    let connection = pool.get().await.expect("database connection");
    connection
        .interact(|connection| db::migrations::run_migrations(connection))
        .await
        .expect("migration interaction")
        .expect("migration result");
    (routes::build_router(config, pool), temp)
}

/// Sends one empty request to the router and returns its status and parsed body.
async fn request_json(app: &Router, method: &str, path: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("response body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("JSON response")
    };
    (status, body)
}

/// Confirms liveness, readiness, and public configuration are available.
#[tokio::test]
async fn public_foundation_routes_match_contract() {
    let (app, _temp) = test_router().await;

    let (live_status, live_body) = request_json(&app, "GET", "/health/live").await;
    assert_eq!(live_status, StatusCode::OK);
    assert_eq!(live_body, serde_json::json!({ "status": "ok" }));

    let (ready_status, ready_body) = request_json(&app, "GET", "/health/ready").await;
    assert_eq!(ready_status, StatusCode::OK);
    assert_eq!(ready_body, serde_json::json!({ "status": "ready" }));

    let (config_status, config_body) = request_json(&app, "GET", "/api/config").await;
    assert_eq!(config_status, StatusCode::OK);
    assert_eq!(config_body["app_name"], "Local IT Desk");
    assert_eq!(config_body["setup_required"], true);
    assert!(config_body.get("database_path").is_none());
    assert!(config_body.get("cookie_secure").is_none());
}

/// Confirms retained API families are mounted as explicit foundation placeholders.
#[tokio::test]
async fn retained_api_families_are_mounted() {
    let (app, _temp) = test_router().await;
    for (method, path) in [
        ("GET", "/api/tickets"),
        ("POST", "/api/attachments"),
        ("GET", "/api/users"),
        ("GET", "/api/admin/audit-log"),
    ] {
        let (status, _body) = request_json(&app, method, path).await;
        assert_eq!(
            status,
            StatusCode::NOT_IMPLEMENTED,
            "retained route {method} {path} must be mounted"
        );
    }
}

/// Confirms every explicitly excluded API and event path remains unmounted.
#[tokio::test]
async fn excluded_routes_return_not_found() {
    let (app, _temp) = test_router().await;
    for path in [
        "/api/auth/callback",
        "/api/auth/refresh",
        "/api/channels",
        "/api/dms",
        "/api/documents",
        "/api/changelog",
        "/api/push-subscriptions",
        "/api/api-tokens",
        "/api/unread",
        "/ws",
    ] {
        let (status, _body) = request_json(&app, "GET", path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path} must not be mounted");
    }
}

/// Confirms the SPA fallback cannot disguise an excluded backend path as a page.
#[tokio::test]
async fn frontend_fallback_preserves_backend_not_found_responses() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let frontend_dir = temp.path().join("frontend");
    std::fs::create_dir_all(&frontend_dir).expect("frontend directory");
    std::fs::write(frontend_dir.join("index.html"), "local desk shell").expect("frontend index");

    let mut config = Config::for_test(temp.path().join("desk.db"), temp.path().join("uploads"));
    config.serve_frontend = true;
    config.frontend_dir = frontend_dir;
    let pool = db::create_pool(&config.database_path);
    let connection = pool.get().await.expect("database connection");
    connection
        .interact(|connection| db::migrations::run_migrations(connection))
        .await
        .expect("migration interaction")
        .expect("migration result");
    let app = routes::build_router(config, pool);

    for path in ["/api/channels", "/api/not-a-real-route", "/ws"] {
        let (status, _body) = request_json(&app, "GET", path).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{path} must remain an API 404"
        );
    }
}
