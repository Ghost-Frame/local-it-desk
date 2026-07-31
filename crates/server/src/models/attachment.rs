//! Attachment metadata and approved parent boundaries.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable attachment parent kinds stored in SQLite and exposed by the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentParentKind {
    /// File attached directly to a ticket.
    Ticket,
    /// File attached to a requester-visible comment.
    PublicComment,
    /// File attached to a staff-only internal note.
    InternalNote,
    /// File attached to an administrator-authored announcement.
    Announcement,
}

/// Typed parent reference that prevents attachment scope confusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentParent {
    /// Direct ticket attachment.
    Ticket(Uuid),
    /// Requester-visible comment attachment.
    PublicComment(Uuid),
    /// Staff-only internal note attachment.
    InternalNote(Uuid),
    /// Announcement attachment.
    Announcement(Uuid),
}

/// Parent identity helpers for persistence and authorization code.
impl AttachmentParent {
    /// Returns the stable parent kind for this reference.
    pub const fn kind(self) -> AttachmentParentKind {
        match self {
            Self::Ticket(_) => AttachmentParentKind::Ticket,
            Self::PublicComment(_) => AttachmentParentKind::PublicComment,
            Self::InternalNote(_) => AttachmentParentKind::InternalNote,
            Self::Announcement(_) => AttachmentParentKind::Announcement,
        }
    }

    /// Returns the referenced parent identifier.
    pub const fn id(self) -> Uuid {
        match self {
            Self::Ticket(id)
            | Self::PublicComment(id)
            | Self::InternalNote(id)
            | Self::Announcement(id) => id,
        }
    }
}

/// Public metadata for a stored attachment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Stable attachment identifier.
    pub id: Uuid,
    /// Approved attachment parent kind.
    pub parent_kind: AttachmentParentKind,
    /// Approved parent identifier.
    pub parent_id: Uuid,
    /// Account that uploaded the file.
    pub uploader_id: Uuid,
    /// Original human-facing filename.
    pub original_name: String,
    /// Randomized filename stored outside the web root.
    pub stored_name: String,
    /// Media type detected by the server.
    pub media_type: String,
    /// Exact uploaded size in bytes.
    pub size_bytes: u64,
    /// Hex-encoded SHA-256 checksum.
    pub sha256: String,
    /// UTC creation timestamp.
    pub created_at: String,
}
