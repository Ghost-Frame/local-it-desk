//! Opaque server-session token primitives.

use base64::Engine;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::Role;

/// Number of random bytes used in a session bearer token.
const SESSION_TOKEN_BYTES: usize = 32;

/// Identity recovered after Plan 02 validates an active persisted session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionIdentity {
    /// Account that owns the session.
    pub user_id: Uuid,
    /// Current role loaded from the account record.
    pub role: Role,
}

/// Generates a high-entropy URL-safe session token for one-time client delivery.
pub fn generate_token() -> String {
    let mut bytes = [0_u8; SESSION_TOKEN_BYTES];
    rand::fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Hashes a raw session token before database storage or lookup.
pub fn hash_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
/// Unit tests for opaque session token primitives.
mod tests {
    use super::{generate_token, hash_token};

    /// Confirms generated bearer material is not its persisted representation.
    #[test]
    fn token_hash_differs_from_raw_token() {
        let token = generate_token();
        assert_ne!(token, hash_token(&token));
        assert_ne!(generate_token(), token);
    }
}
