//! HTTP router construction for the reduced Local IT Desk surface.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::{any, get};
use axum::{Json, Router};
use deadpool_sqlite::Pool;
use serde::Serialize;
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

use crate::auth::rate_limit::LoginRateLimiter;
use crate::config::Config;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::settings::{Category, RuntimeSettings};
use crate::models::ticket::TicketPriority;

/// Administrator foundation routes.
pub mod admin_routes;
/// Signed-in staff announcement and administrator bulletin routes.
pub mod announcement_routes;
/// Ticket and announcement attachment foundation routes.
pub mod attachment_routes;
/// Local account foundation routes.
pub mod auth_routes;
/// Private current-account notification routes.
pub mod notification_routes;
/// Runtime settings, categories, and branding routes.
pub mod settings_routes;
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
    /// Shared direct-peer throttle for public credential endpoints.
    pub login_limiter: LoginRateLimiter,
}

/// Small JSON status body returned by health endpoints.
#[derive(Debug, Serialize)]
struct HealthResponse {
    /// Machine-readable health state.
    status: &'static str,
}

/// Non-secret runtime settings safe for an unauthenticated browser.
#[derive(Debug, Serialize)]
pub(super) struct PublicConfigResponse {
    /// Name shown throughout the browser application.
    app_name: String,
    /// Optional operator-provided help contact.
    support_contact: Option<String>,
    /// Stable public raster logo endpoint when configured.
    logo_url: Option<&'static str>,
    /// Configured per-file attachment byte limit.
    max_upload_bytes: u64,
    /// Configured aggregate per-ticket attachment byte limit.
    max_ticket_upload_bytes: u64,
    /// Active requester-selectable categories.
    categories: Vec<PublicCategoryResponse>,
    /// Active category preselected for new tickets.
    default_category_id: Option<Uuid>,
    /// Priority preselected for new tickets.
    default_priority: TicketPriority,
    /// Whether the browser must show first-administrator setup.
    setup_required: bool,
    /// Running server package version.
    version: &'static str,
}

/// Public category fields needed by ticket submission controls.
#[derive(Debug, Serialize)]
struct PublicCategoryResponse {
    /// Stable category identifier.
    id: Uuid,
    /// Human-facing category name.
    name: String,
    /// Optional explanatory category text.
    description: Option<String>,
    /// Administrator-controlled display order.
    sort_order: i64,
}

/// Public configuration response construction from typed persisted state.
impl PublicConfigResponse {
    /// Builds the fixed public allowlist without arbitrary setting keys.
    pub(super) fn new(
        settings: RuntimeSettings,
        categories: Vec<Category>,
        setup_required: bool,
        max_upload_bytes: u64,
        max_ticket_upload_bytes: u64,
    ) -> Self {
        Self {
            app_name: settings.app_name,
            support_contact: settings.support_contact,
            logo_url: settings.logo_stored_name.map(|_| "/api/branding/logo"),
            max_upload_bytes,
            max_ticket_upload_bytes,
            categories: categories
                .into_iter()
                .map(PublicCategoryResponse::from)
                .collect(),
            default_category_id: settings.default_category_id,
            default_priority: settings.default_priority,
            setup_required,
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}

/// Converts an active persisted category into its public response shape.
impl From<Category> for PublicCategoryResponse {
    /// Removes administrative lifecycle timestamps and inactive state.
    fn from(category: Category) -> Self {
        Self {
            id: category.id,
            name: category.name,
            description: category.description,
            sort_order: category.sort_order,
        }
    }
}

/// Builds the complete HTTP router and attaches validated shared state.
pub fn build_router(config: Config, pool: Pool) -> Router {
    let state = AppState {
        config: Arc::new(config),
        pool,
        login_limiter: LoginRateLimiter::default(),
    };

    let mut router = Router::new()
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .merge(auth_routes::router())
        .merge(announcement_routes::router())
        .merge(notification_routes::router())
        .merge(settings_routes::router(state.config.max_upload_bytes))
        .merge(ticket_routes::router())
        .merge(attachment_routes::router(state.config.max_upload_bytes))
        .merge(user_routes::router(state.config.max_roster_bytes))
        .merge(admin_routes::router())
        .route("/api/{*path}", any(not_found))
        .route("/ws", any(not_found));

    if state.config.serve_frontend {
        let index = state.config.frontend_dir.join("index.html");
        let frontend = ServeDir::new(&state.config.frontend_dir).fallback(ServeFile::new(index));
        router = router.fallback_service(frontend);
    }

    router
        .layer(axum::middleware::from_fn(
            crate::middleware::security_headers::apply,
        ))
        .with_state(state)
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

/// Prevents unknown backend paths from falling through to the browser shell.
async fn not_found() -> AppError {
    AppError::NotFound
}
