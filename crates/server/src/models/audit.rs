//! Privacy-bounded administrative audit record.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Persisted audit entry that excludes credentials and complete ticket bodies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Stable audit entry identifier.
    pub id: Uuid,
    /// Account responsible for the action when it still exists.
    pub actor_id: Option<Uuid>,
    /// Stable machine-readable action name.
    pub action: String,
    /// Kind of entity affected by the action.
    pub target_type: String,
    /// Affected entity identifier when applicable.
    pub target_id: Option<String>,
    /// Short non-sensitive action summary.
    pub summary: String,
    /// Source network address when policy permits recording it.
    pub source_address: Option<String>,
    /// UTC creation timestamp.
    pub created_at: String,
}
