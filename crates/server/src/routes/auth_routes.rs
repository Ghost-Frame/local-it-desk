//! Local authentication route placeholders completed in Plan 02.

use axum::Router;
use axum::routing::post;

use super::{AppState, not_implemented};

/// Mounts the local account lifecycle endpoints without external login paths.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/setup", post(not_implemented))
        .route("/api/auth/login", post(not_implemented))
        .route("/api/auth/logout", post(not_implemented))
        .route("/api/auth/change-password", post(not_implemented))
}
