//! Contract tests for the offline administrator recovery command.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use local_it_desk_server::auth::Role;
use local_it_desk_server::auth::session;
use local_it_desk_server::db::migrations::run_migrations;
use local_it_desk_server::models::user::{self, NewUser};
use rusqlite::{Connection, OptionalExtension};
use tempfile::TempDir;

/// Path to the Cargo-built recovery binary under test.
fn recovery_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_local-it-desk-admin"))
}

/// Creates one migrated temporary database and returns its owning directory.
fn database() -> (TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("local-it-desk.sqlite3");
    let connection = Connection::open(&path).expect("temporary database");
    run_migrations(&connection).expect("fresh schema");
    drop(connection);
    (directory, path)
}

/// Creates one local account in the supplied database.
fn create_account(path: &Path, username: &str, role: Role) -> user::User {
    let connection = Connection::open(path).expect("open database");
    user::create(
        &connection,
        &NewUser {
            username,
            display_name: "Recovery Target",
            email: None,
            password: "original correct horse battery",
            role,
            must_change_password: false,
        },
    )
    .expect("create account")
}

/// Runs the reset command with a redirected two-line password and confirmation.
fn reset(path: &Path, username: &str, password: &str) -> Output {
    let mut child = Command::new(recovery_binary())
        .args([
            "reset-password",
            "--database",
            path.to_str().expect("UTF-8 database path"),
            "--username",
            username,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn recovery command");
    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(format!("{password}\n{password}\n").as_bytes())
        .expect("write protected password input");
    child.wait_with_output().expect("recovery output")
}

/// Confirms help documents the command, explicit target flags, and input safety.
#[test]
fn help_describes_recovery_contract_without_password_argument() {
    let output = Command::new(recovery_binary())
        .arg("--help")
        .output()
        .expect("help output");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("reset-password"));
    assert!(stdout.contains("standard input"));
    assert!(!stdout.contains("--password"));
}

/// Confirms recovery never creates a missing database as a side effect.
#[test]
fn missing_database_fails_without_creating_a_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("missing.sqlite3");
    let output = reset(&path, "desk.admin", "replacement correct horse");
    assert_eq!(output.status.code(), Some(4));
    assert!(!path.exists());
}

/// Confirms missing, malformed, and non-administrator identities cannot be reset.
#[test]
fn target_resolution_requires_one_exact_normalized_administrator() {
    let (_directory, path) = database();
    create_account(&path, "staff.user", Role::Requester);
    create_account(&path, "desk.admin", Role::Administrator);

    let missing = reset(&path, "missing.user", "replacement correct horse");
    assert_eq!(missing.status.code(), Some(5));
    let non_admin = reset(&path, "staff.user", "replacement correct horse");
    assert_eq!(non_admin.status.code(), Some(5));
    let non_normalized = reset(&path, "Desk.Admin", "replacement correct horse");
    assert_eq!(non_normalized.status.code(), Some(5));
}

/// Confirms recovery is atomic, revokes sessions, forces change, and records an audit entry.
#[test]
fn successful_recovery_updates_only_the_target_administrator() {
    let (_directory, path) = database();
    let administrator = create_account(&path, "desk.admin", Role::Administrator);
    let requester = create_account(&path, "staff.user", Role::Requester);
    let connection = Connection::open(&path).expect("open database");
    let issued = session::create(&connection, administrator.id, 7).expect("active session");
    drop(connection);

    let replacement = "replacement correct horse";
    let output = reset(&path, "desk.admin", replacement);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("desk.admin"));
    assert!(!stdout.contains(replacement));
    assert!(!stderr.contains(replacement));

    let connection = Connection::open(&path).expect("open database");
    let recovered = user::authenticate(&connection, "desk.admin", replacement)
        .expect("replacement password authenticates");
    assert!(recovered.must_change_password);
    assert!(
        user::authenticate(&connection, "desk.admin", "original correct horse battery").is_err()
    );
    assert!(
        user::authenticate(&connection, "staff.user", "original correct horse battery").is_ok()
    );
    assert!(
        session::resolve(&connection, &issued.token)
            .expect("session lookup")
            .is_none()
    );
    let audit_summary: Option<String> = connection
        .query_row(
            "SELECT summary FROM audit_log
             WHERE action = 'account.recovery_password_reset' AND target_id = ?1",
            [administrator.id.to_string()],
            |row| row.get(0),
        )
        .optional()
        .expect("audit lookup");
    assert_eq!(
        audit_summary.as_deref(),
        Some("Offline administrator password recovery completed")
    );
    assert!(!audit_summary.is_some_and(|summary| summary.contains(replacement)));
    let stored_hash: String = connection
        .query_row(
            "SELECT password_hash FROM users WHERE id = ?1",
            [administrator.id.to_string()],
            |row| row.get(0),
        )
        .expect("stored password hash");
    assert!(!stdout.contains(&stored_hash));
    assert!(!stderr.contains(&stored_hash));
    assert_ne!(requester.id, administrator.id);
}

/// Confirms password mismatch and policy failures leave credentials and sessions unchanged.
#[test]
fn invalid_password_input_makes_no_database_changes() {
    let (_directory, path) = database();
    let administrator = create_account(&path, "desk.admin", Role::Administrator);
    let connection = Connection::open(&path).expect("open database");
    let issued = session::create(&connection, administrator.id, 7).expect("active session");
    drop(connection);

    let mut child = Command::new(recovery_binary())
        .args([
            "reset-password",
            "--database",
            path.to_str().expect("UTF-8 database path"),
            "--username",
            "desk.admin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn recovery command");
    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(b"replacement correct horse\ndifferent confirmation value\n")
        .expect("write password input");
    let output = child.wait_with_output().expect("recovery output");
    assert_eq!(output.status.code(), Some(3));

    let connection = Connection::open(&path).expect("open database");
    assert!(
        user::authenticate(&connection, "desk.admin", "original correct horse battery").is_ok()
    );
    assert!(
        session::resolve(&connection, &issued.token)
            .expect("session lookup")
            .is_some()
    );
    let audit_count: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'account.recovery_password_reset'",
            [],
            |row| row.get(0),
        )
        .expect("audit count");
    assert_eq!(audit_count, 0);
}
