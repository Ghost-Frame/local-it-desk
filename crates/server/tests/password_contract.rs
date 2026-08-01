//! Contract tests for local credentials and persisted staff accounts.

use local_it_desk_server::auth::Role;
use local_it_desk_server::auth::password::{
    hash_password, normalize_username, validate_display_name, validate_password, verify_password,
};
use local_it_desk_server::db::migrations::run_migrations;
use local_it_desk_server::error::AppError;
use local_it_desk_server::models::user::{self, NewUser};
use rusqlite::Connection;

/// Opens one migrated in-memory database for account contract checks.
fn database() -> Connection {
    let connection = Connection::open_in_memory().expect("in-memory database");
    run_migrations(&connection).expect("fresh schema");
    connection
}

/// Builds a valid requester account request with overridable identity fields.
fn requester<'a>(username: &'a str, display_name: &'a str, password: &'a str) -> NewUser<'a> {
    NewUser {
        username,
        display_name,
        email: None,
        password,
        role: Role::Requester,
        must_change_password: false,
    }
}

/// Confirms usernames normalize case but reject every character outside the contract.
#[test]
fn username_normalization_and_grammar_are_exact() {
    assert_eq!(
        normalize_username("  Teacher.One  ").expect("valid username"),
        "teacher.one"
    );
    for invalid in [
        "ab",
        "a".repeat(33).as_str(),
        "staff member",
        "teacher@school",
        "téacher",
        "teacher/name",
    ] {
        assert!(normalize_username(invalid).is_err(), "{invalid:?} must fail");
    }
}

/// Confirms display names accept Unicode while rejecting empty and oversized values.
#[test]
fn display_name_validation_is_unicode_aware() {
    assert_eq!(
        validate_display_name("  Renée O'Connor  ").expect("valid display name"),
        "Renée O'Connor"
    );
    assert!(validate_display_name(" ").is_err());
    assert!(validate_display_name(&"é".repeat(81)).is_err());
}

/// Confirms passphrases permit spaces and enforce the documented character bounds.
#[test]
fn passphrase_policy_accepts_spaces_at_twelve_characters() {
    validate_password("twelve chars!").expect("twelve-character passphrase");
    assert!(validate_password("short words").is_err());
    assert!(validate_password(&"x".repeat(257)).is_err());
}

/// Confirms password storage uses Argon2id and malformed hashes fail closed.
#[test]
fn argon2id_hashes_verify_without_panicking() {
    let hash = hash_password("correct horse battery staple").expect("password hash");
    assert!(hash.starts_with("$argon2id$"));
    assert!(verify_password("correct horse battery staple", &hash));
    assert!(!verify_password("wrong horse battery staple", &hash));
    assert!(!verify_password("correct horse battery staple", "not-a-password-hash"));
}

/// Confirms persisted row decoding exposes account state but never credential material.
#[test]
fn account_creation_and_lookup_round_trip() {
    let connection = database();
    let created = user::create(
        &connection,
        &requester("Teacher.One", "Renée O'Connor", "correct horse battery staple"),
    )
    .expect("account creation");
    let loaded = user::find_by_id(&connection, created.id)
        .expect("account lookup")
        .expect("created account");

    assert_eq!(loaded.username, "teacher.one");
    assert_eq!(loaded.display_name, "Renée O'Connor");
    assert_eq!(loaded.role, Role::Requester);
    assert!(loaded.is_active);
    assert!(!loaded.must_change_password);
    let serialized = serde_json::to_string(&loaded).expect("public account JSON");
    assert!(!serialized.contains("password_hash"));
    assert!(!serialized.contains("correct horse"));
}

/// Confirms normalized duplicate usernames are rejected without corrupting the first row.
#[test]
fn normalized_duplicate_usernames_conflict() {
    let connection = database();
    user::create(
        &connection,
        &requester("teacher.one", "First Teacher", "correct horse battery staple"),
    )
    .expect("first account");
    let error = user::create(
        &connection,
        &requester("TEACHER.ONE", "Second Teacher", "another correct horse battery"),
    )
    .expect_err("duplicate account must fail");
    assert!(matches!(error, AppError::Conflict(_)));
}

/// Confirms missing, disabled, malformed-hash, and mismatched accounts share one failure.
#[test]
fn authentication_failures_are_uniform() {
    let connection = database();
    let account = user::create(
        &connection,
        &requester("teacher.one", "First Teacher", "correct horse battery staple"),
    )
    .expect("account creation");

    for result in [
        user::authenticate(&connection, "missing.user", "correct horse battery staple"),
        user::authenticate(&connection, "teacher.one", "wrong horse battery staple"),
    ] {
        assert!(matches!(result, Err(AppError::Unauthorized)));
    }

    connection
        .execute(
            "UPDATE users SET is_active = 0 WHERE id = ?1",
            [account.id.to_string()],
        )
        .expect("disable account");
    assert!(matches!(
        user::authenticate(&connection, "teacher.one", "correct horse battery staple"),
        Err(AppError::Unauthorized)
    ));

    connection
        .execute(
            "UPDATE users SET is_active = 1, password_hash = 'malformed' WHERE id = ?1",
            [account.id.to_string()],
        )
        .expect("replace hash");
    assert!(matches!(
        user::authenticate(&connection, "teacher.one", "correct horse battery staple"),
        Err(AppError::Unauthorized)
    ));
}
