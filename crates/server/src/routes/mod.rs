//! HTTP router construction for the reduced Local IT Desk surface.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::{any, get};
use axum::{Json, Router};
use deadpool_sqlite::Pool;
use serde::Serialize;
use tower_http::services::{ServeDir, ServeFile};

use crate::config::Config;
use crate::db;
use crate::error::{AppError, AppResult};

/// Administrator foundation routes.
pub mod admin_routes;
/// Ticket and announcement attachment foundation routes.
pub mod attachment_routes;
/// Local account foundation routes.
pub mod auth_routes;
/// Support ticket foundation routes.
pub mod ticket_routes;
/// Staff account foundation routes.
pub mod user_routes;

/// Shared application dependencies available to stateful route handlers.
#[derive(Clone)]
pub struct AppState {
    /// Validated runtime configuration.
    pub config: Arc<Config>,
    /// SQLite connection pool.
    pub pool: Pool,
}

/// Small JSON status body returned by health endpoints.
#[derive(Debug, Serialize)]
struct HealthResponse {
    /// Machine-readable health state.
    status: &'static str,
}

/// Non-secret runtime settings safe for an unauthenticated browser.
#[derive(Debug, Serialize)]
struct PublicConfigResponse {
    /// Name shown throughout the browser application.
    app_name: String,
    /// Optional operator-provided help contact.
    support_contact: Option<String>,
    /// Whether the browser must show first-administrator setup.
    setup_required: bool,
}

/// Builds the complete HTTP router and attaches validated shared state.
pub fn build_router(config: Config, pool: Pool) -> Router {
    let state = AppState {
        config: Arc::new(config),
        pool,
    };

    let mut router = Router::new()
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .route("/api/config", get(public_config))
        .merge(auth_routes::router())
        .merge(ticket_routes::router())
        .merge(attachment_routes::router())
        .merge(user_routes::router())
        .merge(admin_routes::router())
        .route("/api/{*path}", any(not_found))
        .route("/ws", any(not_found));

    if state.config.serve_frontend {
        let index = state.config.frontend_dir.join("index.html");
        let frontend = ServeDir::new(&state.config.frontend_dir).fallback(ServeFile::new(index));
        router = router.fallback_service(frontend);
    }

    router.with_state(state)
}

/// Reports process liveness without touching SQLite or other dependencies.
async fn liveness() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

/// Reports readiness only after a successful database round trip.
async fn readiness(State(state): State<AppState>) -> AppResult<Json<HealthResponse>> {
    db::interact(&state.pool, |connection| {
        connection.query_row("SELECT 1", [], |_| Ok(()))?;
        Ok(())
    })
    .await?;
    Ok(Json(HealthResponse { status: "ready" }))
}

/// Returns public branding and whether first-run setup is still required.
async fn public_config(State(state): State<AppState>) -> AppResult<Json<PublicConfigResponse>> {
    let setup_required = db::interact(&state.pool, |connection| {
        let user_exists =
            connection.query_row("SELECT EXISTS(SELECT 1 FROM users LIMIT 1)", [], |row| {
                row.get::<_, bool>(0)
            })?;
        Ok(!user_exists)
    })
    .await?;

    Ok(Json(PublicConfigResponse {
        app_name: state.config.app_name.clone(),
        support_contact: state.config.support_contact.clone(),
        setup_required,
    }))
}

/// Returns the explicit foundation response for work completed in later plans.
pub(crate) async fn not_implemented() -> AppError {
    AppError::NotImplemented
}

/// Prevents unknown backend paths from falling through to the browser shell.
async fn not_found() -> AppError {
    AppError::NotFound
}
