//! Typed runtime settings and category persistence contracts.

use std::str::FromStr;

use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::ticket::TicketPriority;

/// Maximum visible application name length.
pub const MAX_APP_NAME_LENGTH: usize = 80;
/// Maximum visible support contact length.
pub const MAX_SUPPORT_CONTACT_LENGTH: usize = 200;
/// Maximum category name length.
pub const MAX_CATEGORY_NAME_LENGTH: usize = 80;
/// Maximum optional category description length.
pub const MAX_CATEGORY_DESCRIPTION_LENGTH: usize = 500;

/// One administrator-managed help-desk category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    /// Stable category identifier.
    pub id: Uuid,
    /// Unique human-facing category name.
    pub name: String,
    /// Optional explanatory category text.
    pub description: Option<String>,
    /// Whether requesters can select this category.
    pub is_active: bool,
    /// Administrator-controlled display order.
    pub sort_order: i64,
    /// UTC creation timestamp.
    pub created_at: String,
    /// UTC timestamp of the most recent category mutation.
    pub updated_at: String,
}

/// Typed non-secret settings used by API response builders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSettings {
    /// Name shown throughout the browser application.
    pub app_name: String,
    /// Optional operator-provided help contact.
    pub support_contact: Option<String>,
    /// Randomized active raster logo filename.
    pub logo_stored_name: Option<String>,
    /// Active category preselected for new tickets.
    pub default_category_id: Option<Uuid>,
    /// Priority preselected for new tickets.
    pub default_priority: TicketPriority,
}

/// Loads all supported settings without exposing arbitrary table keys.
pub fn load(
    connection: &Connection,
    fallback_app_name: &str,
    fallback_support_contact: Option<&str>,
) -> AppResult<RuntimeSettings> {
    let app_name =
        get_value(connection, "app_name")?.unwrap_or_else(|| fallback_app_name.to_string());
    let support_contact = match get_value(connection, "support_contact")? {
        Some(value) => (!value.trim().is_empty()).then_some(value),
        None => fallback_support_contact.map(str::to_string),
    };
    let logo_stored_name = get_value(connection, "logo_stored_name")?;
    let default_category_id = get_value(connection, "default_category_id")?
        .map(|value| {
            Uuid::parse_str(&value).map_err(|_| {
                AppError::Internal("persisted default category is invalid".to_string())
            })
        })
        .transpose()?;
    let priority_text =
        get_value(connection, "default_priority")?.unwrap_or_else(|| "normal".to_string());
    let default_priority = TicketPriority::from_str(&priority_text)
        .map_err(|_| AppError::Internal("persisted default priority is invalid".to_string()))?;
    Ok(RuntimeSettings {
        app_name,
        support_contact,
        logo_stored_name,
        default_category_id,
        default_priority,
    })
}

/// Lists all categories in stable administrator display order.
pub fn list_categories(connection: &Connection, active_only: bool) -> AppResult<Vec<Category>> {
    let sql = if active_only {
        "SELECT id, name, description, is_active, sort_order, created_at, updated_at
         FROM categories WHERE is_active = 1 ORDER BY sort_order, name COLLATE NOCASE, id"
    } else {
        "SELECT id, name, description, is_active, sort_order, created_at, updated_at
         FROM categories ORDER BY sort_order, name COLLATE NOCASE, id"
    };
    let mut statement = connection.prepare(sql)?;
    Ok(statement
        .query_map([], decode_category)?
        .collect::<Result<Vec<_>, _>>()?)
}

/// Finds one category by stable identifier.
pub fn find_category(connection: &Connection, id: Uuid) -> AppResult<Option<Category>> {
    Ok(connection
        .query_row(
            "SELECT id, name, description, is_active, sort_order, created_at, updated_at
             FROM categories WHERE id = ?1",
            [id.to_string()],
            decode_category,
        )
        .optional()?)
}

/// Returns whether a different category already owns a case-insensitive name.
pub fn category_name_exists(
    connection: &Connection,
    name: &str,
    excluding_id: Option<Uuid>,
) -> AppResult<bool> {
    let exists = if let Some(excluding_id) = excluding_id {
        connection.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM categories WHERE name = ?1 COLLATE NOCASE AND id <> ?2
             )",
            params![name, excluding_id.to_string()],
            |row| row.get(0),
        )?
    } else {
        connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM categories WHERE name = ?1 COLLATE NOCASE)",
            [name],
            |row| row.get(0),
        )?
    };
    Ok(exists)
}

/// Inserts or replaces one recognized setting value.
pub fn set_value(
    connection: &Connection,
    key: &str,
    value: &str,
    actor_id: Uuid,
    updated_at: &str,
) -> AppResult<()> {
    connection.execute(
        "INSERT INTO settings (key, value, updated_by, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(key) DO UPDATE SET
             value = excluded.value,
             updated_by = excluded.updated_by,
             updated_at = excluded.updated_at",
        params![key, value, actor_id.to_string(), updated_at],
    )?;
    Ok(())
}

/// Trims and validates a visible application name.
pub fn validate_app_name(value: &str) -> AppResult<String> {
    validate_required_text(value, "app_name", MAX_APP_NAME_LENGTH)
}

/// Trims and validates an optional support contact where blank clears the value.
pub fn validate_support_contact(value: &str) -> AppResult<Option<String>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > MAX_SUPPORT_CONTACT_LENGTH {
        return Err(AppError::BadRequest(format!(
            "support_contact must be at most {MAX_SUPPORT_CONTACT_LENGTH} characters"
        )));
    }
    Ok(Some(value.to_string()))
}

/// Trims and validates a unique category name candidate.
pub fn validate_category_name(value: &str) -> AppResult<String> {
    validate_required_text(value, "category name", MAX_CATEGORY_NAME_LENGTH)
}

/// Trims and validates an optional category description where blank clears it.
pub fn validate_category_description(value: Option<&str>) -> AppResult<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.chars().count() > MAX_CATEGORY_DESCRIPTION_LENGTH {
        return Err(AppError::BadRequest(format!(
            "category description must be at most {MAX_CATEGORY_DESCRIPTION_LENGTH} characters"
        )));
    }
    Ok(Some(value.to_string()))
}

/// Returns one recognized setting table value.
fn get_value(connection: &Connection, key: &str) -> AppResult<Option<String>> {
    Ok(connection
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()?)
}

/// Validates a required trimmed text field against a character bound.
fn validate_required_text(value: &str, field: &str, maximum: usize) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::BadRequest(format!("{field} must not be empty")));
    }
    if value.chars().count() > maximum {
        return Err(AppError::BadRequest(format!(
            "{field} must be at most {maximum} characters"
        )));
    }
    Ok(value.to_string())
}

/// Decodes the fixed category query column order.
fn decode_category(row: &Row<'_>) -> rusqlite::Result<Category> {
    let id_text: String = row.get(0)?;
    Ok(Category {
        id: Uuid::parse_str(&id_text).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error))
        })?,
        name: row.get(1)?,
        description: row.get(2)?,
        is_active: row.get(3)?,
        sort_order: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}
