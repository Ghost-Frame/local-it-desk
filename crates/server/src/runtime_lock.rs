//! Advisory process lock shared by the server and offline restore command.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use thiserror::Error;

/// Stable lock filename stored beside the SQLite database.
pub const RUNTIME_LOCK_NAME: &str = ".local-it-desk.lock";

/// Held exclusive runtime lock released automatically when dropped.
pub struct RuntimeLock {
    /// Open file descriptor carrying the advisory lock.
    file: File,
    /// Exact lock path retained for operator diagnostics.
    path: PathBuf,
}

/// Safe failures while acquiring the single-process runtime boundary.
#[derive(Debug, Error)]
pub enum RuntimeLockError {
    /// Another server or restore process currently owns the lock.
    #[error("Local IT Desk is running or another restore operation is active")]
    AlreadyHeld,
    /// The lock file could not be opened or secured.
    #[error("runtime lock failed: {0}")]
    Io(#[from] io::Error),
}

/// Acquires the exclusive lock beside one explicit database path.
pub fn acquire_runtime_lock(database_path: &Path) -> Result<RuntimeLock, RuntimeLockError> {
    let parent = database_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let path = parent.join(RUNTIME_LOCK_NAME);
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.try_lock_exclusive().map_err(|error| {
        if error.kind() == io::ErrorKind::WouldBlock {
            RuntimeLockError::AlreadyHeld
        } else {
            RuntimeLockError::Io(error)
        }
    })?;
    Ok(RuntimeLock { file, path })
}

/// Diagnostic accessors for one held runtime lock.
impl RuntimeLock {
    /// Returns the exact lock path owned by this guard.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Releases the advisory lock while retaining the harmless lock file.
impl Drop for RuntimeLock {
    /// Unlocks the file descriptor before it closes.
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}
