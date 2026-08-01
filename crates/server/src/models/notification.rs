//! Private in-app notification records and transactional event recipient rules.

use std::collections::HashSet;
use std::str::FromStr;

use rusqlite::types::Type;
use rusqlite::{Connection, Row, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::ticket::{Ticket, TicketStatus};

/// Exact event types retained by the notification table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    /// Confirmation returned to a requester after ticket submission.
    TicketCreated,
    /// New unassigned work sent to active support accounts.
    NewTicket,
    /// Public reply sent to the opposite ticket participant.
    TicketComment,
    /// General ticket lifecycle change sent to the opposite participant.
    TicketStatusChanged,
    /// Resolved work sent to the requester.
    TicketResolved,
    /// Reopened work sent to the assigned or available support account.
    TicketReopened,
    /// Published staff bulletin sent to active non-author accounts.
    AnnouncementPublished,
}

/// Stable persistence spelling for notification kinds.
impl NotificationKind {
    /// Returns the exact database and API spelling for this event kind.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TicketCreated => "ticket_created",
            Self::NewTicket => "new_ticket",
            Self::TicketComment => "ticket_comment",
            Self::TicketStatusChanged => "ticket_status_changed",
            Self::TicketResolved => "ticket_resolved",
            Self::TicketReopened => "ticket_reopened",
            Self::AnnouncementPublished => "announcement_published",
        }
    }
}

/// Strict parser for persisted notification kinds.
impl FromStr for NotificationKind {
    /// Static parse failure returned for unsupported persisted values.
    type Err = &'static str;

    /// Converts one exact database spelling into a typed event kind.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "ticket_created" => Ok(Self::TicketCreated),
            "new_ticket" => Ok(Self::NewTicket),
            "ticket_comment" => Ok(Self::TicketComment),
            "ticket_status_changed" => Ok(Self::TicketStatusChanged),
            "ticket_resolved" => Ok(Self::TicketResolved),
            "ticket_reopened" => Ok(Self::TicketReopened),
            "announcement_published" => Ok(Self::AnnouncementPublished),
            _ => Err("unsupported notification kind"),
        }
    }
}

/// One private notification owned by exactly one local account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    /// Stable notification identifier.
    pub id: Uuid,
    /// Account allowed to read or mutate this record.
    pub user_id: Uuid,
    /// Typed source event.
    pub kind: NotificationKind,
    /// Short generic event heading.
    pub title: String,
    /// Bounded event summary without ticket or announcement content.
    pub body: String,
    /// Same-origin application path containing the stable resource identifier.
    pub target_path: Option<String>,
    /// UTC creation timestamp.
    pub created_at: String,
    /// UTC first-read timestamp.
    pub read_at: Option<String>,
}

/// Non-secret payload shared across one event's deduplicated recipients.
struct EventPayload<'a> {
    /// Typed source event.
    kind: NotificationKind,
    /// Short generic event heading.
    title: &'a str,
    /// Bounded event summary without submitted content.
    body: String,
    /// Same-origin application path containing the resource identifier.
    target_path: String,
    /// UTC timestamp shared with the triggering transaction.
    created_at: &'a str,
}

/// Lists one account's newest one hundred notifications.
pub fn list_for_user(connection: &Connection, user_id: Uuid) -> AppResult<Vec<Notification>> {
    let mut statement = connection.prepare(
        "SELECT id, user_id, kind, title, body, target_path, created_at, read_at
         FROM notifications
         WHERE user_id = ?1
         ORDER BY created_at DESC, rowid DESC
         LIMIT 100",
    )?;
    Ok(statement
        .query_map([user_id.to_string()], decode_notification)?
        .collect::<Result<Vec<_>, _>>()?)
}

/// Counts unread records owned by one account.
pub fn unread_count(connection: &Connection, user_id: Uuid) -> AppResult<u64> {
    Ok(connection.query_row(
        "SELECT COUNT(*) FROM notifications WHERE user_id = ?1 AND read_at IS NULL",
        [user_id.to_string()],
        |row| row.get(0),
    )?)
}

/// Idempotently marks one owned notification read and hides foreign identifiers.
pub fn mark_read(
    connection: &Connection,
    user_id: Uuid,
    notification_id: Uuid,
    read_at: &str,
) -> AppResult<()> {
    let updated = connection.execute(
        "UPDATE notifications
         SET read_at = COALESCE(read_at, ?1)
         WHERE id = ?2 AND user_id = ?3",
        params![read_at, notification_id.to_string(), user_id.to_string()],
    )?;
    if updated != 1 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Marks every currently unread notification owned by one account read.
pub fn mark_all_read(connection: &Connection, user_id: Uuid, read_at: &str) -> AppResult<u64> {
    Ok(connection.execute(
        "UPDATE notifications SET read_at = ?1 WHERE user_id = ?2 AND read_at IS NULL",
        params![read_at, user_id.to_string()],
    )? as u64)
}

/// Creates requester confirmation and active-support notices for a new ticket.
pub fn ticket_created(
    connection: &Connection,
    ticket: &Ticket,
    actor_id: Uuid,
    created_at: &str,
) -> AppResult<()> {
    let target_path = format!("/tickets/{}", ticket.id);
    create_for_users(
        connection,
        [ticket.requester_id],
        &EventPayload {
            kind: NotificationKind::TicketCreated,
            title: "Ticket submitted",
            body: format!("Ticket #{} was submitted successfully.", ticket.number),
            target_path: target_path.clone(),
            created_at,
        },
    )?;
    let recipients = active_support_ids(connection, Some(actor_id))?;
    create_for_users(
        connection,
        recipients,
        &EventPayload {
            kind: NotificationKind::NewTicket,
            title: "New ticket",
            body: format!("Ticket #{} needs attention.", ticket.number),
            target_path,
            created_at,
        },
    )
}

/// Creates an opposite-side notice for one public ticket comment.
pub fn public_ticket_comment(
    connection: &Connection,
    ticket: &Ticket,
    actor_id: Uuid,
    created_at: &str,
) -> AppResult<()> {
    let recipients = if actor_id == ticket.requester_id {
        support_recipients(connection, ticket.assignee_id, actor_id)?
    } else {
        vec![ticket.requester_id]
    };
    create_for_users(
        connection,
        recipients,
        &EventPayload {
            kind: NotificationKind::TicketComment,
            title: "New ticket reply",
            body: format!("Ticket #{} has a new public reply.", ticket.number),
            target_path: format!("/tickets/{}", ticket.id),
            created_at,
        },
    )
}

/// Creates an opposite-side notice for one real ticket status transition.
pub fn ticket_status_changed(
    connection: &Connection,
    previous: &Ticket,
    updated: &Ticket,
    actor_id: Uuid,
    created_at: &str,
) -> AppResult<()> {
    if previous.status == updated.status {
        return Ok(());
    }
    let (kind, title, body) = match updated.status {
        TicketStatus::Resolved => (
            NotificationKind::TicketResolved,
            "Ticket resolved",
            format!("Ticket #{} was marked resolved.", updated.number),
        ),
        TicketStatus::Open
            if matches!(
                previous.status,
                TicketStatus::Resolved | TicketStatus::Closed
            ) =>
        {
            (
                NotificationKind::TicketReopened,
                "Ticket reopened",
                format!("Ticket #{} was reopened.", updated.number),
            )
        }
        _ => (
            NotificationKind::TicketStatusChanged,
            "Ticket status changed",
            format!(
                "Ticket #{} is now {}.",
                updated.number,
                readable_status(updated.status)
            ),
        ),
    };
    let recipients = if actor_id == updated.requester_id {
        support_recipients(connection, updated.assignee_id, actor_id)?
    } else {
        vec![updated.requester_id]
    };
    create_for_users(
        connection,
        recipients,
        &EventPayload {
            kind,
            title,
            body,
            target_path: format!("/tickets/{}", updated.id),
            created_at,
        },
    )
}

/// Creates one bulletin notice for every active account except its author.
pub fn announcement_published(
    connection: &Connection,
    announcement_id: Uuid,
    actor_id: Uuid,
    created_at: &str,
) -> AppResult<()> {
    let recipients = active_user_ids(connection, Some(actor_id))?;
    create_for_users(
        connection,
        recipients,
        &EventPayload {
            kind: NotificationKind::AnnouncementPublished,
            title: "New staff announcement",
            body: "A staff announcement was published.".to_string(),
            target_path: format!("/announcements/{announcement_id}"),
            created_at,
        },
    )
}

/// Inserts at most one active-account record per event recipient.
fn create_for_users<I>(
    connection: &Connection,
    recipients: I,
    payload: &EventPayload<'_>,
) -> AppResult<()>
where
    I: IntoIterator<Item = Uuid>,
{
    let mut unique = HashSet::new();
    for user_id in recipients {
        if !unique.insert(user_id) {
            continue;
        }
        connection.execute(
            "INSERT INTO notifications (
                 id, user_id, kind, title, body, target_path, created_at, read_at
             )
             SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL
             WHERE EXISTS(SELECT 1 FROM users WHERE id = ?2 AND is_active = 1)",
            params![
                Uuid::new_v4().to_string(),
                user_id.to_string(),
                payload.kind.as_str(),
                payload.title,
                payload.body,
                payload.target_path,
                payload.created_at,
            ],
        )?;
    }
    Ok(())
}

/// Returns active support recipients, preferring one active assignee.
fn support_recipients(
    connection: &Connection,
    assignee_id: Option<Uuid>,
    excluding_id: Uuid,
) -> AppResult<Vec<Uuid>> {
    if let Some(assignee_id) = assignee_id {
        let active = connection.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM users
                 WHERE id = ?1 AND is_active = 1
                   AND role IN ('technician', 'administrator')
             )",
            [assignee_id.to_string()],
            |row| row.get::<_, bool>(0),
        )?;
        if active && assignee_id != excluding_id {
            return Ok(vec![assignee_id]);
        }
    }
    active_support_ids(connection, Some(excluding_id))
}

/// Lists active technician and administrator identifiers with one exclusion.
fn active_support_ids(connection: &Connection, excluding_id: Option<Uuid>) -> AppResult<Vec<Uuid>> {
    query_active_user_ids(
        connection,
        "role IN ('technician', 'administrator')",
        excluding_id,
    )
}

/// Lists every active account identifier with one exclusion.
fn active_user_ids(connection: &Connection, excluding_id: Option<Uuid>) -> AppResult<Vec<Uuid>> {
    query_active_user_ids(connection, "1 = 1", excluding_id)
}

/// Runs one internal active-recipient query and parses stable identifiers.
fn query_active_user_ids(
    connection: &Connection,
    role_predicate: &str,
    excluding_id: Option<Uuid>,
) -> AppResult<Vec<Uuid>> {
    let sql = format!(
        "SELECT id FROM users
         WHERE is_active = 1 AND {role_predicate}
           AND (?1 IS NULL OR id <> ?1)
         ORDER BY id"
    );
    let excluding = excluding_id.map(|value| value.to_string());
    let mut statement = connection.prepare(&sql)?;
    let rows = statement
        .query_map([excluding], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|value| {
            Uuid::parse_str(&value)
                .map_err(|_| AppError::Internal("persisted user identifier is invalid".to_string()))
        })
        .collect()
}

/// Returns a human-facing status phrase without ticket content.
fn readable_status(status: TicketStatus) -> &'static str {
    match status {
        TicketStatus::New => "new",
        TicketStatus::Open => "open",
        TicketStatus::WaitingOnRequester => "waiting on requester",
        TicketStatus::Resolved => "resolved",
        TicketStatus::Closed => "closed",
    }
}

/// Decodes the fixed notification query column order.
fn decode_notification(row: &Row<'_>) -> rusqlite::Result<Notification> {
    let id_text: String = row.get(0)?;
    let user_text: String = row.get(1)?;
    let kind_text: String = row.get(2)?;
    Ok(Notification {
        id: parse_uuid(0, &id_text)?,
        user_id: parse_uuid(1, &user_text)?,
        kind: NotificationKind::from_str(&kind_text).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
            )
        })?,
        title: row.get(3)?,
        body: row.get(4)?,
        target_path: row.get(5)?,
        created_at: row.get(6)?,
        read_at: row.get(7)?,
    })
}

/// Parses one UUID text value into a typed SQLite result.
fn parse_uuid(index: usize, value: &str) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}
