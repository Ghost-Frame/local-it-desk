//! Administrator-managed local staff account lifecycle routes.

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::{SecondsFormat, Utc};
use rusqlite::{TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AppState;
use crate::auth::Role;
use crate::auth::middleware::RequireAdministrator;
use crate::auth::{password, session};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::audit::{self, NewAuditEntry};
use crate::models::roster::{self, RosterPreview};
use crate::models::user::{self, NewUser, User};

/// Default number of accounts returned on one administration page.
const DEFAULT_PAGE_SIZE: u64 = 25;
/// Maximum number of accounts returned by one administration request.
const MAX_PAGE_SIZE: u64 = 100;

/// Optional bounded pagination parameters for account listing.
#[derive(Debug, Deserialize)]
struct PageQuery {
    /// One-based page number.
    page: Option<u64>,
    /// Requested page size, capped by the server.
    page_size: Option<u64>,
}

/// One public page of local staff accounts.
#[derive(Debug, Serialize)]
struct UserPage {
    /// Public account rows without credential material.
    items: Vec<User>,
    /// Effective one-based page number.
    page: u64,
    /// Effective bounded page size.
    page_size: u64,
    /// Total account count before pagination.
    total: u64,
}

/// Administrator input for one new named staff account.
#[derive(Debug, Deserialize)]
struct CreateUserRequest {
    /// Requested normalized local login name.
    username: String,
    /// Human-facing staff name.
    display_name: String,
    /// Initial cumulative authorization role.
    role: Role,
    /// Optional administrative contact metadata.
    email: Option<String>,
}

/// One-time credential delivery returned only by create and reset operations.
#[derive(Serialize)]
struct OneTimeCredentialResponse {
    /// Public account fields after the operation.
    user: User,
    /// High-entropy temporary password that is not persisted in plaintext.
    temporary_password: String,
}

/// Atomic roster apply result containing one-time onboarding credentials.
#[derive(Serialize)]
struct RosterApplyResponse {
    /// Accounts created in CSV order with their one-time temporary passwords.
    created: Vec<OneTimeCredentialResponse>,
}

/// Mutable account fields accepted by administrator lifecycle updates.
#[derive(Debug, Deserialize)]
struct UpdateUserRequest {
    /// Optional replacement human-facing staff name.
    display_name: Option<String>,
    /// Optional replacement cumulative authorization role.
    role: Option<Role>,
    /// Optional account activation state.
    is_active: Option<bool>,
    /// Current password required for self privilege or activation changes.
    current_password: Option<String>,
}

/// Public account mutation result with optional rotated-session CSRF material.
#[derive(Debug, Serialize)]
struct UserMutationResponse {
    /// Public account fields after the operation.
    user: User,
    /// Replacement in-memory CSRF secret when the current session was rotated.
    #[serde(skip_serializing_if = "Option::is_none")]
    csrf_token: Option<String>,
}

/// Mounts administrator-only staff account administration endpoints.
pub fn router(max_roster_bytes: u64) -> Router<AppState> {
    let roster_body_limit = usize::try_from(max_roster_bytes)
        .expect("validated roster body limit must fit this platform");
    let roster_routes = Router::new()
        .route("/api/admin/users/import/preview", post(preview_roster))
        .route("/api/admin/users/import/apply", post(apply_roster))
        .route_layer(DefaultBodyLimit::max(roster_body_limit));
    Router::new()
        .route("/api/admin/users", get(list_users).post(create_user))
        .route("/api/admin/users/{id}", patch(update_user))
        .route("/api/admin/users/{id}/reset-password", post(reset_password))
        .merge(roster_routes)
}

/// Lists a bounded page of public account records.
async fn list_users(
    State(state): State<AppState>,
    RequireAdministrator(_identity): RequireAdministrator,
    Query(query): Query<PageQuery>,
) -> AppResult<Json<UserPage>> {
    let (page, page_size, offset) = pagination(query);
    let (items, total) = db::interact(&state.pool, move |connection| {
        user::list(connection, offset, page_size)
    })
    .await?;
    Ok(Json(UserPage {
        items,
        page,
        page_size,
        total,
    }))
}

/// Creates one forced-change account and returns its temporary password once.
async fn create_user(
    State(state): State<AppState>,
    RequireAdministrator(identity): RequireAdministrator,
    Json(request): Json<CreateUserRequest>,
) -> AppResult<Response> {
    let (account, temporary_password) = db::interact(&state.pool, move |connection| {
        let temporary_password = password::generate_temporary_password();
        let prepared = user::prepare(&NewUser {
            username: &request.username,
            display_name: &request.display_name,
            email: request.email.as_deref(),
            password: &temporary_password,
            role: request.role,
            must_change_password: true,
        })?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let account = user::insert(&transaction, &prepared)?;
        let target_id = account.id.to_string();
        let now = timestamp();
        audit::record(
            &transaction,
            &NewAuditEntry {
                actor_id: Some(identity.user_id),
                action: "account.created",
                target_type: "user",
                target_id: Some(&target_id),
                summary: "Created a named local staff account",
                source_address: None,
                created_at: &now,
            },
        )?;
        transaction.commit()?;
        Ok((account, temporary_password))
    })
    .await?;
    let mut response = (
        StatusCode::CREATED,
        Json(OneTimeCredentialResponse {
            user: account,
            temporary_password,
        }),
    )
        .into_response();
    prevent_storage(&mut response);
    Ok(response)
}

/// Parses and checks a roster without changing any persisted account state.
async fn preview_roster(
    State(state): State<AppState>,
    RequireAdministrator(_identity): RequireAdministrator,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Json<RosterPreview>> {
    require_csv_content_type(&headers)?;
    let max_bytes = state.config.max_roster_bytes;
    let max_rows = state.config.max_roster_rows;
    let mut preview = roster::parse(&body, max_bytes, max_rows)?;
    preview = db::interact(&state.pool, move |connection| {
        roster::add_existing_account_errors(connection, &mut preview)?;
        Ok(preview)
    })
    .await?;
    Ok(Json(preview))
}

/// Atomically creates every valid roster account and returns credentials once.
async fn apply_roster(
    State(state): State<AppState>,
    RequireAdministrator(identity): RequireAdministrator,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Response> {
    require_csv_content_type(&headers)?;
    let max_bytes = state.config.max_roster_bytes;
    let max_rows = state.config.max_roster_rows;
    let mut preview = roster::parse(&body, max_bytes, max_rows)?;
    preview = db::interact(&state.pool, move |connection| {
        roster::add_existing_account_errors(connection, &mut preview)?;
        Ok(preview)
    })
    .await?;
    if !preview.valid {
        return Err(AppError::BadRequest(
            "roster contains validation errors; preview it before applying".to_string(),
        ));
    }

    let created = db::interact(&state.pool, move |connection| {
        let mut prepared = Vec::with_capacity(preview.rows.len());
        for row in preview.rows {
            let temporary_password = password::generate_temporary_password();
            let account = user::prepare(&NewUser {
                username: &row.username,
                display_name: &row.display_name,
                email: row.email.as_deref(),
                password: &temporary_password,
                role: row.role,
                must_change_password: true,
            })?;
            prepared.push((account, temporary_password));
        }
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut created = Vec::with_capacity(prepared.len());
        for (prepared, temporary_password) in prepared {
            let account = user::insert(&transaction, &prepared)?;
            created.push(OneTimeCredentialResponse {
                user: account,
                temporary_password,
            });
        }
        let now = timestamp();
        let summary = format!("Imported {} named staff accounts", created.len());
        audit::record(
            &transaction,
            &NewAuditEntry {
                actor_id: Some(identity.user_id),
                action: "account.roster_imported",
                target_type: "roster",
                target_id: None,
                summary: &summary,
                source_address: None,
                created_at: &now,
            },
        )?;
        transaction.commit()?;
        Ok(created)
    })
    .await?;
    let mut response = (StatusCode::CREATED, Json(RosterApplyResponse { created })).into_response();
    prevent_storage(&mut response);
    Ok(response)
}

/// Updates account metadata and atomically enforces final-administrator policy.
async fn update_user(
    State(state): State<AppState>,
    RequireAdministrator(identity): RequireAdministrator,
    Path(target_id): Path<Uuid>,
    Json(request): Json<UpdateUserRequest>,
) -> AppResult<Response> {
    let requested_display_name = request
        .display_name
        .as_deref()
        .map(password::validate_display_name)
        .transpose()?;
    let requested_role = request.role;
    let requested_active = request.is_active;
    let current_password = request.current_password;

    let session_ttl_days = state.config.session_ttl_days;
    let mutation = db::interact(&state.pool, move |connection| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = user::find_by_id(&transaction, target_id)?.ok_or(AppError::NotFound)?;
        let next_role = requested_role.unwrap_or(current.role);
        let next_active = requested_active.unwrap_or(current.is_active);
        let privilege_changed = next_role != current.role || next_active != current.is_active;
        if target_id == identity.user_id && privilege_changed {
            let supplied = current_password.as_deref().unwrap_or_default();
            if !user::confirms_password(&transaction, identity.user_id, supplied)? {
                return Err(AppError::Unauthorized);
            }
        }
        let removes_active_administrator = current.role == Role::Administrator
            && current.is_active
            && (next_role != Role::Administrator || !next_active);
        if removes_active_administrator {
            let active_administrators: u64 = transaction.query_row(
                "SELECT COUNT(*) FROM users WHERE role = 'administrator' AND is_active = 1",
                [],
                |row| row.get(0),
            )?;
            if active_administrators <= 1 {
                return Err(AppError::Conflict(
                    "at least one active administrator is required".to_string(),
                ));
            }
        }

        let display_name = requested_display_name.unwrap_or(current.display_name);
        let now = timestamp();
        transaction.execute(
            "UPDATE users
             SET display_name = ?1, role = ?2, is_active = ?3, updated_at = ?4
             WHERE id = ?5",
            params![
                display_name,
                next_role.as_str(),
                next_active,
                now,
                target_id.to_string(),
            ],
        )?;
        if privilege_changed {
            session::revoke_all_for_user(&transaction, target_id)?;
        }
        let target_text = target_id.to_string();
        audit::record(
            &transaction,
            &NewAuditEntry {
                actor_id: Some(identity.user_id),
                action: "account.updated",
                target_type: "user",
                target_id: Some(&target_text),
                summary: "Updated local staff account metadata or access state",
                source_address: None,
                created_at: &now,
            },
        )?;
        let issued = if target_id == identity.user_id && privilege_changed && next_active {
            Some(session::create(
                &transaction,
                identity.user_id,
                session_ttl_days,
            )?)
        } else {
            None
        };
        let account = user::find_by_id(&transaction, target_id)?.ok_or(AppError::NotFound)?;
        transaction.commit()?;
        Ok((
            account,
            issued,
            target_id == identity.user_id && !next_active,
        ))
    })
    .await?;

    account_mutation_response(
        mutation.0,
        mutation.1,
        mutation.2,
        state.config.cookie_secure,
        session_ttl_days,
    )
}

/// Replaces one account password, forces change, and revokes all its sessions.
async fn reset_password(
    State(state): State<AppState>,
    RequireAdministrator(identity): RequireAdministrator,
    Path(target_id): Path<Uuid>,
) -> AppResult<Response> {
    let (account, temporary_password) = db::interact(&state.pool, move |connection| {
        let temporary_password = password::generate_temporary_password();
        let password_hash = password::hash_password(&temporary_password)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if user::find_by_id(&transaction, target_id)?.is_none() {
            return Err(AppError::NotFound);
        }
        let now = timestamp();
        transaction.execute(
            "UPDATE users
             SET password_hash = ?1, must_change_password = 1, updated_at = ?2
             WHERE id = ?3",
            params![password_hash, now, target_id.to_string()],
        )?;
        session::revoke_all_for_user(&transaction, target_id)?;
        let target_text = target_id.to_string();
        audit::record(
            &transaction,
            &NewAuditEntry {
                actor_id: Some(identity.user_id),
                action: "account.password_reset",
                target_type: "user",
                target_id: Some(&target_text),
                summary: "Reset a local account password and revoked its sessions",
                source_address: None,
                created_at: &now,
            },
        )?;
        let account = user::find_by_id(&transaction, target_id)?.ok_or(AppError::NotFound)?;
        transaction.commit()?;
        Ok((account, temporary_password))
    })
    .await?;
    let mut response = Json(OneTimeCredentialResponse {
        user: account,
        temporary_password,
    })
    .into_response();
    prevent_storage(&mut response);
    if target_id == identity.user_id {
        insert_cookie(
            &mut response,
            &session::clear_session_cookie(state.config.cookie_secure),
        )?;
    }
    Ok(response)
}

/// Normalizes optional pagination into a bounded page, size, and offset.
fn pagination(query: PageQuery) -> (u64, u64, u64) {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query
        .page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    (
        page,
        page_size,
        page.saturating_sub(1).saturating_mul(page_size),
    )
}

/// Builds an account mutation response and applies session cookie changes.
fn account_mutation_response(
    user: User,
    issued: Option<session::IssuedSession>,
    clear_cookie: bool,
    cookie_secure: bool,
    session_ttl_days: u64,
) -> AppResult<Response> {
    let csrf_token = issued.as_ref().map(|value| value.csrf_token.clone());
    let mut response = Json(UserMutationResponse { user, csrf_token }).into_response();
    if let Some(issued) = issued {
        insert_cookie(
            &mut response,
            &session::session_cookie(&issued.token, cookie_secure, session_ttl_days),
        )?;
    } else if clear_cookie {
        insert_cookie(&mut response, &session::clear_session_cookie(cookie_secure))?;
    }
    Ok(response)
}

/// Inserts one validated Set-Cookie header without logging bearer material.
fn insert_cookie(response: &mut Response, cookie: &str) -> AppResult<()> {
    let value = HeaderValue::from_str(cookie)
        .map_err(|_| AppError::Internal("generated session cookie was invalid".to_string()))?;
    response.headers_mut().insert(SET_COOKIE, value);
    Ok(())
}

/// Marks a response containing one-time credential material as non-cacheable.
fn prevent_storage(response: &mut Response) {
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

/// Requires the documented raw CSV request media type.
fn require_csv_content_type(headers: &HeaderMap) -> AppResult<()> {
    let is_csv = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("text/csv"));
    if !is_csv {
        return Err(AppError::UnsupportedMediaType(
            "expected text/csv".to_string(),
        ));
    }
    Ok(())
}

/// Returns one UTC timestamp in the schema's stable text representation.
fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
