//! HTTP contract tests for runtime settings, categories, and safe branding.

use std::collections::BTreeSet;
use std::net::SocketAddr;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use deadpool_sqlite::Pool;
use local_it_desk_server::auth::Role;
use local_it_desk_server::auth::middleware::SESSION_COOKIE_NAME;
use local_it_desk_server::auth::session;
use local_it_desk_server::config::Config;
use local_it_desk_server::db;
use local_it_desk_server::models::user::{self, NewUser};
use local_it_desk_server::routes;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

/// Browser authentication material for one named test account.
struct BrowserSession {
    /// Opaque session cookie request pair.
    cookie: String,
    /// Per-session CSRF request proof.
    csrf: String,
}

/// Captured response status, headers, and raw bytes.
struct TestResponse {
    /// HTTP response status.
    status: StatusCode,
    /// Complete response headers.
    headers: HeaderMap,
    /// Bounded response bytes.
    body: Vec<u8>,
}

/// Isolated settings application with persisted administrator and requester fixtures.
struct TestContext {
    /// Application router under test.
    app: Router,
    /// SQLite pool used for audit and persistence assertions.
    pool: Pool,
    /// Temporary runtime root retained for the test lifetime.
    _temp: TempDir,
    /// Administrator browser session.
    administrator: BrowserSession,
    /// Requester browser session.
    requester: BrowserSession,
    /// Initially active default category.
    default_category_id: Uuid,
    /// Initially disabled category.
    disabled_category_id: Uuid,
}

/// Builds one migrated application with typed settings and category fixtures.
async fn test_app() -> TestContext {
    let temp = tempfile::tempdir().expect("temporary directory");
    let mut config = Config::for_test(temp.path().join("desk.db"), temp.path().join("uploads"));
    config.max_upload_bytes = 4096;
    config.max_ticket_upload_bytes = 16384;
    config
        .prepare_runtime_directories()
        .expect("runtime directories");
    let pool = db::create_pool(&config.database_path);
    let connection = pool.get().await.expect("database connection");
    let (administrator_id, requester_id, default_category_id, disabled_category_id) = connection
        .interact(|connection| {
            db::migrations::run_migrations(connection)?;
            let administrator_id = create_account(connection, "desk.admin", Role::Administrator)?;
            let requester_id = create_account(connection, "avery", Role::Requester)?;
            let default_category_id = Uuid::new_v4();
            let disabled_category_id = Uuid::new_v4();
            let now = "2026-01-01T00:00:00.000Z";
            connection.execute(
                "INSERT INTO categories (
                     id, name, description, is_active, sort_order, created_at, updated_at
                 ) VALUES (?1, 'General', 'Everyday support', 1, 0, ?3, ?3),
                          (?2, 'Retired systems', NULL, 0, 1, ?3, ?3)",
                rusqlite::params![
                    default_category_id.to_string(),
                    disabled_category_id.to_string(),
                    now,
                ],
            )?;
            for (key, value) in [
                ("app_name", "Vocational IT Desk".to_string()),
                ("support_contact", "Room 214".to_string()),
                ("default_category_id", default_category_id.to_string()),
                ("default_priority", "normal".to_string()),
            ] {
                connection.execute(
                    "INSERT INTO settings (key, value, updated_by, updated_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![key, value, administrator_id.to_string(), now],
                )?;
            }
            Ok::<_, local_it_desk_server::error::AppError>((
                administrator_id,
                requester_id,
                default_category_id,
                disabled_category_id,
            ))
        })
        .await
        .expect("fixture interaction")
        .expect("fixture result");
    let administrator = issue_session(&pool, administrator_id).await;
    let requester = issue_session(&pool, requester_id).await;
    TestContext {
        app: routes::build_router(config, pool.clone()),
        pool,
        _temp: temp,
        administrator,
        requester,
        default_category_id,
        disabled_category_id,
    }
}

/// Creates one active account that can immediately use product routes.
fn create_account(
    connection: &rusqlite::Connection,
    username: &str,
    role: Role,
) -> local_it_desk_server::error::AppResult<Uuid> {
    Ok(user::create(
        connection,
        &NewUser {
            username,
            display_name: username,
            email: None,
            password: "fixture password value",
            role,
            must_change_password: false,
        },
    )?
    .id)
}

/// Issues one server-side browser session for a fixture account.
async fn issue_session(pool: &Pool, user_id: Uuid) -> BrowserSession {
    let issued = db::interact(pool, move |connection| {
        session::create(connection, user_id, 14)
    })
    .await
    .expect("session");
    BrowserSession {
        cookie: format!("{SESSION_COOKIE_NAME}={}", issued.token),
        csrf: issued.csrf_token,
    }
}

/// Sends one optional-session request and captures its bounded raw response.
async fn send(
    app: &Router,
    method: &str,
    path: &str,
    content_type: Option<&str>,
    body: Vec<u8>,
    session: Option<&BrowserSession>,
) -> TestResponse {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .extension(ConnectInfo(
            "192.0.2.72:42000"
                .parse::<SocketAddr>()
                .expect("static peer"),
        ));
    if let Some(content_type) = content_type {
        builder = builder.header("content-type", content_type);
    }
    if let Some(session) = session {
        builder = builder
            .header("cookie", &session.cookie)
            .header("x-csrf-token", &session.csrf);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body)).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("bounded body")
        .to_vec();
    TestResponse {
        status,
        headers,
        body,
    }
}

/// Sends a JSON request through the shared request helper.
async fn send_json(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    session: Option<&BrowserSession>,
) -> TestResponse {
    send(
        app,
        method,
        path,
        Some("application/json"),
        body.to_string().into_bytes(),
        session,
    )
    .await
}

/// Parses one successful JSON response body.
fn json_body(response: &TestResponse) -> Value {
    serde_json::from_slice(&response.body).expect("JSON response")
}

/// Returns a deterministic valid one-pixel PNG fixture.
fn png_bytes() -> Vec<u8> {
    STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
        .expect("static PNG")
}

/// Encodes one branding upload as a multipart form body.
fn logo_body(filename: &str, bytes: &[u8]) -> (String, Vec<u8>) {
    let boundary = "local-it-desk-branding-boundary";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary.to_string(), body)
}

/// Confirms the unauthenticated configuration response is explicit and browser-safe.
#[tokio::test]
async fn public_config_exposes_only_allowlisted_runtime_fields() {
    let context = test_app().await;
    let response = send_json(&context.app, "GET", "/api/config", json!({}), None).await;
    assert_eq!(response.status, StatusCode::OK);
    let body = json_body(&response);
    let keys = body
        .as_object()
        .expect("config object")
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        keys,
        BTreeSet::from([
            "app_name".to_string(),
            "categories".to_string(),
            "default_category_id".to_string(),
            "default_priority".to_string(),
            "logo_url".to_string(),
            "max_ticket_upload_bytes".to_string(),
            "max_upload_bytes".to_string(),
            "setup_required".to_string(),
            "support_contact".to_string(),
            "version".to_string(),
        ])
    );
    assert_eq!(body["app_name"], "Vocational IT Desk");
    assert_eq!(body["support_contact"], "Room 214");
    assert_eq!(
        body["default_category_id"],
        context.default_category_id.to_string()
    );
    assert_eq!(body["categories"].as_array().expect("categories").len(), 1);
    assert_eq!(body["categories"][0]["name"], "General");
    assert_eq!(body["setup_required"], false);
    assert_eq!(body["max_upload_bytes"], 4096);
    assert_eq!(body["max_ticket_upload_bytes"], 16384);
}

/// Confirms only administrators can inspect or mutate non-secret runtime settings.
#[tokio::test]
async fn settings_are_admin_only_validated_and_audited() {
    let context = test_app().await;
    let forbidden = send_json(
        &context.app,
        "GET",
        "/api/admin/settings",
        json!({}),
        Some(&context.requester),
    )
    .await;
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);

    let invalid_name = send_json(
        &context.app,
        "PATCH",
        "/api/admin/settings",
        json!({"app_name":" "}),
        Some(&context.administrator),
    )
    .await;
    assert_eq!(invalid_name.status, StatusCode::BAD_REQUEST);
    let invalid_contact = send_json(
        &context.app,
        "PATCH",
        "/api/admin/settings",
        json!({"support_contact":"x".repeat(201)}),
        Some(&context.administrator),
    )
    .await;
    assert_eq!(invalid_contact.status, StatusCode::BAD_REQUEST);

    let updated = send_json(
        &context.app,
        "PATCH",
        "/api/admin/settings",
        json!({
            "app_name":"Career Center Help Desk",
            "support_contact":"",
            "default_priority":"high"
        }),
        Some(&context.administrator),
    )
    .await;
    assert_eq!(updated.status, StatusCode::OK);
    let body = json_body(&updated);
    assert_eq!(body["app_name"], "Career Center Help Desk");
    assert!(body["support_contact"].is_null());
    assert_eq!(body["default_priority"], "high");

    let audit_count = db::interact(&context.pool, |connection| {
        Ok(connection.query_row(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'settings.updated'",
            [],
            |row| row.get::<_, u64>(0),
        )?)
    })
    .await
    .expect("audit count");
    assert_eq!(audit_count, 1);
}

/// Confirms category names are unique and the active default cannot be disabled.
#[tokio::test]
async fn category_lifecycle_protects_the_active_default() {
    let context = test_app().await;
    let duplicate = send_json(
        &context.app,
        "POST",
        "/api/admin/categories",
        json!({"name":"general", "description":null}),
        Some(&context.administrator),
    )
    .await;
    assert_eq!(duplicate.status, StatusCode::CONFLICT);

    let created = send_json(
        &context.app,
        "POST",
        "/api/admin/categories",
        json!({"name":"Network access", "description":"Wired and wireless", "sort_order":7}),
        Some(&context.administrator),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let created_id = json_body(&created)["id"]
        .as_str()
        .expect("created id")
        .to_string();

    let protected = send_json(
        &context.app,
        "PATCH",
        &format!("/api/admin/categories/{}", context.default_category_id),
        json!({"is_active":false}),
        Some(&context.administrator),
    )
    .await;
    assert_eq!(protected.status, StatusCode::CONFLICT);

    let disabled_default = send_json(
        &context.app,
        "POST",
        &format!(
            "/api/admin/categories/{}/default",
            context.disabled_category_id
        ),
        json!({}),
        Some(&context.administrator),
    )
    .await;
    assert_eq!(disabled_default.status, StatusCode::CONFLICT);

    let selected = send_json(
        &context.app,
        "POST",
        &format!("/api/admin/categories/{created_id}/default"),
        json!({}),
        Some(&context.administrator),
    )
    .await;
    assert_eq!(selected.status, StatusCode::OK);

    let retired = send_json(
        &context.app,
        "PATCH",
        &format!("/api/admin/categories/{}", context.default_category_id),
        json!({"name":"General requests", "sort_order":9, "is_active":false}),
        Some(&context.administrator),
    )
    .await;
    assert_eq!(retired.status, StatusCode::OK);
    assert_eq!(json_body(&retired)["is_active"], false);

    let listed = send_json(
        &context.app,
        "GET",
        "/api/admin/categories",
        json!({}),
        Some(&context.administrator),
    )
    .await;
    assert_eq!(listed.status, StatusCode::OK);
    assert_eq!(
        json_body(&listed).as_array().expect("category list").len(),
        3
    );
}

/// Confirms branding uses detected raster bytes and randomized stored names.
#[tokio::test]
async fn logo_upload_rejects_active_content_and_serves_safe_raster_bytes() {
    let context = test_app().await;
    let (svg_boundary, svg_body) = logo_body("school.svg", b"<svg><script>alert(1)</script></svg>");
    let rejected = send(
        &context.app,
        "POST",
        "/api/admin/settings/logo",
        Some(&format!("multipart/form-data; boundary={svg_boundary}")),
        svg_body,
        Some(&context.administrator),
    )
    .await;
    assert_eq!(rejected.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let png = png_bytes();
    let (png_boundary, png_body) = logo_body("school-logo.png", &png);
    let uploaded = send(
        &context.app,
        "POST",
        "/api/admin/settings/logo",
        Some(&format!("multipart/form-data; boundary={png_boundary}")),
        png_body,
        Some(&context.administrator),
    )
    .await;
    assert_eq!(uploaded.status, StatusCode::OK);
    assert_eq!(json_body(&uploaded)["logo_url"], "/api/branding/logo");

    let stored_name = db::interact(&context.pool, |connection| {
        Ok(connection.query_row(
            "SELECT value FROM settings WHERE key = 'logo_stored_name'",
            [],
            |row| row.get::<_, String>(0),
        )?)
    })
    .await
    .expect("stored logo name");
    assert_ne!(stored_name, "school-logo.png");
    assert!(stored_name.ends_with(".png"));

    let served = send(
        &context.app,
        "GET",
        "/api/branding/logo",
        None,
        Vec::new(),
        None,
    )
    .await;
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(served.body, png);
    assert_eq!(served.headers["content-type"], "image/png");
    assert_eq!(served.headers["x-content-type-options"], "nosniff");
}
