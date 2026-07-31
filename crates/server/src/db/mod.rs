/// Database connection pool and migration management.
pub mod migrations;
pub mod pool;

pub use pool::{create_pool, interact};
