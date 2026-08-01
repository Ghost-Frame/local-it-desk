//! Staff announcement records, validation, and lifecycle policy.

use std::str::FromStr;

use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// Maximum visible announcement title length.
pub const MAX_ANNOUNCEMENT_TITLE_LENGTH: usize = 160;
/// Maximum retained Markdown source length.
pub const MAX_ANNOUNCEMENT_BODY_LENGTH: usize = 10_000;

/// Supported staff announcement lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnouncementState {
    /// Administrator-only work not yet visible to staff.
    Draft,
    /// Staff-visible announcement in the signed-in feed.
    Published,
    /// Read-only historical announcement removed from the staff feed.
    Archived,
}

/// Stable persistence spelling for announcement states.
impl AnnouncementState {
    /// Returns the exact database and API value for this state.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
            Self::Archived => "archived",
        }
    }
}

/// Strict parser for persisted announcement states.
impl FromStr for AnnouncementState {
    /// Static parse failure returned for unsupported persisted values.
    type Err = &'static str;

    /// Converts one exact database spelling into a typed state.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "draft" => Ok(Self::Draft),
            "published" => Ok(Self::Published),
            "archived" => Ok(Self::Archived),
            _ => Err("unsupported announcement state"),
        }
    }
}

/// One administrator-authored staff bulletin record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Announcement {
    /// Stable announcement identifier.
    pub id: Uuid,
    /// Concise human-facing heading.
    pub title: String,
    /// Plain Markdown source that has not been rendered as HTML.
    pub body: String,
    /// Administrator who created the announcement.
    pub author_id: Uuid,
    /// Current lifecycle state.
    pub state: AnnouncementState,
    /// Whether the announcement sorts ahead of ordinary published items.
    pub is_pinned: bool,
    /// UTC timestamp of first publication.
    pub published_at: Option<String>,
    /// UTC creation timestamp.
    pub created_at: String,
    /// UTC timestamp of the latest mutation.
    pub updated_at: String,
}

/// Lists the complete administrator announcement history newest first.
pub fn list_all(connection: &Connection) -> AppResult<Vec<Announcement>> {
    query_list(
        connection,
        "SELECT id, title, body, author_id, state, is_pinned, published_at,
                created_at, updated_at
         FROM announcements
         ORDER BY created_at DESC, id DESC",
    )
}

/// Lists only published announcements with pinned items first.
pub fn list_published(connection: &Connection) -> AppResult<Vec<Announcement>> {
    query_list(
        connection,
        "SELECT id, title, body, author_id, state, is_pinned, published_at,
                created_at, updated_at
         FROM announcements
         WHERE state = 'published'
         ORDER BY is_pinned DESC, published_at DESC, id DESC",
    )
}

/// Finds one announcement by stable identifier.
pub fn find(connection: &Connection, id: Uuid) -> AppResult<Option<Announcement>> {
    Ok(connection
        .query_row(
            "SELECT id, title, body, author_id, state, is_pinned, published_at,
                    created_at, updated_at
             FROM announcements WHERE id = ?1",
            [id.to_string()],
            decode_announcement,
        )
        .optional()?)
}

/// Trims and bounds a visible announcement title.
pub fn validate_title(value: &str) -> AppResult<String> {
    validate_required_markdown(value, "announcement title", MAX_ANNOUNCEMENT_TITLE_LENGTH)
}

/// Trims and bounds unrendered announcement Markdown source.
pub fn validate_body(value: &str) -> AppResult<String> {
    validate_required_markdown(value, "announcement body", MAX_ANNOUNCEMENT_BODY_LENGTH)
}

/// Returns whether ordinary content edits are allowed for this state.
pub const fn can_edit(state: AnnouncementState) -> bool {
    !matches!(state, AnnouncementState::Archived)
}

/// Returns whether this state can enter the published feed.
pub const fn can_publish(state: AnnouncementState) -> bool {
    matches!(state, AnnouncementState::Draft)
}

/// Returns whether this state can become archived.
pub const fn can_archive(state: AnnouncementState) -> bool {
    !matches!(state, AnnouncementState::Archived)
}

/// Runs one fixed announcement list query.
fn query_list(connection: &Connection, sql: &str) -> AppResult<Vec<Announcement>> {
    let mut statement = connection.prepare(sql)?;
    Ok(statement
        .query_map([], decode_announcement)?
        .collect::<Result<Vec<_>, _>>()?)
}

/// Trims required Markdown text and enforces its character limit.
fn validate_required_markdown(value: &str, field: &str, maximum: usize) -> AppResult<String> {
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

/// Decodes the fixed announcement query column order.
fn decode_announcement(row: &Row<'_>) -> rusqlite::Result<Announcement> {
    let id_text: String = row.get(0)?;
    let author_text: String = row.get(3)?;
    let state_text: String = row.get(4)?;
    Ok(Announcement {
        id: parse_uuid(0, &id_text)?,
        title: row.get(1)?,
        body: row.get(2)?,
        author_id: parse_uuid(3, &author_text)?,
        state: AnnouncementState::from_str(&state_text).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
            )
        })?,
        is_pinned: row.get(5)?,
        published_at: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

/// Parses one UUID text value into a typed SQLite result.
fn parse_uuid(index: usize, value: &str) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}
