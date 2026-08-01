//! HTTP contract tests for private in-app notifications and event recipients.

use std::net::SocketAddr;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
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

/// Captured response status and parsed JSON body.
struct TestResponse {
    /// HTTP status returned by the application.
    status: StatusCode,
    /// Parsed JSON body or null for an empty response.
    body: Value,
}

/// Isolated event application with active and disabled account fixtures.
struct TestContext {
    /// Application router under test.
    app: Router,
    /// SQLite pool used for notification assertions.
    pool: Pool,
    /// Temporary runtime root retained for the test lifetime.
    _temp: TempDir,
    /// Administrator browser session.
    administrator: BrowserSession,
    /// Technician browser session.
    technician: BrowserSession,
    /// Ticket-owning requester browser session.
    requester: BrowserSession,
    /// Unrelated requester browser session.
    stranger: BrowserSession,
    /// Disabled technician identifier used to prove recipient filtering.
    disabled_technician_id: Uuid,
    /// Active category used for ticket submission.
    category_id: Uuid,
}

/// Builds one migrated application with all notification actor roles.
async fn test_app() -> TestContext {
    let temp = tempfile::tempdir().expect("temporary directory");
    let config = Config::for_test(temp.path().join("desk.db"), temp.path().join("uploads"));
    let pool = db::create_pool(&config.database_path);
    let connection = pool.get().await.expect("database connection");
    let (
        administrator_id,
        technician_id,
        requester_id,
        stranger_id,
        disabled_technician_id,
        category_id,
    ) = connection
        .interact(|connection| {
            db::migrations::run_migrations(connection)?;
            let administrator_id = create_account(connection, "desk.admin", Role::Administrator)?;
            let technician_id = create_account(connection, "desk.tech", Role::Technician)?;
            let requester_id = create_account(connection, "riley", Role::Requester)?;
            let stranger_id = create_account(connection, "sam", Role::Requester)?;
            let disabled_technician_id =
                create_account(connection, "retired.tech", Role::Technician)?;
            connection.execute(
                "UPDATE users SET is_active = 0 WHERE id = ?1",
                [disabled_technician_id.to_string()],
            )?;
            let category_id = Uuid::new_v4();
            connection.execute(
                "INSERT INTO categories (
                     id, name, description, is_active, sort_order, created_at, updated_at
                 ) VALUES (?1, 'Classroom technology', NULL, 1, 0, ?2, ?2)",
                rusqlite::params![category_id.to_string(), "2026-01-01T00:00:00.000Z"],
            )?;
            Ok::<_, local_it_desk_server::error::AppError>((
                administrator_id,
                technician_id,
                requester_id,
                stranger_id,
                disabled_technician_id,
                category_id,
            ))
        })
        .await
        .expect("fixture interaction")
        .expect("fixture result");
    let administrator = issue_session(&pool, administrator_id).await;
    let technician = issue_session(&pool, technician_id).await;
    let requester = issue_session(&pool, requester_id).await;
    let stranger = issue_session(&pool, stranger_id).await;
    TestContext {
        app: routes::build_router(config, pool.clone()),
        pool,
        _temp: temp,
        administrator,
        technician,
        requester,
        stranger,
        disabled_technician_id,
        category_id,
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
                    "192.0.2.91:42000"
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

/// Lists the current account's newest-first private notifications.
async fn notifications(context: &TestContext, session: &BrowserSession) -> Vec<Value> {
    let response = send(
        &context.app,
        "GET",
        "/api/notifications",
        json!({}),
        session,
    )
    .await;
    assert_eq!(response.status, StatusCode::OK);
    response.body.as_array().expect("notification list").clone()
}

/// Returns the current account's unread notification count.
async fn unread_count(context: &TestContext, session: &BrowserSession) -> u64 {
    let response = send(
        &context.app,
        "GET",
        "/api/notifications/unread-count",
        json!({}),
        session,
    )
    .await;
    assert_eq!(response.status, StatusCode::OK);
    response.body["count"].as_u64().expect("unread count")
}

/// Submits one ticket and returns its API record.
async fn create_ticket(context: &TestContext, session: &BrowserSession) -> Value {
    let response = send(
        &context.app,
        "POST",
        "/api/tickets",
        json!({
            "title":"Projector image is distorted",
            "description":"The ceiling projector shows a doubled image during class.",
            "category_id":context.category_id,
            "priority":"normal"
        }),
        session,
    )
    .await;
    assert_eq!(response.status, StatusCode::CREATED);
    response.body
}

/// Confirms ticket events notify active participants without leaking internal notes.
#[tokio::test]
async fn ticket_events_target_active_participants_and_exclude_internal_notes() {
    let context = test_app().await;
    let ticket = create_ticket(&context, &context.requester).await;
    let ticket_id = ticket["id"].as_str().expect("ticket id");

    assert_eq!(
        notifications(&context, &context.requester).await[0]["kind"],
        "ticket_created"
    );
    assert_eq!(
        notifications(&context, &context.administrator).await[0]["kind"],
        "new_ticket"
    );
    assert_eq!(
        notifications(&context, &context.technician).await[0]["kind"],
        "new_ticket"
    );
    let disabled_count = db::interact(&context.pool, {
        let disabled_id = context.disabled_technician_id;
        move |connection| {
            Ok(connection.query_row(
                "SELECT COUNT(*) FROM notifications WHERE user_id = ?1",
                [disabled_id.to_string()],
                |row| row.get::<_, u64>(0),
            )?)
        }
    })
    .await
    .expect("disabled notification count");
    assert_eq!(disabled_count, 0);

    let assigned = send(
        &context.app,
        "PATCH",
        &format!("/api/tickets/{ticket_id}"),
        json!({"status":"open", "assignee_id":context.technician.user_id}),
        &context.administrator,
    )
    .await;
    assert_eq!(assigned.status, StatusCode::OK);
    assert_eq!(
        notifications(&context, &context.requester).await[0]["kind"],
        "ticket_status_changed"
    );

    let requester_reply = send(
        &context.app,
        "POST",
        &format!("/api/tickets/{ticket_id}/comments"),
        json!({"body":"It affects both HDMI inputs.", "visibility":"public"}),
        &context.requester,
    )
    .await;
    assert_eq!(requester_reply.status, StatusCode::CREATED);
    assert_eq!(
        notifications(&context, &context.technician).await[0]["kind"],
        "ticket_comment"
    );
    assert_eq!(
        notifications(&context, &context.administrator)
            .await
            .iter()
            .filter(|notice| notice["kind"] == "ticket_comment")
            .count(),
        0
    );

    let technician_reply = send(
        &context.app,
        "POST",
        &format!("/api/tickets/{ticket_id}/comments"),
        json!({"body":"I will check the splitter after lunch.", "visibility":"public"}),
        &context.technician,
    )
    .await;
    assert_eq!(technician_reply.status, StatusCode::CREATED);
    assert_eq!(
        notifications(&context, &context.requester).await[0]["kind"],
        "ticket_comment"
    );

    let counts_before_internal = (
        unread_count(&context, &context.requester).await,
        unread_count(&context, &context.technician).await,
        unread_count(&context, &context.administrator).await,
    );
    let internal = send(
        &context.app,
        "POST",
        &format!("/api/tickets/{ticket_id}/comments"),
        json!({"body":"Replacement lamp is in storage.", "visibility":"internal"}),
        &context.administrator,
    )
    .await;
    assert_eq!(internal.status, StatusCode::CREATED);
    assert_eq!(
        counts_before_internal,
        (
            unread_count(&context, &context.requester).await,
            unread_count(&context, &context.technician).await,
            unread_count(&context, &context.administrator).await,
        )
    );

    let resolved = send(
        &context.app,
        "PATCH",
        &format!("/api/tickets/{ticket_id}"),
        json!({"status":"resolved"}),
        &context.administrator,
    )
    .await;
    assert_eq!(resolved.status, StatusCode::OK);
    assert_eq!(
        notifications(&context, &context.requester).await[0]["kind"],
        "ticket_resolved"
    );

    let reopened = send(
        &context.app,
        "PATCH",
        &format!("/api/tickets/{ticket_id}"),
        json!({"status":"open"}),
        &context.requester,
    )
    .await;
    assert_eq!(reopened.status, StatusCode::OK);
    assert_eq!(
        notifications(&context, &context.technician).await[0]["kind"],
        "ticket_reopened"
    );
    for notice in notifications(&context, &context.requester).await {
        assert!(
            !notice["body"]
                .as_str()
                .expect("notification body")
                .contains("Projector image is distorted")
        );
        assert_eq!(notice["target_path"], format!("/tickets/{ticket_id}"));
    }
}

/// Confirms bulletin publication targets active accounts except the author.
#[tokio::test]
async fn announcement_publish_notifies_active_non_author_accounts_once() {
    let context = test_app().await;
    let draft = send(
        &context.app,
        "POST",
        "/api/admin/announcements",
        json!({"title":"Building maintenance", "body":"Power testing begins at 5 PM."}),
        &context.administrator,
    )
    .await;
    assert_eq!(draft.status, StatusCode::CREATED);
    let announcement_id = draft.body["id"].as_str().expect("announcement id");
    let published = send(
        &context.app,
        "POST",
        &format!("/api/admin/announcements/{announcement_id}/publish"),
        json!({}),
        &context.administrator,
    )
    .await;
    assert_eq!(published.status, StatusCode::OK);

    for session in [&context.technician, &context.requester, &context.stranger] {
        let notices = notifications(&context, session).await;
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0]["kind"], "announcement_published");
        assert_eq!(
            notices[0]["target_path"],
            format!("/announcements/{announcement_id}")
        );
    }
    assert!(
        notifications(&context, &context.administrator)
            .await
            .is_empty()
    );
    let disabled_count = db::interact(&context.pool, {
        let disabled_id = context.disabled_technician_id;
        move |connection| {
            Ok(connection.query_row(
                "SELECT COUNT(*) FROM notifications WHERE user_id = ?1",
                [disabled_id.to_string()],
                |row| row.get::<_, u64>(0),
            )?)
        }
    })
    .await
    .expect("disabled notification count");
    assert_eq!(disabled_count, 0);
}

/// Confirms notification ownership, unread counts, idempotent read, and read-all behavior.
#[tokio::test]
async fn notification_read_state_is_private_and_idempotent() {
    let context = test_app().await;
    create_ticket(&context, &context.requester).await;
    let notices = notifications(&context, &context.requester).await;
    assert_eq!(notices.len(), 1);
    assert_eq!(unread_count(&context, &context.requester).await, 1);
    let notification_id = notices[0]["id"].as_str().expect("notification id");

    let stranger_read = send(
        &context.app,
        "POST",
        &format!("/api/notifications/{notification_id}/read"),
        json!({}),
        &context.stranger,
    )
    .await;
    assert_eq!(stranger_read.status, StatusCode::NOT_FOUND);
    let marked = send(
        &context.app,
        "POST",
        &format!("/api/notifications/{notification_id}/read"),
        json!({}),
        &context.requester,
    )
    .await;
    assert_eq!(marked.status, StatusCode::NO_CONTENT);
    assert_eq!(unread_count(&context, &context.requester).await, 0);
    let repeated = send(
        &context.app,
        "POST",
        &format!("/api/notifications/{notification_id}/read"),
        json!({}),
        &context.requester,
    )
    .await;
    assert_eq!(repeated.status, StatusCode::NO_CONTENT);

    create_ticket(&context, &context.requester).await;
    assert_eq!(unread_count(&context, &context.requester).await, 1);
    let read_all = send(
        &context.app,
        "POST",
        "/api/notifications/read-all",
        json!({}),
        &context.requester,
    )
    .await;
    assert_eq!(read_all.status, StatusCode::NO_CONTENT);
    assert_eq!(unread_count(&context, &context.requester).await, 0);
    assert_eq!(unread_count(&context, &context.stranger).await, 0);
}
