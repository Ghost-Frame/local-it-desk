//! Administrator settings, category lifecycle, and safe public branding routes.

use std::path::{Path as FilePath, PathBuf};

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, HeaderName, HeaderValue};
use axum::response::Response;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use super::AppState;
use crate::auth::middleware::RequireAdministrator;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::audit::{self, NewAuditEntry};
use crate::models::settings::{self, Category, RuntimeSettings};
use crate::models::ticket::TicketPriority;

/// Multipart allowance beyond configured logo bytes for bounded form metadata.
const MULTIPART_OVERHEAD_BYTES: u64 = 64 * 1024;
/// Maximum source filename length accepted for validation.
const MAX_LOGO_FILENAME_LENGTH: usize = 255;
/// X-Content-Type-Options header used on public logo responses.
const X_CONTENT_TYPE_OPTIONS: HeaderName = HeaderName::from_static("x-content-type-options");

/// Administrator-visible typed settings response.
#[derive(Debug, Serialize)]
pub struct AdminSettingsResponse {
    /// Name shown throughout the browser application.
    pub app_name: String,
    /// Optional operator-provided help contact.
    pub support_contact: Option<String>,
    /// Stable public logo endpoint when a logo is configured.
    pub logo_url: Option<&'static str>,
    /// Active category preselected for new tickets.
    pub default_category_id: Option<Uuid>,
    /// Priority preselected for new tickets.
    pub default_priority: TicketPriority,
}

/// Partial non-secret settings mutation accepted from an administrator.
#[derive(Debug, Deserialize)]
struct UpdateSettingsRequest {
    /// Optional replacement visible application name.
    app_name: Option<String>,
    /// Optional replacement support contact where blank clears it.
    support_contact: Option<String>,
    /// Optional replacement default priority.
    default_priority: Option<TicketPriority>,
}

/// New administrator-managed category input.
#[derive(Debug, Deserialize)]
struct CreateCategoryRequest {
    /// Unique visible category name.
    name: String,
    /// Optional explanatory category text.
    description: Option<String>,
    /// Optional explicit display order.
    sort_order: Option<i64>,
}

/// Partial category mutation accepted from an administrator.
#[derive(Debug, Deserialize)]
struct UpdateCategoryRequest {
    /// Optional replacement category name.
    name: Option<String>,
    /// Optional replacement description where blank clears it.
    description: Option<String>,
    /// Optional replacement active state.
    is_active: Option<bool>,
    /// Optional replacement display order.
    sort_order: Option<i64>,
}

/// Validated temporary raster logo awaiting transactional persistence.
struct PreparedLogo {
    /// Temporary file inside the persistent branding directory.
    temporary_path: PathBuf,
    /// Safe extension selected from detected bytes.
    extension: &'static str,
}

/// Mounts public config, public branding, and administrator settings routes.
pub fn router(max_upload_bytes: u64) -> Router<AppState> {
    let multipart_limit = max_upload_bytes
        .saturating_add(MULTIPART_OVERHEAD_BYTES)
        .min(usize::MAX as u64) as usize;
    let logo_upload = Router::new()
        .route("/api/admin/settings/logo", post(upload_logo))
        .route_layer(DefaultBodyLimit::max(multipart_limit));
    Router::new()
        .route("/api/config", get(public_config))
        .route("/api/branding/logo", get(public_logo))
        .route(
            "/api/admin/settings",
            get(get_admin_settings).patch(update_admin_settings),
        )
        .route(
            "/api/admin/categories",
            get(list_categories).post(create_category),
        )
        .route("/api/admin/categories/{id}", patch(update_category))
        .route(
            "/api/admin/categories/{id}/default",
            post(select_default_category),
        )
        .merge(logo_upload)
}

/// Returns the explicit non-secret configuration allowlist for browser startup.
async fn public_config(
    State(state): State<AppState>,
) -> AppResult<Json<super::PublicConfigResponse>> {
    let fallback_name = state.config.app_name.clone();
    let fallback_contact = state.config.support_contact.clone();
    let (settings, categories, setup_required) = db::interact(&state.pool, move |connection| {
        let mut settings = settings::load(connection, &fallback_name, fallback_contact.as_deref())?;
        let categories = settings::list_categories(connection, true)?;
        if settings
            .default_category_id
            .is_some_and(|default_id| !categories.iter().any(|category| category.id == default_id))
        {
            settings.default_category_id = None;
        }
        let user_exists =
            connection.query_row("SELECT EXISTS(SELECT 1 FROM users LIMIT 1)", [], |row| {
                row.get::<_, bool>(0)
            })?;
        Ok((settings, categories, !user_exists))
    })
    .await?;
    Ok(Json(super::PublicConfigResponse::new(
        settings,
        categories,
        setup_required,
        state.config.max_upload_bytes,
        state.config.max_ticket_upload_bytes,
    )))
}

/// Returns all editable non-secret settings to an administrator.
async fn get_admin_settings(
    State(state): State<AppState>,
    RequireAdministrator(_identity): RequireAdministrator,
) -> AppResult<Json<AdminSettingsResponse>> {
    let fallback_name = state.config.app_name.clone();
    let fallback_contact = state.config.support_contact.clone();
    let settings = db::interact(&state.pool, move |connection| {
        settings::load(connection, &fallback_name, fallback_contact.as_deref())
    })
    .await?;
    Ok(Json(AdminSettingsResponse::from(settings)))
}

/// Validates and atomically applies a partial non-secret settings update.
async fn update_admin_settings(
    State(state): State<AppState>,
    RequireAdministrator(identity): RequireAdministrator,
    Json(request): Json<UpdateSettingsRequest>,
) -> AppResult<Json<AdminSettingsResponse>> {
    if request.app_name.is_none()
        && request.support_contact.is_none()
        && request.default_priority.is_none()
    {
        return Err(AppError::BadRequest(
            "at least one setting is required".to_string(),
        ));
    }
    let app_name = request
        .app_name
        .as_deref()
        .map(settings::validate_app_name)
        .transpose()?;
    let support_contact = request
        .support_contact
        .as_deref()
        .map(settings::validate_support_contact)
        .transpose()?;
    let fallback_name = state.config.app_name.clone();
    let fallback_contact = state.config.support_contact.clone();
    let updated = db::interact(&state.pool, move |connection| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = timestamp();
        if let Some(app_name) = app_name.as_deref() {
            settings::set_value(&transaction, "app_name", app_name, identity.user_id, &now)?;
        }
        if let Some(support_contact) = support_contact {
            if let Some(value) = support_contact.as_deref() {
                settings::set_value(
                    &transaction,
                    "support_contact",
                    value,
                    identity.user_id,
                    &now,
                )?;
            } else {
                settings::set_value(&transaction, "support_contact", "", identity.user_id, &now)?;
            }
        }
        if let Some(priority) = request.default_priority {
            settings::set_value(
                &transaction,
                "default_priority",
                priority.as_str(),
                identity.user_id,
                &now,
            )?;
        }
        audit::record(
            &transaction,
            &NewAuditEntry {
                actor_id: Some(identity.user_id),
                action: "settings.updated",
                target_type: "settings",
                target_id: None,
                summary: "Updated non-secret application settings",
                source_address: None,
                created_at: &now,
            },
        )?;
        let updated = settings::load(&transaction, &fallback_name, fallback_contact.as_deref())?;
        transaction.commit()?;
        Ok(updated)
    })
    .await?;
    Ok(Json(AdminSettingsResponse::from(updated)))
}

/// Lists active and inactive categories for administrator management.
async fn list_categories(
    State(state): State<AppState>,
    RequireAdministrator(_identity): RequireAdministrator,
) -> AppResult<Json<Vec<Category>>> {
    let categories = db::interact(&state.pool, |connection| {
        settings::list_categories(connection, false)
    })
    .await?;
    Ok(Json(categories))
}

/// Creates one active category with a unique case-insensitive name.
async fn create_category(
    State(state): State<AppState>,
    RequireAdministrator(identity): RequireAdministrator,
    Json(request): Json<CreateCategoryRequest>,
) -> AppResult<(StatusCode, Json<Category>)> {
    let name = settings::validate_category_name(&request.name)?;
    let description = settings::validate_category_description(request.description.as_deref())?;
    let category = db::interact(&state.pool, move |connection| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if settings::category_name_exists(&transaction, &name, None)? {
            return Err(AppError::Conflict(
                "category name already exists".to_string(),
            ));
        }
        let id = Uuid::new_v4();
        let now = timestamp();
        transaction.execute(
            "INSERT INTO categories (
                 id, name, description, is_active, sort_order, created_at, updated_at
             ) VALUES (?1, ?2, ?3, 1, ?4, ?5, ?5)",
            params![
                id.to_string(),
                name,
                description,
                request.sort_order.unwrap_or(0),
                now,
            ],
        )?;
        audit::record(
            &transaction,
            &NewAuditEntry {
                actor_id: Some(identity.user_id),
                action: "category.created",
                target_type: "category",
                target_id: Some(&id.to_string()),
                summary: "Created a help-desk category",
                source_address: None,
                created_at: &now,
            },
        )?;
        let category = settings::find_category(&transaction, id)?
            .ok_or_else(|| AppError::Internal("created category was missing".to_string()))?;
        transaction.commit()?;
        Ok(category)
    })
    .await?;
    Ok((StatusCode::CREATED, Json(category)))
}

/// Renames, reorders, or changes the active state of one category.
async fn update_category(
    State(state): State<AppState>,
    RequireAdministrator(identity): RequireAdministrator,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateCategoryRequest>,
) -> AppResult<Json<Category>> {
    if request.name.is_none()
        && request.description.is_none()
        && request.is_active.is_none()
        && request.sort_order.is_none()
    {
        return Err(AppError::BadRequest(
            "at least one category field is required".to_string(),
        ));
    }
    let name = request
        .name
        .as_deref()
        .map(settings::validate_category_name)
        .transpose()?;
    let description = request
        .description
        .as_deref()
        .map(|value| settings::validate_category_description(Some(value)))
        .transpose()?;
    let category = db::interact(&state.pool, move |connection| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = settings::find_category(&transaction, id)?.ok_or(AppError::NotFound)?;
        if let Some(name) = name.as_deref()
            && settings::category_name_exists(&transaction, name, Some(id))?
        {
            return Err(AppError::Conflict(
                "category name already exists".to_string(),
            ));
        }
        if request.is_active == Some(false) && current.is_active {
            let default_id =
                settings::load(&transaction, "Local IT Desk", None)?.default_category_id;
            if default_id == Some(id) {
                return Err(AppError::Conflict(
                    "the default category cannot be disabled".to_string(),
                ));
            }
        }
        let now = timestamp();
        transaction.execute(
            "UPDATE categories SET
                 name = COALESCE(?1, name),
                 description = CASE WHEN ?2 THEN ?3 ELSE description END,
                 is_active = COALESCE(?4, is_active),
                 sort_order = COALESCE(?5, sort_order),
                 updated_at = ?6
             WHERE id = ?7",
            params![
                name,
                description.is_some(),
                description.flatten(),
                request.is_active,
                request.sort_order,
                now,
                id.to_string(),
            ],
        )?;
        audit::record(
            &transaction,
            &NewAuditEntry {
                actor_id: Some(identity.user_id),
                action: "category.updated",
                target_type: "category",
                target_id: Some(&id.to_string()),
                summary: "Updated a help-desk category",
                source_address: None,
                created_at: &now,
            },
        )?;
        let category = settings::find_category(&transaction, id)?
            .ok_or_else(|| AppError::Internal("updated category was missing".to_string()))?;
        transaction.commit()?;
        Ok(category)
    })
    .await?;
    Ok(Json(category))
}

/// Selects one active category as the default for new requests.
async fn select_default_category(
    State(state): State<AppState>,
    RequireAdministrator(identity): RequireAdministrator,
    Path(id): Path<Uuid>,
) -> AppResult<Json<AdminSettingsResponse>> {
    let fallback_name = state.config.app_name.clone();
    let fallback_contact = state.config.support_contact.clone();
    let settings = db::interact(&state.pool, move |connection| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let category = settings::find_category(&transaction, id)?.ok_or(AppError::NotFound)?;
        if !category.is_active {
            return Err(AppError::Conflict(
                "the default category must be active".to_string(),
            ));
        }
        let now = timestamp();
        settings::set_value(
            &transaction,
            "default_category_id",
            &id.to_string(),
            identity.user_id,
            &now,
        )?;
        audit::record(
            &transaction,
            &NewAuditEntry {
                actor_id: Some(identity.user_id),
                action: "category.default_selected",
                target_type: "category",
                target_id: Some(&id.to_string()),
                summary: "Selected the default help-desk category",
                source_address: None,
                created_at: &now,
            },
        )?;
        let settings = settings::load(&transaction, &fallback_name, fallback_contact.as_deref())?;
        transaction.commit()?;
        Ok(settings)
    })
    .await?;
    Ok(Json(AdminSettingsResponse::from(settings)))
}

/// Streams and atomically activates one detected safe raster logo.
async fn upload_logo(
    State(state): State<AppState>,
    RequireAdministrator(identity): RequireAdministrator,
    multipart: Multipart,
) -> AppResult<Json<AdminSettingsResponse>> {
    let temporary_path = state
        .config
        .branding_dir
        .join(format!(".logo-{}.part", Uuid::new_v4()));
    let received = receive_logo(multipart, &temporary_path, state.config.max_upload_bytes).await;
    let prepared = match received {
        Ok(prepared) => prepared,
        Err(error) => {
            remove_if_present(&temporary_path);
            return Err(error);
        }
    };
    let branding_dir = state.config.branding_dir.clone();
    let fallback_name = state.config.app_name.clone();
    let fallback_contact = state.config.support_contact.clone();
    let cleanup_path = prepared.temporary_path.clone();
    let result = db::interact(&state.pool, move |connection| {
        persist_logo(
            connection,
            identity.user_id,
            prepared,
            &branding_dir,
            &fallback_name,
            fallback_contact.as_deref(),
        )
    })
    .await;
    match result {
        Ok(settings) => Ok(Json(AdminSettingsResponse::from(settings))),
        Err(error) => {
            remove_if_present(&cleanup_path);
            Err(error)
        }
    }
}

/// Serves the configured raster logo from a fixed same-origin endpoint.
async fn public_logo(State(state): State<AppState>) -> AppResult<Response> {
    let fallback_name = state.config.app_name.clone();
    let fallback_contact = state.config.support_contact.clone();
    let stored_name = db::interact(&state.pool, move |connection| {
        Ok(
            settings::load(connection, &fallback_name, fallback_contact.as_deref())?
                .logo_stored_name,
        )
    })
    .await?
    .ok_or(AppError::NotFound)?;
    let (path, media_type) = safe_logo_path(&state.config.branding_dir, &stored_name)?;
    let metadata = tokio::fs::metadata(&path).await.map_err(map_logo_io)?;
    if metadata.len() > state.config.max_upload_bytes {
        return Err(AppError::NotFound);
    }
    let bytes = tokio::fs::read(path).await.map_err(map_logo_io)?;
    let mut response = Response::new(Body::from(bytes));
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(media_type));
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
    response
        .headers_mut()
        .insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

/// Streams one bounded multipart file and validates its detected raster format.
async fn receive_logo(
    mut multipart: Multipart,
    temporary_path: &FilePath,
    max_upload_bytes: u64,
) -> AppResult<PreparedLogo> {
    let mut prepared = None;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::BadRequest("invalid multipart form".to_string()))?
    {
        if field.name() != Some("file") || prepared.is_some() {
            return Err(AppError::BadRequest(
                "exactly one logo file is required".to_string(),
            ));
        }
        let original_name = validate_logo_filename(
            field
                .file_name()
                .ok_or_else(|| AppError::BadRequest("logo filename is required".to_string()))?,
        )?;
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(temporary_path)
            .await?;
        let mut size_bytes = 0_u64;
        let mut prefix = Vec::with_capacity(8192);
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|_| AppError::BadRequest("invalid logo stream".to_string()))?
        {
            size_bytes = size_bytes
                .checked_add(chunk.len() as u64)
                .ok_or(AppError::PayloadTooLarge)?;
            if size_bytes > max_upload_bytes {
                return Err(AppError::PayloadTooLarge);
            }
            let remaining = 8192_usize.saturating_sub(prefix.len());
            prefix.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        if size_bytes == 0 {
            return Err(AppError::BadRequest(
                "logo file must not be empty".to_string(),
            ));
        }
        let extension = detect_logo(&prefix, &original_name)?;
        prepared = Some(PreparedLogo {
            temporary_path: temporary_path.to_path_buf(),
            extension,
        });
    }
    prepared.ok_or_else(|| AppError::BadRequest("logo file is required".to_string()))
}

/// Persists the active logo pointer and renames bytes before transaction commit.
fn persist_logo(
    connection: &mut Connection,
    actor_id: Uuid,
    prepared: PreparedLogo,
    branding_dir: &FilePath,
    fallback_name: &str,
    fallback_contact: Option<&str>,
) -> AppResult<RuntimeSettings> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let stored_name = format!("{}.{}", Uuid::new_v4(), prepared.extension);
    let final_path = branding_dir.join(&stored_name);
    if final_path.exists() {
        return Err(AppError::Internal(
            "generated branding path already exists".to_string(),
        ));
    }
    let now = timestamp();
    settings::set_value(
        &transaction,
        "logo_stored_name",
        &stored_name,
        actor_id,
        &now,
    )?;
    audit::record(
        &transaction,
        &NewAuditEntry {
            actor_id: Some(actor_id),
            action: "settings.logo_updated",
            target_type: "settings",
            target_id: None,
            summary: "Updated the raster application logo",
            source_address: None,
            created_at: &now,
        },
    )?;
    let updated = settings::load(&transaction, fallback_name, fallback_contact)?;
    std::fs::rename(&prepared.temporary_path, &final_path)?;
    if let Err(error) = transaction.commit() {
        remove_if_present(&final_path);
        return Err(AppError::Database(error));
    }
    Ok(updated)
}

/// Validates a source filename without retaining it in runtime settings.
fn validate_logo_filename(value: &str) -> AppResult<String> {
    if value.is_empty()
        || value.chars().count() > MAX_LOGO_FILENAME_LENGTH
        || value.chars().any(char::is_control)
        || value.contains('/')
        || value.contains('\\')
        || value == "."
        || value == ".."
    {
        return Err(AppError::BadRequest("invalid logo filename".to_string()));
    }
    Ok(value.to_string())
}

/// Detects an allowlisted raster type and verifies the source extension agrees.
fn detect_logo(prefix: &[u8], original_name: &str) -> AppResult<&'static str> {
    let detected = infer::get(prefix)
        .map(|kind| kind.mime_type())
        .ok_or_else(|| {
            AppError::UnsupportedMediaType("logo must be PNG, JPEG, or WebP".to_string())
        })?;
    let extension = original_name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .ok_or_else(|| {
            AppError::UnsupportedMediaType("logo filename needs an extension".to_string())
        })?;
    let (safe_extension, accepted_extensions): (&str, &[&str]) = match detected {
        "image/png" => ("png", &["png"]),
        "image/jpeg" => ("jpg", &["jpg", "jpeg"]),
        "image/webp" => ("webp", &["webp"]),
        _ => {
            return Err(AppError::UnsupportedMediaType(
                "logo must be PNG, JPEG, or WebP".to_string(),
            ));
        }
    };
    if !accepted_extensions.contains(&extension.as_str()) {
        return Err(AppError::UnsupportedMediaType(
            "logo extension does not match its bytes".to_string(),
        ));
    }
    Ok(safe_extension)
}

/// Resolves a randomized logo filename without permitting path traversal.
fn safe_logo_path(base: &FilePath, stored_name: &str) -> AppResult<(PathBuf, &'static str)> {
    let (id, extension) = stored_name.rsplit_once('.').ok_or(AppError::NotFound)?;
    Uuid::parse_str(id).map_err(|_| AppError::NotFound)?;
    let media_type = match extension {
        "png" => "image/png",
        "jpg" => "image/jpeg",
        "webp" => "image/webp",
        _ => return Err(AppError::NotFound),
    };
    Ok((base.join(stored_name), media_type))
}

/// Maps logo file absence to a non-disclosing not-found response.
fn map_logo_io(error: std::io::Error) -> AppError {
    match error.kind() {
        std::io::ErrorKind::NotFound => AppError::NotFound,
        _ => AppError::Io(error),
    }
}

/// Best-effort removal for uncommitted logo files.
fn remove_if_present(path: &FilePath) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "failed to remove uncommitted logo")
        }
    }
}

/// Returns a millisecond-resolution UTC timestamp for settings mutations.
fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Converts typed persisted settings into the administrator response contract.
impl From<RuntimeSettings> for AdminSettingsResponse {
    /// Builds an administrator response without the private stored logo name.
    fn from(settings: RuntimeSettings) -> Self {
        Self {
            app_name: settings.app_name,
            support_contact: settings.support_contact,
            logo_url: settings.logo_stored_name.map(|_| "/api/branding/logo"),
            default_category_id: settings.default_category_id,
            default_priority: settings.default_priority,
        }
    }
}
