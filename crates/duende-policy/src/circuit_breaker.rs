//! Circuit breaker pattern implementation.
//!
//! # Reference
//! Fowler, M. (2014). Circuit Breaker pattern. martinfowler.com.
//!
//! # Toyota Way: Jidoka (自働化)
//! Automatic stop when failure threshold reached.

use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Circuit breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed (requests allowed).
    Closed,
    /// Circuit is open (requests blocked).
    Open,
    /// Circuit is half-open (testing recovery).
    HalfOpen,
}

/// Circuit breaker for failure protection.
///
/// Implements the three-state circuit breaker pattern:
/// - **Closed**: Normal operation, requests pass through
/// - **Open**: Failure threshold exceeded, requests blocked
/// - **Half-Open**: Testing if service has recovered
pub struct CircuitBreaker {
    /// Current state.
    state: RwLock<CircuitState>,
    /// Failure threshold before opening.
    failure_threshold: u32,
    /// Current failure count.
    failure_count: AtomicU64,
    /// Success count in half-open state.
    success_count: AtomicU64,
    /// Recovery timeout.
    recovery_timeout: Duration,
    /// Time when circuit opened.
    opened_at: RwLock<Option<Instant>>,
}

impl CircuitBreaker {
    /// Creates a new circuit breaker.
    ///
    /// # Arguments
    /// * `failure_threshold` - Number of failures before opening circuit
    /// * `recovery_timeout` - Time to wait before testing recovery
    #[must_use]
    pub fn new(failure_threshold: u32, recovery_timeout: Duration) -> Self {
        Self {
            state: RwLock::new(CircuitState::Closed),
            failure_threshold,
            failure_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            recovery_timeout,
            opened_at: RwLock::new(None),
        }
    }

    /// Returns the current circuit state.
    #[must_use]
    pub fn state(&self) -> CircuitState {
        self.maybe_transition();
        *self
            .state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Returns true if requests should be allowed.
    #[must_use]
    pub fn allow(&self) -> bool {
        self.maybe_transition();

        let state = self
            .state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match *state {
            CircuitState::Closed | CircuitState::HalfOpen => true,
            CircuitState::Open => false,
        }
    }

    /// Records a successful request.
    pub fn record_success(&self) {
        let state = self
            .state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        match *state {
            CircuitState::Closed => {
                // Reset failure count on success
                self.failure_count.store(0, Ordering::Relaxed);
            }
            CircuitState::HalfOpen => {
                // Increment success count
                let count = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;
                // Close circuit after enough successes
                if count >= 3 {
                    drop(state);
                    self.close();
                }
            }
            CircuitState::Open => {
                // Shouldn't happen, but ignore
            }
        }
    }

    /// Records a failed request.
    pub fn record_failure(&self) {
        let state = self
            .state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        match *state {
            CircuitState::Closed => {
                let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
                if count >= u64::from(self.failure_threshold) {
                    drop(state);
                    self.open();
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open reopens circuit
                drop(state);
                self.open();
            }
            CircuitState::Open => {
                // Already open, ignore
            }
        }
    }

    /// Returns the failure count.
    #[must_use]
    pub fn failure_count(&self) -> u64 {
        self.failure_count.load(Ordering::Relaxed)
    }

    /// Resets the circuit breaker to closed state.
    pub fn reset(&self) {
        self.close();
    }

    /// Opens the circuit.
    fn open(&self) {
        {
            let mut state = self
                .state
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *state = CircuitState::Open;
        }

        {
            let mut opened_at = self
                .opened_at
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *opened_at = Some(Instant::now());
        }

        self.success_count.store(0, Ordering::Relaxed);

        tracing::warn!("circuit breaker opened");
    }

    /// Closes the circuit.
    fn close(&self) {
        {
            let mut state = self
                .state
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *state = CircuitState::Closed;
        }

        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);

        {
            let mut opened_at = self
                .opened_at
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *opened_at = None;
        }

        tracing::info!("circuit breaker closed");
    }

    /// Transitions to half-open if recovery timeout has passed.
    fn maybe_transition(&self) {
        let state = self
            .state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if *state != CircuitState::Open {
            return;
        }
        drop(state);

        let opened_at = self
            .opened_at
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let should_transition = opened_at
            .map(|opened| opened.elapsed() >= self.recovery_timeout)
            .unwrap_or(false);
        drop(opened_at);

        if should_transition {
            let mut state = self
                .state
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if *state == CircuitState::Open {
                *state = CircuitState::HalfOpen;
                drop(state);
                self.success_count.store(0, Ordering::Relaxed);
                tracing::info!("circuit breaker half-open, testing recovery");
            }
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(5, Duration::from_secs(30))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_closed_by_default() {
        let breaker = CircuitBreaker::new(5, Duration::from_secs(30));
        assert_eq!(breaker.state(), CircuitState::Closed);
        assert!(breaker.allow());
    }

    #[test]
    fn test_circuit_breaker_opens_on_failures() {
        let breaker = CircuitBreaker::new(3, Duration::from_secs(30));

        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Closed);

        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(!breaker.allow());
    }

    #[test]
    fn test_circuit_breaker_success_resets_count() {
        let breaker = CircuitBreaker::new(5, Duration::from_secs(30));

        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.failure_count(), 2);

        breaker.record_success();
        assert_eq!(breaker.failure_count(), 0);
    }

    #[test]
    fn test_circuit_breaker_reset() {
        let breaker = CircuitBreaker::new(2, Duration::from_secs(30));

        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);

        breaker.reset();
        assert_eq!(breaker.state(), CircuitState::Closed);
    }
}
