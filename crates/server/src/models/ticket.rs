//! Ticket workflow vocabulary and authorization-independent domain policy.

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::Role;

/// Supported lifecycle states for a help-desk ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    /// Newly submitted and not yet triaged.
    New,
    /// Accepted into the active support queue.
    Open,
    /// Waiting for more information or action from the requester.
    WaitingOnRequester,
    /// Work is complete but the ticket may still be reopened.
    Resolved,
    /// Administratively closed and read-only.
    Closed,
}

/// String conversion for persisted ticket states.
impl TicketStatus {
    /// Returns the stable database and API spelling for this status.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Open => "open",
            Self::WaitingOnRequester => "waiting_on_requester",
            Self::Resolved => "resolved",
            Self::Closed => "closed",
        }
    }
}

/// Strictly parses a ticket state without inherited aliases.
impl FromStr for TicketStatus {
    /// Static parse failure returned for unsupported persisted ticket states.
    type Err = &'static str;

    /// Converts one exact database or API spelling into its ticket state.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "new" => Ok(Self::New),
            "open" => Ok(Self::Open),
            "waiting_on_requester" => Ok(Self::WaitingOnRequester),
            "resolved" => Ok(Self::Resolved),
            "closed" => Ok(Self::Closed),
            _ => Err("unsupported ticket status"),
        }
    }
}

/// Supported urgency levels for a help-desk ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketPriority {
    /// Work that can safely wait behind routine requests.
    Low,
    /// Default priority for ordinary support requests.
    Normal,
    /// Time-sensitive work with material staff impact.
    High,
    /// Immediate work with broad or critical operational impact.
    Urgent,
}

/// String conversion for persisted priority values.
impl TicketPriority {
    /// Returns the stable database and API spelling for this priority.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Urgent => "urgent",
        }
    }
}

/// Strictly parses a ticket priority without inherited aliases.
impl FromStr for TicketPriority {
    /// Static parse failure returned for unsupported persisted priorities.
    type Err = &'static str;

    /// Converts one exact database or API spelling into its priority.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "low" => Ok(Self::Low),
            "normal" => Ok(Self::Normal),
            "high" => Ok(Self::High),
            "urgent" => Ok(Self::Urgent),
            _ => Err("unsupported ticket priority"),
        }
    }
}

/// Visibility boundary for one ticket conversation entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentVisibility {
    /// Visible to the requester and support staff.
    Public,
    /// Visible only to technicians and administrators.
    Internal,
}

/// Stable public ticket record shared by persistence and API layers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ticket {
    /// Stable ticket identifier.
    pub id: Uuid,
    /// Human-readable sequential ticket number.
    pub number: u64,
    /// Concise requester-supplied summary.
    pub title: String,
    /// Detailed requester-supplied problem description.
    pub description: String,
    /// Account that submitted the ticket.
    pub requester_id: Uuid,
    /// Technician currently responsible for the ticket.
    pub assignee_id: Option<Uuid>,
    /// Current lifecycle state.
    pub status: TicketStatus,
    /// Current operator-assigned priority.
    pub priority: TicketPriority,
}

/// Centralized pure policy used by later persistence and route implementations.
pub struct TicketPolicy;

/// Role and lifecycle checks for ticket operations.
impl TicketPolicy {
    /// Returns whether an actor can see a ticket owned by the given requester.
    pub const fn can_view(role: Role, actor_id: Uuid, requester_id: Uuid) -> bool {
        role.can_work_tickets() || uuid_equal(actor_id, requester_id)
    }

    /// Returns whether a role can see the selected comment visibility.
    pub const fn can_view_comment(role: Role, visibility: CommentVisibility) -> bool {
        matches!(visibility, CommentVisibility::Public) || role.can_work_tickets()
    }

    /// Returns whether ordinary writes are permitted in the current lifecycle state.
    pub const fn can_write(_role: Role, status: TicketStatus) -> bool {
        !matches!(status, TicketStatus::Closed)
    }

    /// Returns whether an actor can reopen the current state.
    pub const fn can_reopen(role: Role, status: TicketStatus, is_requester: bool) -> bool {
        match status {
            TicketStatus::Resolved => role.can_administer() || is_requester,
            TicketStatus::Closed => role.can_administer(),
            _ => false,
        }
    }
}

/// Compares UUIDs in a const-compatible form.
const fn uuid_equal(left: Uuid, right: Uuid) -> bool {
    left.as_u128() == right.as_u128()
}
