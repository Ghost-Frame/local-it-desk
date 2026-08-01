//! HTTP contract tests for administrator announcements and signed-in visibility.

use std::net::SocketAddr;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
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
    /// Stable fixture account identifier.
    user_id: Uuid,
    /// Opaque session cookie request pair.
    cookie: String,
    /// Per-session CSRF request proof.
    csrf: String,
}

/// Captured status and parsed JSON response body.
struct TestResponse {
    /// HTTP status returned by the application.
    status: StatusCode,
    /// Parsed JSON body or null for an empty response.
    body: Value,
}

/// Captured status and bytes for attachment integration checks.
struct BinaryResponse {
    /// HTTP status returned by the application.
    status: StatusCode,
    /// Complete bounded response bytes.
    body: Vec<u8>,
}

/// Isolated announcement application and named account fixtures.
struct TestContext {
    /// Application router under test.
    app: Router,
    /// SQLite pool used for audit assertions.
    pool: Pool,
    /// Temporary runtime root retained for the test lifetime.
    _temp: TempDir,
    /// Administrator browser session.
    administrator: BrowserSession,
    /// Ordinary staff requester browser session.
    requester: BrowserSession,
}

/// Builds one migrated application with administrator and requester accounts.
async fn test_app() -> TestContext {
    let temp = tempfile::tempdir().expect("temporary directory");
    let config = Config::for_test(temp.path().join("desk.db"), temp.path().join("uploads"));
    config
        .prepare_runtime_directories()
        .expect("runtime directories");
    let pool = db::create_pool(&config.database_path);
    let connection = pool.get().await.expect("database connection");
    let (administrator_id, requester_id) = connection
        .interact(|connection| {
            db::migrations::run_migrations(connection)?;
            Ok::<_, local_it_desk_server::error::AppError>((
                create_account(connection, "desk.admin", Role::Administrator)?,
                create_account(connection, "jordan", Role::Requester)?,
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
        user_id,
        cookie: format!("{SESSION_COOKIE_NAME}={}", issued.token),
        csrf: issued.csrf_token,
    }
}

/// Sends one authenticated JSON request through the application router.
async fn send(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    session: &BrowserSession,
) -> TestResponse {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .header("cookie", &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .extension(ConnectInfo(
                    "192.0.2.81:42000"
                        .parse::<SocketAddr>()
                        .expect("static peer"),
                ))
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 256 * 1024)
        .await
        .expect("bounded body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("JSON response")
    };
    TestResponse { status, body }
}

/// Sends one authenticated request with caller-provided content bytes.
async fn send_binary(
    app: &Router,
    method: &str,
    path: &str,
    content_type: Option<&str>,
    body: Vec<u8>,
    session: &BrowserSession,
) -> BinaryResponse {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("cookie", &session.cookie)
        .header("x-csrf-token", &session.csrf)
        .extension(ConnectInfo(
            "192.0.2.82:42000"
                .parse::<SocketAddr>()
                .expect("static peer"),
        ));
    if let Some(content_type) = content_type {
        builder = builder.header("content-type", content_type);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body)).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let body = to_bytes(response.into_body(), 256 * 1024)
        .await
        .expect("bounded binary body")
        .to_vec();
    BinaryResponse { status, body }
}

/// Returns a deterministic valid one-pixel PNG fixture.
fn png_bytes() -> Vec<u8> {
    STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
        .expect("static PNG")
}

/// Encodes one announcement attachment as a multipart form body.
fn attachment_body(announcement_id: &str, bytes: &[u8]) -> (String, Vec<u8>) {
    let boundary = "local-it-desk-announcement-boundary";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"parent_kind\"\r\n\r\nannouncement\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"parent_id\"\r\n\r\n{announcement_id}\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"notice.png\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary.to_string(), body)
}

/// Creates one draft announcement and returns its JSON record.
async fn create_draft(context: &TestContext, title: &str, body: &str, pinned: bool) -> Value {
    let response = send(
        &context.app,
        "POST",
        "/api/admin/announcements",
        json!({"title":title, "body":body, "is_pinned":pinned}),
        &context.administrator,
    )
    .await;
    assert_eq!(response.status, StatusCode::CREATED);
    response.body
}

/// Publishes one draft announcement through its dedicated lifecycle endpoint.
async fn publish(context: &TestContext, id: &str) -> Value {
    let response = send(
        &context.app,
        "POST",
        &format!("/api/admin/announcements/{id}/publish"),
        json!({}),
        &context.administrator,
    )
    .await;
    assert_eq!(response.status, StatusCode::OK);
    response.body
}

/// Confirms drafts stay private and publish, edit, and archive transitions are audited.
#[tokio::test]
async fn lifecycle_enforces_draft_isolation_and_archive_immutability() {
    let context = test_app().await;
    let forbidden = send(
        &context.app,
        "GET",
        "/api/admin/announcements",
        json!({}),
        &context.requester,
    )
    .await;
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);

    let draft = create_draft(
        &context,
        "Network maintenance",
        "The wireless network will restart after 4 PM.",
        false,
    )
    .await;
    assert_eq!(draft["state"], "draft");
    assert!(draft["published_at"].is_null());
    assert_eq!(
        draft["author_id"],
        context.administrator.user_id.to_string()
    );
    let id = draft["id"].as_str().expect("draft id");

    let hidden = send(
        &context.app,
        "GET",
        "/api/announcements",
        json!({}),
        &context.requester,
    )
    .await;
    assert_eq!(hidden.status, StatusCode::OK);
    assert_eq!(hidden.body, json!([]));

    let edited = send(
        &context.app,
        "PATCH",
        &format!("/api/admin/announcements/{id}"),
        json!({"title":"Wireless maintenance", "is_pinned":true}),
        &context.administrator,
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK);
    assert_eq!(edited.body["title"], "Wireless maintenance");
    assert_eq!(edited.body["is_pinned"], true);

    let published = publish(&context, id).await;
    assert_eq!(published["state"], "published");
    assert!(published["published_at"].is_string());
    let visible = send(
        &context.app,
        "GET",
        "/api/announcements",
        json!({}),
        &context.requester,
    )
    .await;
    assert_eq!(visible.body.as_array().expect("published list").len(), 1);

    let archived = send(
        &context.app,
        "POST",
        &format!("/api/admin/announcements/{id}/archive"),
        json!({}),
        &context.administrator,
    )
    .await;
    assert_eq!(archived.status, StatusCode::OK);
    assert_eq!(archived.body["state"], "archived");
    let immutable = send(
        &context.app,
        "PATCH",
        &format!("/api/admin/announcements/{id}"),
        json!({"title":"Changed after archive"}),
        &context.administrator,
    )
    .await;
    assert_eq!(immutable.status, StatusCode::CONFLICT);
    let republish = send(
        &context.app,
        "POST",
        &format!("/api/admin/announcements/{id}/publish"),
        json!({}),
        &context.administrator,
    )
    .await;
    assert_eq!(republish.status, StatusCode::CONFLICT);
    let hidden_after_archive = send(
        &context.app,
        "GET",
        "/api/announcements",
        json!({}),
        &context.requester,
    )
    .await;
    assert_eq!(hidden_after_archive.body, json!([]));

    let audit_count = db::interact(&context.pool, |connection| {
        Ok(connection.query_row(
            "SELECT COUNT(*) FROM audit_log WHERE target_type = 'announcement'",
            [],
            |row| row.get::<_, u64>(0),
        )?)
    })
    .await
    .expect("announcement audit count");
    assert_eq!(audit_count, 4);
}

/// Confirms pinned ordering, bounded content, and source-only Markdown transport.
#[tokio::test]
async fn published_feed_is_stable_bounded_and_never_returns_rendered_html() {
    let context = test_app().await;
    let unsafe_markdown = "<script>alert('no')</script>\n\n**Important:** save your work.";
    let ordinary = create_draft(&context, "Ordinary notice", "Routine information", false).await;
    let pinned = create_draft(&context, "Pinned notice", unsafe_markdown, true).await;
    publish(&context, ordinary["id"].as_str().expect("ordinary id")).await;
    publish(&context, pinned["id"].as_str().expect("pinned id")).await;

    let feed = send(
        &context.app,
        "GET",
        "/api/announcements",
        json!({}),
        &context.requester,
    )
    .await;
    assert_eq!(feed.status, StatusCode::OK);
    let items = feed.body.as_array().expect("published feed");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["title"], "Pinned notice");
    assert_eq!(items[0]["body"], unsafe_markdown);
    assert!(items[0].get("html").is_none());
    assert!(items[0].get("rendered_html").is_none());

    for invalid in [
        json!({"title":" ", "body":"valid"}),
        json!({"title":"x".repeat(161), "body":"valid"}),
        json!({"title":"Valid", "body":" "}),
        json!({"title":"Valid", "body":"x".repeat(10_001)}),
    ] {
        let response = send(
            &context.app,
            "POST",
            "/api/admin/announcements",
            invalid,
            &context.administrator,
        )
        .await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST);
    }

    let admin = send(
        &context.app,
        "GET",
        "/api/admin/announcements",
        json!({}),
        &context.administrator,
    )
    .await;
    assert_eq!(admin.status, StatusCode::OK);
    assert_eq!(admin.body.as_array().expect("administrator list").len(), 2);
}

/// Confirms announcement attachments inherit draft, publish, and archive visibility.
#[tokio::test]
async fn attachment_visibility_tracks_announcement_state() {
    let context = test_app().await;
    let draft = create_draft(&context, "Map update", "A new room map is attached.", false).await;
    let announcement_id = draft["id"].as_str().expect("draft id");
    let png = png_bytes();
    let (boundary, body) = attachment_body(announcement_id, &png);
    let upload = send_binary(
        &context.app,
        "POST",
        "/api/attachments",
        Some(&format!("multipart/form-data; boundary={boundary}")),
        body,
        &context.administrator,
    )
    .await;
    assert_eq!(upload.status, StatusCode::CREATED);
    let attachment: Value = serde_json::from_slice(&upload.body).expect("attachment JSON");
    let attachment_path = format!(
        "/api/attachments/{}",
        attachment["id"].as_str().expect("attachment id")
    );

    let hidden_draft = send_binary(
        &context.app,
        "GET",
        &attachment_path,
        None,
        Vec::new(),
        &context.requester,
    )
    .await;
    assert_eq!(hidden_draft.status, StatusCode::NOT_FOUND);

    publish(&context, announcement_id).await;
    let visible = send_binary(
        &context.app,
        "GET",
        &attachment_path,
        None,
        Vec::new(),
        &context.requester,
    )
    .await;
    assert_eq!(visible.status, StatusCode::OK);
    assert_eq!(visible.body, png);

    let archived = send(
        &context.app,
        "POST",
        &format!("/api/admin/announcements/{announcement_id}/archive"),
        json!({}),
        &context.administrator,
    )
    .await;
    assert_eq!(archived.status, StatusCode::OK);
    let hidden_archive = send_binary(
        &context.app,
        "GET",
        &attachment_path,
        None,
        Vec::new(),
        &context.requester,
    )
    .await;
    assert_eq!(hidden_archive.status, StatusCode::NOT_FOUND);
}
