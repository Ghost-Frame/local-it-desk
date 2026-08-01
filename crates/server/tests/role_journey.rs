//! Cohesive HTTP journey across first setup, requester work, administration, and audit history.

use std::net::SocketAddr;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use base64::prelude::*;
use deadpool_sqlite::Pool;
use local_it_desk_server::config::Config;
use local_it_desk_server::db;
use local_it_desk_server::routes;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

/// Browser authentication material returned by setup, login, or password rotation.
struct BrowserSession {
    /// Stable current-account identifier.
    user_id: Uuid,
    /// Opaque session cookie request pair.
    cookie: String,
    /// In-memory CSRF proof paired with the session.
    csrf: String,
}

/// Captured status, JSON body, and optional rotated cookie from one route call.
struct TestResponse {
    /// HTTP status returned by the application.
    status: StatusCode,
    /// Parsed JSON body or null for an empty response.
    body: Value,
    /// Optional authentication cookie returned by the route.
    set_cookie: Option<String>,
}

/// Isolated migrated application retained with its database and runtime directories.
struct TestContext {
    /// Complete application router.
    app: Router,
    /// SQLite pool used for audit-order assertions.
    pool: Pool,
    /// Temporary runtime root retained for the full journey.
    _temp: TempDir,
}

/// Builds one empty application through the same migration and directory preparation path as startup.
async fn test_app() -> TestContext {
    let temp = tempfile::tempdir().expect("temporary runtime root");
    let config = Config::for_test(temp.path().join("desk.db"), temp.path().join("uploads"));
    config
        .prepare_runtime_directories()
        .expect("runtime directories");
    let pool = db::create_pool(&config.database_path);
    let connection = pool.get().await.expect("database connection");
    connection
        .interact(|connection| db::migrations::run_migrations(connection))
        .await
        .expect("migration interaction")
        .expect("migration result");
    TestContext {
        app: routes::build_router(config, pool.clone()),
        pool,
        _temp: temp,
    }
}

/// Sends one same-origin JSON request with optional browser session material.
async fn send_json(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    session: Option<&BrowserSession>,
) -> TestResponse {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("origin", "http://localhost:3000")
        .extension(ConnectInfo(
            "192.0.2.80:43000"
                .parse::<SocketAddr>()
                .expect("static peer address"),
        ));
    if let Some(session) = session {
        request = request
            .header("cookie", &session.cookie)
            .header("x-csrf-token", &session.csrf);
    }
    let response = app
        .clone()
        .oneshot(
            request
                .body(Body::from(body.to_string()))
                .expect("request body"),
        )
        .await
        .expect("router response");
    capture_json_response(response).await
}

/// Captures one bounded Axum response as the shared JSON test representation.
async fn capture_json_response(response: axum::response::Response) -> TestResponse {
    let status = response.status();
    let set_cookie = response
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("bounded response body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("JSON response")
    };
    TestResponse {
        status,
        body,
        set_cookie,
    }
}

/// Extracts one Cookie request pair from a Set-Cookie response value.
fn cookie_pair(set_cookie: &str) -> String {
    set_cookie
        .split(';')
        .next()
        .expect("cookie pair")
        .to_string()
}

/// Converts a successful authentication response into reusable browser state.
fn session_from_response(response: &TestResponse) -> BrowserSession {
    BrowserSession {
        user_id: Uuid::parse_str(
            response.body["user"]["id"]
                .as_str()
                .expect("authenticated user id"),
        )
        .expect("user UUID"),
        cookie: cookie_pair(response.set_cookie.as_deref().expect("session cookie")),
        csrf: response.body["csrf_token"]
            .as_str()
            .expect("CSRF token")
            .to_string(),
    }
}

/// Runs first-administrator setup through the public bootstrap contract.
async fn setup_administrator(app: &Router) -> BrowserSession {
    let response = send_json(
        app,
        "POST",
        "/api/setup",
        json!({
            "username": "teacher.admin",
            "display_name": "Technology Teacher",
            "password": "correct horse battery staple"
        }),
        None,
    )
    .await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["user"]["role"], "administrator");
    session_from_response(&response)
}

/// Logs in one named account and returns its authentication response.
async fn login(app: &Router, username: &str, password: &str) -> TestResponse {
    send_json(
        app,
        "POST",
        "/api/auth/login",
        json!({ "username": username, "password": password }),
        None,
    )
    .await
}

/// Returns deterministic valid one-pixel PNG bytes for the journey attachment.
fn png_bytes() -> Vec<u8> {
    BASE64_STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
        .expect("static PNG")
}

/// Encodes one ticket attachment as a deterministic multipart request body.
fn multipart_body(ticket_id: Uuid, bytes: &[u8]) -> (String, Vec<u8>) {
    let boundary = "local-it-desk-role-journey";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"parent_kind\"\r\n\r\nticket\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"parent_id\"\r\n\r\n{ticket_id}\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"projector.png\"\r\n\
         Content-Type: image/png\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary.to_string(), body)
}

/// Uploads one requester-owned ticket attachment through the multipart API.
async fn upload_attachment(
    app: &Router,
    ticket_id: Uuid,
    session: &BrowserSession,
) -> TestResponse {
    let (boundary, body) = multipart_body(ticket_id, &png_bytes());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/attachments")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .header("origin", "http://localhost:3000")
                .header("cookie", &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .extension(ConnectInfo(
                    "192.0.2.80:43000"
                        .parse::<SocketAddr>()
                        .expect("static peer address"),
                ))
                .body(Body::from(body))
                .expect("multipart request"),
        )
        .await
        .expect("attachment response");
    capture_json_response(response).await
}

/// Asserts that required audit actions appear in exact chronological subsequence order.
fn assert_action_subsequence(actions: &[String], expected: &[&str]) {
    let mut cursor = 0;
    for required in expected {
        let offset = actions[cursor..]
            .iter()
            .position(|action| action == required)
            .unwrap_or_else(|| panic!("missing ordered audit action {required}; saw {actions:?}"));
        cursor += offset + 1;
    }
}

/// Exercises the complete named-staff help-desk journey through public HTTP contracts.
#[tokio::test]
async fn named_staff_complete_ticket_and_announcement_journey() {
    let context = test_app().await;
    let setup_status = send_json(&context.app, "GET", "/api/setup/status", json!({}), None).await;
    assert_eq!(setup_status.body, json!({ "setup_required": true }));
    let administrator = setup_administrator(&context.app).await;

    let config = send_json(&context.app, "GET", "/api/config", json!({}), None).await;
    assert_eq!(config.status, StatusCode::OK);
    let category_id = Uuid::parse_str(
        config.body["default_category_id"]
            .as_str()
            .expect("default category"),
    )
    .expect("category UUID");

    let created_account = send_json(
        &context.app,
        "POST",
        "/api/admin/users",
        json!({
            "username": "math.staff",
            "display_name": "Math Department Staff",
            "role": "requester",
            "email": null
        }),
        Some(&administrator),
    )
    .await;
    assert_eq!(created_account.status, StatusCode::CREATED);
    assert_eq!(created_account.body["user"]["must_change_password"], true);
    let temporary_password = created_account.body["temporary_password"]
        .as_str()
        .expect("one-time password")
        .to_string();
    let requester_login = login(&context.app, "math.staff", &temporary_password).await;
    assert_eq!(requester_login.status, StatusCode::OK);
    let temporary_session = session_from_response(&requester_login);

    let blocked_ticket = send_json(
        &context.app,
        "POST",
        "/api/tickets",
        json!({
            "title": "Blocked until password change",
            "description": "This request must not be accepted yet.",
            "category_id": category_id,
            "priority": "normal"
        }),
        Some(&temporary_session),
    )
    .await;
    assert_eq!(blocked_ticket.status, StatusCode::FORBIDDEN);

    let changed_password = send_json(
        &context.app,
        "POST",
        "/api/auth/password",
        json!({
            "current_password": temporary_password,
            "new_password": "requester permanent passphrase 2026"
        }),
        Some(&temporary_session),
    )
    .await;
    assert_eq!(changed_password.status, StatusCode::OK);
    assert_eq!(changed_password.body["user"]["must_change_password"], false);
    let requester = session_from_response(&changed_password);

    let created_ticket = send_json(
        &context.app,
        "POST",
        "/api/tickets",
        json!({
            "title": "Classroom projector has no image",
            "description": "The teacher computer is on but the ceiling projector reports no signal.",
            "category_id": category_id,
            "priority": "high"
        }),
        Some(&requester),
    )
    .await;
    assert_eq!(created_ticket.status, StatusCode::CREATED);
    assert_eq!(
        created_ticket.body["requester_id"],
        requester.user_id.to_string()
    );
    let ticket_id = Uuid::parse_str(
        created_ticket.body["id"]
            .as_str()
            .expect("created ticket id"),
    )
    .expect("ticket UUID");

    let assigned = send_json(
        &context.app,
        "PATCH",
        &format!("/api/tickets/{ticket_id}"),
        json!({
            "assignee_id": administrator.user_id,
            "status": "open",
            "priority": "urgent"
        }),
        Some(&administrator),
    )
    .await;
    assert_eq!(assigned.status, StatusCode::OK);
    assert_eq!(
        assigned.body["assignee_id"],
        administrator.user_id.to_string()
    );
    assert_eq!(assigned.body["status"], "open");

    let attachment = upload_attachment(&context.app, ticket_id, &requester).await;
    assert_eq!(attachment.status, StatusCode::CREATED);
    assert_eq!(attachment.body["parent_kind"], "ticket");
    assert_eq!(attachment.body["original_name"], "projector.png");

    let public_comment = send_json(
        &context.app,
        "POST",
        &format!("/api/tickets/{ticket_id}/comments"),
        json!({
            "body": "I reseated the classroom HDMI cable and the issue remains.",
            "visibility": "public"
        }),
        Some(&requester),
    )
    .await;
    assert_eq!(public_comment.status, StatusCode::CREATED);
    let internal_note = send_json(
        &context.app,
        "POST",
        &format!("/api/tickets/{ticket_id}/comments"),
        json!({
            "body": "Bring the spare short-throw projector before visiting the room.",
            "visibility": "internal"
        }),
        Some(&administrator),
    )
    .await;
    assert_eq!(internal_note.status, StatusCode::CREATED);

    let requester_comments = send_json(
        &context.app,
        "GET",
        &format!("/api/tickets/{ticket_id}/comments"),
        json!({}),
        Some(&requester),
    )
    .await;
    assert_eq!(
        requester_comments
            .body
            .as_array()
            .expect("requester-visible comments")
            .len(),
        1
    );

    for status in ["waiting_on_requester", "open", "resolved"] {
        let transition = send_json(
            &context.app,
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            json!({ "status": status }),
            Some(&administrator),
        )
        .await;
        assert_eq!(transition.status, StatusCode::OK);
        assert_eq!(transition.body["status"], status);
    }

    let draft = send_json(
        &context.app,
        "POST",
        "/api/admin/announcements",
        json!({
            "title": "Projector service restored",
            "body": "The classroom projector service is **available** again.",
            "is_pinned": true
        }),
        Some(&administrator),
    )
    .await;
    assert_eq!(draft.status, StatusCode::CREATED);
    assert_eq!(draft.body["state"], "draft");
    let announcement_id = draft.body["id"]
        .as_str()
        .expect("announcement id")
        .to_string();
    let published = send_json(
        &context.app,
        "POST",
        &format!("/api/admin/announcements/{announcement_id}/publish"),
        json!({}),
        Some(&administrator),
    )
    .await;
    assert_eq!(published.status, StatusCode::OK);
    assert_eq!(published.body["state"], "published");

    let notices = send_json(
        &context.app,
        "GET",
        "/api/notifications",
        json!({}),
        Some(&requester),
    )
    .await;
    let kinds = notices
        .body
        .as_array()
        .expect("requester notifications")
        .iter()
        .filter_map(|notice| notice["kind"].as_str())
        .collect::<Vec<_>>();
    for required in [
        "ticket_created",
        "ticket_status_changed",
        "ticket_resolved",
        "announcement_published",
    ] {
        assert!(
            kinds.contains(&required),
            "missing notification kind {required}"
        );
    }
    assert!(
        notices
            .body
            .as_array()
            .expect("notices")
            .iter()
            .any(|notice| {
                notice["kind"] == "announcement_published"
                    && notice["target_path"] == format!("/announcements/{announcement_id}")
            })
    );

    let disabled = send_json(
        &context.app,
        "PATCH",
        &format!("/api/admin/users/{}", requester.user_id),
        json!({ "is_active": false }),
        Some(&administrator),
    )
    .await;
    assert_eq!(disabled.status, StatusCode::OK);
    assert_eq!(disabled.body["user"]["is_active"], false);
    assert_eq!(
        send_json(
            &context.app,
            "GET",
            "/api/auth/session",
            json!({}),
            Some(&requester),
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(
            &context.app,
            "math.staff",
            "requester permanent passphrase 2026"
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );

    let audit_page = send_json(
        &context.app,
        "GET",
        "/api/admin/audit-log?page=1&page_size=100",
        json!({}),
        Some(&administrator),
    )
    .await;
    assert_eq!(audit_page.status, StatusCode::OK);
    assert!(
        audit_page.body["total"]
            .as_u64()
            .is_some_and(|total| total >= 14)
    );

    let actions = db::interact(&context.pool, |connection| {
        let mut statement =
            connection.prepare("SELECT action FROM audit_log ORDER BY rowid ASC")?;
        Ok::<_, local_it_desk_server::error::AppError>(
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?,
        )
    })
    .await
    .expect("chronological audit actions");
    assert_action_subsequence(
        &actions,
        &[
            "setup.completed",
            "account.created",
            "account.password_changed",
            "ticket.created",
            "ticket.status_changed",
            "attachment.created",
            "ticket.comment_added",
            "ticket.internal_note_added",
            "ticket.status_changed",
            "ticket.status_changed",
            "ticket.status_changed",
            "announcement.created",
            "announcement.published",
            "account.updated",
        ],
    );
}
