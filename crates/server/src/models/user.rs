//! Local credential persistence and staff account records.

use std::str::FromStr;
use std::sync::OnceLock;

use chrono::{SecondsFormat, Utc};
use rusqlite::types::Type;
use rusqlite::{Connection, ErrorCode, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::Role;
use crate::auth::password::{
    hash_password, normalize_username, validate_display_name, verify_password,
};
use crate::error::{AppError, AppResult};

/// Dummy Argon2id hash used to equalize missing-account authentication work.
static DUMMY_PASSWORD_HASH: OnceLock<String> = OnceLock::new();

/// Public local account record with no credential material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// Stable account identifier.
    pub id: Uuid,
    /// Normalized case-insensitive login name.
    pub username: String,
    /// Human-facing staff name.
    pub display_name: String,
    /// Optional administrative contact metadata that is not used for login.
    pub email: Option<String>,
    /// Current cumulative authorization role.
    pub role: Role,
    /// Whether the account may authenticate.
    pub is_active: bool,
    /// Whether normal access is blocked pending a password change.
    pub must_change_password: bool,
    /// RFC 3339 time when the account was created.
    pub created_at: String,
    /// RFC 3339 time when account metadata last changed.
    pub updated_at: String,
    /// RFC 3339 time of the most recent successful login.
    pub last_login_at: Option<String>,
}

/// Validated account creation input whose plaintext password is never retained.
#[derive(Debug, Clone, Copy)]
pub struct NewUser<'a> {
    /// Requested case-insensitive login name.
    pub username: &'a str,
    /// Human-facing staff name.
    pub display_name: &'a str,
    /// Optional contact metadata.
    pub email: Option<&'a str>,
    /// Plaintext password consumed only for Argon2id hashing.
    pub password: &'a str,
    /// Initial cumulative authorization role.
    pub role: Role,
    /// Whether product access must remain blocked until password replacement.
    pub must_change_password: bool,
}

/// Fully validated owned account data with a one-way password hash.
#[derive(Debug, Clone)]
pub struct PreparedUser {
    /// Stable identifier selected before insertion.
    id: Uuid,
    /// Normalized case-insensitive login name.
    username: String,
    /// Validated human-facing staff name.
    display_name: String,
    /// Normalized optional contact metadata.
    email: Option<String>,
    /// Argon2id PHC string derived before opening a write transaction.
    password_hash: String,
    /// Initial cumulative authorization role.
    role: Role,
    /// Whether product access requires immediate password replacement.
    must_change_password: bool,
    /// Shared creation and update timestamp.
    created_at: String,
}

/// Internal credential row used only during uniform authentication.
#[derive(Debug)]
struct CredentialUser {
    /// Public account fields safe to return after successful authentication.
    user: User,
    /// Argon2id PHC string loaded only for password verification.
    password_hash: String,
}

/// Creates one validated local account and returns only its public fields.
pub fn create(connection: &Connection, input: &NewUser<'_>) -> AppResult<User> {
    let prepared = prepare(input)?;
    insert(connection, &prepared)
}

/// Validates and hashes account input before a caller opens a write transaction.
pub fn prepare(input: &NewUser<'_>) -> AppResult<PreparedUser> {
    Ok(PreparedUser {
        id: Uuid::new_v4(),
        username: normalize_username(input.username)?,
        display_name: validate_display_name(input.display_name)?,
        email: normalize_email(input.email)?,
        password_hash: hash_password(input.password)?,
        role: input.role,
        must_change_password: input.must_change_password,
        created_at: timestamp(),
    })
}

/// Inserts one prepared account into the caller's connection or transaction.
pub fn insert(connection: &Connection, prepared: &PreparedUser) -> AppResult<User> {
    let result = connection.execute(
        "INSERT INTO users (
             id, username, display_name, email, password_hash, role,
             is_active, must_change_password, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?8)",
        params![
            prepared.id.to_string(),
            prepared.username,
            prepared.display_name,
            prepared.email,
            prepared.password_hash,
            prepared.role.as_str(),
            prepared.must_change_password,
            prepared.created_at,
        ],
    );
    match result {
        Ok(_) => find_by_id(connection, prepared.id)?
            .ok_or_else(|| AppError::Internal("created account could not be reloaded".to_string())),
        Err(error) if error.sqlite_error_code() == Some(ErrorCode::ConstraintViolation) => Err(
            AppError::Conflict("username or account metadata already exists".to_string()),
        ),
        Err(error) => Err(error.into()),
    }
}

/// Finds one public account by stable identifier.
pub fn find_by_id(connection: &Connection, id: Uuid) -> AppResult<Option<User>> {
    connection
        .query_row(
            "SELECT id, username, display_name, email, role, is_active,
                    must_change_password, created_at, updated_at, last_login_at
             FROM users WHERE id = ?1",
            [id.to_string()],
            decode_user,
        )
        .optional()
        .map_err(Into::into)
}

/// Lists one bounded page of public staff accounts and the total row count.
pub fn list(connection: &Connection, offset: u64, limit: u64) -> AppResult<(Vec<User>, u64)> {
    let total = connection.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;
    let mut statement = connection.prepare(
        "SELECT id, username, display_name, email, role, is_active,
                must_change_password, created_at, updated_at, last_login_at
         FROM users
         ORDER BY display_name COLLATE NOCASE, username COLLATE NOCASE
         LIMIT ?1 OFFSET ?2",
    )?;
    let users = statement
        .query_map(params![limit, offset], decode_user)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok((users, total))
}

/// Authenticates one normalized account with a uniform public failure result.
pub fn authenticate(connection: &Connection, username: &str, password: &str) -> AppResult<User> {
    let normalized = normalize_username(username).map_err(|_| AppError::Unauthorized)?;
    let credentials = find_credentials(connection, &normalized)?;
    let password_matches = match credentials.as_ref() {
        Some(record) => verify_password(password, &record.password_hash),
        None => verify_password(password, dummy_password_hash()),
    };

    let Some(mut credentials) = credentials else {
        return Err(AppError::Unauthorized);
    };
    if !password_matches || !credentials.user.is_active {
        return Err(AppError::Unauthorized);
    }

    let last_login_at = timestamp();
    connection.execute(
        "UPDATE users SET last_login_at = ?1 WHERE id = ?2",
        params![last_login_at, credentials.user.id.to_string()],
    )?;
    credentials.user.last_login_at = Some(last_login_at);
    Ok(credentials.user)
}

/// Confirms one active account's current password without mutating login metadata.
pub fn confirms_password(connection: &Connection, id: Uuid, password: &str) -> AppResult<bool> {
    let password_hash = connection
        .query_row(
            "SELECT password_hash FROM users WHERE id = ?1 AND is_active = 1",
            [id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(password_hash
        .as_deref()
        .is_some_and(|stored| verify_password(password, stored)))
}

/// Returns whether the atomic first-administrator setup remains available.
pub fn setup_required(connection: &Connection) -> AppResult<bool> {
    let count: u64 = connection.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;
    Ok(count == 0)
}

/// Loads private credential material for one already-normalized username.
fn find_credentials(
    connection: &Connection,
    normalized_username: &str,
) -> AppResult<Option<CredentialUser>> {
    connection
        .query_row(
            "SELECT id, username, display_name, email, role, is_active,
                    must_change_password, created_at, updated_at, last_login_at,
                    password_hash
             FROM users WHERE username = ?1",
            [normalized_username],
            |row| {
                Ok(CredentialUser {
                    user: decode_user(row)?,
                    password_hash: row.get(10)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
}

/// Decodes the shared public user column order used by account queries.
fn decode_user(row: &Row<'_>) -> rusqlite::Result<User> {
    let id_text: String = row.get(0)?;
    let role_text: String = row.get(4)?;
    Ok(User {
        id: Uuid::parse_str(&id_text).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error))
        })?,
        username: row.get(1)?,
        display_name: row.get(2)?,
        email: row.get(3)?,
        role: Role::from_str(&role_text).map_err(|message| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    message,
                )),
            )
        })?,
        is_active: row.get(5)?,
        must_change_password: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        last_login_at: row.get(9)?,
    })
}

/// Normalizes optional email metadata without treating it as an identity key.
pub(crate) fn normalize_email(email: Option<&str>) -> AppResult<Option<String>> {
    let Some(email) = email else {
        return Ok(None);
    };
    let normalized = email.trim().to_ascii_lowercase();
    let is_valid = normalized.len() <= 254
        && normalized.contains('@')
        && !normalized.chars().any(char::is_control);
    if !is_valid {
        return Err(AppError::BadRequest(
            "email metadata is invalid".to_string(),
        ));
    }
    Ok(Some(normalized))
}

/// Returns a process-stable valid hash for missing-account verification work.
fn dummy_password_hash() -> &'static str {
    DUMMY_PASSWORD_HASH
        .get_or_init(|| {
            hash_password("uniform authentication workload")
                .expect("static dummy password must satisfy password policy")
        })
        .as_str()
}

/// Returns one UTC timestamp in a stable SQLite and API representation.
fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
