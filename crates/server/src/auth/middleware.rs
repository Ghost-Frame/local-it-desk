//! Axum extractors for cookie sessions, CSRF, forced changes, and cumulative roles.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method};
use uuid::Uuid;

use super::Role;
use super::session;
use crate::db;
use crate::error::AppError;
use crate::routes::AppState;

/// Name of the server-managed HttpOnly session cookie.
pub const SESSION_COOKIE_NAME: &str = "local_it_desk_session";
/// Header carrying the per-session CSRF secret for state-changing requests.
pub const CSRF_HEADER_NAME: &str = "x-csrf-token";

/// Session identity that may still be restricted to password replacement routes.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionUser {
    /// Stable server-side session identifier.
    pub session_id: Uuid,
    /// Stable local account identifier.
    pub user_id: Uuid,
    /// Current persisted authorization role.
    pub role: Role,
    /// Whether product access is blocked pending password replacement.
    pub must_change_password: bool,
    /// Stable derived CSRF secret returned only by the session bootstrap route.
    pub(crate) csrf_token: String,
}

/// Extracts a valid session and enforces CSRF on unsafe HTTP methods.
impl FromRequestParts<AppState> for SessionUser {
    /// HTTP-safe rejection returned when session or CSRF validation fails.
    type Rejection = AppError;

    /// Resolves the cookie through SQLite and applies request-integrity checks.
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = session_cookie_value(&parts.headers).ok_or(AppError::Unauthorized)?;
        let csrf_token = session::csrf_for_session_token(&token);
        let resolved = db::interact(&state.pool, move |connection| {
            session::resolve(connection, &token)
        })
        .await?
        .ok_or(AppError::Unauthorized)?;

        if requires_csrf(&parts.method) {
            let submitted = parts
                .headers
                .get(CSRF_HEADER_NAME)
                .and_then(|value| value.to_str().ok())
                .ok_or(AppError::Forbidden)?;
            if !resolved.verify_csrf(submitted) {
                return Err(AppError::Forbidden);
            }
        }

        Ok(Self {
            session_id: resolved.id,
            user_id: resolved.user_id,
            role: resolved.role,
            must_change_password: resolved.must_change_password,
            csrf_token,
        })
    }
}

/// Authenticated identity allowed to enter ordinary product routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthenticatedUser {
    /// Stable server-side session identifier.
    pub session_id: Uuid,
    /// Stable local account identifier.
    pub user_id: Uuid,
    /// Current persisted authorization role.
    pub role: Role,
}

/// Extracts a valid session and blocks accounts awaiting password replacement.
impl FromRequestParts<AppState> for AuthenticatedUser {
    /// HTTP-safe rejection returned when authentication or policy fails.
    type Rejection = AppError;

    /// Applies session, CSRF, and forced-change checks before product access.
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session_user = SessionUser::from_request_parts(parts, state).await?;
        if session_user.must_change_password {
            return Err(AppError::Forbidden);
        }
        Ok(Self {
            session_id: session_user.session_id,
            user_id: session_user.user_id,
            role: session_user.role,
        })
    }
}

/// Capability helpers for authenticated request identities.
impl AuthenticatedUser {
    /// Returns whether this identity can work the shared ticket queue.
    pub const fn can_work_tickets(self) -> bool {
        self.role.can_work_tickets()
    }

    /// Returns whether this identity can administer the application.
    pub const fn can_administer(self) -> bool {
        self.role.can_administer()
    }
}

/// Identity wrapper for technician-or-administrator extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequireTechnician(pub AuthenticatedUser);

/// Extracts an authenticated identity with shared-queue capability.
impl FromRequestParts<AppState> for RequireTechnician {
    /// HTTP-safe rejection returned when authentication or role policy fails.
    type Rejection = AppError;

    /// Rejects requester-only accounts after ordinary authentication succeeds.
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let identity = AuthenticatedUser::from_request_parts(parts, state).await?;
        if !identity.can_work_tickets() {
            return Err(AppError::Forbidden);
        }
        Ok(Self(identity))
    }
}

/// Identity wrapper for administrator-only extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequireAdministrator(pub AuthenticatedUser);

/// Extracts an authenticated identity with account-administration capability.
impl FromRequestParts<AppState> for RequireAdministrator {
    /// HTTP-safe rejection returned when authentication or role policy fails.
    type Rejection = AppError;

    /// Rejects non-administrators after ordinary authentication succeeds.
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let identity = AuthenticatedUser::from_request_parts(parts, state).await?;
        if !identity.can_administer() {
            return Err(AppError::Forbidden);
        }
        Ok(Self(identity))
    }
}

/// Returns the raw session cookie value from one Cookie header.
fn session_cookie_value(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all("cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .filter_map(|cookie| cookie.trim().split_once('='))
        .find_map(|(name, value)| {
            (name == SESSION_COOKIE_NAME && !value.is_empty()).then(|| value.to_string())
        })
}

/// Returns whether the HTTP method requires a submitted CSRF secret.
fn requires_csrf(method: &Method) -> bool {
    matches!(
        method,
        &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
    )
}
