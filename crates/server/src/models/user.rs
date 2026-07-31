//! Local account records required by ticket ownership and administration.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::Role;
use crate::error::AppResult;

/// Public local account record with no credential material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// Stable account identifier.
    pub id: Uuid,
    /// Normalized case-insensitive login name.
    pub username: String,
    /// Human-facing staff name.
    pub display_name: String,
    /// Current cumulative authorization role.
    pub role: Role,
    /// Whether the account may authenticate.
    pub is_active: bool,
    /// Whether normal access is blocked pending a password change.
    pub must_change_password: bool,
}

/// Returns whether the atomic first-administrator setup remains available.
pub fn setup_required(connection: &Connection) -> AppResult<bool> {
    let count: u64 = connection.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;
    Ok(count == 0)
}
