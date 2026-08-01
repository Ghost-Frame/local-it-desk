//! Signed-in staff announcement feed and administrator lifecycle routes.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::{SecondsFormat, Utc};
use rusqlite::{TransactionBehavior, params};
use serde::Deserialize;
use uuid::Uuid;

use super::AppState;
use crate::auth::middleware::{AuthenticatedUser, RequireAdministrator};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::announcement::{self, Announcement};
use crate::models::audit::{self, NewAuditEntry};

/// New administrator-authored announcement input.
#[derive(Debug, Deserialize)]
struct CreateAnnouncementRequest {
    /// Concise visible heading.
    title: String,
    /// Unrendered Markdown source.
    body: String,
    /// Optional initial pinned state.
    is_pinned: Option<bool>,
}

/// Partial editable announcement fields.
#[derive(Debug, Deserialize)]
struct UpdateAnnouncementRequest {
    /// Optional replacement heading.
    title: Option<String>,
    /// Optional replacement unrendered Markdown source.
    body: Option<String>,
    /// Optional replacement pinned state.
    is_pinned: Option<bool>,
}

/// Mounts signed-in feed and administrator announcement lifecycle routes.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/announcements", get(list_published))
        .route(
            "/api/admin/announcements",
            get(list_all).post(create_announcement),
        )
        .route("/api/admin/announcements/{id}", patch(update_announcement))
        .route(
            "/api/admin/announcements/{id}/publish",
            post(publish_announcement),
        )
        .route(
            "/api/admin/announcements/{id}/archive",
            post(archive_announcement),
        )
}

/// Lists published announcements for any signed-in staff account.
async fn list_published(
    State(state): State<AppState>,
    _identity: AuthenticatedUser,
) -> AppResult<Json<Vec<Announcement>>> {
    let announcements = db::interact(&state.pool, |connection| {
        announcement::list_published(connection)
    })
    .await?;
    Ok(Json(announcements))
}

/// Lists every announcement state for administrator management.
async fn list_all(
    State(state): State<AppState>,
    RequireAdministrator(_identity): RequireAdministrator,
) -> AppResult<Json<Vec<Announcement>>> {
    let announcements =
        db::interact(&state.pool, |connection| announcement::list_all(connection)).await?;
    Ok(Json(announcements))
}

/// Creates one administrator-only draft and records its audit event.
async fn create_announcement(
    State(state): State<AppState>,
    RequireAdministrator(identity): RequireAdministrator,
    Json(request): Json<CreateAnnouncementRequest>,
) -> AppResult<(StatusCode, Json<Announcement>)> {
    let title = announcement::validate_title(&request.title)?;
    let body = announcement::validate_body(&request.body)?;
    let created = db::interact(&state.pool, move |connection| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = Uuid::new_v4();
        let now = timestamp();
        transaction.execute(
            "INSERT INTO announcements (
                 id, title, body, author_id, state, is_pinned,
                 published_at, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, 'draft', ?5, NULL, ?6, ?6)",
            params![
                id.to_string(),
                title,
                body,
                identity.user_id.to_string(),
                request.is_pinned.unwrap_or(false),
                now,
            ],
        )?;
        record_audit(
            &transaction,
            identity.user_id,
            "announcement.created",
            id,
            "Created a draft staff announcement",
            &now,
        )?;
        let created = announcement::find(&transaction, id)?
            .ok_or_else(|| AppError::Internal("created announcement was missing".to_string()))?;
        transaction.commit()?;
        Ok(created)
    })
    .await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// Applies bounded content or pin changes to a non-archived announcement.
async fn update_announcement(
    State(state): State<AppState>,
    RequireAdministrator(identity): RequireAdministrator,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateAnnouncementRequest>,
) -> AppResult<Json<Announcement>> {
    if request.title.is_none() && request.body.is_none() && request.is_pinned.is_none() {
        return Err(AppError::BadRequest(
            "at least one announcement field is required".to_string(),
        ));
    }
    let title = request
        .title
        .as_deref()
        .map(announcement::validate_title)
        .transpose()?;
    let body = request
        .body
        .as_deref()
        .map(announcement::validate_body)
        .transpose()?;
    let updated = db::interact(&state.pool, move |connection| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = announcement::find(&transaction, id)?.ok_or(AppError::NotFound)?;
        if !announcement::can_edit(current.state) {
            return Err(AppError::Conflict(
                "archived announcements are read-only".to_string(),
            ));
        }
        let now = timestamp();
        transaction.execute(
            "UPDATE announcements SET
                 title = COALESCE(?1, title),
                 body = COALESCE(?2, body),
                 is_pinned = COALESCE(?3, is_pinned),
                 updated_at = ?4
             WHERE id = ?5",
            params![title, body, request.is_pinned, now, id.to_string()],
        )?;
        record_audit(
            &transaction,
            identity.user_id,
            "announcement.updated",
            id,
            "Updated a staff announcement",
            &now,
        )?;
        let updated = announcement::find(&transaction, id)?
            .ok_or_else(|| AppError::Internal("updated announcement was missing".to_string()))?;
        transaction.commit()?;
        Ok(updated)
    })
    .await?;
    Ok(Json(updated))
}

/// Moves one draft into the signed-in published feed exactly once.
async fn publish_announcement(
    State(state): State<AppState>,
    RequireAdministrator(identity): RequireAdministrator,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Announcement>> {
    let published = db::interact(&state.pool, move |connection| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = announcement::find(&transaction, id)?.ok_or(AppError::NotFound)?;
        if !announcement::can_publish(current.state) {
            return Err(AppError::Conflict(
                "only draft announcements can be published".to_string(),
            ));
        }
        let now = timestamp();
        transaction.execute(
            "UPDATE announcements
             SET state = 'published', published_at = ?1, updated_at = ?1
             WHERE id = ?2",
            params![now, id.to_string()],
        )?;
        record_audit(
            &transaction,
            identity.user_id,
            "announcement.published",
            id,
            "Published a staff announcement",
            &now,
        )?;
        let published = announcement::find(&transaction, id)?
            .ok_or_else(|| AppError::Internal("published announcement was missing".to_string()))?;
        transaction.commit()?;
        Ok(published)
    })
    .await?;
    Ok(Json(published))
}

/// Archives one draft or published announcement and removes it from the feed.
async fn archive_announcement(
    State(state): State<AppState>,
    RequireAdministrator(identity): RequireAdministrator,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Announcement>> {
    let archived = db::interact(&state.pool, move |connection| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = announcement::find(&transaction, id)?.ok_or(AppError::NotFound)?;
        if !announcement::can_archive(current.state) {
            return Err(AppError::Conflict(
                "announcement is already archived".to_string(),
            ));
        }
        let now = timestamp();
        transaction.execute(
            "UPDATE announcements SET state = 'archived', updated_at = ?1 WHERE id = ?2",
            params![now, id.to_string()],
        )?;
        record_audit(
            &transaction,
            identity.user_id,
            "announcement.archived",
            id,
            "Archived a staff announcement",
            &now,
        )?;
        let archived = announcement::find(&transaction, id)?
            .ok_or_else(|| AppError::Internal("archived announcement was missing".to_string()))?;
        transaction.commit()?;
        Ok(archived)
    })
    .await?;
    Ok(Json(archived))
}

/// Records one bounded announcement lifecycle audit entry.
fn record_audit(
    connection: &rusqlite::Connection,
    actor_id: Uuid,
    action: &str,
    target_id: Uuid,
    summary: &str,
    created_at: &str,
) -> AppResult<()> {
    audit::record(
        connection,
        &NewAuditEntry {
            actor_id: Some(actor_id),
            action,
            target_type: "announcement",
            target_id: Some(&target_id.to_string()),
            summary,
            source_address: None,
            created_at,
        },
    )?;
    Ok(())
}

/// Returns a millisecond-resolution UTC timestamp for announcement mutations.
fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
