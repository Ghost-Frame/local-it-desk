//! Offline administrative maintenance command for Local IT Desk.

use std::io::{self, IsTerminal, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use local_it_desk_server::admin_cli::{RecoveryError, reset_password};

/// Maximum redirected input accepted for two bounded password values.
const MAX_INPUT_BYTES: u64 = 2_048;

/// Local-only administrative maintenance entry point.
#[derive(Parser)]
#[command(
    name = "local-it-desk-admin",
    about = "Offline maintenance for a Local IT Desk database",
    after_help = "Replacement passwords are read from a hidden terminal or standard input. They are never accepted as command-line arguments. Stop the server before running recovery."
)]
struct Cli {
    /// Selected offline maintenance operation.
    #[command(subcommand)]
    command: Command,
}

/// Supported offline maintenance operations.
#[derive(Subcommand)]
enum Command {
    /// Replace one administrator password and revoke every active session.
    ResetPassword {
        /// Existing SQLite database file to update.
        #[arg(long, value_name = "PATH")]
        database: PathBuf,
        /// Exact normalized username of the administrator account.
        #[arg(long, value_name = "USERNAME")]
        username: String,
    },
}

/// Reads protected input, runs recovery, and maps failures to documented codes.
fn run(cli: Cli) -> Result<(), CliFailure> {
    match cli.command {
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
    }
}
