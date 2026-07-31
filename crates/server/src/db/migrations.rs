//! Fresh SQLite schema for the local-only help-desk product.

use rusqlite::Connection;

/// Current schema version for newly created databases.
pub const SCHEMA_VERSION: u32 = 1;

/// Applies the complete version-one schema idempotently.
pub fn run_migrations(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;

         CREATE TABLE IF NOT EXISTS schema_version (
             version INTEGER NOT NULL CHECK (version > 0)
         );

         CREATE TABLE IF NOT EXISTS users (
             id TEXT PRIMARY KEY NOT NULL,
             username TEXT NOT NULL COLLATE NOCASE UNIQUE,
             display_name TEXT NOT NULL,
             email TEXT,
             password_hash TEXT NOT NULL,
             role TEXT NOT NULL CHECK (role IN ('requester', 'technician', 'administrator')),
             is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
             must_change_password INTEGER NOT NULL DEFAULT 1 CHECK (must_change_password IN (0, 1)),
             created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL,
             last_login_at TEXT
         );

         CREATE TABLE IF NOT EXISTS sessions (
             id TEXT PRIMARY KEY NOT NULL,
             user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
             token_hash TEXT NOT NULL UNIQUE,
             csrf_hash TEXT NOT NULL,
             created_at TEXT NOT NULL,
             expires_at TEXT NOT NULL,
             last_seen_at TEXT NOT NULL,
             revoked_at TEXT
         );
         CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
         CREATE INDEX IF NOT EXISTS idx_sessions_expiry ON sessions(expires_at);

         CREATE TABLE IF NOT EXISTS categories (
             id TEXT PRIMARY KEY NOT NULL,
             name TEXT NOT NULL COLLATE NOCASE UNIQUE,
             description TEXT,
             is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
             sort_order INTEGER NOT NULL DEFAULT 0,
             created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL
         );

         CREATE TABLE IF NOT EXISTS tickets (
             id TEXT PRIMARY KEY NOT NULL,
             number INTEGER NOT NULL UNIQUE,
             title TEXT NOT NULL,
             description TEXT NOT NULL,
             requester_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
             assignee_id TEXT REFERENCES users(id) ON DELETE SET NULL,
             category_id TEXT REFERENCES categories(id) ON DELETE SET NULL,
             status TEXT NOT NULL DEFAULT 'new'
                 CHECK (status IN ('new', 'open', 'waiting_on_requester', 'resolved', 'closed')),
             priority TEXT NOT NULL DEFAULT 'normal'
                 CHECK (priority IN ('low', 'normal', 'high', 'urgent')),
             created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL,
             resolved_at TEXT,
             closed_at TEXT
         );
         CREATE INDEX IF NOT EXISTS idx_tickets_requester ON tickets(requester_id);
         CREATE INDEX IF NOT EXISTS idx_tickets_assignee ON tickets(assignee_id);
         CREATE INDEX IF NOT EXISTS idx_tickets_status ON tickets(status);
         CREATE INDEX IF NOT EXISTS idx_tickets_updated ON tickets(updated_at);

         CREATE TABLE IF NOT EXISTS ticket_comments (
             id TEXT PRIMARY KEY NOT NULL,
             ticket_id TEXT NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
             author_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
             body TEXT NOT NULL,
             visibility TEXT NOT NULL CHECK (visibility IN ('public', 'internal')),
             created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_ticket_comments_ticket ON ticket_comments(ticket_id);

         CREATE TABLE IF NOT EXISTS announcements (
             id TEXT PRIMARY KEY NOT NULL,
             title TEXT NOT NULL,
             body TEXT NOT NULL,
             author_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
             state TEXT NOT NULL DEFAULT 'draft' CHECK (state IN ('draft', 'published', 'archived')),
             is_pinned INTEGER NOT NULL DEFAULT 0 CHECK (is_pinned IN (0, 1)),
             published_at TEXT,
             created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL
         );

         CREATE TABLE IF NOT EXISTS attachments (
             id TEXT PRIMARY KEY NOT NULL,
             ticket_id TEXT REFERENCES tickets(id) ON DELETE CASCADE,
             comment_id TEXT REFERENCES ticket_comments(id) ON DELETE CASCADE,
             announcement_id TEXT REFERENCES announcements(id) ON DELETE CASCADE,
             parent_kind TEXT NOT NULL
                 CHECK (parent_kind IN ('ticket', 'public_comment', 'internal_note', 'announcement')),
             uploader_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
             original_name TEXT NOT NULL,
             stored_name TEXT NOT NULL UNIQUE,
             media_type TEXT NOT NULL,
             size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
             sha256 TEXT NOT NULL,
             created_at TEXT NOT NULL,
             CHECK (
                 (CASE WHEN ticket_id IS NOT NULL THEN 1 ELSE 0 END
                  + CASE WHEN comment_id IS NOT NULL THEN 1 ELSE 0 END
                  + CASE WHEN announcement_id IS NOT NULL THEN 1 ELSE 0 END) = 1
             ),
             CHECK (
                 (parent_kind = 'ticket' AND ticket_id IS NOT NULL)
                 OR (parent_kind IN ('public_comment', 'internal_note') AND comment_id IS NOT NULL)
                 OR (parent_kind = 'announcement' AND announcement_id IS NOT NULL)
             )
         );
         CREATE INDEX IF NOT EXISTS idx_attachments_ticket ON attachments(ticket_id);
         CREATE INDEX IF NOT EXISTS idx_attachments_comment ON attachments(comment_id);
         CREATE INDEX IF NOT EXISTS idx_attachments_announcement ON attachments(announcement_id);

         CREATE TABLE IF NOT EXISTS notifications (
             id TEXT PRIMARY KEY NOT NULL,
             user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
             kind TEXT NOT NULL,
             title TEXT NOT NULL,
             body TEXT NOT NULL,
             target_path TEXT,
             created_at TEXT NOT NULL,
             read_at TEXT
         );
         CREATE INDEX IF NOT EXISTS idx_notifications_user_unread
             ON notifications(user_id, read_at);

         CREATE TABLE IF NOT EXISTS settings (
             key TEXT PRIMARY KEY NOT NULL,
             value TEXT NOT NULL,
             updated_by TEXT REFERENCES users(id) ON DELETE SET NULL,
             updated_at TEXT NOT NULL
         );

         CREATE TABLE IF NOT EXISTS audit_log (
             id TEXT PRIMARY KEY NOT NULL,
             actor_id TEXT REFERENCES users(id) ON DELETE SET NULL,
             action TEXT NOT NULL,
             target_type TEXT NOT NULL,
             target_id TEXT,
             summary TEXT NOT NULL,
             source_address TEXT,
             created_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_audit_log_created ON audit_log(created_at);

         INSERT INTO schema_version (version)
         SELECT 1
         WHERE NOT EXISTS (SELECT 1 FROM schema_version);",
    )?;

    let version: u32 =
        connection.query_row("SELECT version FROM schema_version", [], |row| row.get(0))?;
    if version != SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(())
}
