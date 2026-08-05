//! Bounded in-memory throttling keyed by normalized identity and direct peer address.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};

/// Maximum failed attempts in one default limiter window.
const DEFAULT_MAX_ATTEMPTS: u32 = 5;
/// Default rolling failure window.
const DEFAULT_WINDOW: Duration = Duration::from_secs(15 * 60);
/// Default lockout after the attempt threshold is reached.
const DEFAULT_LOCKOUT: Duration = Duration::from_secs(15 * 60);
/// Maximum identity and peer pairs retained across all public login attempts.
const MAX_TRACKED_ATTEMPTS: usize = 4_096;

/// Shareable limiter for public credential endpoints.
#[derive(Clone)]
pub struct LoginRateLimiter {
    /// Mutable attempt state shared across router clones.
    entries: Arc<Mutex<HashMap<AttemptKey, AttemptState>>>,
    /// Failures accepted before lockout.
    max_attempts: u32,
    /// Duration after which a non-blocked attempt sequence resets.
    window: Duration,
    /// Duration of threshold-triggered lockout.
    lockout: Duration,
}

/// Normalized account and direct network peer key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AttemptKey {
    /// Bounded normalized account spelling or setup sentinel.
    identity: String,
    /// Direct socket peer address, never a forwarding header.
    peer: IpAddr,
}

/// Failure counters and optional lockout time for one key.
#[derive(Debug, Clone)]
struct AttemptState {
    /// Start of the current rolling failure window.
    window_started: Instant,
    /// Failures recorded during the current window.
    failures: u32,
    /// Time before which all attempts must be rejected.
    blocked_until: Option<Instant>,
}

/// Default limiter construction for production router state.
impl Default for LoginRateLimiter {
    /// Builds the documented five-attempt, fifteen-minute policy.
    fn default() -> Self {
        Self::new(DEFAULT_MAX_ATTEMPTS, DEFAULT_WINDOW, DEFAULT_LOCKOUT)
    }
}

/// Public credential throttling operations.
impl LoginRateLimiter {
    /// Builds a limiter with explicit values for deterministic tests and policy changes.
    pub fn new(max_attempts: u32, window: Duration, lockout: Duration) -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            max_attempts: max_attempts.max(1),
            window,
            lockout,
        }
    }

    /// Rejects a currently blocked identity and peer pair.
    pub fn check(&self, identity: &str, peer: IpAddr) -> AppResult<()> {
        let key = attempt_key(identity, peer);
        let now = Instant::now();
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        entries.retain(|_, state| !state.is_stale(now, self.window, self.lockout));
        if !entries.contains_key(&key) && entries.len() >= MAX_TRACKED_ATTEMPTS {
            return Err(AppError::TooManyRequests);
        }
        if entries
            .get(&key)
            .and_then(|state| state.blocked_until)
            .is_some_and(|blocked_until| blocked_until > now)
        {
            return Err(AppError::TooManyRequests);
        }
        Ok(())
    }

    /// Records one public authentication failure and activates lockout at the threshold.
    pub fn record_failure(&self, identity: &str, peer: IpAddr) {
        let key = attempt_key(identity, peer);
        let now = Instant::now();
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if !entries.contains_key(&key) && entries.len() >= MAX_TRACKED_ATTEMPTS {
            return;
        }
        let state = entries.entry(key).or_insert(AttemptState {
            window_started: now,
            failures: 0,
            blocked_until: None,
        });
        if now.duration_since(state.window_started) >= self.window {
            state.window_started = now;
            state.failures = 0;
            state.blocked_until = None;
        }
        state.failures = state.failures.saturating_add(1);
        if state.failures >= self.max_attempts {
            state.blocked_until = Some(now + self.lockout);
        }
    }

    /// Clears failure state after successful authentication or setup.
    pub fn record_success(&self, identity: &str, peer: IpAddr) {
        let key = attempt_key(identity, peer);
        self.entries
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&key);
    }
}

/// Attempt-state expiry helpers.
impl AttemptState {
    /// Returns whether a record can be removed without weakening active lockout.
    fn is_stale(&self, now: Instant, window: Duration, lockout: Duration) -> bool {
        let window_stale = now.duration_since(self.window_started) >= window + lockout;
        let block_stale = self
            .blocked_until
            .is_none_or(|blocked_until| blocked_until <= now);
        window_stale && block_stale
    }
}

/// Builds a bounded, case-insensitive limiter key without validating account existence.
fn attempt_key(identity: &str, peer: IpAddr) -> AttemptKey {
    AttemptKey {
        identity: identity
            .trim()
            .to_ascii_lowercase()
            .chars()
            .take(64)
            .collect(),
        peer,
    }
}

/// Regression coverage for bounded credential-throttling state.
#[cfg(test)]
mod tests {
    use super::*;

    /// Confirms unique identity floods cannot grow the tracked-attempt map without limit.
    #[test]
    fn unique_identity_flood_is_bounded_and_fails_closed() {
        let limiter = LoginRateLimiter::default();
        let peer = "192.0.2.10".parse().expect("static peer address");
        for index in 0..=MAX_TRACKED_ATTEMPTS {
            limiter.record_failure(&format!("unknown-{index}"), peer);
        }

        let tracked = limiter
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .len();
        assert!(tracked <= MAX_TRACKED_ATTEMPTS);
        assert!(matches!(
            limiter.check("another-unknown-user", peer),
            Err(AppError::TooManyRequests)
        ));
    }
}
