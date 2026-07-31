//! Administrator route placeholders completed in Plans 02 and 03.

use axum::Router;
use axum::routing::{delete, get};

use super::{AppState, not_implemented};

/// Mounts audit and session-revocation administration boundaries.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/audit-log", get(not_implemented))
        .route("/api/admin/users/{id}/sessions", delete(not_implemented))
}
