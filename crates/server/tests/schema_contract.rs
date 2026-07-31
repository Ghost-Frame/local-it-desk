//! Exact SQLite schema contract for a fresh Local IT Desk database.

use std::collections::BTreeSet;

use rusqlite::Connection;

use local_it_desk_server::db::migrations::{SCHEMA_VERSION, run_migrations};

/// Returns user-defined schema object names for one SQLite object type.
fn schema_names(connection: &Connection, object_type: &str) -> BTreeSet<String> {
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_schema
             WHERE type = ?1 AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )
        .expect("schema query");
    statement
        .query_map([object_type], |row| row.get::<_, String>(0))
        .expect("schema rows")
        .map(|row| row.expect("schema name"))
        .collect()
}

/// Confirms a blank file migrates to the exact approved table and index inventory.
#[test]
fn fresh_database_matches_exact_schema() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let database_path = temp.path().join("desk.db");
    let connection = Connection::open(&database_path).expect("database");

    run_migrations(&connection).expect("first migration");
    run_migrations(&connection).expect("idempotent migration");

    let expected_tables = BTreeSet::from([
        "announcements".to_string(),
        "attachments".to_string(),
        "audit_log".to_string(),
        "categories".to_string(),
        "notifications".to_string(),
        "schema_version".to_string(),
        "sessions".to_string(),
        "settings".to_string(),
        "ticket_comments".to_string(),
        "tickets".to_string(),
        "users".to_string(),
    ]);
    assert_eq!(schema_names(&connection, "table"), expected_tables);

    let expected_indexes = BTreeSet::from([
        "idx_attachments_announcement".to_string(),
        "idx_attachments_comment".to_string(),
        "idx_attachments_ticket".to_string(),
        "idx_audit_log_created".to_string(),
        "idx_notifications_user_unread".to_string(),
        "idx_sessions_expiry".to_string(),
        "idx_sessions_user".to_string(),
        "idx_ticket_comments_ticket".to_string(),
        "idx_tickets_assignee".to_string(),
        "idx_tickets_requester".to_string(),
        "idx_tickets_status".to_string(),
        "idx_tickets_updated".to_string(),
    ]);
    assert_eq!(schema_names(&connection, "index"), expected_indexes);

    let version: u32 = connection
        .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
        .expect("schema version");
    assert_eq!(version, SCHEMA_VERSION);

    let foreign_keys: u32 = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .expect("foreign key pragma");
    assert_eq!(foreign_keys, 1);

    let journal_mode: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("journal mode");
    assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
}

/// Confirms a fresh database contains no product or operator records.
#[test]
fn fresh_database_contains_no_runtime_records() {
    let connection = Connection::open_in_memory().expect("database");
    run_migrations(&connection).expect("migration");

    for table in ["users", "tickets", "announcements", "settings"] {
        let count: u32 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("row count");
        assert_eq!(count, 0, "{table} must start empty");
    }
}
