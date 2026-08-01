//! Administrator audit inspection and explicit account-session revocation.

use axum::extract::{Path, Query, State};
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get};
use axum::{Json, Router};
use chrono::{SecondsFormat, Utc};
use rusqlite::TransactionBehavior;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AppState;
use crate::auth::middleware::RequireAdministrator;
use crate::auth::session;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::audit::{self, AuditEntry, NewAuditEntry};
use crate::models::user;

/// Default number of audit rows returned on one page.
const DEFAULT_PAGE_SIZE: u64 = 50;
/// Maximum number of audit rows returned by one request.
const MAX_PAGE_SIZE: u64 = 100;

/// Optional bounded pagination parameters for audit listing.
#[derive(Debug, Deserialize)]
struct PageQuery {
    /// One-based page number.
    page: Option<u64>,
    /// Requested page size, capped by the server.
    page_size: Option<u64>,
}

/// One newest-first page of privacy-bounded audit records.
#[derive(Debug, Serialize)]
struct AuditPage {
    /// Persisted non-secret audit entries.
    items: Vec<AuditEntry>,
    /// Effective one-based page number.
    page: u64,
    /// Effective bounded page size.
    page_size: u64,
    /// Total audit record count before pagination.
    total: u64,
}

/// Mounts audit and session-revocation administration boundaries.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/audit-log", get(list_audit_log))
        .route("/api/admin/users/{id}/sessions", delete(revoke_sessions))
}

/// Lists a bounded newest-first page of non-secret audit entries.
async fn list_audit_log(
    State(state): State<AppState>,
    RequireAdministrator(_identity): RequireAdministrator,
    Query(query): Query<PageQuery>,
) -> AppResult<Json<AuditPage>> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query
        .page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let offset = page.saturating_sub(1).saturating_mul(page_size);
    let (items, total) = db::interact(&state.pool, move |connection| {
        audit::list(connection, offset, page_size)
    })
    .await?;
    Ok(Json(AuditPage {
        items,
        page,
        page_size,
        total,
    }))
}

/// Revokes every active session for one existing local account.
async fn revoke_sessions(
    State(state): State<AppState>,
    RequireAdministrator(identity): RequireAdministrator,
    Path(target_id): Path<Uuid>,
) -> AppResult<Response> {
    db::interact(&state.pool, move |connection| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if user::find_by_id(&transaction, target_id)?.is_none() {
            return Err(AppError::NotFound);
        }
        session::revoke_all_for_user(&transaction, target_id)?;
        let target_text = target_id.to_string();
        let now = timestamp();
        audit::record(
            &transaction,
            &NewAuditEntry {
                actor_id: Some(identity.user_id),
                action: "account.sessions_revoked",
                target_type: "user",
                target_id: Some(&target_text),
                summary: "Revoked all active sessions for a local account",
                source_address: None,
                created_at: &now,
            },
        )?;
        transaction.commit()?;
        Ok(())
    })
    .await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    if target_id == identity.user_id {
        insert_cookie(
            &mut response,
            &session::clear_session_cookie(state.config.cookie_secure),
        )?;
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

/// Returns one UTC timestamp in the schema's stable text representation.
fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
