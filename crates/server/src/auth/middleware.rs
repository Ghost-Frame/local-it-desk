//! Request identity contracts for the later session-authentication plan.

use uuid::Uuid;

use super::Role;

/// Name of the server-managed HttpOnly session cookie.
pub const SESSION_COOKIE_NAME: &str = "local_it_desk_session";

/// Authenticated identity that Plan 02 will resolve from a server-side session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthenticatedUser {
    /// Stable local account identifier.
    pub user_id: Uuid,
    /// Current persisted authorization role.
    pub role: Role,
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

/// Identity wrapper reserved for technician-or-administrator extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequireTechnician(pub AuthenticatedUser);

/// Identity wrapper reserved for administrator-only extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequireAdministrator(pub AuthenticatedUser);
