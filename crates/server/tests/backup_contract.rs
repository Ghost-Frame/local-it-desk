//! Contract tests for consistent, allowlisted, self-verifying backup archives.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use chrono::{SecondsFormat, Utc};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use local_it_desk_server::backup::{
    ARCHIVE_FORMAT_VERSION, BackupFile, BackupManifest, DATABASE_ARCHIVE_PATH, MANIFEST_PATH,
};
use local_it_desk_server::db::migrations::{SCHEMA_VERSION, run_migrations};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use tar::{Archive, Builder, EntryType, Header};
use tempfile::TempDir;

/// Fixed referenced attachment name used by the archive fixture.
const ATTACHMENT_NAME: &str = "11111111-1111-4111-8111-111111111111.png";
/// Fixed active branding name used by the archive fixture.
const BRANDING_NAME: &str = "22222222-2222-4222-8222-222222222222.png";
/// Safe raster-like bytes retained as the referenced attachment.
const ATTACHMENT_BYTES: &[u8] = b"\x89PNG\r\n\x1a\nattachment fixture";
/// Safe raster-like bytes retained as the active logo.
const BRANDING_BYTES: &[u8] = b"\x89PNG\r\n\x1a\nbranding fixture";

/// Complete isolated database and storage roots for one backup command.
struct BackupFixture {
    /// Owning temporary root retained for the lifetime of every path.
    _root: TempDir,
    /// Live WAL-mode SQLite database path.
    database: PathBuf,
    /// Explicit attachment storage root.
    attachments: PathBuf,
    /// Explicit branding storage root.
    branding: PathBuf,
    /// New archive path supplied to the command.
    archive: PathBuf,
    /// Open live connection proving online backup behavior.
    connection: Connection,
}

/// Path to the Cargo-built offline administration binary under test.
fn admin_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_local-it-desk-admin"))
}

/// Creates one migrated WAL database with referenced and unrelated files.
fn backup_fixture() -> BackupFixture {
    let root = tempfile::tempdir().expect("temporary backup fixture");
    let database = root.path().join("data/local-it-desk.db");
    let attachments = root.path().join("attachments");
    let branding = root.path().join("branding");
    let archive = root.path().join("backups/desk-backup.tar.gz");
    fs::create_dir_all(database.parent().expect("database parent")).expect("database directory");
    fs::create_dir_all(&attachments).expect("attachment directory");
    fs::create_dir_all(&branding).expect("branding directory");
    fs::create_dir_all(archive.parent().expect("archive parent")).expect("backup directory");

    let connection = Connection::open(&database).expect("fixture database");
    run_migrations(&connection).expect("fixture schema");
    connection
        .execute(
            "INSERT INTO users (
                 id, username, display_name, email, password_hash, role,
                 is_active, must_change_password, created_at, updated_at
             ) VALUES (?1, 'staff.user', 'Staff User', NULL, 'fixture-hash',
                       'requester', 1, 0, ?2, ?2)",
            params!["33333333-3333-4333-8333-333333333333", timestamp()],
        )
        .expect("fixture user");
    connection
        .execute(
            "INSERT INTO tickets (
                 id, number, title, description, requester_id, status,
                 priority, created_at, updated_at
             ) VALUES (?1, 1, 'Printer offline', 'Front office printer', ?2,
                       'new', 'normal', ?3, ?3)",
            params![
                "44444444-4444-4444-8444-444444444444",
                "33333333-3333-4333-8333-333333333333",
                timestamp()
            ],
        )
        .expect("fixture ticket");
    connection
        .execute(
            "INSERT INTO attachments (
                 id, ticket_id, comment_id, announcement_id, parent_kind,
                 uploader_id, original_name, stored_name, media_type,
                 size_bytes, sha256, created_at
             ) VALUES (?1, ?2, NULL, NULL, 'ticket', ?3, 'photo.png', ?4,
                       'image/png', ?5, ?6, ?7)",
            params![
                "55555555-5555-4555-8555-555555555555",
                "44444444-4444-4444-8444-444444444444",
                "33333333-3333-4333-8333-333333333333",
                ATTACHMENT_NAME,
                ATTACHMENT_BYTES.len() as u64,
                sha256(ATTACHMENT_BYTES),
                timestamp()
            ],
        )
        .expect("fixture attachment");
    connection
        .execute(
            "INSERT INTO settings (key, value, updated_by, updated_at)
             VALUES ('logo_stored_name', ?1, NULL, ?2)",
            params![BRANDING_NAME, timestamp()],
        )
        .expect("fixture logo reference");

    fs::write(attachments.join(ATTACHMENT_NAME), ATTACHMENT_BYTES).expect("attachment bytes");
    fs::write(branding.join(BRANDING_NAME), BRANDING_BYTES).expect("branding bytes");
    fs::write(attachments.join(".upload-abandoned.part"), b"partial").expect("partial upload");
    fs::write(attachments.join(".env"), b"SESSION_SECRET=do-not-copy").expect("environment decoy");
    fs::write(branding.join("tls.key"), b"private-key-decoy").expect("key decoy");
    fs::write(branding.join("old-logo.png"), b"unreferenced").expect("old logo decoy");
    fs::write(root.path().join("application.log"), b"log decoy").expect("log decoy");

    BackupFixture {
        _root: root,
        database,
        attachments,
        branding,
        archive,
        connection,
    }
}

/// Returns a stable RFC 3339 timestamp for fixture rows and manifests.
fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Executes the public backup command against one fixture.
fn run_backup(fixture: &BackupFixture) -> Output {
    Command::new(admin_binary())
        .args([
            "backup",
            "--database",
            path_text(&fixture.database),
            "--attachments",
            path_text(&fixture.attachments),
            "--branding",
            path_text(&fixture.branding),
            "--output",
            path_text(&fixture.archive),
        ])
        .output()
        .expect("backup command")
}

/// Executes the public streaming verifier for one explicit archive.
fn run_verify(archive: &Path) -> Output {
    Command::new(admin_binary())
        .args(["verify-backup", "--archive", path_text(archive)])
        .output()
        .expect("verify command")
}

/// Converts a temporary fixture path into one command argument.
fn path_text(path: &Path) -> &str {
    path.to_str().expect("UTF-8 fixture path")
}

/// Reads every regular tar payload for direct archive contract assertions.
fn read_archive(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let decoder = GzDecoder::new(fs::File::open(path).expect("open archive"));
    let mut archive = Archive::new(decoder);
    archive
        .entries()
        .expect("archive entries")
        .map(|entry| {
            let mut entry = entry.expect("valid entry");
            let path = entry
                .path()
                .expect("valid path")
                .to_str()
                .expect("UTF-8 path")
                .to_string();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).expect("entry bytes");
            (path, bytes)
        })
        .collect()
}

/// Produces a valid format-one manifest for one synthetic database payload.
fn synthetic_manifest(schema_version: u32, sha256: String) -> BackupManifest {
    let logical_counts = [
        "announcements",
        "attachments",
        "audit_log",
        "categories",
        "notifications",
        "sessions",
        "settings",
        "ticket_comments",
        "tickets",
        "users",
    ]
    .into_iter()
    .map(|table| (table.to_string(), 0))
    .collect();
    BackupManifest {
        archive_format_version: ARCHIVE_FORMAT_VERSION,
        application_version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version,
        created_at: timestamp(),
        logical_counts,
        files: vec![BackupFile {
            path: DATABASE_ARCHIVE_PATH.to_string(),
            size_bytes: 1,
            sha256,
        }],
    }
}

/// Writes one synthetic archive and can place a raw unsafe path in its payload header.
fn write_synthetic_archive(path: &Path, manifest: &BackupManifest, raw_path: &[u8]) {
    let encoder = GzEncoder::new(
        fs::File::create(path).expect("synthetic archive"),
        Compression::default(),
    );
    let mut archive = Builder::new(encoder);
    let manifest_bytes = serde_json::to_vec(manifest).expect("synthetic manifest");
    append_entry(&mut archive, MANIFEST_PATH.as_bytes(), &manifest_bytes);
    append_entry(&mut archive, raw_path, b"x");
    let encoder = archive.into_inner().expect("finish tar");
    encoder.finish().expect("finish gzip");
}

/// Appends one regular entry after explicitly setting raw header path bytes.
fn append_entry<W: Write>(archive: &mut Builder<W>, raw_path: &[u8], bytes: &[u8]) {
    assert!(raw_path.len() < 100);
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Regular);
    header.set_mode(0o600);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_size(bytes.len() as u64);
    header.as_mut_bytes()[..100].fill(0);
    header.as_mut_bytes()[..raw_path.len()].copy_from_slice(raw_path);
    header.set_cksum();
    archive
        .append(&header, Cursor::new(bytes))
        .expect("append synthetic entry");
}

/// Returns one lowercase SHA-256 digest for fixture bytes.
fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Requires the public CLI to expose the approved backup command surface.
#[test]
fn help_exposes_backup_and_verification_commands() {
    let output = Command::new(admin_binary())
        .arg("--help")
        .output()
        .expect("admin help output");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("backup"));
    assert!(stdout.contains("verify-backup"));
}

/// Proves online backup, referenced inclusion, manifest integrity, and secret exclusion.
#[test]
fn live_database_backup_contains_only_owned_verified_payloads() {
    let fixture = backup_fixture();
    fixture
        .connection
        .execute(
            "INSERT INTO audit_log (
                 id, actor_id, action, target_type, target_id, summary,
                 source_address, created_at
             ) VALUES (?1, NULL, 'fixture.committed', 'test', NULL,
                       'Committed while live connection remains open', NULL, ?2)",
            params!["66666666-6666-4666-8666-666666666666", timestamp()],
        )
        .expect("live committed row");

    let backup = run_backup(&fixture);
    assert_eq!(
        backup.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&backup.stderr)
    );
    let entries = read_archive(&fixture.archive);
    assert_eq!(
        entries.keys().cloned().collect::<Vec<_>>(),
        vec![
            "attachments/11111111-1111-4111-8111-111111111111.png",
            "branding/22222222-2222-4222-8222-222222222222.png",
            "data/local-it-desk.db",
            "manifest.json",
        ]
    );
    assert_eq!(
        entries[&format!("attachments/{ATTACHMENT_NAME}")],
        ATTACHMENT_BYTES
    );
    assert_eq!(
        entries[&format!("branding/{BRANDING_NAME}")],
        BRANDING_BYTES
    );
    let manifest: BackupManifest =
        serde_json::from_slice(&entries[MANIFEST_PATH]).expect("backup manifest");
    assert_eq!(manifest.archive_format_version, ARCHIVE_FORMAT_VERSION);
    assert_eq!(manifest.schema_version, SCHEMA_VERSION);
    assert_eq!(manifest.logical_counts["users"], 1);
    assert_eq!(manifest.logical_counts["tickets"], 1);
    assert_eq!(manifest.logical_counts["attachments"], 1);
    assert_eq!(manifest.logical_counts["audit_log"], 1);
    for file in &manifest.files {
        assert_eq!(file.sha256, sha256(&entries[&file.path]));
        assert_eq!(file.size_bytes, entries[&file.path].len() as u64);
    }
    let all_paths = entries.keys().cloned().collect::<Vec<_>>().join("\n");
    assert!(!all_paths.contains(".env"));
    assert!(!all_paths.contains("tls.key"));
    assert!(!all_paths.contains("application.log"));
    assert!(!all_paths.contains(".part"));
    assert!(!all_paths.contains("old-logo"));

    let verify = run_verify(&fixture.archive);
    assert_eq!(
        verify.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
    assert!(String::from_utf8_lossy(&verify.stdout).contains("Backup verified"));
}

/// Confirms a missing referenced file fails closed without publishing an archive.
#[test]
fn missing_referenced_file_and_existing_output_fail_without_overwrite() {
    let fixture = backup_fixture();
    fs::remove_file(fixture.attachments.join(ATTACHMENT_NAME)).expect("remove fixture attachment");
    let missing = run_backup(&fixture);
    assert_eq!(missing.status.code(), Some(7));
    assert!(!fixture.archive.exists());

    fs::write(&fixture.archive, b"operator-owned existing archive").expect("existing output");
    let original = fs::read(&fixture.archive).expect("original output bytes");
    let existing = run_backup(&fixture);
    assert_eq!(existing.status.code(), Some(7));
    assert_eq!(
        fs::read(&fixture.archive).expect("preserved output"),
        original
    );
}

/// Confirms corrupt streams, checksum mismatches, and incompatible schemas are rejected.
#[test]
fn verifier_rejects_corrupt_checksum_and_incompatible_archives() {
    let root = tempfile::tempdir().expect("invalid archive fixture");
    let corrupt = root.path().join("corrupt.tar.gz");
    fs::write(&corrupt, b"not a gzip archive").expect("corrupt archive");
    assert_eq!(run_verify(&corrupt).status.code(), Some(7));

    let checksum = root.path().join("checksum.tar.gz");
    write_synthetic_archive(
        &checksum,
        &synthetic_manifest(SCHEMA_VERSION, "0".repeat(64)),
        DATABASE_ARCHIVE_PATH.as_bytes(),
    );
    assert_eq!(run_verify(&checksum).status.code(), Some(7));

    let incompatible = root.path().join("incompatible.tar.gz");
    write_synthetic_archive(
        &incompatible,
        &synthetic_manifest(SCHEMA_VERSION + 1, sha256(b"x")),
        DATABASE_ARCHIVE_PATH.as_bytes(),
    );
    assert_eq!(run_verify(&incompatible).status.code(), Some(7));
}

/// Confirms a valid archive cannot be truncated or extended without detection.
#[test]
fn verifier_requires_the_exact_complete_compressed_stream() {
    let fixture = backup_fixture();
    assert!(run_backup(&fixture).status.success());
    let bytes = fs::read(&fixture.archive).expect("valid archive bytes");

    let truncated = fixture.archive.with_file_name("truncated.tar.gz");
    fs::write(&truncated, &bytes[..bytes.len() - 4]).expect("truncated archive");
    assert_eq!(run_verify(&truncated).status.code(), Some(7));

    let trailing = fixture.archive.with_file_name("trailing.tar.gz");
    let mut extended = bytes;
    extended.extend_from_slice(b"trailing-garbage");
    fs::write(&trailing, extended).expect("extended archive");
    assert_eq!(run_verify(&trailing).status.code(), Some(7));
}

/// Confirms traversal paths are rejected even when their tar checksum is valid.
#[test]
fn verifier_rejects_traversal_entries_before_extraction() {
    let root = tempfile::tempdir().expect("traversal archive fixture");
    let archive = root.path().join("traversal.tar.gz");
    write_synthetic_archive(
        &archive,
        &synthetic_manifest(SCHEMA_VERSION, sha256(b"x")),
        b"../escape.db",
    );
    let output = run_verify(&archive);
    assert_eq!(output.status.code(), Some(7));
    assert!(String::from_utf8_lossy(&output.stderr).contains("archive"));
    assert!(!root.path().join("escape.db").exists());
}
