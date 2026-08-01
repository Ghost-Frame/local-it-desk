//! First-run setup, local login, session inspection, logout, and password routes.

use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, State};
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{SecondsFormat, Utc};
use rusqlite::{TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AppState;
use crate::auth::Role;
use crate::auth::middleware::SessionUser;
use crate::auth::{password, session};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::user::{self, NewUser, User};

/// Sentinel identity used to throttle repeated first-run setup attempts.
const SETUP_LIMITER_IDENTITY: &str = "__first_setup__";

/// Public setup availability response.
#[derive(Debug, Serialize)]
struct SetupStatusResponse {
    /// Whether no account exists and first-administrator setup remains available.
    setup_required: bool,
}

/// First-administrator input accepted only while the database is empty.
#[derive(Debug, Deserialize)]
struct SetupRequest {
    /// Requested normalized login name.
    username: String,
    /// Human-facing administrator name.
    display_name: String,
    /// Initial administrator passphrase.
    password: String,
}

/// Local username and password login input.
#[derive(Debug, Deserialize)]
struct LoginRequest {
    /// Case-insensitive local login name.
    username: String,
    /// Account passphrase.
    password: String,
}

/// Current-password-confirmed password replacement input.
#[derive(Debug, Deserialize)]
struct ChangePasswordRequest {
    /// Existing passphrase used to confirm the account holder.
    current_password: String,
    /// Replacement passphrase subject to the local policy.
    new_password: String,
}

/// Authenticated browser bootstrap response with non-persistent CSRF material.
#[derive(Debug, Serialize)]
struct AuthResponse {
    /// Public current account fields.
    user: User,
    /// Raw per-session CSRF secret returned only to browser memory.
    csrf_token: String,
}

/// Mounts the complete local account session lifecycle.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/setup/status", get(setup_status))
        .route("/api/setup", post(setup))
        .route("/api/auth/login", post(login))
        .route("/api/auth/session", get(current_session))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/password", post(change_password))
}

/// Reports whether first-administrator setup remains available.
async fn setup_status(State(state): State<AppState>) -> AppResult<Json<SetupStatusResponse>> {
    let setup_required =
        db::interact(&state.pool, |connection| user::setup_required(connection)).await?;
    Ok(Json(SetupStatusResponse { setup_required }))
}

/// Atomically provisions the first administrator, defaults, audit row, and session.
async fn setup(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<SetupRequest>,
) -> AppResult<Response> {
    require_same_origin(&headers, &state.config.application_origin)?;
    let peer = direct_peer(peer);
    state.login_limiter.check(SETUP_LIMITER_IDENTITY, peer)?;

    let config = state.config.clone();
    let source_address = peer.to_string();
    let result = db::interact(&state.pool, move |connection| {
        let prepared = user::prepare(&NewUser {
            username: &request.username,
            display_name: &request.display_name,
            email: None,
            password: &request.password,
            role: Role::Administrator,
            must_change_password: false,
        })?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !user::setup_required(&transaction)? {
            return Err(AppError::Conflict("setup is already complete".to_string()));
        }

        let administrator = user::insert(&transaction, &prepared)?;
        let category_id = Uuid::new_v4();
        let now = timestamp();
        transaction.execute(
            "INSERT INTO categories (
                 id, name, description, is_active, sort_order, created_at, updated_at
             ) VALUES (?1, 'General', 'General staff support requests', 1, 0, ?2, ?2)",
            params![category_id.to_string(), now],
        )?;
        for (key, value) in [
            ("default_priority", "normal".to_string()),
            ("app_name", config.app_name.clone()),
        ] {
            transaction.execute(
                "INSERT INTO settings (key, value, updated_by, updated_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![key, value, administrator.id.to_string(), now],
            )?;
        }
        if let Some(contact) = config.support_contact.as_deref() {
            transaction.execute(
                "INSERT INTO settings (key, value, updated_by, updated_at)
                 VALUES ('support_contact', ?1, ?2, ?3)",
                params![contact, administrator.id.to_string(), now],
            )?;
        }
        transaction.execute(
            "INSERT INTO audit_log (
                 id, actor_id, action, target_type, target_id, summary,
                 source_address, created_at
             ) VALUES (?1, ?2, 'setup.completed', 'user', ?2,
                       'Created the first administrator and local defaults', ?3, ?4)",
            params![
                Uuid::new_v4().to_string(),
                administrator.id.to_string(),
                source_address,
                now,
            ],
        )?;
        let issued = session::create(&transaction, administrator.id, config.session_ttl_days)?;
        transaction.commit()?;
        Ok((administrator, issued))
    })
    .await;

    match result {
        Ok((administrator, issued)) => {
            state
                .login_limiter
                .record_success(SETUP_LIMITER_IDENTITY, peer);
            session_response(
                administrator,
                issued,
                state.config.cookie_secure,
                state.config.session_ttl_days,
            )
        }
        Err(error) => {
            state
                .login_limiter
                .record_failure(SETUP_LIMITER_IDENTITY, peer);
            Err(error)
        }
    }
}

/// Authenticates one named active account and issues a fresh server-side session.
async fn login(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> AppResult<Response> {
    require_same_origin(&headers, &state.config.application_origin)?;
    let peer = direct_peer(peer);
    let limiter_identity = request.username.trim().to_ascii_lowercase();
    state.login_limiter.check(&limiter_identity, peer)?;
    let session_ttl_days = state.config.session_ttl_days;
    let result = db::interact(&state.pool, move |connection| {
        let account = user::authenticate(connection, &request.username, &request.password)?;
        let issued = session::create(connection, account.id, session_ttl_days)?;
        Ok((account, issued))
    })
    .await;

    match result {
        Ok((account, issued)) => {
            state.login_limiter.record_success(&limiter_identity, peer);
            session_response(
                account,
                issued,
                state.config.cookie_secure,
                state.config.session_ttl_days,
            )
        }
        Err(AppError::Unauthorized) => {
            state.login_limiter.record_failure(&limiter_identity, peer);
            Err(AppError::Unauthorized)
        }
        Err(error) => Err(error),
    }
}

/// Returns the current account and re-derives its in-memory-only CSRF secret.
async fn current_session(
    State(state): State<AppState>,
    identity: SessionUser,
) -> AppResult<Json<AuthResponse>> {
    let account = db::interact(&state.pool, move |connection| {
        user::find_by_id(connection, identity.user_id)?.ok_or(AppError::Unauthorized)
    })
    .await?;
    Ok(Json(AuthResponse {
        user: account,
        csrf_token: identity.csrf_token,
    }))
}

/// Revokes the current session and clears its browser cookie.
async fn logout(State(state): State<AppState>, identity: SessionUser) -> AppResult<Response> {
    db::interact(&state.pool, move |connection| {
        session::revoke(connection, identity.session_id)?;
        Ok(())
    })
    .await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    insert_cookie(
        &mut response,
        &session::clear_session_cookie(state.config.cookie_secure),
    )?;
    Ok(response)
}

/// Replaces the current password, revokes all sessions, and issues one fresh session.
async fn change_password(
    State(state): State<AppState>,
    identity: SessionUser,
    Json(request): Json<ChangePasswordRequest>,
) -> AppResult<Response> {
    let session_ttl_days = state.config.session_ttl_days;
    let result = db::interact(&state.pool, move |connection| {
        let current =
            user::find_by_id(connection, identity.user_id)?.ok_or(AppError::Unauthorized)?;
        user::authenticate(connection, &current.username, &request.current_password)?;
        let password_hash = password::hash_password(&request.new_password)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = timestamp();
        let updated = transaction.execute(
            "UPDATE users
             SET password_hash = ?1, must_change_password = 0, updated_at = ?2
             WHERE id = ?3 AND is_active = 1",
            params![password_hash, now, identity.user_id.to_string()],
        )?;
        if updated != 1 {
            return Err(AppError::Unauthorized);
        }
        session::revoke_all_for_user(&transaction, identity.user_id)?;
        let issued = session::create(&transaction, identity.user_id, session_ttl_days)?;
        transaction.execute(
            "INSERT INTO audit_log (
                 id, actor_id, action, target_type, target_id, summary, created_at
             ) VALUES (?1, ?2, 'account.password_changed', 'user', ?2,
                       'Account holder changed the local password', ?3)",
            params![
                Uuid::new_v4().to_string(),
                identity.user_id.to_string(),
                now,
            ],
        )?;
        transaction.commit()?;
        let account =
            user::find_by_id(connection, identity.user_id)?.ok_or(AppError::Unauthorized)?;
        Ok((account, issued))
    })
    .await?;
    session_response(
        result.0,
        result.1,
        state.config.cookie_secure,
        state.config.session_ttl_days,
    )
}

/// Requires an exact configured Origin on public credential-changing requests.
fn require_same_origin(headers: &HeaderMap, application_origin: &str) -> AppResult<()> {
    let origin = headers
        .get("origin")
        .and_then(|value| value.to_str().ok())
        .ok_or(AppError::Forbidden)?;
    if origin != application_origin {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

/// Returns the direct connection peer or a loopback-only test fallback.
fn direct_peer(peer: SocketAddr) -> IpAddr {
    peer.ip()
}

/// Builds a JSON authentication response and attaches the strict session cookie.
fn session_response(
    user: User,
    issued: session::IssuedSession,
    cookie_secure: bool,
    session_ttl_days: u64,
) -> AppResult<Response> {
    let cookie = session::session_cookie(&issued.token, cookie_secure, session_ttl_days);
    let mut response = Json(AuthResponse {
        user,
        csrf_token: issued.csrf_token,
    })
    .into_response();
    insert_cookie(&mut response, &cookie)?;
    Ok(response)
}

/// Inserts one validated Set-Cookie header without exposing it to logs.
fn insert_cookie(response: &mut Response, cookie: &str) -> AppResult<()> {
    let value = HeaderValue::from_str(cookie)
        .map_err(|_| AppError::Internal("generated session cookie was invalid".to_string()))?;
    response.headers_mut().insert(SET_COOKIE, value);
    Ok(())
}

/// Returns one UTC timestamp in the schema's stable text representation.
fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
