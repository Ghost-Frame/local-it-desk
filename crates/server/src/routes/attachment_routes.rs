//! Attachment route placeholders completed in Plan 03.

use axum::Router;
use axum::routing::{get, post};

use super::{AppState, not_implemented};

/// Mounts upload, download, and ticket attachment listing boundaries.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/attachments", post(not_implemented))
        .route("/api/attachments/{id}", get(not_implemented))
        .route("/api/tickets/{id}/attachments", get(not_implemented))
}
