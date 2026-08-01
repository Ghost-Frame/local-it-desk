//! Streamed attachment storage with parent-scoped authorization and safe downloads.

use std::path::{Path as FilePath, PathBuf};
use std::str::FromStr;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::StatusCode;
use axum::http::header::{
    CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, HeaderName, HeaderValue,
};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{SecondsFormat, Utc};
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use super::AppState;
use crate::auth::middleware::AuthenticatedUser;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::attachment::{Attachment, AttachmentParentKind};
use crate::models::audit::{self, NewAuditEntry};
use crate::models::ticket::TicketStatus;

/// Multipart allowance beyond the configured file bytes for bounded form metadata.
const MULTIPART_OVERHEAD_BYTES: u64 = 64 * 1024;
/// Maximum original filename length retained as display metadata.
const MAX_FILENAME_LENGTH: usize = 255;
/// X-Content-Type-Options header name used on all attachment downloads.
const X_CONTENT_TYPE_OPTIONS: HeaderName = HeaderName::from_static("x-content-type-options");

/// Streamed upload metadata collected before one transactional persistence step.
struct PreparedUpload {
    /// Validated human-facing filename.
    original_name: String,
    /// Temporary file written inside the attachment volume.
    temporary_path: PathBuf,
    /// Server-detected allowlisted media type.
    media_type: String,
    /// Safe extension paired with the detected media type.
    extension: String,
    /// Exact bytes written to temporary storage.
    size_bytes: u64,
    /// Hex-encoded SHA-256 of the stored bytes.
    sha256: String,
}

/// Authorized parent information resolved inside the persistence transaction.
struct ParentAccess {
    /// Parent kind confirmed against persisted state.
    kind: AttachmentParentKind,
    /// Stable parent identifier.
    id: Uuid,
    /// Owning ticket when aggregate ticket limits apply.
    ticket_id: Option<Uuid>,
}

/// Mounts upload, download, and ticket attachment listing boundaries.
pub fn router(max_upload_bytes: u64) -> Router<AppState> {
    let multipart_limit = max_upload_bytes
        .saturating_add(MULTIPART_OVERHEAD_BYTES)
        .min(usize::MAX as u64) as usize;
    Router::new()
        .route("/api/attachments", post(upload_attachment))
        .route("/api/attachments/{id}", get(download_attachment))
        .route(
            "/api/tickets/{id}/attachments",
            get(list_ticket_attachments),
        )
        .route_layer(DefaultBodyLimit::max(multipart_limit))
}

/// Streams, validates, authorizes, and atomically persists one attachment.
async fn upload_attachment(
    State(state): State<AppState>,
    identity: AuthenticatedUser,
    multipart: Multipart,
) -> AppResult<(StatusCode, Json<Attachment>)> {
    let temporary_path = state
        .config
        .upload_dir
        .join(format!(".upload-{}.part", Uuid::new_v4()));
    let received =
        receive_multipart(multipart, &temporary_path, state.config.max_upload_bytes).await;
    let (parent_kind, parent_id, prepared) = match received {
        Ok(value) => value,
        Err(error) => {
            remove_if_present(&temporary_path);
            return Err(error);
        }
    };
    let upload_dir = state.config.upload_dir.clone();
    let max_ticket_upload_bytes = state.config.max_ticket_upload_bytes;
    let cleanup_path = prepared.temporary_path.clone();
    let result = db::interact(&state.pool, move |connection| {
        persist_attachment(
            connection,
            identity,
            parent_kind,
            parent_id,
            prepared,
            &upload_dir,
            max_ticket_upload_bytes,
        )
    })
    .await;
    match result {
        Ok(attachment) => Ok((StatusCode::CREATED, Json(attachment))),
        Err(error) => {
            remove_if_present(&cleanup_path);
            Err(error)
        }
    }
}

/// Downloads one authorized attachment with non-inline defensive headers.
async fn download_attachment(
    State(state): State<AppState>,
    identity: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> AppResult<Response> {
    let attachment = db::interact(&state.pool, move |connection| {
        let attachment = find_attachment(connection, id)?.ok_or(AppError::NotFound)?;
        authorize_parent(
            connection,
            identity,
            attachment.parent_kind,
            attachment.parent_id,
            false,
        )?;
        Ok(attachment)
    })
    .await?;
    let path = safe_stored_path(&state.config.upload_dir, &attachment.stored_name)?;
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => AppError::NotFound,
            _ => AppError::Io(error),
        })?;
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&attachment.media_type)
            .map_err(|_| AppError::Internal("stored media type was invalid".to_string()))?,
    );
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&attachment_disposition(&attachment.original_name))
            .map_err(|_| AppError::Internal("generated download header was invalid".to_string()))?,
    );
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
        .headers_mut()
        .insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

/// Lists attachment metadata visible through one authorized parent ticket.
async fn list_ticket_attachments(
    State(state): State<AppState>,
    identity: AuthenticatedUser,
    Path(ticket_id): Path<Uuid>,
) -> AppResult<Json<Vec<Attachment>>> {
    let attachments = db::interact(&state.pool, move |connection| {
        authorize_ticket(connection, identity, ticket_id, false)?;
        let sql = if identity.can_work_tickets() {
            "SELECT a.id, a.parent_kind,
                    COALESCE(a.ticket_id, a.comment_id, a.announcement_id),
                    a.uploader_id, a.original_name, a.stored_name, a.media_type,
                    a.size_bytes, a.sha256, a.created_at
             FROM attachments a
             LEFT JOIN ticket_comments c ON c.id = a.comment_id
             WHERE a.ticket_id = ?1 OR c.ticket_id = ?1
             ORDER BY a.created_at, a.id"
        } else {
            "SELECT a.id, a.parent_kind,
                    COALESCE(a.ticket_id, a.comment_id, a.announcement_id),
                    a.uploader_id, a.original_name, a.stored_name, a.media_type,
                    a.size_bytes, a.sha256, a.created_at
             FROM attachments a
             LEFT JOIN ticket_comments c ON c.id = a.comment_id
             WHERE (a.ticket_id = ?1 OR c.ticket_id = ?1)
               AND a.parent_kind IN ('ticket', 'public_comment')
             ORDER BY a.created_at, a.id"
        };
        let mut statement = connection.prepare(sql)?;
        let attachments = statement
            .query_map([ticket_id.to_string()], decode_attachment)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(attachments)
    })
    .await?;
    Ok(Json(attachments))
}

/// Parses multipart fields while streaming file bytes to bounded temporary storage.
async fn receive_multipart(
    mut multipart: Multipart,
    temporary_path: &FilePath,
    max_upload_bytes: u64,
) -> AppResult<(AttachmentParentKind, Uuid, PreparedUpload)> {
    let mut parent_kind = None;
    let mut parent_id = None;
    let mut prepared = None;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::BadRequest("invalid multipart form".to_string()))?
    {
        match field.name() {
            Some("parent_kind") if parent_kind.is_none() => {
                let value = field
                    .text()
                    .await
                    .map_err(|_| AppError::BadRequest("invalid parent_kind field".to_string()))?;
                parent_kind =
                    Some(AttachmentParentKind::from_str(value.trim()).map_err(|_| {
                        AppError::BadRequest("invalid attachment parent".to_string())
                    })?);
            }
            Some("parent_id") if parent_id.is_none() => {
                let value = field
                    .text()
                    .await
                    .map_err(|_| AppError::BadRequest("invalid parent_id field".to_string()))?;
                parent_id =
                    Some(Uuid::parse_str(value.trim()).map_err(|_| {
                        AppError::BadRequest("invalid attachment parent".to_string())
                    })?);
            }
            Some("file") if prepared.is_none() => {
                let original_name =
                    validate_filename(field.file_name().ok_or_else(|| {
                        AppError::BadRequest("file name is required".to_string())
                    })?)?;
                let mut file = tokio::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(temporary_path)
                    .await?;
                let mut hasher = Sha256::new();
                let mut size_bytes = 0_u64;
                let mut prefix = Vec::with_capacity(8192);
                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|_| AppError::BadRequest("invalid file stream".to_string()))?
                {
                    size_bytes = size_bytes
                        .checked_add(chunk.len() as u64)
                        .ok_or(AppError::PayloadTooLarge)?;
                    if size_bytes > max_upload_bytes {
                        return Err(AppError::PayloadTooLarge);
                    }
                    let remaining = 8192_usize.saturating_sub(prefix.len());
                    prefix.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
                    hasher.update(&chunk);
                    file.write_all(&chunk).await?;
                }
                file.flush().await?;
                if size_bytes == 0 {
                    return Err(AppError::BadRequest("file must not be empty".to_string()));
                }
                let (media_type, extension) = detect_media(&prefix, &original_name)?;
                prepared = Some(PreparedUpload {
                    original_name,
                    temporary_path: temporary_path.to_path_buf(),
                    media_type: media_type.to_string(),
                    extension: extension.to_string(),
                    size_bytes,
                    sha256: format!("{:x}", hasher.finalize()),
                });
            }
            _ => {
                return Err(AppError::BadRequest(
                    "multipart fields are missing, duplicated, or unsupported".to_string(),
                ));
            }
        }
    }
    Ok((
        parent_kind.ok_or_else(|| AppError::BadRequest("parent_kind is required".to_string()))?,
        parent_id.ok_or_else(|| AppError::BadRequest("parent_id is required".to_string()))?,
        prepared.ok_or_else(|| AppError::BadRequest("file is required".to_string()))?,
    ))
}

/// Persists metadata and renames temporary bytes within one immediate transaction.
fn persist_attachment(
    connection: &mut Connection,
    identity: AuthenticatedUser,
    parent_kind: AttachmentParentKind,
    parent_id: Uuid,
    prepared: PreparedUpload,
    upload_dir: &FilePath,
    max_ticket_upload_bytes: u64,
) -> AppResult<Attachment> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let parent = authorize_parent(&transaction, identity, parent_kind, parent_id, true)?;
    if let Some(ticket_id) = parent.ticket_id {
        enforce_ticket_aggregate_limit(
            &transaction,
            ticket_id,
            prepared.size_bytes,
            max_ticket_upload_bytes,
        )?;
    }
    let id = Uuid::new_v4();
    let stored_name = format!("{id}.{}", prepared.extension);
    let final_path = upload_dir.join(&stored_name);
    if final_path.exists() {
        return Err(AppError::Internal(
            "generated attachment path already exists".to_string(),
        ));
    }
    let now = timestamp();
    let (ticket_id, comment_id, announcement_id) = parent_columns(parent.kind, parent.id);
    let attachment = Attachment {
        id,
        parent_kind: parent.kind,
        parent_id: parent.id,
        uploader_id: identity.user_id,
        original_name: prepared.original_name.clone(),
        stored_name: stored_name.clone(),
        media_type: prepared.media_type.clone(),
        size_bytes: prepared.size_bytes,
        sha256: prepared.sha256.clone(),
        created_at: now.clone(),
    };
    transaction.execute(
        "INSERT INTO attachments (
             id, ticket_id, comment_id, announcement_id, parent_kind, uploader_id,
             original_name, stored_name, media_type, size_bytes, sha256, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            id.to_string(),
            ticket_id.map(|value| value.to_string()),
            comment_id.map(|value| value.to_string()),
            announcement_id.map(|value| value.to_string()),
            parent.kind.as_str(),
            identity.user_id.to_string(),
            prepared.original_name,
            stored_name,
            prepared.media_type,
            prepared.size_bytes,
            prepared.sha256,
            now,
        ],
    )?;
    let target_id = id.to_string();
    audit::record(
        &transaction,
        &NewAuditEntry {
            actor_id: Some(identity.user_id),
            action: "attachment.created",
            target_type: "attachment",
            target_id: Some(&target_id),
            summary: "Added a verified help-desk attachment",
            source_address: None,
            created_at: &now,
        },
    )?;
    std::fs::rename(&prepared.temporary_path, &final_path)?;
    if let Err(error) = transaction.commit() {
        remove_if_present(&final_path);
        return Err(AppError::Database(error));
    }
    Ok(attachment)
}

/// Resolves a parent reference and enforces its ticket or announcement boundary.
fn authorize_parent(
    connection: &Connection,
    identity: AuthenticatedUser,
    kind: AttachmentParentKind,
    id: Uuid,
    write: bool,
) -> AppResult<ParentAccess> {
    match kind {
        AttachmentParentKind::Ticket => {
            authorize_ticket(connection, identity, id, write)?;
            Ok(ParentAccess {
                kind,
                id,
                ticket_id: Some(id),
            })
        }
        AttachmentParentKind::PublicComment | AttachmentParentKind::InternalNote => {
            let mut statement = connection.prepare(
                "SELECT c.ticket_id, c.visibility
                 FROM ticket_comments c WHERE c.id = ?1",
            )?;
            let mut rows = statement.query([id.to_string()])?;
            let Some(row) = rows.next()? else {
                return Err(AppError::NotFound);
            };
            let ticket_id = parse_uuid(row, 0)?;
            let visibility: String = row.get(1)?;
            let expected = match kind {
                AttachmentParentKind::PublicComment => "public",
                AttachmentParentKind::InternalNote => "internal",
                _ => unreachable!("comment kinds handled above"),
            };
            if visibility != expected {
                return Err(AppError::NotFound);
            }
            if kind == AttachmentParentKind::InternalNote && !identity.can_work_tickets() {
                return Err(AppError::NotFound);
            }
            authorize_ticket(connection, identity, ticket_id, write)?;
            Ok(ParentAccess {
                kind,
                id,
                ticket_id: Some(ticket_id),
            })
        }
        AttachmentParentKind::Announcement => {
            let state = connection
                .query_row(
                    "SELECT state FROM announcements WHERE id = ?1",
                    [id.to_string()],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or(AppError::NotFound)?;
            if write && !identity.can_administer() {
                return Err(AppError::Forbidden);
            }
            if !write && state != "published" && !identity.can_administer() {
                return Err(AppError::NotFound);
            }
            Ok(ParentAccess {
                kind,
                id,
                ticket_id: None,
            })
        }
    }
}

/// Enforces ticket ownership and closed-ticket immutability.
fn authorize_ticket(
    connection: &Connection,
    identity: AuthenticatedUser,
    ticket_id: Uuid,
    write: bool,
) -> AppResult<TicketStatus> {
    let result = connection
        .query_row(
            "SELECT requester_id, status FROM tickets WHERE id = ?1",
            [ticket_id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or(AppError::NotFound)?;
    let requester_id = Uuid::parse_str(&result.0)
        .map_err(|error| AppError::Internal(format!("invalid requester UUID: {error}")))?;
    if !identity.can_work_tickets() && identity.user_id != requester_id {
        return Err(AppError::NotFound);
    }
    let status = TicketStatus::from_str(&result.1)
        .map_err(|_| AppError::Internal("invalid persisted ticket status".to_string()))?;
    if write && status == TicketStatus::Closed {
        return Err(AppError::Conflict(
            "closed tickets are read-only".to_string(),
        ));
    }
    Ok(status)
}

/// Rejects one upload when it would exceed the owning ticket's aggregate limit.
fn enforce_ticket_aggregate_limit(
    connection: &Connection,
    ticket_id: Uuid,
    incoming_bytes: u64,
    maximum_bytes: u64,
) -> AppResult<()> {
    let existing = connection.query_row(
        "SELECT COALESCE(SUM(a.size_bytes), 0)
         FROM attachments a
         LEFT JOIN ticket_comments c ON c.id = a.comment_id
         WHERE a.ticket_id = ?1 OR c.ticket_id = ?1",
        [ticket_id.to_string()],
        |row| row.get::<_, u64>(0),
    )?;
    if existing
        .checked_add(incoming_bytes)
        .is_none_or(|total| total > maximum_bytes)
    {
        return Err(AppError::PayloadTooLarge);
    }
    Ok(())
}

/// Converts a typed parent into the schema's exactly-one-reference columns.
fn parent_columns(
    kind: AttachmentParentKind,
    id: Uuid,
) -> (Option<Uuid>, Option<Uuid>, Option<Uuid>) {
    match kind {
        AttachmentParentKind::Ticket => (Some(id), None, None),
        AttachmentParentKind::PublicComment | AttachmentParentKind::InternalNote => {
            (None, Some(id), None)
        }
        AttachmentParentKind::Announcement => (None, None, Some(id)),
    }
}

/// Finds one attachment by stable identifier.
fn find_attachment(connection: &Connection, id: Uuid) -> AppResult<Option<Attachment>> {
    let mut statement = connection.prepare(
        "SELECT id, parent_kind, COALESCE(ticket_id, comment_id, announcement_id),
                uploader_id, original_name, stored_name, media_type, size_bytes,
                sha256, created_at
         FROM attachments WHERE id = ?1",
    )?;
    let mut rows = statement.query([id.to_string()])?;
    rows.next()?
        .map(decode_attachment)
        .transpose()
        .map_err(Into::into)
}

/// Decodes the fixed attachment column order into typed metadata.
fn decode_attachment(row: &Row<'_>) -> rusqlite::Result<Attachment> {
    let kind_text: String = row.get(1)?;
    let parent_kind = AttachmentParentKind::from_str(&kind_text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
        )
    })?;
    Ok(Attachment {
        id: parse_uuid(row, 0)?,
        parent_kind,
        parent_id: parse_uuid(row, 2)?,
        uploader_id: parse_uuid(row, 3)?,
        original_name: row.get(4)?,
        stored_name: row.get(5)?,
        media_type: row.get(6)?,
        size_bytes: row.get(7)?,
        sha256: row.get(8)?,
        created_at: row.get(9)?,
    })
}

/// Parses one required UUID text column.
fn parse_uuid(row: &Row<'_>, index: usize) -> rusqlite::Result<Uuid> {
    let value: String = row.get(index)?;
    Uuid::parse_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

/// Detects an allowlisted media type and enforces its exact filename extension.
fn detect_media(prefix: &[u8], original_name: &str) -> AppResult<(&'static str, &'static str)> {
    let extension = FilePath::new(original_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| AppError::UnsupportedMediaType("file extension is required".to_string()))?;
    let detected = infer::get(prefix)
        .map(|kind| kind.mime_type())
        .ok_or_else(|| AppError::UnsupportedMediaType("file content is not allowed".to_string()))?;
    let allowed_extensions: &[&'static str] = match detected {
        "image/png" => &["png"],
        "image/jpeg" => &["jpg", "jpeg"],
        "image/gif" => &["gif"],
        "image/webp" => &["webp"],
        "application/pdf" => &["pdf"],
        _ => {
            return Err(AppError::UnsupportedMediaType(
                "file content is not allowed".to_string(),
            ));
        }
    };
    let stable_extension = allowed_extensions
        .iter()
        .copied()
        .find(|candidate| *candidate == extension)
        .ok_or_else(|| {
            AppError::UnsupportedMediaType(
                "filename extension does not match detected content".to_string(),
            )
        })?;
    Ok((detected, stable_extension))
}

/// Validates display-only filenames and rejects traversal or control characters.
fn validate_filename(value: &str) -> AppResult<String> {
    let value = value.trim();
    let length = value.chars().count();
    if !(1..=MAX_FILENAME_LENGTH).contains(&length)
        || matches!(value, "." | "..")
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return Err(AppError::BadRequest(
            "invalid attachment filename".to_string(),
        ));
    }
    Ok(value.to_string())
}

/// Reconstructs one generated stored path without accepting path components.
fn safe_stored_path(upload_dir: &FilePath, stored_name: &str) -> AppResult<PathBuf> {
    if stored_name.is_empty()
        || stored_name
            .chars()
            .any(|character| matches!(character, '/' | '\\') || character.is_control())
    {
        return Err(AppError::Internal(
            "stored attachment path was invalid".to_string(),
        ));
    }
    Ok(upload_dir.join(stored_name))
}

/// Builds an attachment-only Content-Disposition with UTF-8 filename support.
fn attachment_disposition(original_name: &str) -> String {
    let ascii_name = original_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | ' ') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!(
        "attachment; filename=\"{ascii_name}\"; filename*=UTF-8''{}",
        percent_encode(original_name.as_bytes())
    )
}

/// Percent-encodes arbitrary UTF-8 bytes for a standards-compatible header parameter.
fn percent_encode(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len());
    for byte in bytes {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'-' | b'_') {
            encoded.push(char::from(*byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// Removes one known temporary or reconciled final file when it still exists.
fn remove_if_present(path: &FilePath) {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::error!(error = %error, "failed to clean attachment file");
    }
}

/// Returns one UTC timestamp in the schema's stable text representation.
fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
