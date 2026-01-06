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

    #[test]
    fn test_circuit_breaker_default() {
        let breaker = CircuitBreaker::default();
        assert_eq!(breaker.state(), CircuitState::Closed);
        assert_eq!(breaker.failure_threshold, 5);
        assert_eq!(breaker.recovery_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_circuit_state_variants() {
        assert_eq!(CircuitState::Closed, CircuitState::Closed);
        assert_eq!(CircuitState::Open, CircuitState::Open);
        assert_eq!(CircuitState::HalfOpen, CircuitState::HalfOpen);
        assert_ne!(CircuitState::Closed, CircuitState::Open);
        assert_ne!(CircuitState::Open, CircuitState::HalfOpen);
    }

    #[test]
    fn test_circuit_state_debug() {
        let state = CircuitState::Closed;
        let debug_str = format!("{:?}", state);
        assert!(debug_str.contains("Closed"));
    }

    #[test]
    fn test_circuit_state_clone() {
        let state = CircuitState::Open;
        let cloned = state;
        assert_eq!(cloned, CircuitState::Open);
    }

    #[test]
    fn test_failure_in_open_state_ignored() {
        let breaker = CircuitBreaker::new(2, Duration::from_secs(30));

        // Open the circuit
        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);

        // Further failures should be ignored
        let count_before = breaker.failure_count();
        breaker.record_failure();
        assert_eq!(breaker.failure_count(), count_before);
    }

    #[test]
    fn test_success_in_open_state_ignored() {
        let breaker = CircuitBreaker::new(2, Duration::from_secs(30));

        // Open the circuit
        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);

        // Success in open state should be ignored (no transition)
        breaker.record_success();
        assert_eq!(breaker.state(), CircuitState::Open);
    }

    #[test]
    fn test_half_open_transition_after_timeout() {
        let breaker = CircuitBreaker::new(2, Duration::from_millis(10));

        // Open the circuit
        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);

        // Wait for recovery timeout
        std::thread::sleep(Duration::from_millis(20));

        // Should transition to half-open
        assert_eq!(breaker.state(), CircuitState::HalfOpen);
        assert!(breaker.allow()); // Half-open allows requests
    }

    #[test]
    fn test_half_open_success_closes_circuit() {
        let breaker = CircuitBreaker::new(2, Duration::from_millis(10));

        // Open the circuit
        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);

        // Wait for recovery timeout
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(breaker.state(), CircuitState::HalfOpen);

        // 3 successes should close the circuit
        breaker.record_success();
        breaker.record_success();
        breaker.record_success();
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[test]
    fn test_half_open_failure_reopens_circuit() {
        let breaker = CircuitBreaker::new(2, Duration::from_millis(10));

        // Open the circuit
        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);

        // Wait for recovery timeout
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(breaker.state(), CircuitState::HalfOpen);

        // Failure in half-open should reopen
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);
    }

    #[test]
    fn test_half_open_partial_success() {
        let breaker = CircuitBreaker::new(2, Duration::from_millis(10));

        // Open the circuit
        breaker.record_failure();
        breaker.record_failure();

        // Wait for recovery timeout
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(breaker.state(), CircuitState::HalfOpen);

        // Only 2 successes (need 3 to close)
        breaker.record_success();
        breaker.record_success();
        assert_eq!(breaker.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_failure_count_accuracy() {
        let breaker = CircuitBreaker::new(10, Duration::from_secs(30));

        for i in 1..=5 {
            breaker.record_failure();
            assert_eq!(breaker.failure_count(), i);
        }
    }

    #[test]
    fn test_reset_clears_failure_count() {
        let breaker = CircuitBreaker::new(10, Duration::from_secs(30));

        breaker.record_failure();
        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.failure_count(), 3);

        breaker.reset();
        assert_eq!(breaker.failure_count(), 0);
    }

    #[test]
    fn test_allow_in_closed_state() {
        let breaker = CircuitBreaker::new(5, Duration::from_secs(30));
        assert!(breaker.allow());
        breaker.record_failure();
        assert!(breaker.allow()); // Still closed
    }

    #[test]
    fn test_no_transition_before_timeout() {
        let breaker = CircuitBreaker::new(2, Duration::from_secs(60));

        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);

        // No sleep, should still be open
        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(!breaker.allow());
    }

    #[test]
    fn test_multiple_resets() {
        let breaker = CircuitBreaker::new(2, Duration::from_secs(30));

        breaker.record_failure();
        breaker.record_failure();
        breaker.reset();
        assert_eq!(breaker.state(), CircuitState::Closed);

        breaker.record_failure();
        breaker.record_failure();
        breaker.reset();
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[test]
    fn test_threshold_boundary() {
        let breaker = CircuitBreaker::new(3, Duration::from_secs(30));

        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Closed);
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Closed);
        // Exactly at threshold
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);
    }

    #[test]
    fn test_single_failure_threshold() {
        let breaker = CircuitBreaker::new(1, Duration::from_secs(30));

        assert_eq!(breaker.state(), CircuitState::Closed);
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);
    }
}

// Property-based tests
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Circuit stays closed until exactly threshold failures
        #[test]
        fn opens_exactly_at_threshold(threshold in 1u32..50) {
            let breaker = CircuitBreaker::new(threshold, Duration::from_secs(60));

            // Record threshold - 1 failures, should stay closed
            for _ in 0..(threshold - 1) {
                breaker.record_failure();
                prop_assert_eq!(breaker.state(), CircuitState::Closed);
            }

            // The threshold-th failure opens the circuit
            breaker.record_failure();
            prop_assert_eq!(breaker.state(), CircuitState::Open);
        }

        /// Reset always returns to closed state with zero failures
        #[test]
        fn reset_always_closes(
            threshold in 1u32..20,
            failures in 0u32..30
        ) {
            let breaker = CircuitBreaker::new(threshold, Duration::from_secs(60));

            // Record some failures
            for _ in 0..failures {
                breaker.record_failure();
            }

            // Reset should always work
            breaker.reset();
            prop_assert_eq!(breaker.state(), CircuitState::Closed);
            prop_assert_eq!(breaker.failure_count(), 0);
        }

        /// Success in closed state resets failure count
        #[test]
        fn success_resets_failures_in_closed(
            threshold in 3u32..20,
            failures in 1u32..3
        ) {
            // threshold > failures so we stay closed
            let breaker = CircuitBreaker::new(threshold, Duration::from_secs(60));

            for _ in 0..failures {
                breaker.record_failure();
            }
            prop_assert!(breaker.failure_count() > 0);

            breaker.record_success();
            prop_assert_eq!(breaker.failure_count(), 0);
        }

        /// Open circuit doesn't allow requests
        #[test]
        fn open_circuit_blocks(threshold in 1u32..10) {
            let breaker = CircuitBreaker::new(threshold, Duration::from_secs(60));

            // Open the circuit
            for _ in 0..threshold {
                breaker.record_failure();
            }

            // Should not allow requests
            prop_assert!(!breaker.allow());
        }

        /// Closed circuit always allows requests
        #[test]
        fn closed_circuit_allows(
            threshold in 2u32..20,
            failures in 0u32..2 // Always less than min threshold (2)
        ) {
            let breaker = CircuitBreaker::new(threshold, Duration::from_secs(60));

            for _ in 0..failures {
                breaker.record_failure();
            }

            // Circuit stays closed, so allow should be true
            prop_assert_eq!(breaker.state(), CircuitState::Closed);
            prop_assert!(breaker.allow());
        }

        /// Failure count is accurately tracked
        #[test]
        fn failure_count_accurate(
            threshold in 10u32..50,
            failures in 1u32..10 // Keep below threshold
        ) {
            let breaker = CircuitBreaker::new(threshold, Duration::from_secs(60));

            for _ in 0..failures {
                breaker.record_failure();
            }

            prop_assert_eq!(breaker.failure_count(), failures as u64);
        }
    }
}
