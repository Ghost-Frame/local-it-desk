//! Shared bounded CSV parsing for roster preview and atomic account creation.

use std::collections::HashSet;
use std::str::FromStr;

use csv::{ByteRecord, ReaderBuilder};
use rusqlite::Connection;
use serde::Serialize;

use crate::auth::Role;
use crate::auth::password::{normalize_username, validate_display_name};
use crate::error::{AppError, AppResult};
use crate::models::user;

/// Exact ordered CSV header accepted by the roster importer.
const EXPECTED_HEADER: [&[u8]; 4] = [b"username", b"display_name", b"role", b"email"];

/// One fully normalized account row safe to show in a preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RosterRow {
    /// One-based physical CSV line containing the row.
    pub row_number: u64,
    /// Normalized case-insensitive local login name.
    pub username: String,
    /// Trimmed Unicode human-facing staff name.
    pub display_name: String,
    /// Requested cumulative authorization role.
    pub role: Role,
    /// Optional normalized administrative email metadata.
    pub email: Option<String>,
}

/// One sanitized validation error that never echoes submitted cell content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RosterError {
    /// One-based CSV line when the failure belongs to a specific row.
    pub row_number: Option<u64>,
    /// Stable header field name when the failure belongs to one cell.
    pub field: Option<String>,
    /// Bounded safe explanation without raw CSV content.
    pub message: String,
}

/// Parsed roster preview shared by dry-run and apply routes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RosterPreview {
    /// Whether the entire roster is safe to apply.
    pub valid: bool,
    /// Fully normalized valid rows.
    pub rows: Vec<RosterRow>,
    /// Sanitized structural, field, duplicate, and conflict errors.
    pub errors: Vec<RosterError>,
}

/// Parses one bounded roster into normalized rows and sanitized validation errors.
pub fn parse(input: &[u8], max_bytes: u64, max_rows: u64) -> AppResult<RosterPreview> {
    if u64::try_from(input.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err(AppError::PayloadTooLarge);
    }
    let mut preview = RosterPreview {
        valid: true,
        rows: Vec::new(),
        errors: Vec::new(),
    };
    let mut reader = ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(input);
    let mut records = reader.byte_records();
    let Some(header) = records.next() else {
        add_error(&mut preview, Some(1), None, "missing CSV header");
        return Ok(preview);
    };
    let header = match header {
        Ok(record) => record,
        Err(_) => {
            add_error(
                &mut preview,
                Some(1),
                None,
                "CSV header could not be parsed",
            );
            return Ok(preview);
        }
    };
    if !header_matches(&header) {
        add_error(
            &mut preview,
            Some(1),
            None,
            "header must be username,display_name,role,email",
        );
        return Ok(preview);
    }

    let mut usernames = HashSet::new();
    let mut row_count = 0_u64;
    for result in records {
        row_count = row_count.saturating_add(1);
        let fallback_line = row_count.saturating_add(1);
        if row_count > max_rows {
            add_error(
                &mut preview,
                Some(fallback_line),
                None,
                "configured roster row limit exceeded",
            );
            break;
        }
        let record = match result {
            Ok(record) => record,
            Err(_) => {
                add_error(
                    &mut preview,
                    Some(fallback_line),
                    None,
                    "CSV row could not be parsed",
                );
                continue;
            }
        };
        let row_number = record
            .position()
            .map_or(fallback_line, |position| position.line());
        if record.len() != EXPECTED_HEADER.len() {
            add_error(
                &mut preview,
                Some(row_number),
                None,
                "row must contain exactly four columns",
            );
            continue;
        }
        parse_row(&record, row_number, &mut usernames, &mut preview);
    }
    if row_count == 0 {
        add_error(
            &mut preview,
            Some(2),
            None,
            "roster must contain at least one staff row",
        );
    }
    preview.valid = preview.errors.is_empty();
    Ok(preview)
}

/// Adds row-specific conflicts for usernames already present in SQLite.
pub fn add_existing_account_errors(
    connection: &Connection,
    preview: &mut RosterPreview,
) -> AppResult<()> {
    let mut statement = connection.prepare("SELECT username FROM users")?;
    let existing = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<HashSet<_>, _>>()?;
    let conflicts = preview
        .rows
        .iter()
        .filter(|row| existing.contains(&row.username))
        .map(|row| row.row_number)
        .collect::<Vec<_>>();
    for row_number in conflicts {
        add_error(
            preview,
            Some(row_number),
            Some("username"),
            "username already has a local account",
        );
    }
    preview.valid = preview.errors.is_empty();
    Ok(())
}

/// Parses and validates one fixed-width record without echoing rejected values.
fn parse_row(
    record: &ByteRecord,
    row_number: u64,
    usernames: &mut HashSet<String>,
    preview: &mut RosterPreview,
) {
    let fields = ["username", "display_name", "role", "email"];
    let mut decoded = Vec::with_capacity(fields.len());
    for (index, field) in fields.iter().enumerate() {
        match std::str::from_utf8(record.get(index).unwrap_or_default()) {
            Ok(value) => decoded.push(value),
            Err(_) => {
                add_error(
                    preview,
                    Some(row_number),
                    Some(field),
                    "field must contain valid UTF-8 text",
                );
                return;
            }
        }
    }
    let mut formula_error = false;
    for (index, value) in decoded.iter().enumerate() {
        if has_formula_prefix(value) {
            add_error(
                preview,
                Some(row_number),
                Some(fields[index]),
                "spreadsheet formula prefixes are not allowed",
            );
            formula_error = true;
        }
    }
    if formula_error {
        return;
    }

    let username = match normalize_username(decoded[0]) {
        Ok(value) => Some(value),
        Err(_) => {
            add_error(
                preview,
                Some(row_number),
                Some("username"),
                "username is invalid",
            );
            None
        }
    };
    let duplicated = username.as_ref().is_some_and(|value| {
        if usernames.insert(value.clone()) {
            false
        } else {
            add_error(
                preview,
                Some(row_number),
                Some("username"),
                "username is duplicated within the roster",
            );
            true
        }
    });
    let display_name = match validate_display_name(decoded[1]) {
        Ok(value) => Some(value),
        Err(_) => {
            add_error(
                preview,
                Some(row_number),
                Some("display_name"),
                "display name is invalid",
            );
            None
        }
    };
    let role = match Role::from_str(decoded[2].trim()) {
        Ok(value) => Some(value),
        Err(_) => {
            add_error(
                preview,
                Some(row_number),
                Some("role"),
                "role must be requester, technician, or administrator",
            );
            None
        }
    };
    let email = match user::normalize_email(optional(decoded[3])) {
        Ok(value) => Some(value),
        Err(_) => {
            add_error(
                preview,
                Some(row_number),
                Some("email"),
                "email metadata is invalid",
            );
            None
        }
    };
    if duplicated {
        return;
    }
    let (Some(username), Some(display_name), Some(role), Some(email)) =
        (username, display_name, role, email)
    else {
        return;
    };
    preview.rows.push(RosterRow {
        row_number,
        username,
        display_name,
        role,
        email,
    });
}

/// Returns whether one record exactly matches the documented ordered header.
fn header_matches(header: &ByteRecord) -> bool {
    header.len() == EXPECTED_HEADER.len()
        && header
            .iter()
            .zip(EXPECTED_HEADER)
            .all(|(actual, expected)| actual == expected)
}

/// Rejects cell prefixes that spreadsheet applications interpret as formulas.
fn has_formula_prefix(value: &str) -> bool {
    value
        .trim_start()
        .as_bytes()
        .first()
        .is_some_and(|prefix| matches!(prefix, b'=' | b'+' | b'-' | b'@'))
}

/// Converts one trimmed empty email cell into absent metadata.
fn optional(value: &str) -> Option<&str> {
    (!value.trim().is_empty()).then_some(value)
}

/// Appends one static sanitized error and marks the preview invalid.
fn add_error(
    preview: &mut RosterPreview,
    row_number: Option<u64>,
    field: Option<&str>,
    message: &str,
) {
    preview.valid = false;
    preview.errors.push(RosterError {
        row_number,
        field: field.map(str::to_string),
        message: message.to_string(),
    });
}
