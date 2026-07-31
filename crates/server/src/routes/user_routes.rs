//! Staff account route placeholders completed in Plan 02.

use axum::Router;
use axum::routing::get;

use super::{AppState, not_implemented};

/// Mounts account listing, detail, and administration boundaries.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/users", get(not_implemented))
        .route(
            "/api/users/{id}",
            get(not_implemented).patch(not_implemented),
        )
}
