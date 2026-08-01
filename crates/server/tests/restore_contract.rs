//! Contract tests for staged, quarantined Local IT Desk restore operations.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use local_it_desk_server::backup::{BackupManifest, DATABASE_ARCHIVE_PATH};
use local_it_desk_server::db::migrations::{SCHEMA_VERSION, run_migrations};
use local_it_desk_server::runtime_lock::acquire_runtime_lock;
use rusqlite::{Connection, params};
use tar::{Archive, Builder, EntryType, Header};
use tempfile::TempDir;
use uuid::Uuid;

/// Isolated named-volume-like state root with one active generation.
struct StateFixture {
    /// Owning temporary directory retained for every fixture path.
    _root: TempDir,
    /// State-volume root containing current, backups, and future quarantine.
    state: PathBuf,
    /// Exact active generation accepted by the restore command.
    current: PathBuf,
}

/// Expected logical identity and referenced bytes for one generated state.
struct GenerationFacts {
    /// Human-facing marker stored in the users table.
    display_name: String,
    /// Referenced attachment filename.
    attachment_name: String,
    /// Referenced attachment bytes.
    attachment_bytes: Vec<u8>,
    /// Active branding filename.
    branding_name: String,
    /// Active branding bytes.
    branding_bytes: Vec<u8>,
}

/// Path to the Cargo-built offline administration binary under test.
fn admin_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_local-it-desk-admin"))
}

/// Creates one state root with required current and backups siblings.
fn state_fixture(label: &str) -> (StateFixture, GenerationFacts) {
    let root = tempfile::tempdir().expect("temporary state root");
    let state = root.path().join("state");
    let current = state.join("current");
    fs::create_dir_all(state.join("backups")).expect("backup directory");
    let facts = populate_generation(&current, label);
    (
        StateFixture {
            _root: root,
            state,
            current,
        },
        facts,
    )
}

/// Populates one generation with a migrated database and referenced files.
fn populate_generation(path: &Path, label: &str) -> GenerationFacts {
    let data = path.join("data");
    let attachments = path.join("attachments");
    let branding = path.join("branding");
    fs::create_dir_all(&data).expect("data directory");
    fs::create_dir_all(&attachments).expect("attachment directory");
    fs::create_dir_all(&branding).expect("branding directory");
    let database = data.join("local-it-desk.db");
    let connection = Connection::open(&database).expect("generation database");
    run_migrations(&connection).expect("generation schema");
    let user_id = Uuid::new_v4().to_string();
    let ticket_id = Uuid::new_v4().to_string();
    let attachment_id = Uuid::new_v4().to_string();
    let attachment_name = format!("{}.png", Uuid::new_v4());
    let branding_name = format!("{}.png", Uuid::new_v4());
    let attachment_bytes = format!("attachment-{label}").into_bytes();
    let branding_bytes = format!("branding-{label}").into_bytes();
    connection
        .execute(
            "INSERT INTO users (
                 id, username, display_name, email, password_hash, role,
                 is_active, must_change_password, created_at, updated_at
             ) VALUES (?1, 'restore.user', ?2, NULL, 'fixture-hash',
                       'requester', 1, 0, '2026-01-01T00:00:00.000Z',
                       '2026-01-01T00:00:00.000Z')",
            params![user_id, label],
        )
        .expect("generation user");
    connection
        .execute(
            "INSERT INTO tickets (
                 id, number, title, description, requester_id, status,
                 priority, created_at, updated_at
             ) VALUES (?1, 1, ?2, 'Restore fixture', ?3, 'new', 'normal',
                       '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            params![ticket_id, format!("Ticket {label}"), user_id],
        )
        .expect("generation ticket");
    connection
        .execute(
            "INSERT INTO attachments (
                 id, ticket_id, comment_id, announcement_id, parent_kind,
                 uploader_id, original_name, stored_name, media_type,
                 size_bytes, sha256, created_at
             ) VALUES (?1, ?2, NULL, NULL, 'ticket', ?3, 'fixture.png', ?4,
                       'image/png', ?5, 'fixture-checksum',
                       '2026-01-01T00:00:00.000Z')",
            params![
                attachment_id,
                ticket_id,
                user_id,
                attachment_name,
                attachment_bytes.len() as u64
            ],
        )
        .expect("generation attachment");
    connection
        .execute(
            "INSERT INTO settings (key, value, updated_by, updated_at)
             VALUES ('logo_stored_name', ?1, NULL, '2026-01-01T00:00:00.000Z')",
            [&branding_name],
        )
        .expect("generation branding reference");
    drop(connection);
    fs::write(attachments.join(&attachment_name), &attachment_bytes)
        .expect("generation attachment bytes");
    fs::write(branding.join(&branding_name), &branding_bytes).expect("generation branding bytes");
    GenerationFacts {
        display_name: label.to_string(),
        attachment_name,
        attachment_bytes,
        branding_name,
        branding_bytes,
    }
}

/// Creates a verified backup from one generation through the public CLI.
fn create_archive(generation: &Path, output: &Path) {
    let result = Command::new(admin_binary())
        .args([
            "backup",
            "--database",
            path_text(&generation.join("data/local-it-desk.db")),
            "--attachments",
            path_text(&generation.join("attachments")),
            "--branding",
            path_text(&generation.join("branding")),
            "--output",
            path_text(output),
        ])
        .output()
        .expect("backup command");
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

/// Runs one restore mode through the exact public CLI contract.
fn run_restore(archive: &Path, target: &Path, mode: &str) -> Output {
    Command::new(admin_binary())
        .args([
            "restore",
            "--archive",
            path_text(archive),
            "--target-root",
            path_text(target),
            mode,
        ])
        .output()
        .expect("restore command")
}

/// Runs the public archive verifier for a generated safety backup.
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

/// Reads the one fixture display name from a restored database.
fn display_name(generation: &Path) -> String {
    Connection::open(generation.join("data/local-it-desk.db"))
        .expect("open restored database")
        .query_row("SELECT display_name FROM users", [], |row| row.get(0))
        .expect("restored display name")
}

/// Captures every regular file beneath a generation for no-mutation assertions.
fn generation_snapshot(generation: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    for directory in ["data", "attachments", "branding"] {
        for entry in fs::read_dir(generation.join(directory)).expect("generation directory") {
            let entry = entry.expect("generation entry");
            if entry.file_type().expect("entry type").is_file()
                && entry.file_name() != ".local-it-desk.lock"
            {
                files.insert(
                    format!("{directory}/{}", entry.file_name().to_string_lossy()),
                    fs::read(entry.path()).expect("generation bytes"),
                );
            }
        }
    }
    files
}

/// Lists direct state-root children with a requested prefix.
fn state_children(state: &Path, prefix: &str) -> Vec<PathBuf> {
    fs::read_dir(state)
        .expect("state directory")
        .map(|entry| entry.expect("state child").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(prefix))
        })
        .collect()
}

/// Reads a gzip tar into ordered entry bytes for controlled corruption tests.
fn read_archive(path: &Path) -> Vec<(String, Vec<u8>)> {
    let decoder = GzDecoder::new(fs::File::open(path).expect("archive input"));
    let mut archive = Archive::new(decoder);
    archive
        .entries()
        .expect("archive entries")
        .map(|entry| {
            let mut entry = entry.expect("archive entry");
            let path = entry
                .path()
                .expect("entry path")
                .to_string_lossy()
                .to_string();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).expect("entry bytes");
            (path, bytes)
        })
        .collect()
}

/// Writes ordered regular entries and can inject one raw traversal path.
fn write_archive(path: &Path, entries: &[(String, Vec<u8>)], raw_override: Option<&[u8]>) {
    let encoder = GzEncoder::new(
        fs::File::create(path).expect("archive output"),
        Compression::default(),
    );
    let mut archive = Builder::new(encoder);
    for (index, (entry_path, bytes)) in entries.iter().enumerate() {
        let raw_path = if index == 1 {
            raw_override.unwrap_or(entry_path.as_bytes())
        } else {
            entry_path.as_bytes()
        };
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
            .expect("archive entry output");
    }
    let encoder = archive.into_inner().expect("finish tar");
    encoder.finish().expect("finish gzip");
}

/// Creates a valid source archive and returns its owning fixture and facts.
fn source_archive() -> (StateFixture, GenerationFacts, PathBuf) {
    let (source, facts) = state_fixture("source-generation");
    let archive = source.state.join("backups/source.tar.gz");
    create_archive(&source.current, &archive);
    (source, facts, archive)
}

/// Requires the public CLI to expose restore and enforce exactly one mode.
#[test]
fn help_exposes_restore_command_and_exclusive_modes() {
    let help = Command::new(admin_binary())
        .arg("--help")
        .output()
        .expect("admin help output");
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("restore"));

    let (target, _) = state_fixture("target");
    let (_source, _facts, archive) = source_archive();
    let missing = Command::new(admin_binary())
        .args([
            "restore",
            "--archive",
            path_text(&archive),
            "--target-root",
            path_text(&target.current),
        ])
        .output()
        .expect("missing restore mode");
    assert_eq!(missing.status.code(), Some(2));
    let both = Command::new(admin_binary())
        .args([
            "restore",
            "--archive",
            path_text(&archive),
            "--target-root",
            path_text(&target.current),
            "--dry-run",
            "--apply",
        ])
        .output()
        .expect("conflicting restore modes");
    assert_eq!(both.status.code(), Some(2));
}

/// Confirms dry-run proves the archive and plan while leaving every path unchanged.
#[test]
fn dry_run_is_complete_and_non_mutating() {
    let (target, _) = state_fixture("target-before-dry-run");
    let (_source, _facts, archive) = source_archive();
    let before = generation_snapshot(&target.current);
    let output = run_restore(&archive, &target.current, "--dry-run");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("target unchanged"));
    assert_eq!(generation_snapshot(&target.current), before);
    assert!(state_children(&target.state, ".restore-").is_empty());
    assert!(state_children(&target.state, ".quarantine-").is_empty());
    assert!(
        fs::read_dir(target.state.join("backups"))
            .expect("backups")
            .next()
            .is_none()
    );
}

/// Confirms apply retains a verified pre-backup and quarantines the old generation.
#[test]
fn apply_restores_logical_data_and_retains_rollback_artifacts() {
    let (target, old_facts) = state_fixture("old-generation");
    let (_source, new_facts, archive) = source_archive();
    let output = run_restore(&archive, &target.current, "--apply");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(display_name(&target.current), new_facts.display_name);
    assert_eq!(
        fs::read(
            target
                .current
                .join("attachments")
                .join(&new_facts.attachment_name)
        )
        .expect("restored attachment"),
        new_facts.attachment_bytes
    );
    assert_eq!(
        fs::read(
            target
                .current
                .join("branding")
                .join(&new_facts.branding_name)
        )
        .expect("restored branding"),
        new_facts.branding_bytes
    );
    let quarantines = state_children(&target.state, ".quarantine-");
    assert_eq!(quarantines.len(), 1);
    assert_eq!(display_name(&quarantines[0]), old_facts.display_name);
    assert_eq!(
        fs::read(
            quarantines[0]
                .join("attachments")
                .join(&old_facts.attachment_name)
        )
        .expect("quarantined attachment"),
        old_facts.attachment_bytes
    );
    assert!(state_children(&target.state, ".restore-").is_empty());
    let pre_backups = fs::read_dir(target.state.join("backups"))
        .expect("pre-backups")
        .map(|entry| entry.expect("pre-backup entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("pre-restore-"))
        })
        .collect::<Vec<_>>();
    assert_eq!(pre_backups.len(), 1);
    assert!(run_verify(&pre_backups[0]).status.success());
}

/// Confirms runtime and SQLite writer locks stop apply before safety artifacts exist.
#[test]
fn apply_refuses_live_application_and_external_writer() {
    let (target, facts) = state_fixture("locked-target");
    let (_source, _facts, archive) = source_archive();
    let before = generation_snapshot(&target.current);
    let runtime_lock = acquire_runtime_lock(&target.current.join("data/local-it-desk.db"))
        .expect("fixture runtime lock");
    let locked = run_restore(&archive, &target.current, "--apply");
    assert_eq!(locked.status.code(), Some(8));
    drop(runtime_lock);
    assert_eq!(generation_snapshot(&target.current), before);

    let writer = Connection::open(target.current.join("data/local-it-desk.db"))
        .expect("external writer connection");
    writer
        .execute_batch("BEGIN IMMEDIATE")
        .expect("external write transaction");
    let busy = run_restore(&archive, &target.current, "--apply");
    assert_eq!(busy.status.code(), Some(8));
    writer
        .execute_batch("ROLLBACK")
        .expect("release external writer");
    assert_eq!(display_name(&target.current), facts.display_name);
    assert_eq!(
        fs::read(
            target
                .current
                .join("attachments")
                .join(&facts.attachment_name)
        )
        .expect("unchanged attachment"),
        facts.attachment_bytes
    );
    assert_eq!(
        fs::read(target.current.join("branding").join(&facts.branding_name))
            .expect("unchanged branding"),
        facts.branding_bytes
    );
    assert!(
        fs::read_dir(target.state.join("backups"))
            .expect("backups")
            .next()
            .is_none()
    );
}

/// Confirms invalid archives fail before target mutation or pre-backup creation.
#[test]
fn corrupt_checksum_traversal_and_schema_fail_before_mutation() {
    let (target, _) = state_fixture("unchanged-target");
    let (_source, _facts, archive) = source_archive();
    let original = generation_snapshot(&target.current);
    let entries = read_archive(&archive);

    let corrupt = archive.with_file_name("corrupt.tar.gz");
    fs::write(&corrupt, b"not gzip").expect("corrupt archive");
    assert_eq!(
        run_restore(&corrupt, &target.current, "--apply")
            .status
            .code(),
        Some(8)
    );

    let checksum = archive.with_file_name("checksum.tar.gz");
    let mut checksum_entries = entries.clone();
    let mut manifest: BackupManifest =
        serde_json::from_slice(&checksum_entries[0].1).expect("checksum manifest");
    manifest
        .files
        .iter_mut()
        .find(|file| file.path == DATABASE_ARCHIVE_PATH)
        .expect("database manifest entry")
        .sha256 = "0".repeat(64);
    checksum_entries[0].1 = serde_json::to_vec(&manifest).expect("checksum manifest bytes");
    write_archive(&checksum, &checksum_entries, None);
    assert_eq!(
        run_restore(&checksum, &target.current, "--apply")
            .status
            .code(),
        Some(8)
    );

    let incompatible = archive.with_file_name("incompatible.tar.gz");
    let mut incompatible_entries = entries.clone();
    let mut manifest: BackupManifest =
        serde_json::from_slice(&incompatible_entries[0].1).expect("schema manifest");
    manifest.schema_version = SCHEMA_VERSION + 1;
    incompatible_entries[0].1 = serde_json::to_vec(&manifest).expect("schema manifest bytes");
    write_archive(&incompatible, &incompatible_entries, None);
    assert_eq!(
        run_restore(&incompatible, &target.current, "--apply")
            .status
            .code(),
        Some(8)
    );

    let traversal = archive.with_file_name("traversal.tar.gz");
    write_archive(&traversal, &entries, Some(b"../escape.db"));
    assert_eq!(
        run_restore(&traversal, &target.current, "--apply")
            .status
            .code(),
        Some(8)
    );

    assert_eq!(generation_snapshot(&target.current), original);
    assert!(state_children(&target.state, ".restore-").is_empty());
    assert!(state_children(&target.state, ".quarantine-").is_empty());
    assert!(
        fs::read_dir(target.state.join("backups"))
            .expect("backups")
            .next()
            .is_none()
    );
}

/// Confirms broad, occupied, symlinked, and malformed targets are refused.
#[test]
fn restore_requires_one_exact_unambiguous_current_generation() {
    let (target, _) = state_fixture("valid-target");
    let (_source, _facts, archive) = source_archive();
    assert_eq!(
        run_restore(&archive, &target.state, "--dry-run")
            .status
            .code(),
        Some(8)
    );

    let occupied = target.state.join("occupied");
    fs::create_dir(&occupied).expect("occupied target");
    fs::write(occupied.join("unrelated.txt"), b"do not overwrite").expect("occupied content");
    assert_eq!(
        run_restore(&archive, &occupied, "--apply").status.code(),
        Some(8)
    );
    assert_eq!(
        fs::read(occupied.join("unrelated.txt")).expect("preserved occupied content"),
        b"do not overwrite"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let symlinked = target.state.join("symlink-current");
        symlink(&target.current, &symlinked).expect("target symlink");
        assert_eq!(
            run_restore(&archive, &symlinked, "--dry-run").status.code(),
            Some(8)
        );
    }
}
