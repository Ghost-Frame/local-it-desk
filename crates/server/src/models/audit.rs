//! Privacy-bounded administrative audit persistence and listing.

use rusqlite::types::Type;
use rusqlite::{Connection, Row, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppResult;

/// Persisted audit entry that excludes credentials and complete ticket bodies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Stable audit entry identifier.
    pub id: Uuid,
    /// Account responsible for the action when it still exists.
    pub actor_id: Option<Uuid>,
    /// Stable machine-readable action name.
    pub action: String,
    /// Kind of entity affected by the action.
    pub target_type: String,
    /// Affected entity identifier when applicable.
    pub target_id: Option<String>,
    /// Short non-sensitive action summary.
    pub summary: String,
    /// Source network address when policy permits recording it.
    pub source_address: Option<String>,
    /// UTC creation timestamp.
    pub created_at: String,
}

/// Non-secret audit data accepted from one completed domain operation.
pub struct NewAuditEntry<'a> {
    /// Account responsible for the action when known.
    pub actor_id: Option<Uuid>,
    /// Stable machine-readable action name.
    pub action: &'a str,
    /// Kind of entity affected by the action.
    pub target_type: &'a str,
    /// Affected entity identifier when applicable.
    pub target_id: Option<&'a str>,
    /// Bounded non-secret action summary.
    pub summary: &'a str,
    /// Direct source network address when available.
    pub source_address: Option<&'a str>,
    /// UTC creation timestamp chosen by the caller's transaction.
    pub created_at: &'a str,
}

/// Inserts one privacy-bounded audit record inside the caller's transaction.
pub fn record(connection: &Connection, entry: &NewAuditEntry<'_>) -> AppResult<Uuid> {
    let id = Uuid::new_v4();
    connection.execute(
        "INSERT INTO audit_log (
             id, actor_id, action, target_type, target_id, summary,
             source_address, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id.to_string(),
            entry.actor_id.map(|value| value.to_string()),
            entry.action,
            entry.target_type,
            entry.target_id,
            entry.summary,
            entry.source_address,
            entry.created_at,
        ],
    )?;
    Ok(id)
}

/// Lists a newest-first bounded page of audit records and the total count.
pub fn list(connection: &Connection, offset: u64, limit: u64) -> AppResult<(Vec<AuditEntry>, u64)> {
    let total = connection.query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0))?;
    let mut statement = connection.prepare(
        "SELECT id, actor_id, action, target_type, target_id, summary,
                source_address, created_at
         FROM audit_log
         ORDER BY created_at DESC, id DESC
         LIMIT ?1 OFFSET ?2",
    )?;
    let entries = statement
        .query_map(params![limit, offset], decode_entry)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok((entries, total))
}

/// Decodes the fixed administrative audit column order.
fn decode_entry(row: &Row<'_>) -> rusqlite::Result<AuditEntry> {
    let id_text: String = row.get(0)?;
    let actor_text: Option<String> = row.get(1)?;
    Ok(AuditEntry {
        id: parse_uuid(0, &id_text)?,
        actor_id: actor_text
            .as_deref()
            .map(|value| parse_uuid(1, value))
            .transpose()?,
        action: row.get(2)?,
        target_type: row.get(3)?,
        target_id: row.get(4)?,
        summary: row.get(5)?,
        source_address: row.get(6)?,
        created_at: row.get(7)?,
    })
}

/// Parses one UUID text value into a typed SQLite result.
fn parse_uuid(index: usize, value: &str) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}
