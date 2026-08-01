//! HTTP contract for requester-owned tickets and the shared support queue.

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
    /** Public user identifier used in assignment assertions. */
    user_id: Uuid,
    /** Opaque cookie request pair. */
    cookie: String,
    /** Per-session CSRF request proof. */
    csrf: String,
}

/// Captured status and JSON response from one route invocation.
struct TestResponse {
    /** HTTP response status. */
    status: StatusCode,
    /** Parsed JSON body or null for empty responses. */
    body: Value,
}

/// Builds one migrated application with three active named accounts.
async fn test_app() -> (
    Router,
    Pool,
    TempDir,
    BrowserSession,
    BrowserSession,
    BrowserSession,
    Uuid,
) {
    let temp = tempfile::tempdir().expect("temporary directory");
    let config = Config::for_test(temp.path().join("desk.db"), temp.path().join("uploads"));
    let pool = db::create_pool(&config.database_path);
    let connection = pool.get().await.expect("database connection");
    let (administrator, requester, stranger, category_id) = connection
        .interact(|connection| {
            db::migrations::run_migrations(connection)?;
            let administrator = create_account(connection, "desk.admin", Role::Administrator)?;
            let requester = create_account(connection, "casey", Role::Requester)?;
            let stranger = create_account(connection, "morgan", Role::Requester)?;
            let category_id = Uuid::new_v4();
            let now = "2026-01-01T00:00:00.000Z";
            connection.execute(
                "INSERT INTO categories (id, name, description, is_active, sort_order, created_at, updated_at)
                 VALUES (?1, 'Classroom technology', NULL, 1, 0, ?2, ?2)",
                rusqlite::params![category_id.to_string(), now],
            )?;
            Ok::<_, local_it_desk_server::error::AppError>((administrator, requester, stranger, category_id))
        })
        .await
        .expect("fixture interaction")
        .expect("fixture result");
    let administrator = issue_session(&pool, administrator).await;
    let requester = issue_session(&pool, requester).await;
    let stranger = issue_session(&pool, stranger).await;
    (
        routes::build_router(config, pool.clone()),
        pool,
        temp,
        administrator,
        requester,
        stranger,
        category_id,
    )
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

/// Sends one JSON request with a named account session.
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
                    "192.0.2.60:42000"
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
        .expect("body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("JSON")
    };
    TestResponse { status, body }
}

/// Creates one ticket through the public requester contract.
async fn create_ticket(
    app: &Router,
    requester: &BrowserSession,
    category_id: Uuid,
    title: &str,
) -> TestResponse {
    send(
        app,
        "POST",
        "/api/tickets",
        json!({
            "title": title,
            "description": "The projector does not display the classroom computer.",
            "category_id": category_id,
            "priority": "normal"
        }),
        requester,
    )
    .await
}

/// Confirms ticket ownership is hidden while staff can manage the shared queue.
#[tokio::test]
async fn requester_ownership_and_staff_queue_are_enforced() {
    let (app, _pool, _temp, administrator, requester, stranger, category_id) = test_app().await;
    let created = create_ticket(&app, &requester, category_id, "Projector is offline").await;
    assert_eq!(created.status, StatusCode::CREATED);
    assert_eq!(created.body["requester_id"], requester.user_id.to_string());
    assert_eq!(created.body["number"], 1);
    let ticket_id = created.body["id"].as_str().expect("ticket id").to_owned();

    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/tickets/{ticket_id}"),
            json!({}),
            &requester
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/tickets/{ticket_id}"),
            json!({}),
            &stranger
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/tickets/{ticket_id}"),
            json!({}),
            &administrator
        )
        .await
        .status,
        StatusCode::OK
    );

    let requester_list = send(&app, "GET", "/api/tickets", json!({}), &requester).await;
    let stranger_list = send(&app, "GET", "/api/tickets", json!({}), &stranger).await;
    assert_eq!(
        requester_list.body["items"]
            .as_array()
            .expect("items")
            .len(),
        1
    );
    assert_eq!(
        stranger_list.body["items"].as_array().expect("items").len(),
        0
    );
}

/// Confirms workflow changes, comments, internal notes, and closed immutability.
#[tokio::test]
async fn workflow_and_comment_visibility_follow_role_policy() {
    let (app, pool, _temp, administrator, requester, _stranger, category_id) = test_app().await;
    let created = create_ticket(
        &app,
        &requester,
        category_id,
        "Wireless access is unavailable",
    )
    .await;
    let ticket_id = created.body["id"].as_str().expect("ticket id").to_owned();

    let assigned = send(
        &app,
        "PATCH",
        &format!("/api/tickets/{ticket_id}"),
        json!({"status":"open", "priority":"urgent", "assignee_id": administrator.user_id}),
        &administrator,
    )
    .await;
    assert_eq!(assigned.status, StatusCode::OK);
    assert_eq!(assigned.body["status"], "open");
    assert_eq!(assigned.body["priority"], "urgent");
    let assigned_version = assigned.body["updated_at"]
        .as_str()
        .expect("updated timestamp")
        .to_owned();
    let unassigned = send(
        &app,
        "PATCH",
        &format!("/api/tickets/{ticket_id}"),
        json!({"assignee_id": null}),
        &administrator,
    )
    .await;
    assert_eq!(unassigned.status, StatusCode::OK);
    assert!(unassigned.body["assignee_id"].is_null());
    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            json!({"priority":"low", "expected_updated_at":assigned_version}),
            &administrator,
        )
        .await
        .status,
        StatusCode::CONFLICT
    );

    let public = send(
        &app,
        "POST",
        &format!("/api/tickets/{ticket_id}/comments"),
        json!({"body":"The issue affects two rooms.", "visibility":"public"}),
        &requester,
    )
    .await;
    assert_eq!(public.status, StatusCode::CREATED);
    let internal = send(
        &app,
        "POST",
        &format!("/api/tickets/{ticket_id}/comments"),
        json!({"body":"Check the access point controller.", "visibility":"internal"}),
        &administrator,
    )
    .await;
    assert_eq!(internal.status, StatusCode::CREATED);
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            json!({"body":"Hidden", "visibility":"internal"}),
            &requester
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );

    let requester_comments = send(
        &app,
        "GET",
        &format!("/api/tickets/{ticket_id}/comments"),
        json!({}),
        &requester,
    )
    .await;
    let staff_comments = send(
        &app,
        "GET",
        &format!("/api/tickets/{ticket_id}/comments"),
        json!({}),
        &administrator,
    )
    .await;
    assert_eq!(
        requester_comments.body.as_array().expect("comments").len(),
        1
    );
    assert_eq!(staff_comments.body.as_array().expect("comments").len(), 2);

    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            json!({"status":"waiting_on_requester"}),
            &administrator
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            json!({"status":"open"}),
            &administrator
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            json!({"status":"resolved"}),
            &administrator
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            json!({"status":"waiting_on_requester"}),
            &administrator
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            json!({"status":"open"}),
            &requester
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            json!({"status":"closed"}),
            &administrator
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            json!({"body":"Too late", "visibility":"public"}),
            &requester
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            json!({"priority":"low"}),
            &administrator
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            json!({"status":"open"}),
            &administrator
        )
        .await
        .status,
        StatusCode::OK
    );

    let lifecycle_audits = db::interact(&pool, move |connection| {
        let count = connection.query_row(
            "SELECT COUNT(*) FROM audit_log WHERE target_id = ?1 AND action LIKE 'ticket.%'",
            [&ticket_id],
            |row| row.get::<_, u64>(0),
        )?;
        let transition_count = connection.query_row(
            "SELECT COUNT(*) FROM audit_log
             WHERE target_id = ?1
               AND action = 'ticket.status_changed'
               AND summary LIKE 'Changed ticket status from % to %'",
            [&ticket_id],
            |row| row.get::<_, u64>(0),
        )?;
        Ok::<_, local_it_desk_server::error::AppError>((count, transition_count))
    })
    .await
    .expect("audit count");
    assert!(lifecycle_audits.0 >= 9);
    assert!(lifecycle_audits.1 >= 6);
}

/// Confirms filters, search, category validation, and stable cursor pagination.
#[tokio::test]
async fn filters_and_cursor_pagination_are_bounded_and_stable() {
    let (app, _pool, _temp, administrator, requester, _stranger, category_id) = test_app().await;
    assert_eq!(
        create_ticket(&app, &requester, category_id, "Alpha projector")
            .await
            .status,
        StatusCode::CREATED
    );
    assert_eq!(
        create_ticket(&app, &requester, category_id, "Beta laptop")
            .await
            .status,
        StatusCode::CREATED
    );

    let first = send(
        &app,
        "GET",
        "/api/tickets?page_size=1",
        json!({}),
        &administrator,
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(first.body["items"].as_array().expect("items").len(), 1);
    let cursor = first.body["next_cursor"].as_str().expect("next cursor");
    let second = send(
        &app,
        "GET",
        &format!("/api/tickets?page_size=1&cursor={cursor}"),
        json!({}),
        &administrator,
    )
    .await;
    assert_eq!(second.status, StatusCode::OK);
    assert_ne!(first.body["items"][0]["id"], second.body["items"][0]["id"]);

    let filtered = send(
        &app,
        "GET",
        &format!("/api/tickets?search=alpha&category_id={category_id}&status=new&priority=normal"),
        json!({}),
        &administrator,
    )
    .await;
    assert_eq!(filtered.body["items"].as_array().expect("items").len(), 1);
    assert_eq!(
        send(
            &app,
            "GET",
            "/api/tickets?page_size=1000",
            json!({}),
            &administrator
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(send(&app, "POST", "/api/tickets", json!({"title":"Bad category", "description":"A sufficiently detailed issue report.", "category_id":Uuid::new_v4(), "priority":"normal"}), &requester).await.status, StatusCode::BAD_REQUEST);
}
