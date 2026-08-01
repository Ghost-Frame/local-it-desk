//! Password hashing and local account-field validation.

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use base64::Engine;

use crate::error::{AppError, AppResult};

/// Normalizes and validates a case-insensitive login username.
pub fn normalize_username(username: &str) -> AppResult<String> {
    let normalized = username.trim().to_ascii_lowercase();
    let valid_length = (3..=32).contains(&normalized.len());
    let valid_characters = normalized
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte));
    if !valid_length || !valid_characters {
        return Err(AppError::BadRequest(
            "username must be 3-32 characters using letters, numbers, dot, dash, or underscore"
                .to_string(),
        ));
    }
    Ok(normalized)
}

/// Validates and trims a human-facing display name.
pub fn validate_display_name(display_name: &str) -> AppResult<String> {
    let trimmed = display_name.trim();
    if !(2..=80).contains(&trimmed.chars().count()) {
        return Err(AppError::BadRequest(
            "display name must be 2-80 characters".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

/// Enforces the passphrase-oriented local password policy.
pub fn validate_password(password: &str) -> AppResult<()> {
    let length = password.chars().count();
    if !(12..=256).contains(&length) {
        return Err(AppError::BadRequest(
            "password must contain between 12 and 256 characters".to_string(),
        ));
    }
    Ok(())
}

/// Hashes a validated password with Argon2id and a fresh random salt.
pub fn hash_password(password: &str) -> AppResult<String> {
    validate_password(password)?;
    let mut salt_bytes = [0_u8; 16];
    rand::fill(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|error| AppError::Internal(format!("salt encoding failed: {error}")))?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| AppError::Internal(format!("password hashing failed: {error}")))
}

/// Verifies a password against one stored Argon2id string.
pub fn verify_password(password: &str, password_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(password_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// Generates a high-entropy temporary passphrase suitable for one-time delivery.
pub fn generate_temporary_password() -> String {
    let mut bytes = [0_u8; 24];
    rand::fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
/// Unit tests for password and username primitives.
mod tests {
    use super::{hash_password, normalize_username, verify_password};

    /// Confirms usernames normalize to the approved lowercase alphabet.
    #[test]
    fn username_normalization_is_case_insensitive() {
        assert_eq!(
            normalize_username(" Teacher.One ").expect("valid username"),
            "teacher.one"
        );
        assert!(normalize_username("bad name").is_err());
    }

    /// Confirms Argon2 hashes accept only the original passphrase.
    #[test]
    fn password_hash_round_trip() {
        let hash = hash_password("correct horse battery staple").expect("password hash");
        assert!(verify_password("correct horse battery staple", &hash));
        assert!(!verify_password("incorrect horse battery staple", &hash));
    }
}
