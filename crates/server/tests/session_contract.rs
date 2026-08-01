//! Contract tests for opaque sessions, CSRF enforcement, and role extractors.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::{Router, response::IntoResponse};
use chrono::{Duration, Utc};
use local_it_desk_server::auth::Role;
use local_it_desk_server::auth::middleware::{
    AuthenticatedUser, RequireAdministrator, SESSION_COOKIE_NAME,
};
use local_it_desk_server::auth::session;
use local_it_desk_server::config::Config;
use local_it_desk_server::db;
use local_it_desk_server::models::user::{self, NewUser};
use local_it_desk_server::routes::AppState;
use rusqlite::Connection;
use tempfile::TempDir;
use tower::ServiceExt;

/// Opens one migrated in-memory database for direct session checks.
fn database() -> Connection {
    let connection = Connection::open_in_memory().expect("in-memory database");
    db::migrations::run_migrations(&connection).expect("fresh schema");
    connection
}

/// Creates one account with a deterministic contract passphrase.
fn account(connection: &Connection, role: Role, must_change_password: bool) -> user::User {
    user::create(
        connection,
        &NewUser {
            username: "teacher.one",
            display_name: "Teacher One",
            email: None,
            password: "correct horse battery staple",
            role,
            must_change_password,
        },
    )
    .expect("account creation")
}

/// Confirms cookie flags are strict and Secure follows deployment configuration.
#[test]
fn session_cookie_flags_match_contract() {
    let insecure = session::session_cookie("opaque-token", false, 14);
    assert!(insecure.starts_with(&format!("{SESSION_COOKIE_NAME}=opaque-token;")));
    assert!(insecure.contains("HttpOnly"));
    assert!(insecure.contains("SameSite=Strict"));
    assert!(insecure.contains("Path=/"));
    assert!(insecure.contains("Max-Age=1209600"));
    assert!(!insecure.contains("Secure"));

    let secure = session::session_cookie("opaque-token", true, 14);
    assert!(secure.contains("Secure"));
    let cleared = session::clear_session_cookie(true);
    assert!(cleared.contains("Max-Age=0"));
    assert!(cleared.contains("Secure"));
}

/// Confirms databases contain only hashes while issued secrets validate correctly.
#[test]
fn session_and_csrf_tokens_are_hashed_at_rest() {
    let connection = database();
    let user = account(&connection, Role::Requester, false);
    let issued = session::create(&connection, user.id, 14).expect("session creation");
    let (token_hash, csrf_hash): (String, String) = connection
        .query_row(
            "SELECT token_hash, csrf_hash FROM sessions WHERE id = ?1",
            [issued.id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("persisted session");

    assert_ne!(issued.token, token_hash);
    assert_ne!(issued.csrf_token, csrf_hash);
    assert_eq!(session::hash_secret(&issued.token), token_hash);
    assert!(session::verify_csrf(&issued.csrf_token, &csrf_hash));
    assert!(!session::verify_csrf("wrong-csrf-token", &csrf_hash));
}

/// Confirms expiry, revocation, and disabled accounts invalidate lookup immediately.
#[test]
fn invalid_session_states_fail_closed() {
    let connection = database();
    let user = account(&connection, Role::Requester, false);

    let expired = session::create(&connection, user.id, 14).expect("expired fixture");
    connection
        .execute(
            "UPDATE sessions SET expires_at = ?1 WHERE id = ?2",
            [
                (Utc::now() - Duration::minutes(1)).to_rfc3339(),
                expired.id.to_string(),
            ],
        )
        .expect("expire session");
    assert!(session::resolve(&connection, &expired.token).expect("lookup").is_none());

    let revoked = session::create(&connection, user.id, 14).expect("revoked fixture");
    session::revoke(&connection, revoked.id).expect("revoke session");
    assert!(session::resolve(&connection, &revoked.token).expect("lookup").is_none());

    let disabled = session::create(&connection, user.id, 14).expect("disabled fixture");
    connection
        .execute(
            "UPDATE users SET is_active = 0 WHERE id = ?1",
            [user.id.to_string()],
        )
        .expect("disable account");
    assert!(session::resolve(&connection, &disabled.token).expect("lookup").is_none());
}

/// Confirms password reset revocation and role changes take effect without stale claims.
#[test]
fn session_revocation_and_live_role_loading_are_immediate() {
    let connection = database();
    let user = account(&connection, Role::Requester, false);
    let first = session::create(&connection, user.id, 14).expect("first session");
    let second = session::create(&connection, user.id, 14).expect("second session");

    connection
        .execute(
            "UPDATE users SET role = 'administrator' WHERE id = ?1",
            [user.id.to_string()],
        )
        .expect("promote user");
    let resolved = session::resolve(&connection, &first.token)
        .expect("lookup")
        .expect("active session");
    assert_eq!(resolved.role, Role::Administrator);

    assert_eq!(
        session::revoke_all_for_user(&connection, user.id).expect("revoke all"),
        2
    );
    assert!(session::resolve(&connection, &first.token).expect("lookup").is_none());
    assert!(session::resolve(&connection, &second.token).expect("lookup").is_none());
}

/// Confirms privilege-sensitive session rotation invalidates the prior bearer token.
#[test]
fn session_rotation_replaces_all_browser_secrets() {
    let connection = database();
    let user = account(&connection, Role::Administrator, false);
    let original = session::create(&connection, user.id, 14).expect("original session");
    let replacement = session::rotate(&connection, original.id, user.id, 14)
        .expect("rotated session");

    assert_ne!(original.token, replacement.token);
    assert_ne!(original.csrf_token, replacement.csrf_token);
    assert!(session::resolve(&connection, &original.token).expect("lookup").is_none());
    assert!(
        session::resolve(&connection, &replacement.token)
            .expect("lookup")
            .is_some()
    );
}

/// Builds one isolated HTTP router backed by a persisted session.
async fn protected_router(
    role: Role,
    must_change_password: bool,
) -> (Router, session::IssuedSession, TempDir) {
    let temp = tempfile::tempdir().expect("temporary directory");
    let config = Config::for_test(temp.path().join("desk.db"), temp.path().join("uploads"));
    let pool = db::create_pool(&config.database_path);
    let connection = pool.get().await.expect("database connection");
    let issued = connection
        .interact(move |connection| {
            db::migrations::run_migrations(connection)?;
            let user = account(connection, role, must_change_password);
            session::create(connection, user.id, 14)
        })
        .await
        .expect("database interaction")
        .expect("session fixture");
    let state = AppState {
        config: Arc::new(config),
        pool,
    };
    let router = Router::new()
        .route("/product", get(product_handler).post(product_handler))
        .route("/administrator", get(administrator_handler))
        .with_state(state);
    (router, issued, temp)
}

/// Returns success only after the ordinary authenticated extractor succeeds.
async fn product_handler(_identity: AuthenticatedUser) -> impl IntoResponse {
    StatusCode::NO_CONTENT
}

/// Returns success only after administrator role extraction succeeds.
async fn administrator_handler(_identity: RequireAdministrator) -> impl IntoResponse {
    StatusCode::NO_CONTENT
}

/// Sends one request with session and optional CSRF headers.
async fn protected_request(
    app: &Router,
    method: &str,
    path: &str,
    issued: &session::IssuedSession,
    csrf: Option<&str>,
) -> StatusCode {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("cookie", format!("{SESSION_COOKIE_NAME}={}", issued.token));
    if let Some(csrf) = csrf {
        request = request.header("x-csrf-token", csrf);
    }
    app.clone()
        .oneshot(request.body(Body::empty()).expect("request"))
        .await
        .expect("response")
        .status()
}

/// Confirms forced-change accounts, role denial, and CSRF failures are enforced server-side.
#[tokio::test]
async fn extractors_enforce_product_role_and_csrf_policy() {
    let (requester_app, requester_session, _temp) = protected_router(Role::Requester, false).await;
    assert_eq!(
        protected_request(&requester_app, "GET", "/product", &requester_session, None).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        protected_request(
            &requester_app,
            "GET",
            "/administrator",
            &requester_session,
            None,
        )
        .await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        protected_request(&requester_app, "POST", "/product", &requester_session, None).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        protected_request(
            &requester_app,
            "POST",
            "/product",
            &requester_session,
            Some(&requester_session.csrf_token),
        )
        .await,
        StatusCode::NO_CONTENT
    );

    let (forced_app, forced_session, _temp) = protected_router(Role::Administrator, true).await;
    assert_eq!(
        protected_request(&forced_app, "GET", "/product", &forced_session, None).await,
        StatusCode::FORBIDDEN
    );

    let (admin_app, admin_session, _temp) = protected_router(Role::Administrator, false).await;
    assert_eq!(
        protected_request(&admin_app, "GET", "/administrator", &admin_session, None).await,
        StatusCode::NO_CONTENT
    );
}
