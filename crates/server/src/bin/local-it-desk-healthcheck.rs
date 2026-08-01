//! Dependency-light container readiness probe for the local HTTP server.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::ExitCode;
use std::time::Duration;

/// Maximum readiness response accepted into healthcheck memory.
const MAX_RESPONSE_BYTES: u64 = 16 * 1024;

/// Connects to readiness and validates its status line and exact JSON state.
fn check_readiness() -> Result<(), String> {
    let address = std::env::var("HEALTHCHECK_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3000".to_string())
        .parse::<SocketAddr>()
        .map_err(|_| "HEALTHCHECK_ADDR must be an IP socket address".to_string())?;
    let timeout = Duration::from_secs(2);
    let mut stream = TcpStream::connect_timeout(&address, timeout)
        .map_err(|error| format!("readiness connection failed: {error}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| format!("could not set readiness read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| format!("could not set readiness write timeout: {error}"))?;
    stream
        .write_all(
            b"GET /health/ready HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nAccept: application/json\r\n\r\n",
        )
        .map_err(|error| format!("readiness request failed: {error}"))?;

    let mut response = String::new();
    stream
        .take(MAX_RESPONSE_BYTES)
        .read_to_string(&mut response)
        .map_err(|error| format!("readiness response failed: {error}"))?;
    validate_response(&response)
}

/// Validates the bounded HTTP response without accepting redirects or degraded states.
fn validate_response(response: &str) -> Result<(), String> {
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "readiness response was not valid HTTP".to_string())?;
    let status_line = headers.lines().next().unwrap_or_default();
    if !matches!(status_line, "HTTP/1.1 200 OK" | "HTTP/1.0 200 OK") {
        return Err(format!("readiness returned {status_line}"));
    }
    if body.trim() != r#"{"status":"ready"}"# {
        return Err("readiness returned an unexpected body".to_string());
    }
    Ok(())
}

/// Returns a container-friendly process status without exposing response content.
fn main() -> ExitCode {
    match check_readiness() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("Healthcheck failed: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Focused parser contracts for unhealthy and malformed readiness responses.
#[cfg(test)]
mod tests {
    use super::validate_response;

    /// Accepts only the exact successful readiness state.
    #[test]
    fn accepts_ready_response() {
        assert!(
            validate_response(
                "HTTP/1.1 200 OK\r\nContent-Length: 18\r\n\r\n{\"status\":\"ready\"}"
            )
            .is_ok()
        );
    }

    /// Rejects a non-success response even when its body claims readiness.
    #[test]
    fn rejects_non_success_status() {
        assert!(
            validate_response("HTTP/1.1 503 Service Unavailable\r\n\r\n{\"status\":\"ready\"}")
                .is_err()
        );
    }

    /// Rejects successful HTTP carrying any other application health state.
    #[test]
    fn rejects_unexpected_body() {
        assert!(validate_response("HTTP/1.1 200 OK\r\n\r\n{\"status\":\"ok\"}").is_err());
    }
}
