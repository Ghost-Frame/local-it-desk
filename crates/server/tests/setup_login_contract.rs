//! HTTP contract tests for first-run setup and local authentication routes.

use std::net::SocketAddr;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use deadpool_sqlite::Pool;
use local_it_desk_server::auth::Role;
use local_it_desk_server::auth::middleware::SESSION_COOKIE_NAME;
use local_it_desk_server::config::Config;
use local_it_desk_server::db;
use local_it_desk_server::models::user::{self, NewUser};
use local_it_desk_server::routes;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

/// Captured HTTP response fields used throughout the route contract.
struct TestResponse {
    /// HTTP status returned by the application.
    status: StatusCode,
    /// Parsed JSON body or null for an empty response.
    body: Value,
    /// Optional Set-Cookie value returned by authentication routes.
    set_cookie: Option<String>,
}

/// Builds one migrated isolated application and exposes its pool for state assertions.
async fn test_app() -> (Router, Pool, TempDir) {
    let temp = tempfile::tempdir().expect("temporary directory");
    let config = Config::for_test(temp.path().join("desk.db"), temp.path().join("uploads"));
    let pool = db::create_pool(&config.database_path);
    let connection = pool.get().await.expect("database connection");
    connection
        .interact(|connection| db::migrations::run_migrations(connection))
        .await
        .expect("migration interaction")
        .expect("migration result");
    (routes::build_router(config, pool.clone()), pool, temp)
}

/// Sends one JSON request with optional browser authentication headers.
async fn send(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    origin: Option<&str>,
    cookie: Option<&str>,
    csrf: Option<&str>,
) -> TestResponse {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .extension(ConnectInfo(
            "192.0.2.25:43123"
                .parse::<SocketAddr>()
                .expect("static peer address"),
        ));
    if let Some(origin) = origin {
        request = request.header("origin", origin);
    }
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    if let Some(csrf) = csrf {
        request = request.header("x-csrf-token", csrf);
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
    let status = response.status();
    let set_cookie = response
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = to_bytes(response.into_body(), 128 * 1024)
        .await
        .expect("response body");
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

/// Returns the Cookie request pair from one Set-Cookie response value.
fn cookie_pair(set_cookie: &str) -> String {
    set_cookie
        .split(';')
        .next()
        .expect("cookie pair")
        .to_string()
}

/// Returns a valid setup request with a selected administrator username.
fn setup_body(username: &str) -> Value {
    json!({
        "username": username,
        "display_name": "Local Administrator",
        "password": "correct horse battery staple"
    })
}

/// Confirms setup provisions the exact built-in records and browser session atomically.
#[tokio::test]
async fn first_run_setup_provisions_defaults_and_session() {
    let (app, pool, _temp) = test_app().await;
    let before = send(
        &app,
        "GET",
        "/api/setup/status",
        json!({}),
        None,
        None,
        None,
    )
    .await;
    assert_eq!(before.status, StatusCode::OK);
    assert_eq!(before.body, json!({ "setup_required": true }));

    let setup = send(
        &app,
        "POST",
        "/api/setup",
        setup_body("Teacher.Admin"),
        Some("http://localhost:3000"),
        None,
        None,
    )
    .await;
    assert_eq!(setup.status, StatusCode::OK);
    assert_eq!(setup.body["user"]["username"], "teacher.admin");
    assert_eq!(setup.body["user"]["role"], "administrator");
    assert_eq!(setup.body["user"]["must_change_password"], false);
    assert!(
        setup.body["csrf_token"]
            .as_str()
            .is_some_and(|value| value.len() >= 32)
    );
    let set_cookie = setup.set_cookie.expect("session cookie");
    assert!(set_cookie.starts_with(&format!("{SESSION_COOKIE_NAME}=")));
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Strict"));

    let counts = db::interact(&pool, |connection| {
        Ok((
            connection.query_row("SELECT COUNT(*) FROM users", [], |row| row.get::<_, u64>(0))?,
            connection.query_row("SELECT COUNT(*) FROM sessions", [], |row| {
                row.get::<_, u64>(0)
            })?,
            connection.query_row(
                "SELECT COUNT(*) FROM categories WHERE name = 'General'",
                [],
                |row| row.get::<_, u64>(0),
            )?,
            connection.query_row("SELECT COUNT(*) FROM settings", [], |row| {
                row.get::<_, u64>(0)
            })?,
            connection.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'setup.completed'",
                [],
                |row| row.get::<_, u64>(0),
            )?,
        ))
    })
    .await
    .expect("setup counts");
    assert_eq!(counts, (1, 1, 1, 3, 1));

    let after = send(
        &app,
        "GET",
        "/api/setup/status",
        json!({}),
        None,
        None,
        None,
    )
    .await;
    assert_eq!(after.body, json!({ "setup_required": false }));
    let repeated = send(
        &app,
        "POST",
        "/api/setup",
        setup_body("second.admin"),
        Some("http://localhost:3000"),
        None,
        None,
    )
    .await;
    assert_eq!(repeated.status, StatusCode::CONFLICT);
}

/// Confirms two racing setup requests cannot create two first administrators.
#[tokio::test]
async fn concurrent_setup_creates_exactly_one_administrator() {
    let (app, pool, _temp) = test_app().await;
    let first = send(
        &app,
        "POST",
        "/api/setup",
        setup_body("first.admin"),
        Some("http://localhost:3000"),
        None,
        None,
    );
    let second = send(
        &app,
        "POST",
        "/api/setup",
        setup_body("second.admin"),
        Some("http://localhost:3000"),
        None,
        None,
    );
    let (first, second) = tokio::join!(first, second);
    let mut statuses = [first.status, second.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::CONFLICT]);

    let count = db::interact(&pool, |connection| {
        connection
            .query_row("SELECT COUNT(*) FROM users", [], |row| row.get::<_, u64>(0))
            .map_err(Into::into)
    })
    .await
    .expect("user count");
    assert_eq!(count, 1);
}

/// Confirms missing, wrong, disabled, and forced-change login behavior is non-enumerating.
#[tokio::test]
async fn login_is_generic_and_preserves_forced_change_state() {
    let (app, pool, _temp) = test_app().await;
    send(
        &app,
        "POST",
        "/api/setup",
        setup_body("teacher.admin"),
        Some("http://localhost:3000"),
        None,
        None,
    )
    .await;

    let wrong = send(
        &app,
        "POST",
        "/api/auth/login",
        json!({ "username": "teacher.admin", "password": "wrong password value" }),
        Some("http://localhost:3000"),
        None,
        None,
    )
    .await;
    let missing = send(
        &app,
        "POST",
        "/api/auth/login",
        json!({ "username": "missing.user", "password": "wrong password value" }),
        Some("http://localhost:3000"),
        None,
        None,
    )
    .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(missing.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.body, missing.body);

    let requester = db::interact(&pool, |connection| {
        user::create(
            connection,
            &NewUser {
                username: "staff.requester",
                display_name: "Staff Requester",
                email: None,
                password: "temporary staff passphrase",
                role: Role::Requester,
                must_change_password: true,
            },
        )
    })
    .await
    .expect("requester creation");
    let forced = send(
        &app,
        "POST",
        "/api/auth/login",
        json!({ "username": "staff.requester", "password": "temporary staff passphrase" }),
        Some("http://localhost:3000"),
        None,
        None,
    )
    .await;
    assert_eq!(forced.status, StatusCode::OK);
    assert_eq!(forced.body["user"]["must_change_password"], true);

    db::interact(&pool, move |connection| {
        connection.execute(
            "UPDATE users SET is_active = 0 WHERE id = ?1",
            [requester.id.to_string()],
        )?;
        Ok(())
    })
    .await
    .expect("disable requester");
    let disabled = send(
        &app,
        "POST",
        "/api/auth/login",
        json!({ "username": "staff.requester", "password": "temporary staff passphrase" }),
        Some("http://localhost:3000"),
        None,
        None,
    )
    .await;
    assert_eq!(disabled.status, StatusCode::UNAUTHORIZED);
    assert_eq!(disabled.body, wrong.body);
}

/// Confirms Origin validation and direct-peer throttling protect public credential routes.
#[tokio::test]
async fn invalid_origin_and_repeated_failures_are_rejected() {
    let (app, _pool, _temp) = test_app().await;
    let invalid_origin = send(
        &app,
        "POST",
        "/api/setup",
        setup_body("teacher.admin"),
        Some("http://attacker.invalid"),
        None,
        None,
    )
    .await;
    assert_eq!(invalid_origin.status, StatusCode::FORBIDDEN);

    for _ in 0..5 {
        let failure = send(
            &app,
            "POST",
            "/api/auth/login",
            json!({ "username": "missing.user", "password": "wrong password value" }),
            Some("http://localhost:3000"),
            None,
            None,
        )
        .await;
        assert_eq!(failure.status, StatusCode::UNAUTHORIZED);
    }
    let throttled = send(
        &app,
        "POST",
        "/api/auth/login",
        json!({ "username": "missing.user", "password": "wrong password value" }),
        Some("http://localhost:3000"),
        None,
        None,
    )
    .await;
    assert_eq!(throttled.status, StatusCode::TOO_MANY_REQUESTS);
}

/// Confirms session bootstrap, password rotation, and logout revoke old cookies.
#[tokio::test]
async fn password_change_and_logout_rotate_session_state() {
    let (app, _pool, _temp) = test_app().await;
    let setup = send(
        &app,
        "POST",
        "/api/setup",
        setup_body("teacher.admin"),
        Some("http://localhost:3000"),
        None,
        None,
    )
    .await;
    let original_cookie = cookie_pair(setup.set_cookie.as_deref().expect("setup cookie"));
    let original_csrf = setup.body["csrf_token"].as_str().expect("setup CSRF");

    let changed = send(
        &app,
        "POST",
        "/api/auth/password",
        json!({
            "current_password": "correct horse battery staple",
            "new_password": "replacement horse battery staple"
        }),
        None,
        Some(&original_cookie),
        Some(original_csrf),
    )
    .await;
    assert_eq!(changed.status, StatusCode::OK);
    let replacement_cookie = cookie_pair(changed.set_cookie.as_deref().expect("new cookie"));
    assert_ne!(original_cookie, replacement_cookie);

    let old_session = send(
        &app,
        "GET",
        "/api/auth/session",
        json!({}),
        None,
        Some(&original_cookie),
        None,
    )
    .await;
    assert_eq!(old_session.status, StatusCode::UNAUTHORIZED);

    let current = send(
        &app,
        "GET",
        "/api/auth/session",
        json!({}),
        None,
        Some(&replacement_cookie),
        None,
    )
    .await;
    assert_eq!(current.status, StatusCode::OK);
    let current_csrf = current.body["csrf_token"].as_str().expect("rotated CSRF");
    assert_eq!(
        current_csrf,
        changed.body["csrf_token"].as_str().expect("issued CSRF")
    );

    let old_login = send(
        &app,
        "POST",
        "/api/auth/login",
        json!({ "username": "teacher.admin", "password": "correct horse battery staple" }),
        Some("http://localhost:3000"),
        None,
        None,
    )
    .await;
    assert_eq!(old_login.status, StatusCode::UNAUTHORIZED);
    let new_login = send(
        &app,
        "POST",
        "/api/auth/login",
        json!({ "username": "teacher.admin", "password": "replacement horse battery staple" }),
        Some("http://localhost:3000"),
        None,
        None,
    )
    .await;
    assert_eq!(new_login.status, StatusCode::OK);

    let logout = send(
        &app,
        "POST",
        "/api/auth/logout",
        json!({}),
        None,
        Some(&replacement_cookie),
        Some(current_csrf),
    )
    .await;
    assert_eq!(logout.status, StatusCode::NO_CONTENT);
    assert!(
        logout
            .set_cookie
            .as_deref()
            .is_some_and(|value| value.contains("Max-Age=0"))
    );
    let after_logout = send(
        &app,
        "GET",
        "/api/auth/session",
        json!({}),
        None,
        Some(&replacement_cookie),
        None,
    )
    .await;
    assert_eq!(after_logout.status, StatusCode::UNAUTHORIZED);
}
