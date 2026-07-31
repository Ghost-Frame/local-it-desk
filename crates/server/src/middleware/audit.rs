//! Request metadata permitted in later audit records.

use uuid::Uuid;

/// Non-sensitive context collected for one auditable request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditContext {
    /// Authenticated actor when one was resolved.
    pub actor_id: Option<Uuid>,
    /// Peer network address when deployment policy permits recording it.
    pub source_address: Option<String>,
}
