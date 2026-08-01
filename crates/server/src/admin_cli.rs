//! Offline administrator password recovery without a network endpoint.

use std::path::Path;

use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use thiserror::Error;
use uuid::Uuid;

use crate::auth::Role;
use crate::auth::password::{hash_password, normalize_username};
use crate::auth::session;
use crate::models::audit::{self, NewAuditEntry};

/// Non-secret result returned after one committed recovery operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryResult {
    /// Exact normalized administrator username that was recovered.
    pub username: String,
    /// Number of active browser sessions invalidated by recovery.
    pub revoked_sessions: usize,
}

/// Safe failures produced while resolving or recovering an administrator.
#[derive(Debug, Error)]
pub enum RecoveryError {
    /// The explicit SQLite path does not name an existing regular file.
    #[error("database file does not exist")]
    DatabaseMissing,
    /// SQLite could not safely open or update the selected database.
    #[error("database operation failed: {0}")]
    Database(#[from] rusqlite::Error),
    /// The supplied identity was not already in canonical login form.
    #[error("username must use its exact normalized form")]
    IdentityNotNormalized,
    /// No account matched the explicit normalized username.
    #[error("administrator account was not found")]
    AccountNotFound,
    /// More than one account matched, indicating an unsafe database invariant.
    #[error("administrator identity is ambiguous")]
    AmbiguousIdentity,
    /// The selected account does not hold the administrator role.
    #[error("selected account is not an administrator")]
    NotAdministrator,
    /// The replacement password failed the product password policy.
    #[error("replacement password does not meet policy: {0}")]
    InvalidPassword(String),
    /// A validated password could not be hashed.
    #[error("replacement password could not be secured")]
    PasswordHash,
    /// Persisted identity data could not be decoded safely.
    #[error("administrator identity record is invalid")]
    InvalidIdentity,
}

/// Resolves one exact administrator and atomically replaces its password.
pub fn reset_password(
    database_path: &Path,
    supplied_username: &str,
    new_password: &str,
) -> Result<RecoveryResult, RecoveryError> {
    if !database_path.is_file() {
        return Err(RecoveryError::DatabaseMissing);
    }
    let normalized =
        normalize_username(supplied_username).map_err(|_| RecoveryError::IdentityNotNormalized)?;
    if normalized != supplied_username {
        return Err(RecoveryError::IdentityNotNormalized);
    }
    let password_hash = match hash_password(new_password) {
        Ok(hash) => hash,
        Err(crate::error::AppError::BadRequest(message)) => {
            return Err(RecoveryError::InvalidPassword(message));
        }
        Err(_) => return Err(RecoveryError::PasswordHash),
    };

    let mut connection =
        Connection::open_with_flags(database_path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let target = resolve_target(&transaction, &normalized)?;
    if target.role != Role::Administrator {
        return Err(RecoveryError::NotAdministrator);
    }

    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    transaction.execute(
        "UPDATE users
         SET password_hash = ?1, must_change_password = 1, updated_at = ?2
         WHERE id = ?3",
        params![password_hash, now, target.id.to_string()],
    )?;
    let revoked_sessions =
        session::revoke_all_for_user(&transaction, target.id).map_err(app_error_to_recovery)?;
    let target_id = target.id.to_string();
    audit::record(
        &transaction,
        &NewAuditEntry {
            actor_id: None,
            action: "account.recovery_password_reset",
            target_type: "user",
            target_id: Some(&target_id),
            summary: "Offline administrator password recovery completed",
            source_address: None,
            created_at: &now,
        },
    )
    .map_err(app_error_to_recovery)?;
    transaction.commit()?;

    Ok(RecoveryResult {
        username: target.username,
        revoked_sessions,
    })
}

/// Minimal persisted identity required to authorize offline recovery.
struct RecoveryTarget {
    /// Stable account identifier used by update and audit rows.
    id: Uuid,
    /// Exact canonical username confirmed from storage.
    username: String,
    /// Persisted authorization role that must be administrator.
    role: Role,
}

/// Resolves exactly one canonical identity without accepting case aliases.
fn resolve_target(
    connection: &Connection,
    normalized_username: &str,
) -> Result<RecoveryTarget, RecoveryError> {
    let mut statement = connection.prepare(
        "SELECT id, username, role FROM users
         WHERE username = ?1 COLLATE NOCASE LIMIT 2",
    )?;
    let rows = statement
        .query_map([normalized_username], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let [(id_text, stored_username, role_text)] = rows.as_slice() else {
        return match rows.len() {
            0 => Err(RecoveryError::AccountNotFound),
            _ => Err(RecoveryError::AmbiguousIdentity),
        };
    };
    if stored_username != normalized_username {
        return Err(RecoveryError::IdentityNotNormalized);
    }
    Ok(RecoveryTarget {
        id: Uuid::parse_str(id_text).map_err(|_| RecoveryError::InvalidIdentity)?,
        username: stored_username.clone(),
        role: role_text
            .parse()
            .map_err(|_| RecoveryError::InvalidIdentity)?,
    })
}

/// Converts shared model errors without exposing secret-bearing internals.
fn app_error_to_recovery(error: crate::error::AppError) -> RecoveryError {
    match error {
        crate::error::AppError::Database(error) => RecoveryError::Database(error),
        _ => RecoveryError::PasswordHash,
    }
}
