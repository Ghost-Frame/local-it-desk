//! Environment-backed Local IT Desk runtime configuration.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use axum::http::Uri;

/// Validated runtime settings shared by startup and request handlers.
#[derive(Debug, Clone)]
pub struct Config {
    /// Address used by the HTTP listener.
    pub listen_addr: SocketAddr,
    /// Browser origin used for cookie and deployment validation.
    pub application_origin: String,
    /// SQLite database path.
    pub database_path: PathBuf,
    /// Persistent attachment directory.
    pub upload_dir: PathBuf,
    /// Persistent operator-provided branding directory.
    pub branding_dir: PathBuf,
    /// Maximum accepted attachment size.
    pub max_upload_bytes: u64,
    /// Maximum accepted raw staff roster CSV size.
    pub max_roster_bytes: u64,
    /// Maximum number of non-empty staff rows in one roster.
    pub max_roster_rows: u64,
    /// Name displayed in the browser.
    pub app_name: String,
    /// Optional operator-provided help contact.
    pub support_contact: Option<String>,
    /// Whether login cookies carry the Secure flag.
    pub cookie_secure: bool,
    /// Absolute session lifetime in days.
    pub session_ttl_days: u64,
    /// Whether the Rust server serves the compiled frontend.
    pub serve_frontend: bool,
    /// Compiled frontend directory.
    pub frontend_dir: PathBuf,
}

/// Environment loading, validation, and runtime directory preparation.
impl Config {
    /// Loads and validates configuration, stopping startup on invalid values.
    pub fn from_env() -> Self {
        let app_name = env_or("APP_NAME", "Local IT Desk");
        let app_name = app_name.trim().to_string();
        assert!(
            !app_name.is_empty(),
            "APP_NAME must contain at least one visible character"
        );

        let application_origin = env_or("APP_ORIGIN", "http://localhost:3000");
        validate_origin(&application_origin);
        let cookie_secure = parse_bool("COOKIE_SECURE", false);
        if application_origin.starts_with("https://") && !cookie_secure {
            panic!("COOKIE_SECURE must be true when APP_ORIGIN uses HTTPS");
        }

        Self {
            listen_addr: env_or("LISTEN_ADDR", "0.0.0.0:3000")
                .parse()
                .expect("LISTEN_ADDR must be a socket address such as 0.0.0.0:3000"),
            application_origin,
            database_path: PathBuf::from(env_or("DATABASE_PATH", "data/local-it-desk.db")),
            upload_dir: PathBuf::from(env_or("UPLOAD_DIR", "uploads")),
            branding_dir: PathBuf::from(env_or("BRANDING_DIR", "branding")),
            max_upload_bytes: parse_bounded_u64(
                "MAX_UPLOAD_BYTES",
                "26214400",
                1,
                1024 * 1024 * 1024,
            ),
            max_roster_bytes: parse_bounded_u64(
                "MAX_ROSTER_BYTES",
                "1048576",
                1024,
                10 * 1024 * 1024,
            ),
            max_roster_rows: parse_bounded_u64("MAX_ROSTER_ROWS", "500", 1, 10_000),
            app_name,
            support_contact: env_optional("SUPPORT_CONTACT"),
            cookie_secure,
            session_ttl_days: parse_bounded_u64("SESSION_TTL_DAYS", "14", 1, 365),
            serve_frontend: parse_bool("SERVE_FRONTEND", false),
            frontend_dir: PathBuf::from(env_or("FRONTEND_DIR", "frontend/dist")),
        }
    }

    /// Builds deterministic settings for unit and integration tests.
    pub fn for_test(database_path: PathBuf, upload_dir: PathBuf) -> Self {
        let runtime_root = database_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        Self {
            listen_addr: "127.0.0.1:0".parse().expect("static test address"),
            application_origin: "http://localhost:3000".to_string(),
            database_path,
            upload_dir,
            branding_dir: runtime_root.join("branding"),
            max_upload_bytes: 25 * 1024 * 1024,
            max_roster_bytes: 1024 * 1024,
            max_roster_rows: 500,
            app_name: "Local IT Desk".to_string(),
            support_contact: None,
            cookie_secure: false,
            session_ttl_days: 14,
            serve_frontend: false,
            frontend_dir: PathBuf::from("frontend/dist"),
        }
    }

    /// Creates writable runtime directories with owner-oriented permissions.
    pub fn prepare_runtime_directories(&self) -> std::io::Result<()> {
        if let Some(database_parent) = self.database_path.parent()
            && !database_parent.as_os_str().is_empty()
        {
            create_runtime_directory(database_parent)?;
        }
        create_runtime_directory(&self.upload_dir)?;
        create_runtime_directory(&self.branding_dir)?;
        Ok(())
    }
}

/// Creates one runtime directory and restricts it on Unix platforms.
fn create_runtime_directory(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o750))?;
    }
    Ok(())
}

/// Validates that an application origin is a path-free HTTP or HTTPS origin.
fn validate_origin(value: &str) {
    let uri = value
        .parse::<Uri>()
        .expect("APP_ORIGIN must be an absolute HTTP or HTTPS origin");
    let scheme = uri
        .scheme_str()
        .expect("APP_ORIGIN must include http:// or https://");
    if !matches!(scheme, "http" | "https") || uri.authority().is_none() {
        panic!("APP_ORIGIN must be an absolute HTTP or HTTPS origin");
    }
    if uri.path() != "/" || uri.query().is_some() {
        panic!("APP_ORIGIN must not contain a path, query, or fragment");
    }
}

/// Returns an environment value or a static default.
fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Returns a trimmed environment value when one was supplied.
fn env_optional(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Parses and bounds an unsigned integer configuration value.
fn parse_bounded_u64(key: &str, default: &str, minimum: u64, maximum: u64) -> u64 {
    let value = env_or(key, default)
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("{key} must be a positive whole number"));
    if !(minimum..=maximum).contains(&value) {
        panic!("{key} must be between {minimum} and {maximum}");
    }
    value
}

/// Parses a boolean configuration value from common operator spellings.
fn parse_bool(key: &str, default: bool) -> bool {
    let fallback = if default { "true" } else { "false" };
    match env_or(key, fallback).to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => true,
        "false" | "0" | "no" | "off" => false,
        _ => panic!("{key} must be true or false"),
    }
}
