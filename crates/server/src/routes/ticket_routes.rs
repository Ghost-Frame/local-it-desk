//! Authenticated requester tickets and the shared staff support queue.

use std::str::FromStr;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::types::{Type, Value};
use rusqlite::{Connection, Row, TransactionBehavior, params, params_from_iter};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use super::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::audit::{self, NewAuditEntry};
use crate::models::ticket::{
    CommentVisibility, Ticket, TicketComment, TicketPolicy, TicketPriority, TicketStatus,
};

/// Default ticket page size when a client does not provide one.
const DEFAULT_PAGE_SIZE: u64 = 25;
/// Largest ticket page accepted by the server.
const MAX_PAGE_SIZE: u64 = 100;
/// Maximum title length stored in one ticket.
const MAX_TITLE_LENGTH: usize = 160;
/// Maximum description length stored in one ticket.
const MAX_DESCRIPTION_LENGTH: usize = 10_000;
/// Maximum body length stored in one ticket comment.
const MAX_COMMENT_LENGTH: usize = 5_000;
/// Maximum free-text search length accepted by the queue.
const MAX_SEARCH_LENGTH: usize = 100;
/// Maximum encoded continuation cursor length accepted by the queue.
const MAX_CURSOR_LENGTH: usize = 256;

/// Request body accepted when a named staff member submits a ticket.
#[derive(Debug, Deserialize)]
struct CreateTicketRequest {
    /// Concise problem summary.
    title: String,
    /// Detailed problem description.
    description: String,
    /// Active administrator-configured category.
    category_id: Uuid,
    /// Optional initial priority, defaulting to normal.
    priority: Option<TicketPriority>,
}

/// Optional ticket mutations accepted from staff or the owning requester.
#[derive(Debug, Deserialize)]
struct UpdateTicketRequest {
    /// Replacement lifecycle state.
    status: Option<TicketStatus>,
    /// Replacement staff-controlled priority.
    priority: Option<TicketPriority>,
    /// Replacement staff-controlled assignee.
    #[serde(default)]
    assignee_id: AssignmentChange,
    /// Replacement active category.
    category_id: Option<Uuid>,
    /// Client-observed timestamp used to reject stale updates.
    expected_updated_at: Option<String>,
}

/// Three-state assignment mutation that distinguishes omission from JSON null.
#[derive(Debug, Default, Clone, Copy)]
enum AssignmentChange {
    /// Preserve the current assignment.
    #[default]
    Unchanged,
    /// Assign the ticket to one active support account.
    Set(Uuid),
    /// Remove the current assignment.
    Clear,
}

/// Decodes a present UUID or null while serde default handles an omitted field.
impl<'de> Deserialize<'de> for AssignmentChange {
    /// Converts JSON null to clear and a UUID string to set.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<Uuid>::deserialize(deserializer).map(|value| value.map_or(Self::Clear, Self::Set))
    }
}

/// Assignment mutation helpers used by ticket update validation.
impl AssignmentChange {
    /// Returns whether the request explicitly changed assignment.
    const fn is_changed(&self) -> bool {
        !matches!(self, Self::Unchanged)
    }

    /// Resolves this mutation against the currently persisted assignment.
    const fn resolve(&self, current: Option<Uuid>) -> Option<Uuid> {
        match self {
            Self::Unchanged => current,
            Self::Set(id) => Some(*id),
            Self::Clear => None,
        }
    }
}

/// Request body accepted for one public comment or internal note.
#[derive(Debug, Deserialize)]
struct CreateCommentRequest {
    /// Plain-text conversation body.
    body: String,
    /// Requester-visible or staff-only visibility.
    visibility: CommentVisibility,
}

/// Bounded queue filters and stable cursor pagination inputs.
#[derive(Debug, Deserialize)]
struct TicketQuery {
    /// Exact lifecycle status filter.
    status: Option<TicketStatus>,
    /// Exact priority filter.
    priority: Option<TicketPriority>,
    /// Exact category filter.
    category_id: Option<Uuid>,
    /// Exact assignee filter.
    assignee_id: Option<Uuid>,
    /// Case-insensitive title and description search.
    search: Option<String>,
    /// Opaque continuation cursor from a preceding page.
    cursor: Option<String>,
    /// Requested result count.
    page_size: Option<u64>,
}

/// One stable newest-first ticket page.
#[derive(Debug, Serialize)]
struct TicketPage {
    /// Tickets visible to the current account.
    items: Vec<Ticket>,
    /// Opaque cursor for the next page when additional rows exist.
    next_cursor: Option<String>,
}

/// Decoded stable cursor fields used only to build a bounded query.
struct TicketCursor {
    /// UTC update timestamp of the last returned row.
    updated_at: String,
    /// Stable identifier of the last returned row.
    id: Uuid,
}

/// Mounts ticket collection, detail, and comment boundaries.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/tickets", get(list_tickets).post(create_ticket))
        .route("/api/tickets/{id}", get(get_ticket).patch(update_ticket))
        .route(
            "/api/tickets/{id}/comments",
            get(list_comments).post(create_comment),
        )
}

/// Creates one requester-owned ticket and its privacy-bounded audit event.
async fn create_ticket(
    State(state): State<AppState>,
    identity: AuthenticatedUser,
    Json(request): Json<CreateTicketRequest>,
) -> AppResult<(StatusCode, Json<Ticket>)> {
    let title = bounded_text(request.title, "title", 3, MAX_TITLE_LENGTH)?;
    let description = bounded_text(
        request.description,
        "description",
        10,
        MAX_DESCRIPTION_LENGTH,
    )?;
    let priority = request.priority.unwrap_or(TicketPriority::Normal);
    let ticket = db::interact(&state.pool, move |connection| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_active_category(&transaction, request.category_id)?;
        let id = Uuid::new_v4();
        let number = transaction.query_row(
            "SELECT COALESCE(MAX(number), 0) + 1 FROM tickets",
            [],
            |row| row.get::<_, u64>(0),
        )?;
        let now = timestamp();
        transaction.execute(
            "INSERT INTO tickets (
                 id, number, title, description, requester_id, assignee_id,
                 category_id, status, priority, created_at, updated_at,
                 resolved_at, closed_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, 'new', ?7, ?8, ?8, NULL, NULL)",
            params![
                id.to_string(),
                number,
                title,
                description,
                identity.user_id.to_string(),
                request.category_id.to_string(),
                priority.as_str(),
                now,
            ],
        )?;
        record_ticket_audit(
            &transaction,
            identity.user_id,
            "ticket.created",
            id,
            "Created a help-desk ticket",
            &now,
        )?;
        let ticket = find_ticket(&transaction, id)?.ok_or(AppError::NotFound)?;
        transaction.commit()?;
        Ok(ticket)
    })
    .await?;
    Ok((StatusCode::CREATED, Json(ticket)))
}

/// Lists one access-controlled, filtered, newest-first ticket page.
async fn list_tickets(
    State(state): State<AppState>,
    identity: AuthenticatedUser,
    Query(query): Query<TicketQuery>,
) -> AppResult<Json<TicketPage>> {
    let page_size = query.page_size.unwrap_or(DEFAULT_PAGE_SIZE);
    if !(1..=MAX_PAGE_SIZE).contains(&page_size) {
        return Err(AppError::BadRequest(
            "page_size must be between 1 and 100".to_string(),
        ));
    }
    let search = query
        .search
        .as_ref()
        .map(|value| bounded_text(value.clone(), "search", 1, MAX_SEARCH_LENGTH))
        .transpose()?;
    let cursor = query
        .cursor
        .as_ref()
        .map(|value| decode_cursor(value))
        .transpose()?;
    let page = db::interact(&state.pool, move |connection| {
        query_tickets(connection, identity, query, search, cursor, page_size)
    })
    .await?;
    Ok(Json(page))
}

/// Returns one ticket when it is visible to the current account.
async fn get_ticket(
    State(state): State<AppState>,
    identity: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Ticket>> {
    let ticket = db::interact(&state.pool, move |connection| {
        visible_ticket(connection, identity, id)
    })
    .await?;
    Ok(Json(ticket))
}

/// Applies one authorized lifecycle, assignment, priority, or category change.
async fn update_ticket(
    State(state): State<AppState>,
    identity: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateTicketRequest>,
) -> AppResult<Json<Ticket>> {
    let ticket = db::interact(&state.pool, move |connection| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = visible_ticket(&transaction, identity, id)?;
        if request
            .expected_updated_at
            .as_ref()
            .is_some_and(|expected| expected != &current.updated_at)
        {
            return Err(AppError::Conflict(
                "ticket changed since it was loaded".to_string(),
            ));
        }
        validate_ticket_update(&transaction, identity, &current, &request)?;
        let next_status = request.status.unwrap_or(current.status);
        let next_priority = request.priority.unwrap_or(current.priority);
        let next_assignee = request.assignee_id.resolve(current.assignee_id);
        let next_category = request.category_id.or(current.category_id);
        let now = next_timestamp(&current.updated_at);
        let resolved_at = match next_status {
            TicketStatus::Resolved => Some(current.resolved_at.clone().unwrap_or(now.clone())),
            TicketStatus::Closed => current.resolved_at.clone(),
            _ => None,
        };
        let closed_at = if next_status == TicketStatus::Closed {
            Some(current.closed_at.clone().unwrap_or(now.clone()))
        } else {
            None
        };
        transaction.execute(
            "UPDATE tickets
             SET status = ?1, priority = ?2, assignee_id = ?3, category_id = ?4,
                 updated_at = ?5,
                 resolved_at = ?6,
                 closed_at = ?7
             WHERE id = ?8",
            params![
                next_status.as_str(),
                next_priority.as_str(),
                next_assignee.map(|value| value.to_string()),
                next_category.map(|value| value.to_string()),
                now,
                resolved_at,
                closed_at,
                id.to_string(),
            ],
        )?;
        let (action, summary) = if next_status != current.status {
            (
                "ticket.status_changed",
                format!(
                    "Changed ticket status from {} to {}",
                    current.status.as_str(),
                    next_status.as_str()
                ),
            )
        } else {
            (
                "ticket.updated",
                "Updated ticket assignment, priority, or category".to_string(),
            )
        };
        record_ticket_audit(&transaction, identity.user_id, action, id, &summary, &now)?;
        let updated = find_ticket(&transaction, id)?.ok_or(AppError::NotFound)?;
        transaction.commit()?;
        Ok(updated)
    })
    .await?;
    Ok(Json(ticket))
}

/// Lists comments visible through the parent ticket and comment boundary.
async fn list_comments(
    State(state): State<AppState>,
    identity: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<TicketComment>>> {
    let comments = db::interact(&state.pool, move |connection| {
        visible_ticket(connection, identity, id)?;
        let sql = if identity.can_work_tickets() {
            "SELECT id, ticket_id, author_id, body, visibility, created_at, updated_at
             FROM ticket_comments WHERE ticket_id = ?1 ORDER BY created_at, id"
        } else {
            "SELECT id, ticket_id, author_id, body, visibility, created_at, updated_at
             FROM ticket_comments
             WHERE ticket_id = ?1 AND visibility = 'public' ORDER BY created_at, id"
        };
        let mut statement = connection.prepare(sql)?;
        let comments = statement
            .query_map([id.to_string()], decode_comment)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(comments)
    })
    .await?;
    Ok(Json(comments))
}

/// Adds one authorized public comment or staff-only internal note.
async fn create_comment(
    State(state): State<AppState>,
    identity: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(request): Json<CreateCommentRequest>,
) -> AppResult<(StatusCode, Json<TicketComment>)> {
    let body = bounded_text(request.body, "comment body", 1, MAX_COMMENT_LENGTH)?;
    if request.visibility == CommentVisibility::Internal && !identity.can_work_tickets() {
        return Err(AppError::Forbidden);
    }
    let comment = db::interact(&state.pool, move |connection| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let ticket = visible_ticket(&transaction, identity, id)?;
        if !TicketPolicy::can_write(identity.role, ticket.status) {
            return Err(AppError::Conflict(
                "closed tickets are read-only".to_string(),
            ));
        }
        let comment_id = Uuid::new_v4();
        let now = next_timestamp(&ticket.updated_at);
        transaction.execute(
            "INSERT INTO ticket_comments (
                 id, ticket_id, author_id, body, visibility, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![
                comment_id.to_string(),
                id.to_string(),
                identity.user_id.to_string(),
                body,
                request.visibility.as_str(),
                now,
            ],
        )?;
        transaction.execute(
            "UPDATE tickets SET updated_at = ?1 WHERE id = ?2",
            params![now, id.to_string()],
        )?;
        record_ticket_audit(
            &transaction,
            identity.user_id,
            if request.visibility == CommentVisibility::Internal {
                "ticket.internal_note_added"
            } else {
                "ticket.comment_added"
            },
            id,
            if request.visibility == CommentVisibility::Internal {
                "Added an internal ticket note"
            } else {
                "Added a public ticket comment"
            },
            &now,
        )?;
        let comment = find_comment(&transaction, comment_id)?.ok_or(AppError::NotFound)?;
        transaction.commit()?;
        Ok(comment)
    })
    .await?;
    Ok((StatusCode::CREATED, Json(comment)))
}

/// Builds and executes one role-bounded dynamic ticket list query.
fn query_tickets(
    connection: &Connection,
    identity: AuthenticatedUser,
    query: TicketQuery,
    search: Option<String>,
    cursor: Option<TicketCursor>,
    page_size: u64,
) -> AppResult<TicketPage> {
    let mut sql = String::from(
        "SELECT id, number, title, description, requester_id, assignee_id,
                category_id, status, priority, created_at, updated_at, resolved_at, closed_at
         FROM tickets WHERE 1 = 1",
    );
    let mut values = Vec::<Value>::new();
    if !identity.can_work_tickets() {
        push_filter(
            &mut sql,
            &mut values,
            "requester_id =",
            identity.user_id.to_string(),
        );
    }
    if let Some(status) = query.status {
        push_filter(
            &mut sql,
            &mut values,
            "status =",
            status.as_str().to_string(),
        );
    }
    if let Some(priority) = query.priority {
        push_filter(
            &mut sql,
            &mut values,
            "priority =",
            priority.as_str().to_string(),
        );
    }
    if let Some(category_id) = query.category_id {
        push_filter(
            &mut sql,
            &mut values,
            "category_id =",
            category_id.to_string(),
        );
    }
    if let Some(assignee_id) = query.assignee_id {
        push_filter(
            &mut sql,
            &mut values,
            "assignee_id =",
            assignee_id.to_string(),
        );
    }
    if let Some(search) = search {
        values.push(Value::Text(format!("%{search}%")));
        let index = values.len();
        sql.push_str(&format!(
            " AND (title LIKE ?{index} COLLATE NOCASE OR description LIKE ?{index} COLLATE NOCASE)"
        ));
    }
    if let Some(cursor) = cursor {
        values.push(Value::Text(cursor.updated_at));
        let timestamp_index = values.len();
        values.push(Value::Text(cursor.id.to_string()));
        let id_index = values.len();
        sql.push_str(&format!(
            " AND (updated_at < ?{timestamp_index} OR (updated_at = ?{timestamp_index} AND id < ?{id_index}))"
        ));
    }
    values.push(Value::Integer((page_size + 1) as i64));
    let limit_index = values.len();
    sql.push_str(&format!(
        " ORDER BY updated_at DESC, id DESC LIMIT ?{limit_index}"
    ));
    let mut statement = connection.prepare(&sql)?;
    let mut items = statement
        .query_map(params_from_iter(values), decode_ticket)?
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = items.len() > page_size as usize;
    if has_more {
        items.pop();
    }
    let next_cursor =
        (has_more && !items.is_empty()).then(|| encode_cursor(items.last().expect("checked")));
    Ok(TicketPage { items, next_cursor })
}

/// Adds one indexed equality predicate and its owned SQLite value.
fn push_filter(sql: &mut String, values: &mut Vec<Value>, column: &str, value: String) {
    values.push(Value::Text(value));
    sql.push_str(&format!(" AND {column} ?{}", values.len()));
}

/// Validates a ticket update against role, ownership, lifecycle, and references.
fn validate_ticket_update(
    connection: &Connection,
    identity: AuthenticatedUser,
    current: &Ticket,
    request: &UpdateTicketRequest,
) -> AppResult<()> {
    let has_staff_fields = request.priority.is_some()
        || request.assignee_id.is_changed()
        || request.category_id.is_some();
    if !identity.can_work_tickets() {
        if has_staff_fields
            || request.status != Some(TicketStatus::Open)
            || !TicketPolicy::can_reopen(identity.role, current.status, true)
        {
            return Err(AppError::Forbidden);
        }
        return Ok(());
    }
    if current.status == TicketStatus::Closed
        && !(identity.can_administer()
            && request.status == Some(TicketStatus::Open)
            && !has_staff_fields)
    {
        return Err(AppError::Conflict(
            "closed tickets are read-only".to_string(),
        ));
    }
    if let Some(status) = request.status
        && !TicketPolicy::can_staff_transition(identity.role, current.status, status)
    {
        return Err(AppError::Conflict(
            "unsupported ticket status transition".to_string(),
        ));
    }
    if let Some(category_id) = request.category_id {
        require_active_category(connection, category_id)?;
    }
    if let AssignmentChange::Set(assignee_id) = request.assignee_id {
        require_active_staff(connection, assignee_id)?;
    }
    if request.status.is_none() && !has_staff_fields {
        return Err(AppError::BadRequest(
            "no ticket changes were supplied".to_string(),
        ));
    }
    Ok(())
}

/// Loads one ticket and deliberately hides unauthorized ownership as not found.
fn visible_ticket(
    connection: &Connection,
    identity: AuthenticatedUser,
    id: Uuid,
) -> AppResult<Ticket> {
    let ticket = find_ticket(connection, id)?.ok_or(AppError::NotFound)?;
    if !TicketPolicy::can_view(identity.role, identity.user_id, ticket.requester_id) {
        return Err(AppError::NotFound);
    }
    Ok(ticket)
}

/// Finds one ticket by its stable identifier.
fn find_ticket(connection: &Connection, id: Uuid) -> AppResult<Option<Ticket>> {
    let mut statement = connection.prepare(
        "SELECT id, number, title, description, requester_id, assignee_id,
                category_id, status, priority, created_at, updated_at, resolved_at, closed_at
         FROM tickets WHERE id = ?1",
    )?;
    let mut rows = statement.query([id.to_string()])?;
    rows.next()?
        .map(decode_ticket)
        .transpose()
        .map_err(Into::into)
}

/// Finds one comment by its stable identifier.
fn find_comment(connection: &Connection, id: Uuid) -> AppResult<Option<TicketComment>> {
    let mut statement = connection.prepare(
        "SELECT id, ticket_id, author_id, body, visibility, created_at, updated_at
         FROM ticket_comments WHERE id = ?1",
    )?;
    let mut rows = statement.query([id.to_string()])?;
    rows.next()?
        .map(decode_comment)
        .transpose()
        .map_err(Into::into)
}

/// Decodes the fixed ticket column order into typed API data.
fn decode_ticket(row: &Row<'_>) -> rusqlite::Result<Ticket> {
    Ok(Ticket {
        id: parse_uuid(row, 0)?,
        number: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        requester_id: parse_uuid(row, 4)?,
        assignee_id: parse_optional_uuid(row, 5)?,
        category_id: parse_optional_uuid(row, 6)?,
        status: parse_ticket_status(row, 7)?,
        priority: parse_ticket_priority(row, 8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        resolved_at: row.get(11)?,
        closed_at: row.get(12)?,
    })
}

/// Decodes the fixed comment column order into typed API data.
fn decode_comment(row: &Row<'_>) -> rusqlite::Result<TicketComment> {
    Ok(TicketComment {
        id: parse_uuid(row, 0)?,
        ticket_id: parse_uuid(row, 1)?,
        author_id: parse_uuid(row, 2)?,
        body: row.get(3)?,
        visibility: parse_comment_visibility(row, 4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

/// Parses one required UUID text column.
fn parse_uuid(row: &Row<'_>, index: usize) -> rusqlite::Result<Uuid> {
    let value: String = row.get(index)?;
    Uuid::parse_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

/// Parses one nullable UUID text column.
fn parse_optional_uuid(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<Uuid>> {
    let value: Option<String> = row.get(index)?;
    value
        .map(|value| {
            Uuid::parse_str(&value).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
            })
        })
        .transpose()
}

/// Parses one exact persisted ticket status.
fn parse_ticket_status(row: &Row<'_>, index: usize) -> rusqlite::Result<TicketStatus> {
    let value: String = row.get(index)?;
    TicketStatus::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
        )
    })
}

/// Parses one exact persisted ticket priority.
fn parse_ticket_priority(row: &Row<'_>, index: usize) -> rusqlite::Result<TicketPriority> {
    let value: String = row.get(index)?;
    TicketPriority::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
        )
    })
}

/// Parses one exact persisted comment visibility.
fn parse_comment_visibility(row: &Row<'_>, index: usize) -> rusqlite::Result<CommentVisibility> {
    let value: String = row.get(index)?;
    CommentVisibility::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
        )
    })
}

/// Requires an active category before it can be attached to a ticket.
fn require_active_category(connection: &Connection, id: Uuid) -> AppResult<()> {
    let exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM categories WHERE id = ?1 AND is_active = 1)",
        [id.to_string()],
        |row| row.get::<_, bool>(0),
    )?;
    if !exists {
        return Err(AppError::BadRequest(
            "category does not exist or is inactive".to_string(),
        ));
    }
    Ok(())
}

/// Requires an active technician or administrator before assignment.
fn require_active_staff(connection: &Connection, id: Uuid) -> AppResult<()> {
    let exists = connection.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM users
             WHERE id = ?1 AND is_active = 1 AND role IN ('technician', 'administrator')
         )",
        [id.to_string()],
        |row| row.get::<_, bool>(0),
    )?;
    if !exists {
        return Err(AppError::BadRequest(
            "assignee is not an active support account".to_string(),
        ));
    }
    Ok(())
}

/// Trims and bounds one required plain-text field.
fn bounded_text(value: String, label: &str, minimum: usize, maximum: usize) -> AppResult<String> {
    let value = value.trim().to_string();
    let length = value.chars().count();
    if !(minimum..=maximum).contains(&length) {
        return Err(AppError::BadRequest(format!(
            "{label} must contain between {minimum} and {maximum} characters"
        )));
    }
    Ok(value)
}

/// Encodes the last row's stable sort key as an opaque URL-safe cursor.
fn encode_cursor(ticket: &Ticket) -> String {
    URL_SAFE_NO_PAD.encode(format!("{}\n{}", ticket.updated_at, ticket.id))
}

/// Decodes and validates one opaque stable queue cursor.
fn decode_cursor(value: &str) -> AppResult<TicketCursor> {
    if value.len() > MAX_CURSOR_LENGTH {
        return Err(AppError::BadRequest("invalid ticket cursor".to_string()));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AppError::BadRequest("invalid ticket cursor".to_string()))?;
    let decoded = String::from_utf8(bytes)
        .map_err(|_| AppError::BadRequest("invalid ticket cursor".to_string()))?;
    let (updated_at, id) = decoded
        .split_once('\n')
        .ok_or_else(|| AppError::BadRequest("invalid ticket cursor".to_string()))?;
    let id = Uuid::parse_str(id)
        .map_err(|_| AppError::BadRequest("invalid ticket cursor".to_string()))?;
    if updated_at.is_empty() {
        return Err(AppError::BadRequest("invalid ticket cursor".to_string()));
    }
    Ok(TicketCursor {
        updated_at: updated_at.to_string(),
        id,
    })
}

/// Records one non-sensitive ticket event within the caller's transaction.
fn record_ticket_audit(
    connection: &Connection,
    actor_id: Uuid,
    action: &str,
    ticket_id: Uuid,
    summary: &str,
    created_at: &str,
) -> AppResult<()> {
    let target_id = ticket_id.to_string();
    audit::record(
        connection,
        &NewAuditEntry {
            actor_id: Some(actor_id),
            action,
            target_type: "ticket",
            target_id: Some(&target_id),
            summary,
            source_address: None,
            created_at,
        },
    )?;
    Ok(())
}

/// Returns one UTC timestamp in the schema's stable text representation.
fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Returns a UTC timestamp that is strictly newer than a persisted predecessor.
fn next_timestamp(previous: &str) -> String {
    let now = Utc::now();
    let next = DateTime::parse_from_rfc3339(previous)
        .map(|value| value.with_timezone(&Utc) + Duration::milliseconds(1))
        .map_or(now, |minimum| now.max(minimum));
    next.to_rfc3339_opts(SecondsFormat::Millis, true)
}
