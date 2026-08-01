//! Offline administrative maintenance command for Local IT Desk.

use std::io::{self, IsTerminal, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use local_it_desk_server::admin_cli::{RecoveryError, reset_password};
use local_it_desk_server::backup::{
    BackupError, RestoreMode, create_backup, restore_backup, verify_backup,
};

/// Maximum redirected input accepted for two bounded password values.
const MAX_INPUT_BYTES: u64 = 2_048;

/// Local-only administrative maintenance entry point.
#[derive(Parser)]
#[command(
    name = "local-it-desk-admin",
    about = "Local IT Desk backup and offline maintenance",
    after_help = "Backups use SQLite online snapshot semantics. Replacement passwords are read from a hidden terminal or standard input and are never accepted as command-line arguments. Stop the server before password recovery."
)]
struct Cli {
    /// Selected offline maintenance operation.
    #[command(subcommand)]
    command: Command,
}

/// Supported offline maintenance operations.
#[derive(Subcommand)]
enum Command {
    /// Create a consistent, self-verifying backup archive without stopping the server.
    Backup {
        /// Existing SQLite database file to snapshot.
        #[arg(long, value_name = "PATH")]
        database: PathBuf,
        /// Persistent attachment directory containing database-owned files.
        #[arg(long, value_name = "PATH")]
        attachments: PathBuf,
        /// Persistent branding directory containing the active logo.
        #[arg(long, value_name = "PATH")]
        branding: PathBuf,
        /// New archive path that must not already exist.
        #[arg(long, value_name = "FILE")]
        output: PathBuf,
    },
    /// Replace one administrator password and revoke every active session.
    ResetPassword {
        /// Existing SQLite database file to update.
        #[arg(long, value_name = "PATH")]
        database: PathBuf,
        /// Exact normalized username of the administrator account.
        #[arg(long, value_name = "USERNAME")]
        username: String,
    },
    /// Verify and optionally activate a backup through staging and quarantine.
    Restore(RestoreArgs),
    /// Stream and verify every entry in one backup archive without extracting it.
    VerifyBackup {
        /// Existing backup archive to verify.
        #[arg(long, value_name = "FILE")]
        archive: PathBuf,
    },
}

/// Mutually exclusive restore mode and explicit source and target paths.
#[derive(Args)]
struct RestoreArgs {
    /// Existing verified backup archive to restore.
    #[arg(long, value_name = "FILE")]
    archive: PathBuf,
    /// Existing active generation directory named current.
    #[arg(long, value_name = "PATH")]
    target_root: PathBuf,
    /// Verify and report the restore plan without any mutation.
    #[arg(long, required_unless_present = "apply", conflicts_with = "apply")]
    dry_run: bool,
    /// Create a safety backup, stage, validate, quarantine, and activate.
    #[arg(long, required_unless_present = "dry_run", conflicts_with = "dry_run")]
    apply: bool,
}

/// Dispatches one selected maintenance operation and returns a classified failure.
fn run(cli: Cli) -> Result<(), CliFailure> {
    match cli.command {
        Command::Backup {
            database,
            attachments,
            branding,
            output,
        } => {
            let summary = create_backup(&database, &attachments, &branding, &output)
                .map_err(CliFailure::from)?;
            println!(
                "Backup created: {} payload file(s), {} byte(s), schema version {}.",
                summary.file_count, summary.payload_bytes, summary.schema_version
            );
            Ok(())
        }
        Command::ResetPassword { database, username } => {
            let password = read_password_pair()?;
            let result =
                reset_password(&database, &username, &password).map_err(CliFailure::from)?;
            println!(
                "Administrator '{}' recovered; {} active session(s) revoked. A password change is required at next login.",
                result.username, result.revoked_sessions
            );
            Ok(())
        }
        Command::Restore(arguments) => {
            let mode = if arguments.apply {
                RestoreMode::Apply
            } else {
                RestoreMode::DryRun
            };
            let summary = restore_backup(&arguments.archive, &arguments.target_root, mode)
                .map_err(CliFailure::Restore)?;
            match mode {
                RestoreMode::DryRun => println!(
                    "Restore dry-run verified: {} payload file(s), {} byte(s); target unchanged.",
                    summary.file_count, summary.payload_bytes
                ),
                RestoreMode::Apply => println!(
                    "Restore applied: pre-restore backup '{}'; previous generation quarantined at '{}'.",
                    summary
                        .pre_restore_backup
                        .as_deref()
                        .expect("apply result includes pre-restore backup")
                        .display(),
                    summary
                        .quarantine
                        .as_deref()
                        .expect("apply result includes quarantine")
                        .display()
                ),
            }
            Ok(())
        }
        Command::VerifyBackup { archive } => {
            let summary = verify_backup(&archive).map_err(CliFailure::from)?;
            println!(
                "Backup verified: {} payload file(s), {} byte(s), schema version {}.",
                summary.file_count, summary.payload_bytes, summary.schema_version
            );
            Ok(())
        }
    }
}

/// Reads matching password values from a hidden terminal or redirected input.
fn read_password_pair() -> Result<String, CliFailure> {
    let stdin = io::stdin();
    let (password, confirmation) = if stdin.is_terminal() {
        let password = rpassword::prompt_password("New password: ")
            .map_err(|_| CliFailure::Input("could not read protected input"))?;
        let confirmation = rpassword::prompt_password("Confirm new password: ")
            .map_err(|_| CliFailure::Input("could not read protected input"))?;
        (password, confirmation)
    } else {
        let mut input = String::new();
        stdin
            .lock()
            .take(MAX_INPUT_BYTES + 1)
            .read_to_string(&mut input)
            .map_err(|_| CliFailure::Input("could not read standard input"))?;
        if input.len() as u64 > MAX_INPUT_BYTES {
            return Err(CliFailure::Input("standard input is too large"));
        }
        let mut lines = input.lines();
        let password = lines
            .next()
            .ok_or(CliFailure::Input("standard input must contain two lines"))?;
        let confirmation = lines
            .next()
            .ok_or(CliFailure::Input("standard input must contain two lines"))?;
        if lines.any(|line| !line.is_empty()) {
            return Err(CliFailure::Input(
                "standard input must contain only password and confirmation",
            ));
        }
        (password.to_string(), confirmation.to_string())
    };
    if password != confirmation {
        return Err(CliFailure::Input("password confirmation does not match"));
    }
    Ok(password)
}

/// Redacted command failure with one actionable process exit code.
enum CliFailure {
    /// Protected password input was absent, invalid, or inconsistent.
    Input(&'static str),
    /// The explicit database could not be found or opened safely.
    Database(String),
    /// The explicit identity was missing, ambiguous, or not an administrator.
    Target(String),
    /// Recovery failed after the target was safely resolved.
    Operation(String),
    /// Backup creation or archive verification failed safely.
    Backup(String),
    /// Restore validation, exclusion, staging, or activation failed safely.
    Restore(BackupError),
}

/// Maps recovery-domain failures into stable command failure classes.
impl From<RecoveryError> for CliFailure {
    /// Classifies one redacted domain failure for process exit handling.
    fn from(error: RecoveryError) -> Self {
        match error {
            RecoveryError::DatabaseMissing | RecoveryError::Database(_) => {
                Self::Database(error.to_string())
            }
            RecoveryError::IdentityNotNormalized
            | RecoveryError::AccountNotFound
            | RecoveryError::AmbiguousIdentity
            | RecoveryError::NotAdministrator
            | RecoveryError::InvalidIdentity => Self::Target(error.to_string()),
            RecoveryError::InvalidPassword(_) | RecoveryError::PasswordHash => {
                Self::Operation(error.to_string())
            }
        }
    }
}

/// Maps backup-domain failures into one stable maintenance exit class.
impl From<BackupError> for CliFailure {
    /// Redacts internal error categories behind an actionable local message.
    fn from(error: BackupError) -> Self {
        Self::Backup(error.to_string())
    }
}

/// Executes the parsed command and exits without printing credential material.
fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliFailure::Input(message)) => {
            eprintln!("Input error: {message}");
            ExitCode::from(3)
        }
        Err(CliFailure::Database(message)) => {
            eprintln!("Database error: {message}");
            ExitCode::from(4)
        }
        Err(CliFailure::Target(message)) => {
            eprintln!("Target error: {message}");
            ExitCode::from(5)
        }
        Err(CliFailure::Operation(message)) => {
            eprintln!("Recovery error: {message}");
            ExitCode::from(6)
        }
        Err(CliFailure::Backup(message)) => {
            eprintln!("Backup error: {message}");
            ExitCode::from(7)
        }
        Err(CliFailure::Restore(error)) => {
            eprintln!("Restore error: {error}");
            ExitCode::from(8)
        }
    }
}
