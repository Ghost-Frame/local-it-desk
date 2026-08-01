//! Local IT Desk server library.

/// Offline administrator recovery command and transaction boundary.
pub mod admin_cli;
/// Local account roles and authentication foundation.
pub mod auth;
/// Validated environment-backed runtime configuration.
pub mod config;
/// SQLite pool and clean-schema migration support.
pub mod db;
/// HTTP-safe application errors.
pub mod error;
/// Help-desk domain models.
pub mod models;
/// HTTP route construction and retained route families.
pub mod routes;
