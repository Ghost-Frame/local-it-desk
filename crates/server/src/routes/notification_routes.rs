//! Private notification list, count, and read-state routes.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::db;
use crate::error::AppResult;
use crate::models::notification::{self, Notification};

/// Current account unread-count response.
#[derive(Debug, Serialize)]
struct UnreadCountResponse {
    /// Number of unread private notifications.
    count: u64,
}

/// Mounts current-account notification read and mutation routes.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/notifications", get(list_notifications))
        .route(
            "/api/notifications/unread-count",
            get(unread_notification_count),
        )
        .route(
            "/api/notifications/read-all",
            post(mark_all_notifications_read),
        )
        .route("/api/notifications/{id}/read", post(mark_notification_read))
}

/// Lists only the current account's newest private notifications.
async fn list_notifications(
    State(state): State<AppState>,
    identity: AuthenticatedUser,
) -> AppResult<Json<Vec<Notification>>> {
    let notifications = db::interact(&state.pool, move |connection| {
        notification::list_for_user(connection, identity.user_id)
    })
    .await?;
    Ok(Json(notifications))
}

/// Returns the current account's unread notification count.
async fn unread_notification_count(
    State(state): State<AppState>,
    identity: AuthenticatedUser,
) -> AppResult<Json<UnreadCountResponse>> {
    let count = db::interact(&state.pool, move |connection| {
        notification::unread_count(connection, identity.user_id)
    })
    .await?;
    Ok(Json(UnreadCountResponse { count }))
}

/// Idempotently marks one current-account notification read.
async fn mark_notification_read(
    State(state): State<AppState>,
    identity: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let now = timestamp();
    db::interact(&state.pool, move |connection| {
        notification::mark_read(connection, identity.user_id, id, &now)
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Marks all currently unread current-account notifications read.
async fn mark_all_notifications_read(
    State(state): State<AppState>,
    identity: AuthenticatedUser,
) -> AppResult<StatusCode> {
    let now = timestamp();
    db::interact(&state.pool, move |connection| {
        notification::mark_all_read(connection, identity.user_id, &now)?;
        Ok(())
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Returns a millisecond-resolution UTC timestamp for read-state changes.
fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
