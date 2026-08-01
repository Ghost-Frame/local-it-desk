//! Opaque server-session persistence, cookies, and CSRF token handling.

use std::str::FromStr;

use base64::Engine;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, params};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use super::Role;
use super::middleware::SESSION_COOKIE_NAME;
use crate::error::{AppError, AppResult};

/// Number of random bytes used in a session or CSRF bearer token.
const SECRET_BYTES: usize = 32;
/// Minimum interval between persisted session touch updates.
const TOUCH_INTERVAL_SECONDS: i64 = 300;

/// Secrets issued once to an authenticated browser after session creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedSession {
    /// Stable server-side session identifier.
    pub id: Uuid,
    /// Raw bearer token delivered only in the HttpOnly cookie.
    pub token: String,
    /// Raw CSRF token retained only in browser memory.
    pub csrf_token: String,
    /// Absolute UTC expiry time.
    pub expires_at: DateTime<Utc>,
}

/// Active persisted session joined with current account authorization state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSession {
    /// Stable server-side session identifier.
    pub id: Uuid,
    /// Account that owns the session.
    pub user_id: Uuid,
    /// Current role loaded live from the account row.
    pub role: Role,
    /// Whether product routes remain blocked pending password replacement.
    pub must_change_password: bool,
    /// Stored SHA-256 hash used for request CSRF verification.
    csrf_hash: String,
    /// Absolute UTC expiry time.
    pub expires_at: DateTime<Utc>,
    /// Most recent persisted activity time.
    pub last_seen_at: DateTime<Utc>,
}

/// Session validation and policy helpers.
impl ResolvedSession {
    /// Returns whether this identity may enter ordinary product routes.
    pub const fn allows_product_access(&self) -> bool {
        !self.must_change_password
    }

    /// Verifies one submitted CSRF secret against the stored hash in constant time.
    pub fn verify_csrf(&self, submitted: &str) -> bool {
        verify_csrf(submitted, &self.csrf_hash)
    }
}

/// Creates and persists one opaque session while returning its raw secrets once.
pub fn create(connection: &Connection, user_id: Uuid, ttl_days: u64) -> AppResult<IssuedSession> {
    let id = Uuid::new_v4();
    let token = generate_secret();
    let csrf_token = csrf_for_session_token(&token);
    let created_at = Utc::now();
    let ttl_days = i64::try_from(ttl_days)
        .map_err(|_| AppError::Internal("session lifetime exceeds supported range".to_string()))?;
    let expires_at = created_at + Duration::days(ttl_days);
    connection.execute(
        "INSERT INTO sessions (
             id, user_id, token_hash, csrf_hash, created_at, expires_at, last_seen_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?5)",
        params![
            id.to_string(),
            user_id.to_string(),
            hash_secret(&token),
            hash_secret(&csrf_token),
            timestamp(created_at),
            timestamp(expires_at),
        ],
    )?;
    Ok(IssuedSession {
        id,
        token,
        csrf_token,
        expires_at,
    })
}

/// Resolves one raw cookie token against current session and account state.
pub fn resolve(connection: &Connection, token: &str) -> AppResult<Option<ResolvedSession>> {
    let now = Utc::now();
    let Some(mut session) = connection
        .query_row(
            "SELECT s.id, s.user_id, u.role, u.must_change_password,
                    s.csrf_hash, s.expires_at, s.last_seen_at,
                    s.revoked_at, u.is_active
             FROM sessions s
             JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = ?1",
            [hash_secret(token)],
            decode_resolved_session,
        )
        .optional()?
    else {
        return Ok(None);
    };
    if session.revoked || !session.user_active || session.resolved.expires_at <= now {
        return Ok(None);
    }
    if now - session.resolved.last_seen_at >= Duration::seconds(TOUCH_INTERVAL_SECONDS) {
        connection.execute(
            "UPDATE sessions SET last_seen_at = ?1 WHERE id = ?2 AND revoked_at IS NULL",
            params![timestamp(now), session.resolved.id.to_string()],
        )?;
        session.resolved.last_seen_at = now;
    }
    Ok(Some(session.resolved))
}

/// Atomically revokes one session and replaces it with newly issued secrets.
pub fn rotate(
    connection: &Connection,
    session_id: Uuid,
    user_id: Uuid,
    ttl_days: u64,
) -> AppResult<IssuedSession> {
    let transaction = connection.unchecked_transaction()?;
    if !revoke(&transaction, session_id)? {
        return Err(AppError::Unauthorized);
    }
    let issued = create(&transaction, user_id, ttl_days)?;
    transaction.commit()?;
    Ok(issued)
}

/// Marks one session revoked without deleting its audit-relevant row.
pub fn revoke(connection: &Connection, session_id: Uuid) -> AppResult<bool> {
    let changed = connection.execute(
        "UPDATE sessions SET revoked_at = ?1 WHERE id = ?2 AND revoked_at IS NULL",
        params![timestamp(Utc::now()), session_id.to_string()],
    )?;
    Ok(changed == 1)
}

/// Revokes every active session owned by one account.
pub fn revoke_all_for_user(connection: &Connection, user_id: Uuid) -> AppResult<usize> {
    connection
        .execute(
            "UPDATE sessions SET revoked_at = ?1
             WHERE user_id = ?2 AND revoked_at IS NULL",
            params![timestamp(Utc::now()), user_id.to_string()],
        )
        .map_err(Into::into)
}

/// Deletes expired session rows and returns the number pruned.
pub fn prune_expired(connection: &Connection) -> AppResult<usize> {
    connection
        .execute(
            "DELETE FROM sessions WHERE expires_at <= ?1",
            [timestamp(Utc::now())],
        )
        .map_err(Into::into)
}

/// Builds the session Set-Cookie value for the selected deployment mode.
pub fn session_cookie(token: &str, secure: bool, ttl_days: u64) -> String {
    let max_age = ttl_days.saturating_mul(86_400);
    let secure_flag = if secure { "; Secure" } else { "" };
    format!(
        "{SESSION_COOKIE_NAME}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}{secure_flag}"
    )
}

/// Builds a Set-Cookie value that immediately removes the browser session.
pub fn clear_session_cookie(secure: bool) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    format!(
        "{SESSION_COOKIE_NAME}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0{secure_flag}"
    )
}

/// Returns the stable lowercase SHA-256 representation stored for a secret.
pub fn hash_secret(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}

/// Derives the stable per-session CSRF secret from the unreadable cookie token.
pub fn csrf_for_session_token(session_token: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"local-it-desk-csrf-v1\0");
    digest.update(session_token.as_bytes());
    format!("{:x}", digest.finalize())
}

/// Compares a submitted CSRF token with one stored hash in constant time.
pub fn verify_csrf(submitted: &str, expected_hash: &str) -> bool {
    let submitted_hash = hash_secret(submitted);
    bool::from(
        submitted_hash
            .as_bytes()
            .ct_eq(expected_hash.as_bytes()),
    )
}

/// Temporary decoding structure that includes invalidation flags.
struct DecodedSession {
    /// Publicly useful active session fields.
    resolved: ResolvedSession,
    /// Whether the session was explicitly revoked.
    revoked: bool,
    /// Whether the owning user may still authenticate.
    user_active: bool,
}

/// Decodes the fixed session lookup column order and validates persisted types.
fn decode_resolved_session(row: &Row<'_>) -> rusqlite::Result<DecodedSession> {
    let session_id = parse_uuid(row, 0)?;
    let user_id = parse_uuid(row, 1)?;
    let role_text: String = row.get(2)?;
    let role = Role::from_str(&role_text).map_err(|message| conversion_error(2, message))?;
    let expires_text: String = row.get(5)?;
    let last_seen_text: String = row.get(6)?;
    let revoked_at: Option<String> = row.get(7)?;
    Ok(DecodedSession {
        resolved: ResolvedSession {
            id: session_id,
            user_id,
            role,
            must_change_password: row.get(3)?,
            csrf_hash: row.get(4)?,
            expires_at: parse_timestamp(5, &expires_text)?,
            last_seen_at: parse_timestamp(6, &last_seen_text)?,
        },
        revoked: revoked_at.is_some(),
        user_active: row.get(8)?,
    })
}

/// Parses one UUID text column into its domain representation.
fn parse_uuid(row: &Row<'_>, index: usize) -> rusqlite::Result<Uuid> {
    let value: String = row.get(index)?;
    Uuid::parse_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

/// Parses one RFC 3339 timestamp column into UTC.
fn parse_timestamp(index: usize, value: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
        })
}

/// Builds a typed SQLite conversion failure for invalid persisted vocabulary.
fn conversion_error(index: usize, message: &'static str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        Type::Text,
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, message)),
    )
}

/// Generates one high-entropy URL-safe secret for browser delivery.
fn generate_secret() -> String {
    let mut bytes = [0_u8; SECRET_BYTES];
    rand::fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Formats one UTC timestamp consistently for SQLite comparisons and APIs.
fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[cfg(test)]
/// Unit tests for opaque session token primitives.
mod tests {
    use super::{generate_secret, hash_secret};

    /// Confirms generated bearer material is not its persisted representation.
    #[test]
    fn token_hash_differs_from_raw_token() {
        let token = generate_secret();
        assert_ne!(token, hash_secret(&token));
        assert_ne!(generate_secret(), token);
    }
}
