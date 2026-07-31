//! Support ticket route placeholders completed in Plan 03.

use axum::Router;
use axum::routing::get;

use super::{AppState, not_implemented};

/// Mounts ticket collection, detail, and comment boundaries.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/tickets", get(not_implemented).post(not_implemented))
        .route(
            "/api/tickets/{id}",
            get(not_implemented).patch(not_implemented),
        )
        .route(
            "/api/tickets/{id}/comments",
            get(not_implemented).post(not_implemented),
        )
}
