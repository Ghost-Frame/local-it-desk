//! Local authentication boundaries and cumulative authorization roles.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Request identity types reserved for the Plan 02 session extractor.
pub mod middleware;
/// Password hashing and account-field validation helpers.
pub mod password;
/// Opaque session token primitives reserved for Plan 02 persistence.
pub mod session;

/// Cumulative authorization roles supported by the help desk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// A staff member who can submit and follow their own tickets.
    Requester,
    /// A support worker who can manage the shared ticket queue.
    Technician,
    /// The operator who can also manage accounts and application settings.
    Administrator,
}

/// Role capability helpers shared by route authorization and domain policy.
impl Role {
    /// Returns the stable database and API spelling for this role.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Requester => "requester",
            Self::Technician => "technician",
            Self::Administrator => "administrator",
        }
    }

    /// Returns whether the role can work tickets submitted by other users.
    pub const fn can_work_tickets(self) -> bool {
        matches!(self, Self::Technician | Self::Administrator)
    }

    /// Returns whether the role can administer the application.
    pub const fn can_administer(self) -> bool {
        matches!(self, Self::Administrator)
    }
}

/// Parses a persisted role without accepting inherited aliases.
impl FromStr for Role {
    /// Static parse failure returned for unsupported persisted role values.
    type Err = &'static str;

    /// Converts one exact database or API spelling into its role.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "requester" => Ok(Self::Requester),
            "technician" => Ok(Self::Technician),
            "administrator" => Ok(Self::Administrator),
            _ => Err("unsupported role"),
        }
    }
}
