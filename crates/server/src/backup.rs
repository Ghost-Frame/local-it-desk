//! Consistent SQLite snapshots packaged with referenced files and checksums.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use flate2::Compression;
use flate2::bufread::GzDecoder;
use flate2::write::GzEncoder;
use rusqlite::backup::Backup;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tar::{Archive, Builder, EntryType, Header};
use tempfile::{Builder as TempBuilder, NamedTempFile};
use thiserror::Error;

use crate::db::migrations::SCHEMA_VERSION;

/// Version of the archive layout and manifest contract.
pub const ARCHIVE_FORMAT_VERSION: u32 = 1;
/// Canonical manifest entry that must be first in every backup.
pub const MANIFEST_PATH: &str = "manifest.json";
/// Canonical SQLite snapshot entry required in every backup.
pub const DATABASE_ARCHIVE_PATH: &str = "data/local-it-desk.db";
/// Maximum manifest bytes accepted before JSON decoding.
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
/// Maximum number of payload entries accepted from one archive.
const MAX_ARCHIVE_ENTRIES: usize = 100_000;
/// Maximum size of one verified archive entry.
const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024 * 1024;
/// Maximum aggregate uncompressed payload accepted from one archive.
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024 * 1024;
/// Maximum zero-filled tar terminator padding accepted after entry iteration.
const MAX_TAR_PADDING_BYTES: usize = 1024;

/// Stable metadata stored as the first archive entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupManifest {
    /// Archive layout version understood by the reader.
    pub archive_format_version: u32,
    /// Application version that created the snapshot.
    pub application_version: String,
    /// Exact database schema version recorded by the snapshot.
    pub schema_version: u32,
    /// RFC 3339 UTC timestamp for archive creation.
    pub created_at: String,
    /// Logical row counts used for operator inspection and restore checks.
    pub logical_counts: BTreeMap<String, u64>,
    /// Every payload entry with its exact uncompressed size and checksum.
    pub files: Vec<BackupFile>,
}

/// Integrity metadata for one regular payload entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupFile {
    /// Normalized relative archive path.
    pub path: String,
    /// Exact uncompressed payload size.
    pub size_bytes: u64,
    /// Lowercase hexadecimal SHA-256 digest of the payload.
    pub sha256: String,
}

/// Non-secret summary returned after archive creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupSummary {
    /// Number of payload files, including the SQLite snapshot.
    pub file_count: usize,
    /// Aggregate uncompressed payload bytes.
    pub payload_bytes: u64,
    /// Database schema version captured by the archive.
    pub schema_version: u32,
}

/// Non-secret summary returned after complete archive verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationSummary {
    /// Number of payload files verified against the manifest.
    pub file_count: usize,
    /// Aggregate uncompressed bytes hashed during verification.
    pub payload_bytes: u64,
    /// Database schema version accepted from the manifest.
    pub schema_version: u32,
}

/// Safe failures from backup creation and streaming verification.
#[derive(Debug, Error)]
pub enum BackupError {
    /// One explicit input did not name an existing regular file or directory.
    #[error("required input is missing or has the wrong type: {0}")]
    InvalidInput(String),
    /// The output already exists and will not be overwritten.
    #[error("backup output already exists")]
    OutputExists,
    /// A database reference did not resolve to one safe stored filename.
    #[error("database contains an unsafe stored filename")]
    UnsafeStoredName,
    /// A database-owned file is missing or is not a regular file.
    #[error("referenced file is missing or unsafe: {0}")]
    MissingReferencedFile(String),
    /// The database schema cannot be handled by this application version.
    #[error("backup schema version {found} is incompatible with supported version {supported}")]
    IncompatibleSchema {
        /// Version found in the source or archive manifest.
        found: u32,
        /// Version supported by the running application.
        supported: u32,
    },
    /// SQLite could not create or inspect a consistent snapshot.
    #[error("database backup failed: {0}")]
    Database(#[from] rusqlite::Error),
    /// Filesystem or archive streaming failed.
    #[error("backup I/O failed: {0}")]
    Io(#[from] io::Error),
    /// Manifest serialization or parsing failed.
    #[error("backup manifest is invalid: {0}")]
    ManifestJson(#[from] serde_json::Error),
    /// The archive violates the versioned structure or integrity contract.
    #[error("backup archive is invalid: {0}")]
    InvalidArchive(String),
}

/// Open payload handle paired with immutable archive metadata.
struct Payload {
    /// Canonical path written into the archive.
    archive_path: String,
    /// Open source handle retained across hashing and archive writing.
    source: File,
    /// Exact bytes observed while hashing.
    size_bytes: u64,
    /// Lowercase SHA-256 digest observed from the open handle.
    sha256: String,
}

/// Creates one atomically published archive from a live SQLite database.
pub fn create_backup(
    database_path: &Path,
    attachments_dir: &Path,
    branding_dir: &Path,
    output_path: &Path,
) -> Result<BackupSummary, BackupError> {
    require_regular_file(database_path, "database")?;
    require_directory(attachments_dir, "attachments directory")?;
    require_directory(branding_dir, "branding directory")?;
    if output_path.exists() {
        return Err(BackupError::OutputExists);
    }
    let output_parent = output_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    require_directory(output_parent, "backup output directory")?;

    let snapshot_directory = TempBuilder::new()
        .prefix(".local-it-desk-snapshot-")
        .tempdir_in(output_parent)?;
    let snapshot_path = snapshot_directory.path().join("local-it-desk.db");
    snapshot_database(database_path, &snapshot_path)?;
    let snapshot = Connection::open_with_flags(&snapshot_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let schema_version = read_schema_version(&snapshot)?;
    if schema_version != SCHEMA_VERSION {
        return Err(BackupError::IncompatibleSchema {
            found: schema_version,
            supported: SCHEMA_VERSION,
        });
    }

    let logical_counts = read_logical_counts(&snapshot)?;
    let attachment_names = read_attachment_names(&snapshot)?;
    let branding_name = read_branding_name(&snapshot)?;
    drop(snapshot);

    let mut payloads = vec![open_payload(
        DATABASE_ARCHIVE_PATH.to_string(),
        &snapshot_path,
        snapshot_directory.path(),
    )?];
    for stored_name in attachment_names {
        let path = resolve_referenced_file(attachments_dir, &stored_name)?;
        payloads.push(open_payload(
            format!("attachments/{stored_name}"),
            &path,
            attachments_dir,
        )?);
    }
    if let Some(stored_name) = branding_name {
        let path = resolve_referenced_file(branding_dir, &stored_name)?;
        payloads.push(open_payload(
            format!("branding/{stored_name}"),
            &path,
            branding_dir,
        )?);
    }

    let created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let manifest = BackupManifest {
        archive_format_version: ARCHIVE_FORMAT_VERSION,
        application_version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version,
        created_at,
        logical_counts,
        files: payloads
            .iter()
            .map(|payload| BackupFile {
                path: payload.archive_path.clone(),
                size_bytes: payload.size_bytes,
                sha256: payload.sha256.clone(),
            })
            .collect(),
    };
    validate_manifest(&manifest)?;
    let payload_bytes = checked_payload_total(&manifest.files)?;
    write_archive(output_path, output_parent, &manifest, &mut payloads)?;

    Ok(BackupSummary {
        file_count: manifest.files.len(),
        payload_bytes,
        schema_version,
    })
}

/// Streams and verifies one complete archive without extracting it.
pub fn verify_backup(archive_path: &Path) -> Result<VerificationSummary, BackupError> {
    require_regular_file(archive_path, "backup archive")?;
    let archive_file = File::open(archive_path)?;
    let archive_length = archive_file.metadata()?.len();
    let decoder = GzDecoder::new(BufReader::new(archive_file));
    let mut archive = Archive::new(decoder);
    let mut entries = archive.entries()?;
    let mut manifest_entry = entries
        .next()
        .ok_or_else(|| BackupError::InvalidArchive("archive is empty".to_string()))??;
    require_regular_entry(&manifest_entry, MANIFEST_PATH)?;
    let manifest_path = normalized_archive_path(&manifest_entry)?;
    if manifest_path != MANIFEST_PATH {
        return Err(BackupError::InvalidArchive(
            "manifest must be the first archive entry".to_string(),
        ));
    }
    if manifest_entry.size() > MAX_MANIFEST_BYTES {
        return Err(BackupError::InvalidArchive(
            "manifest exceeds the supported size".to_string(),
        ));
    }
    let mut manifest_bytes = Vec::with_capacity(manifest_entry.size() as usize);
    manifest_entry.read_to_end(&mut manifest_bytes)?;
    let manifest: BackupManifest = serde_json::from_slice(&manifest_bytes)?;
    validate_manifest(&manifest)?;

    let expected = manifest
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    let mut payload_bytes = 0_u64;
    for entry in entries {
        let mut entry = entry?;
        require_regular_entry(&entry, "payload")?;
        let path = normalized_archive_path(&entry)?;
        if path == MANIFEST_PATH || !seen.insert(path.clone()) {
            return Err(BackupError::InvalidArchive(format!(
                "duplicate archive entry: {path}"
            )));
        }
        let expected_file = expected.get(path.as_str()).ok_or_else(|| {
            BackupError::InvalidArchive(format!("unexpected archive entry: {path}"))
        })?;
        if entry.size() != expected_file.size_bytes || entry.size() > MAX_ENTRY_BYTES {
            return Err(BackupError::InvalidArchive(format!(
                "size mismatch for archive entry: {path}"
            )));
        }
        let (size_bytes, sha256) = hash_reader(&mut entry, MAX_ENTRY_BYTES)?;
        if size_bytes != expected_file.size_bytes || sha256 != expected_file.sha256 {
            return Err(BackupError::InvalidArchive(format!(
                "checksum mismatch for archive entry: {path}"
            )));
        }
        payload_bytes = payload_bytes
            .checked_add(size_bytes)
            .ok_or_else(|| BackupError::InvalidArchive("payload size overflow".to_string()))?;
        if payload_bytes > MAX_TOTAL_BYTES {
            return Err(BackupError::InvalidArchive(
                "archive payload exceeds the supported size".to_string(),
            ));
        }
    }
    if seen.len() != expected.len() {
        let missing = expected
            .keys()
            .find(|path| !seen.contains(**path))
            .copied()
            .unwrap_or("unknown");
        return Err(BackupError::InvalidArchive(format!(
            "manifest entry is missing from archive: {missing}"
        )));
    }
    drop(manifest_entry);
    drop(seen);
    drop(expected);

    let mut decoder = archive.into_inner();
    verify_tar_padding(&mut decoder)?;
    let mut compressed_reader = decoder.into_inner();
    if compressed_reader.stream_position()? != archive_length {
        return Err(BackupError::InvalidArchive(
            "archive contains trailing or concatenated compressed data".to_string(),
        ));
    }

    Ok(VerificationSummary {
        file_count: manifest.files.len(),
        payload_bytes,
        schema_version: manifest.schema_version,
    })
}

/// Drains only bounded zero-filled tar terminator padding from the decoder.
fn verify_tar_padding(reader: &mut impl Read) -> Result<(), BackupError> {
    let mut padding = [0_u8; MAX_TAR_PADDING_BYTES + 1];
    let mut consumed = 0_usize;
    loop {
        let read = reader.read(&mut padding[consumed..])?;
        if read == 0 {
            break;
        }
        if padding[consumed..consumed + read]
            .iter()
            .any(|byte| *byte != 0)
        {
            return Err(BackupError::InvalidArchive(
                "archive contains payload after the tar terminator".to_string(),
            ));
        }
        consumed += read;
        if consumed > MAX_TAR_PADDING_BYTES {
            return Err(BackupError::InvalidArchive(
                "archive contains excessive tar terminator padding".to_string(),
            ));
        }
    }
    Ok(())
}

/// Copies the live source into a transactionally consistent standalone database.
fn snapshot_database(source_path: &Path, destination_path: &Path) -> Result<(), BackupError> {
    let source = Connection::open_with_flags(source_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.busy_timeout(Duration::from_secs(5))?;
    let mut destination = Connection::open(destination_path)?;
    {
        let backup = Backup::new(&source, &mut destination)?;
        backup.run_to_completion(128, Duration::from_millis(10), None)?;
    }
    let integrity: String =
        destination.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(BackupError::InvalidArchive(
            "SQLite snapshot did not pass integrity_check".to_string(),
        ));
    }
    Ok(())
}

/// Reads exactly one positive schema version from a migrated snapshot.
fn read_schema_version(connection: &Connection) -> Result<u32, BackupError> {
    let mut statement = connection.prepare("SELECT version FROM schema_version LIMIT 2")?;
    let versions = statement
        .query_map([], |row| row.get::<_, u32>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    match versions.as_slice() {
        [version] => Ok(*version),
        _ => Err(BackupError::InvalidArchive(
            "database must contain exactly one schema version".to_string(),
        )),
    }
}

/// Captures stable row counts for the application's logical tables.
fn read_logical_counts(connection: &Connection) -> Result<BTreeMap<String, u64>, BackupError> {
    const TABLES: [&str; 10] = [
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
    ];
    let mut counts = BTreeMap::new();
    for table in TABLES {
        let count = connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get::<_, u64>(0)
        })?;
        counts.insert(table.to_string(), count);
    }
    Ok(counts)
}

/// Reads the sorted unique attachment filenames referenced by the snapshot.
fn read_attachment_names(connection: &Connection) -> Result<Vec<String>, BackupError> {
    let mut statement =
        connection.prepare("SELECT stored_name FROM attachments ORDER BY stored_name")?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let unique = names.iter().collect::<BTreeSet<_>>();
    if unique.len() != names.len() {
        return Err(BackupError::InvalidArchive(
            "database contains duplicate attachment filenames".to_string(),
        ));
    }
    Ok(names)
}

/// Reads the optional active logo filename referenced by runtime settings.
fn read_branding_name(connection: &Connection) -> Result<Option<String>, BackupError> {
    let mut statement =
        connection.prepare("SELECT value FROM settings WHERE key = 'logo_stored_name' LIMIT 2")?;
    let values = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    match values.as_slice() {
        [] => Ok(None),
        [value] if !value.is_empty() => Ok(Some(value.clone())),
        [..] => Err(BackupError::InvalidArchive(
            "active branding reference is invalid".to_string(),
        )),
    }
}

/// Resolves one database-owned filename without permitting directory traversal.
fn resolve_referenced_file(root: &Path, stored_name: &str) -> Result<PathBuf, BackupError> {
    if !is_single_component(stored_name) {
        return Err(BackupError::UnsafeStoredName);
    }
    let path = root.join(stored_name);
    require_regular_file(&path, stored_name)
        .map_err(|_| BackupError::MissingReferencedFile(stored_name.to_string()))?;
    let canonical_root = root.canonicalize()?;
    let canonical_path = path.canonicalize()?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(BackupError::MissingReferencedFile(stored_name.to_string()));
    }
    Ok(path)
}

/// Opens and hashes one regular file while retaining the same handle for writing.
fn open_payload(
    archive_path: String,
    source_path: &Path,
    confinement_root: &Path,
) -> Result<Payload, BackupError> {
    validate_archive_path(&archive_path)?;
    let canonical_root = confinement_root.canonicalize()?;
    let canonical_source = source_path.canonicalize()?;
    if !canonical_source.starts_with(canonical_root) {
        return Err(BackupError::InvalidInput(
            "payload escaped its declared storage root".to_string(),
        ));
    }
    let mut source = File::open(source_path)?;
    if !source.metadata()?.is_file() {
        return Err(BackupError::InvalidInput(
            "payload is not a regular file".to_string(),
        ));
    }
    let (size_bytes, sha256) = hash_reader(&mut source, MAX_ENTRY_BYTES)?;
    source.seek(SeekFrom::Start(0))?;
    Ok(Payload {
        archive_path,
        source,
        size_bytes,
        sha256,
    })
}

/// Writes the manifest and payloads into a temporary file before no-clobber publish.
fn write_archive(
    output_path: &Path,
    output_parent: &Path,
    manifest: &BackupManifest,
    payloads: &mut [Payload],
) -> Result<(), BackupError> {
    let temporary = NamedTempFile::new_in(output_parent)?;
    let writer = temporary.reopen()?;
    let encoder = GzEncoder::new(writer, Compression::default());
    let mut archive = Builder::new(encoder);
    let manifest_bytes = serde_json::to_vec_pretty(manifest)?;
    append_bytes(&mut archive, MANIFEST_PATH, &manifest_bytes)?;
    for payload in payloads {
        let mut header = regular_header(payload.size_bytes);
        archive.append_data(&mut header, &payload.archive_path, &mut payload.source)?;
    }
    let encoder = archive.into_inner()?;
    let completed = encoder.finish()?;
    completed.sync_all()?;
    temporary.persist_noclobber(output_path).map_err(|error| {
        if output_path.exists() {
            BackupError::OutputExists
        } else {
            BackupError::Io(error.error)
        }
    })?;
    Ok(())
}

/// Appends one in-memory regular file with restrictive portable metadata.
fn append_bytes<W: Write>(
    archive: &mut Builder<W>,
    path: &str,
    bytes: &[u8],
) -> Result<(), BackupError> {
    let mut header = regular_header(bytes.len() as u64);
    archive.append_data(&mut header, path, bytes)?;
    Ok(())
}

/// Creates a deterministic regular-file tar header with no identity metadata.
fn regular_header(size_bytes: u64) -> Header {
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Regular);
    header.set_mode(0o600);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_size(size_bytes);
    header.set_cksum();
    header
}

/// Validates all manifest fields before writing or trusting payload metadata.
fn validate_manifest(manifest: &BackupManifest) -> Result<(), BackupError> {
    if manifest.archive_format_version != ARCHIVE_FORMAT_VERSION {
        return Err(BackupError::InvalidArchive(format!(
            "unsupported archive format version {}",
            manifest.archive_format_version
        )));
    }
    if manifest.schema_version != SCHEMA_VERSION {
        return Err(BackupError::IncompatibleSchema {
            found: manifest.schema_version,
            supported: SCHEMA_VERSION,
        });
    }
    if manifest.application_version.is_empty()
        || chrono::DateTime::parse_from_rfc3339(&manifest.created_at).is_err()
    {
        return Err(BackupError::InvalidArchive(
            "manifest identity or timestamp is invalid".to_string(),
        ));
    }
    if manifest.files.is_empty() || manifest.files.len() > MAX_ARCHIVE_ENTRIES {
        return Err(BackupError::InvalidArchive(
            "manifest payload count is invalid".to_string(),
        ));
    }
    let mut paths = BTreeSet::new();
    for file in &manifest.files {
        validate_archive_path(&file.path)?;
        if file.path == MANIFEST_PATH || !paths.insert(file.path.as_str()) {
            return Err(BackupError::InvalidArchive(
                "manifest contains duplicate or reserved paths".to_string(),
            ));
        }
        if file.size_bytes > MAX_ENTRY_BYTES
            || file.sha256.len() != 64
            || !file
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(BackupError::InvalidArchive(format!(
                "manifest integrity metadata is invalid for {}",
                file.path
            )));
        }
    }
    if !paths.contains(DATABASE_ARCHIVE_PATH) {
        return Err(BackupError::InvalidArchive(
            "manifest does not contain the SQLite snapshot".to_string(),
        ));
    }
    const REQUIRED_COUNTS: [&str; 10] = [
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
    ];
    if manifest.logical_counts.len() != REQUIRED_COUNTS.len()
        || REQUIRED_COUNTS
            .iter()
            .any(|table| !manifest.logical_counts.contains_key(*table))
    {
        return Err(BackupError::InvalidArchive(
            "manifest logical counts are incomplete".to_string(),
        ));
    }
    checked_payload_total(&manifest.files)?;
    Ok(())
}

/// Sums declared payload bytes with overflow and format limits.
fn checked_payload_total(files: &[BackupFile]) -> Result<u64, BackupError> {
    let total = files.iter().try_fold(0_u64, |total, file| {
        total.checked_add(file.size_bytes).ok_or_else(|| {
            BackupError::InvalidArchive("manifest payload size overflow".to_string())
        })
    })?;
    if total > MAX_TOTAL_BYTES {
        return Err(BackupError::InvalidArchive(
            "manifest payload exceeds the supported size".to_string(),
        ));
    }
    Ok(total)
}

/// Hashes a reader with bounded, checked byte accounting.
fn hash_reader(reader: &mut impl Read, maximum: u64) -> Result<(u64, String), BackupError> {
    let mut hasher = Sha256::new();
    let mut size_bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size_bytes = size_bytes
            .checked_add(read as u64)
            .ok_or_else(|| BackupError::InvalidArchive("payload size overflow".to_string()))?;
        if size_bytes > maximum {
            return Err(BackupError::InvalidArchive(
                "payload exceeds the supported size".to_string(),
            ));
        }
        hasher.update(&buffer[..read]);
    }
    Ok((size_bytes, lowercase_hex(&hasher.finalize())))
}

/// Encodes digest bytes as lowercase hexadecimal without another dependency.
fn lowercase_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

/// Requires an explicit path to name a regular non-symlink file.
fn require_regular_file(path: &Path, label: &str) -> Result<(), BackupError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| BackupError::InvalidInput(label.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(BackupError::InvalidInput(label.to_string()));
    }
    Ok(())
}

/// Requires an explicit path to name a directory.
fn require_directory(path: &Path, label: &str) -> Result<(), BackupError> {
    let metadata = fs::metadata(path).map_err(|_| BackupError::InvalidInput(label.to_string()))?;
    if !metadata.is_dir() {
        return Err(BackupError::InvalidInput(label.to_string()));
    }
    Ok(())
}

/// Requires a database-stored filename to contain exactly one normal component.
fn is_single_component(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\\')
        && matches!(
            Path::new(value).components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        )
}

/// Returns one normalized UTF-8 archive path after rejecting unsafe components.
fn normalized_archive_path<R: Read>(entry: &tar::Entry<'_, R>) -> Result<String, BackupError> {
    let path = entry
        .path()
        .map_err(|_| BackupError::InvalidArchive("archive path is invalid".to_string()))?;
    let value = path
        .to_str()
        .ok_or_else(|| BackupError::InvalidArchive("archive path is not UTF-8".to_string()))?;
    validate_archive_path(value)?;
    Ok(value.to_string())
}

/// Rejects absolute, traversal, empty, and cross-platform ambiguous paths.
fn validate_archive_path(value: &str) -> Result<(), BackupError> {
    if value.is_empty()
        || value.contains('\\')
        || value.starts_with('/')
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || Path::new(value)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(BackupError::InvalidArchive(format!(
            "unsafe archive path: {value}"
        )));
    }
    Ok(())
}

/// Requires one tar entry to be a regular file and optionally names its role.
fn require_regular_entry<R: Read>(
    entry: &tar::Entry<'_, R>,
    label: &str,
) -> Result<(), BackupError> {
    if !entry.header().entry_type().is_file() {
        return Err(BackupError::InvalidArchive(format!(
            "{label} is not a regular file"
        )));
    }
    Ok(())
}
