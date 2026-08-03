//! Reserved request middleware boundary for later audit integration.

/// Privacy-bounded request audit context.
pub mod audit;
/// Uniform browser hardening applied to API and static-file responses.
pub mod security_headers;
