//! SQLite connection pool construction and safe interaction helpers.

use std::path::Path;

use deadpool_sqlite::{Config as PoolConfig, Hook, Manager, Pool, Runtime};
use rusqlite::Connection;

use crate::error::{AppError, AppResult};

/// Creates a bounded SQLite pool and applies safety pragmas to every connection.
pub fn create_pool(database_path: &Path) -> Pool {
    let config = PoolConfig::new(database_path.to_string_lossy().into_owned());
    let manager = Manager::from_config(&config, Runtime::Tokio1);

    Pool::builder(manager)
        .max_size(8)
        .post_create(Hook::async_fn(|connection, _metrics| {
            Box::pin(async move {
                connection
                    .interact(|connection| {
                        connection.execute_batch(
                            "PRAGMA journal_mode = WAL;
                             PRAGMA foreign_keys = ON;
                             PRAGMA busy_timeout = 5000;",
                        )
                    })
                    .await
                    .map_err(|error| deadpool_sqlite::HookError::message(error.to_string()))?
                    .map_err(|error| deadpool_sqlite::HookError::message(error.to_string()))?;
                Ok(())
            })
        }))
        .build()
        .expect("database pool configuration must be valid")
}

/// Runs one synchronous SQLite operation without blocking the async runtime.
pub async fn interact<F, T>(pool: &Pool, operation: F) -> AppResult<T>
where
    F: FnOnce(&mut Connection) -> AppResult<T> + Send + 'static,
    T: Send + 'static,
{
    pool.get()
        .await
        .map_err(|error| AppError::Internal(error.to_string()))?
        .interact(operation)
        .await
        .map_err(AppError::Pool)?
}
