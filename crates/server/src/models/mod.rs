//! Help-desk domain models retained by the reduced product.

/// Staff announcement records, validation, and lifecycle policy.
pub mod announcement;
/// Attachment ownership and metadata contracts.
pub mod attachment;
/// Administrative audit record contracts.
pub mod audit;
/// Bounded staff roster CSV validation contracts.
pub mod roster;
/// Typed runtime settings and category persistence contracts.
pub mod settings;
/// Ticket workflow, comment visibility, and access policies.
pub mod ticket;
/// Local account record contracts.
pub mod user;
