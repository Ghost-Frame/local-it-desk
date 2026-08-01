//! HTTP policy tests for administrator-managed staff account lifecycle controls.

use std::net::SocketAddr;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use deadpool_sqlite::Pool;
use local_it_desk_server::auth::Role;
use local_it_desk_server::config::Config;
use local_it_desk_server::db;
use local_it_desk_server::models::user::{self, NewUser};
use local_it_desk_server::routes;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

/// Captured response fields needed by administration policy assertions.
struct TestResponse {
    /// HTTP status returned by the application.
    status: StatusCode,
    /// Parsed JSON body or null for an empty response.
    body: Value,
    /// Optional authentication cookie returned after session rotation.
    set_cookie: Option<String>,
    /// Optional cache policy applied to sensitive one-time responses.
    cache_control: Option<String>,
}

/// Current browser authentication material used by state-changing requests.
struct BrowserSession {
    /// Cookie request pair containing the opaque session token.
    cookie: String,
    /// Current in-memory CSRF secret.
    csrf: String,
    /// Current user's stable identifier.
    user_id: String,
}

/// Builds one isolated migrated application and database pool.
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

/// Sends one request with optional browser session material.
async fn send(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    session: Option<&BrowserSession>,
    include_origin: bool,
) -> TestResponse {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .extension(ConnectInfo(
            "192.0.2.44:41000"
                .parse::<SocketAddr>()
                .expect("static peer address"),
        ));
    if include_origin {
        request = request.header("origin", "http://localhost:3000");
    }
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
    let status = response.status();
    let set_cookie = response
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let cache_control = response
        .headers()
        .get("cache-control")
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
        cache_control,
    }
}

/// Extracts one request Cookie pair from a Set-Cookie value.
fn cookie_pair(set_cookie: &str) -> String {
    set_cookie
        .split(';')
        .next()
        .expect("cookie pair")
        .to_string()
}

/// Runs first-administrator setup and returns its browser session.
async fn setup_administrator(app: &Router) -> BrowserSession {
    let response = send(
        app,
        "POST",
        "/api/setup",
        json!({
            "username": "teacher.admin",
            "display_name": "Teacher Administrator",
            "password": "correct horse battery staple"
        }),
        None,
        true,
    )
    .await;
    assert_eq!(response.status, StatusCode::OK);
    BrowserSession {
        cookie: cookie_pair(response.set_cookie.as_deref().expect("setup cookie")),
        csrf: response.body["csrf_token"]
            .as_str()
            .expect("setup CSRF")
            .to_string(),
        user_id: response.body["user"]["id"]
            .as_str()
            .expect("administrator id")
            .to_string(),
    }
}

/// Logs in one existing local account and returns its browser session.
async fn login(app: &Router, username: &str, password: &str) -> TestResponse {
    send(
        app,
        "POST",
        "/api/auth/login",
        json!({ "username": username, "password": password }),
        None,
        true,
    )
    .await
}

/// Converts one successful authentication response into reusable browser state.
fn browser_session(response: &TestResponse) -> BrowserSession {
    BrowserSession {
        cookie: cookie_pair(response.set_cookie.as_deref().expect("session cookie")),
        csrf: response.body["csrf_token"]
            .as_str()
            .expect("CSRF token")
            .to_string(),
        user_id: response.body["user"]["id"]
            .as_str()
            .expect("user id")
            .to_string(),
    }
}

/// Confirms requesters cannot invoke any administrator account endpoint.
#[tokio::test]
async fn requester_is_denied_account_administration() {
    let (app, pool, _temp) = test_app().await;
    setup_administrator(&app).await;
    db::interact(&pool, |connection| {
        user::create(
            connection,
            &NewUser {
                username: "staff.requester",
                display_name: "Staff Requester",
                email: None,
                password: "requester permanent passphrase",
                role: Role::Requester,
                must_change_password: false,
            },
        )
    })
    .await
    .expect("requester account");
    let login = login(&app, "staff.requester", "requester permanent passphrase").await;
    let requester = browser_session(&login);

    let response = send(
        &app,
        "GET",
        "/api/admin/users",
        json!({}),
        Some(&requester),
        false,
    )
    .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
}

/// Confirms account creation returns one-time material once and list rows remain public-only.
#[tokio::test]
async fn administrator_creates_and_lists_staff_without_persisting_plaintext() {
    let (app, pool, _temp) = test_app().await;
    let administrator = setup_administrator(&app).await;
    let created = send(
        &app,
        "POST",
        "/api/admin/users",
        json!({
            "username": "staff.one",
            "display_name": "Staff One",
            "role": "requester",
            "email": "Staff.One@Example.invalid"
        }),
        Some(&administrator),
        false,
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    assert_eq!(created.body["user"]["username"], "staff.one");
    assert_eq!(created.body["user"]["email"], "staff.one@example.invalid");
    assert_eq!(created.body["user"]["must_change_password"], true);
    assert_eq!(created.cache_control.as_deref(), Some("no-store"));
    let temporary_password = created.body["temporary_password"]
        .as_str()
        .expect("one-time password");
    assert!(temporary_password.len() >= 20);

    let listing = send(
        &app,
        "GET",
        "/api/admin/users?page=1&page_size=25",
        json!({}),
        Some(&administrator),
        false,
    )
    .await;
    assert_eq!(listing.status, StatusCode::OK);
    assert_eq!(listing.body["total"], 2);
    assert!(
        listing.body["items"]
            .as_array()
            .is_some_and(|items| { items.iter().any(|item| item["username"] == "staff.one") })
    );
    assert!(!listing.body.to_string().contains("temporary_password"));

    let leaked = db::interact(&pool, {
        let temporary_password = temporary_password.to_string();
        move |connection| {
            let rows: u64 = connection.query_row(
                "SELECT COUNT(*) FROM users
                 WHERE password_hash = ?1 OR username = ?1 OR display_name = ?1 OR email = ?1",
                [&temporary_password],
                |row| row.get(0),
            )?;
            let audit_rows: u64 = connection.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE summary LIKE '%' || ?1 || '%'",
                [&temporary_password],
                |row| row.get(0),
            )?;
            Ok(rows + audit_rows)
        }
    })
    .await
    .expect("plaintext scan");
    assert_eq!(leaked, 0);
}

/// Confirms resets revoke sessions and replace a password with one-time material.
#[tokio::test]
async fn password_reset_revokes_old_sessions_and_forces_change() {
    let (app, _pool, _temp) = test_app().await;
    let administrator = setup_administrator(&app).await;
    let created = send(
        &app,
        "POST",
        "/api/admin/users",
        json!({
            "username": "staff.one",
            "display_name": "Staff One",
            "role": "requester",
            "email": null
        }),
        Some(&administrator),
        false,
    )
    .await;
    let user_id = created.body["user"]["id"].as_str().expect("user id");
    let original_password = created.body["temporary_password"]
        .as_str()
        .expect("temporary password");
    let original_login = login(&app, "staff.one", original_password).await;
    assert_eq!(original_login.status, StatusCode::OK);
    let original_session = browser_session(&original_login);

    let reset = send(
        &app,
        "POST",
        &format!("/api/admin/users/{user_id}/reset-password"),
        json!({}),
        Some(&administrator),
        false,
    )
    .await;
    assert_eq!(reset.status, StatusCode::OK);
    assert_eq!(reset.cache_control.as_deref(), Some("no-store"));
    let replacement_password = reset.body["temporary_password"]
        .as_str()
        .expect("replacement password");
    assert_ne!(original_password, replacement_password);

    let stale = send(
        &app,
        "GET",
        "/api/auth/session",
        json!({}),
        Some(&original_session),
        false,
    )
    .await;
    assert_eq!(stale.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        login(&app, "staff.one", original_password).await.status,
        StatusCode::UNAUTHORIZED
    );
    let replacement_login = login(&app, "staff.one", replacement_password).await;
    assert_eq!(replacement_login.status, StatusCode::OK);
    assert_eq!(replacement_login.body["user"]["must_change_password"], true);
}

/// Confirms lifecycle changes revoke target sessions and final-administrator policy is atomic.
#[tokio::test]
async fn lifecycle_updates_protect_final_administrator_and_revoke_sessions() {
    let (app, _pool, _temp) = test_app().await;
    let administrator = setup_administrator(&app).await;

    for patch in [
        json!({ "role": "technician", "current_password": "correct horse battery staple" }),
        json!({ "is_active": false, "current_password": "correct horse battery staple" }),
    ] {
        let rejected = send(
            &app,
            "PATCH",
            &format!("/api/admin/users/{}", administrator.user_id),
            patch,
            Some(&administrator),
            false,
        )
        .await;
        assert_eq!(rejected.status, StatusCode::CONFLICT);
    }

    let staff = send(
        &app,
        "POST",
        "/api/admin/users",
        json!({
            "username": "staff.one",
            "display_name": "Staff One",
            "role": "requester",
            "email": null
        }),
        Some(&administrator),
        false,
    )
    .await;
    let staff_id = staff.body["user"]["id"].as_str().expect("staff id");
    let staff_password = staff.body["temporary_password"]
        .as_str()
        .expect("staff password");
    let staff_login = login(&app, "staff.one", staff_password).await;
    let staff_session = browser_session(&staff_login);

    let disabled = send(
        &app,
        "PATCH",
        &format!("/api/admin/users/{staff_id}"),
        json!({ "display_name": "Staff One Updated", "is_active": false }),
        Some(&administrator),
        false,
    )
    .await;
    assert_eq!(disabled.status, StatusCode::OK);
    assert_eq!(disabled.body["user"]["display_name"], "Staff One Updated");
    assert_eq!(disabled.body["user"]["is_active"], false);
    let stale = send(
        &app,
        "GET",
        "/api/auth/session",
        json!({}),
        Some(&staff_session),
        false,
    )
    .await;
    assert_eq!(stale.status, StatusCode::UNAUTHORIZED);
}

/// Confirms self privilege changes require a password and rotate the browser session.
#[tokio::test]
async fn self_privilege_change_requires_confirmation_and_rotates_session() {
    let (app, _pool, _temp) = test_app().await;
    let administrator = setup_administrator(&app).await;
    let denied = send(
        &app,
        "PATCH",
        &format!("/api/admin/users/{}", administrator.user_id),
        json!({ "role": "technician" }),
        Some(&administrator),
        false,
    )
    .await;
    assert_eq!(denied.status, StatusCode::UNAUTHORIZED);

    let second = send(
        &app,
        "POST",
        "/api/admin/users",
        json!({
            "username": "backup.admin",
            "display_name": "Backup Administrator",
            "role": "administrator",
            "email": null
        }),
        Some(&administrator),
        false,
    )
    .await;
    assert_eq!(second.status, StatusCode::CREATED);

    let changed = send(
        &app,
        "PATCH",
        &format!("/api/admin/users/{}", administrator.user_id),
        json!({
            "role": "technician",
            "current_password": "correct horse battery staple"
        }),
        Some(&administrator),
        false,
    )
    .await;
    assert_eq!(changed.status, StatusCode::OK);
    assert_eq!(changed.body["user"]["role"], "technician");
    let replacement = browser_session(&changed);

    let old_session = send(
        &app,
        "GET",
        "/api/auth/session",
        json!({}),
        Some(&administrator),
        false,
    )
    .await;
    assert_eq!(old_session.status, StatusCode::UNAUTHORIZED);
    let replacement_session = send(
        &app,
        "GET",
        "/api/auth/session",
        json!({}),
        Some(&replacement),
        false,
    )
    .await;
    assert_eq!(replacement_session.status, StatusCode::OK);
    assert_eq!(replacement_session.body["user"]["role"], "technician");
}

/// Confirms explicit revocation invalidates sessions and appears in the audit log.
#[tokio::test]
async fn administrator_revokes_account_sessions_and_reads_audit_log() {
    let (app, _pool, _temp) = test_app().await;
    let administrator = setup_administrator(&app).await;
    let staff = send(
        &app,
        "POST",
        "/api/admin/users",
        json!({
            "username": "staff.one",
            "display_name": "Staff One",
            "role": "requester",
            "email": null
        }),
        Some(&administrator),
        false,
    )
    .await;
    let staff_id = staff.body["user"]["id"].as_str().expect("staff id");
    let staff_password = staff.body["temporary_password"]
        .as_str()
        .expect("staff password");
    let staff_login = login(&app, "staff.one", staff_password).await;
    let staff_session = browser_session(&staff_login);

    let revoked = send(
        &app,
        "DELETE",
        &format!("/api/admin/users/{staff_id}/sessions"),
        json!({}),
        Some(&administrator),
        false,
    )
    .await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);
    let stale = send(
        &app,
        "GET",
        "/api/auth/session",
        json!({}),
        Some(&staff_session),
        false,
    )
    .await;
    assert_eq!(stale.status, StatusCode::UNAUTHORIZED);

    let audit = send(
        &app,
        "GET",
        "/api/admin/audit-log?page=1&page_size=100",
        json!({}),
        Some(&administrator),
        false,
    )
    .await;
    assert_eq!(audit.status, StatusCode::OK);
    assert!(audit.body["items"].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|entry| entry["action"] == "account.sessions_revoked")
    }));
}
