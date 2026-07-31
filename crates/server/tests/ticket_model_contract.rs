//! Pure domain contract for roles, tickets, comments, and attachment parents.

use std::str::FromStr;

use uuid::Uuid;

use local_it_desk_server::auth::Role;
use local_it_desk_server::models::attachment::{AttachmentParent, AttachmentParentKind};
use local_it_desk_server::models::ticket::{
    CommentVisibility, TicketPolicy, TicketPriority, TicketStatus,
};

/// Confirms only the approved status and priority vocabularies are accepted.
#[test]
fn status_and_priority_vocabularies_are_exact() {
    for status in ["new", "open", "waiting_on_requester", "resolved", "closed"] {
        assert!(TicketStatus::from_str(status).is_ok(), "{status}");
    }
    for excluded in ["in_progress", "pending", "cancelled"] {
        assert!(TicketStatus::from_str(excluded).is_err(), "{excluded}");
    }

    for priority in ["low", "normal", "high", "urgent"] {
        assert!(TicketPriority::from_str(priority).is_ok(), "{priority}");
    }
    for excluded in ["medium", "critical", "emergency"] {
        assert!(TicketPriority::from_str(excluded).is_err(), "{excluded}");
    }
}

/// Confirms requester ownership and cumulative staff access rules.
#[test]
fn ticket_visibility_uses_role_and_requester_ownership() {
    let requester = Uuid::new_v4();
    let stranger = Uuid::new_v4();

    assert!(TicketPolicy::can_view(
        Role::Requester,
        requester,
        requester
    ));
    assert!(!TicketPolicy::can_view(
        Role::Requester,
        stranger,
        requester
    ));
    assert!(TicketPolicy::can_view(
        Role::Technician,
        stranger,
        requester
    ));
    assert!(TicketPolicy::can_view(
        Role::Administrator,
        stranger,
        requester
    ));
    assert!(Role::Administrator.can_administer());
    assert!(Role::Administrator.can_work_tickets());
    assert!(Role::Technician.can_work_tickets());
    assert!(!Role::Requester.can_work_tickets());
}

/// Confirms internal notes never become requester-visible ticket comments.
#[test]
fn internal_note_visibility_is_staff_only() {
    assert!(TicketPolicy::can_view_comment(
        Role::Requester,
        CommentVisibility::Public,
    ));
    assert!(!TicketPolicy::can_view_comment(
        Role::Requester,
        CommentVisibility::Internal,
    ));
    assert!(TicketPolicy::can_view_comment(
        Role::Technician,
        CommentVisibility::Internal,
    ));
    assert!(TicketPolicy::can_view_comment(
        Role::Administrator,
        CommentVisibility::Internal,
    ));
}

/// Confirms closed tickets reject writes until an administrator reopens them.
#[test]
fn closed_ticket_write_protection_is_explicit() {
    assert!(!TicketPolicy::can_write(
        Role::Administrator,
        TicketStatus::Closed,
    ));
    assert!(TicketPolicy::can_reopen(
        Role::Administrator,
        TicketStatus::Closed,
        false,
    ));
    assert!(!TicketPolicy::can_reopen(
        Role::Technician,
        TicketStatus::Closed,
        false,
    ));
    assert!(TicketPolicy::can_reopen(
        Role::Requester,
        TicketStatus::Resolved,
        true,
    ));
    assert!(!TicketPolicy::can_reopen(
        Role::Requester,
        TicketStatus::Resolved,
        false,
    ));
}

/// Confirms attachments can reference only approved help-desk parents.
#[test]
fn attachment_parent_vocabulary_is_exact() {
    let id = Uuid::new_v4();
    let parents = [
        (AttachmentParent::Ticket(id), AttachmentParentKind::Ticket),
        (
            AttachmentParent::PublicComment(id),
            AttachmentParentKind::PublicComment,
        ),
        (
            AttachmentParent::InternalNote(id),
            AttachmentParentKind::InternalNote,
        ),
        (
            AttachmentParent::Announcement(id),
            AttachmentParentKind::Announcement,
        ),
    ];

    for (parent, expected_kind) in parents {
        assert_eq!(parent.kind(), expected_kind);
        assert_eq!(parent.id(), id);
    }
}
