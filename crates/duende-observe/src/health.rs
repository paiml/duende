//! Health monitoring for daemons.
//!
//! # Toyota Way: Genchi Genbutsu (現地現物)
//! Direct observation of daemon health via periodic checks.
//!
//! # Toyota Way: Jidoka (自働化)
//! Automatic detection and reporting of unhealthy daemons.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use duende_core::{DaemonId, HealthStatus};
use tokio::sync::{broadcast, RwLock};
use tokio::time::interval;

use crate::error::{ObserveError, Result};

// =============================================================================
// HealthEvent
// =============================================================================

/// Health state transition event.
#[derive(Debug, Clone)]
pub enum HealthEvent {
    /// Daemon became healthy.
    Healthy {
        /// Daemon ID.
        id: DaemonId,
        /// Health status.
        status: HealthStatus,
    },
    /// Daemon became unhealthy.
    Unhealthy {
        /// Daemon ID.
        id: DaemonId,
        /// Health status.
        status: HealthStatus,
        /// Consecutive failure count.
        failure_count: u32,
    },
    /// Health check timed out.
    Timeout {
        /// Daemon ID.
        id: DaemonId,
        /// Timeout duration.
        timeout: Duration,
    },
    /// Daemon recovered after failures.
    Recovered {
        /// Daemon ID.
        id: DaemonId,
        /// Number of failures before recovery.
        failures_before_recovery: u32,
    },
}

// =============================================================================
// HealthConfig
// =============================================================================

/// Configuration for health monitoring.
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Interval between health checks.
    pub check_interval: Duration,
    /// Timeout for individual health checks.
    pub check_timeout: Duration,
    /// Number of consecutive failures before marking unhealthy.
    pub failure_threshold: u32,
    /// Number of consecutive successes before marking recovered.
    pub recovery_threshold: u32,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(30),
            check_timeout: Duration::from_secs(5),
            failure_threshold: 3,
            recovery_threshold: 2,
        }
    }
}

impl HealthConfig {
    /// Creates a new health config.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the check interval.
    #[must_use]
    pub const fn with_check_interval(mut self, interval: Duration) -> Self {
        self.check_interval = interval;
        self
    }

    /// Sets the check timeout.
    #[must_use]
    pub const fn with_check_timeout(mut self, timeout: Duration) -> Self {
        self.check_timeout = timeout;
        self
    }

    /// Sets the failure threshold.
    #[must_use]
    pub const fn with_failure_threshold(mut self, threshold: u32) -> Self {
        self.failure_threshold = threshold;
        self
    }

    /// Sets the recovery threshold.
    #[must_use]
    pub const fn with_recovery_threshold(mut self, threshold: u32) -> Self {
        self.recovery_threshold = threshold;
        self
    }
}

// =============================================================================
// DaemonHealthState
// =============================================================================

/// Health state for a single daemon.
#[derive(Debug, Clone)]
pub struct DaemonHealthState {
    /// Daemon ID.
    pub id: DaemonId,
    /// Current health status (if checked).
    pub last_status: Option<HealthStatus>,
    /// Last check timestamp.
    pub last_check: Option<Instant>,
    /// Consecutive failure count.
    pub consecutive_failures: u32,
    /// Consecutive success count.
    pub consecutive_successes: u32,
    /// Whether the daemon is currently considered healthy.
    pub is_healthy: bool,
    /// Total checks performed.
    pub total_checks: u64,
    /// Total failures.
    pub total_failures: u64,
}

impl DaemonHealthState {
    /// Creates a new health state for a daemon.
    #[must_use]
    pub fn new(id: DaemonId) -> Self {
        Self {
            id,
            last_status: None,
            last_check: None,
            consecutive_failures: 0,
            consecutive_successes: 0,
            is_healthy: true, // Assume healthy until proven otherwise
            total_checks: 0,
            total_failures: 0,
        }
    }

    /// Records a successful health check.
    pub fn record_success(&mut self, status: HealthStatus) {
        self.last_status = Some(status);
        self.last_check = Some(Instant::now());
        self.consecutive_failures = 0;
        self.consecutive_successes += 1;
        self.total_checks += 1;
    }

    /// Records a failed health check.
    pub fn record_failure(&mut self, status: Option<HealthStatus>) {
        self.last_status = status;
        self.last_check = Some(Instant::now());
        self.consecutive_failures += 1;
        self.consecutive_successes = 0;
        self.total_checks += 1;
        self.total_failures += 1;
    }

    /// Returns the failure rate as a percentage.
    #[must_use]
    pub fn failure_rate(&self) -> f64 {
        if self.total_checks == 0 {
            0.0
        } else {
            (self.total_failures as f64 / self.total_checks as f64) * 100.0
        }
    }

    /// Returns time since last check.
    #[must_use]
    pub fn time_since_last_check(&self) -> Option<Duration> {
        self.last_check.map(|t| t.elapsed())
    }
}

// =============================================================================
// HealthMonitor
// =============================================================================

/// Monitor for daemon health checks.
///
/// Provides periodic health checking with configurable thresholds
/// and event broadcasting for health state transitions.
pub struct HealthMonitor {
    /// Configuration.
    config: HealthConfig,
    /// Health state per daemon.
    states: Arc<RwLock<HashMap<DaemonId, DaemonHealthState>>>,
    /// Event broadcaster.
    event_tx: broadcast::Sender<HealthEvent>,
    /// Whether the monitor is running.
    running: Arc<RwLock<bool>>,
}

impl HealthMonitor {
    /// Creates a new health monitor.
    #[must_use]
    pub fn new(config: HealthConfig) -> Self {
        let (event_tx, _) = broadcast::channel(256);

        Self {
            config,
            states: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Creates a monitor with default configuration.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(HealthConfig::default())
    }

    /// Returns the configuration.
    #[must_use]
    pub fn config(&self) -> &HealthConfig {
        &self.config
    }

    /// Subscribes to health events.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<HealthEvent> {
        self.event_tx.subscribe()
    }

    /// Registers a daemon for health monitoring.
    pub async fn register(&self, id: DaemonId) {
        let mut states = self.states.write().await;
        states.insert(id, DaemonHealthState::new(id));
        tracing::debug!(id = %id, "registered daemon for health monitoring");
    }

    /// Unregisters a daemon from health monitoring.
    pub async fn unregister(&self, id: DaemonId) {
        let mut states = self.states.write().await;
        states.remove(&id);
        tracing::debug!(id = %id, "unregistered daemon from health monitoring");
    }

    /// Returns the health state for a daemon.
    pub async fn get_state(&self, id: DaemonId) -> Option<DaemonHealthState> {
        let states = self.states.read().await;
        states.get(&id).cloned()
    }

    /// Returns all health states.
    pub async fn get_all_states(&self) -> Vec<DaemonHealthState> {
        let states = self.states.read().await;
        states.values().cloned().collect()
    }

    /// Returns the number of monitored daemons.
    pub async fn count(&self) -> usize {
        self.states.read().await.len()
    }

    /// Returns the number of healthy daemons.
    pub async fn healthy_count(&self) -> usize {
        let states = self.states.read().await;
        states.values().filter(|s| s.is_healthy).count()
    }

    /// Returns the number of unhealthy daemons.
    pub async fn unhealthy_count(&self) -> usize {
        let states = self.states.read().await;
        states.values().filter(|s| !s.is_healthy).count()
    }

    /// Records a health check result for a daemon.
    ///
    /// This method handles:
    /// - Updating health state
    /// - Checking thresholds
    /// - Broadcasting events
    pub async fn record_check(&self, id: DaemonId, status: HealthStatus) -> Result<()> {
        let mut states = self.states.write().await;

        let state = states
            .get_mut(&id)
            .ok_or_else(|| ObserveError::NotFound(format!("daemon {} not registered", id)))?;

        let was_healthy = state.is_healthy;

        if status.is_healthy() {
            state.record_success(status.clone());

            // Check recovery threshold
            if !was_healthy && state.consecutive_successes >= self.config.recovery_threshold {
                let failures_before = state.total_failures - state.consecutive_successes as u64;
                state.is_healthy = true;

                let _ = self.event_tx.send(HealthEvent::Recovered {
                    id,
                    failures_before_recovery: failures_before as u32,
                });

                tracing::info!(id = %id, "daemon recovered after {} failures", failures_before);
            }

            if state.is_healthy {
                let _ = self.event_tx.send(HealthEvent::Healthy {
                    id,
                    status,
                });
            }
        } else {
            state.record_failure(Some(status.clone()));

            // Check failure threshold
            if was_healthy && state.consecutive_failures >= self.config.failure_threshold {
                state.is_healthy = false;

                let _ = self.event_tx.send(HealthEvent::Unhealthy {
                    id,
                    status: status.clone(),
                    failure_count: state.consecutive_failures,
                });

                tracing::warn!(
                    id = %id,
                    failures = state.consecutive_failures,
                    "daemon marked unhealthy"
                );
            } else {
                let _ = self.event_tx.send(HealthEvent::Unhealthy {
                    id,
                    status,
                    failure_count: state.consecutive_failures,
                });
            }
        }

        Ok(())
    }

    /// Records a health check timeout.
    pub async fn record_timeout(&self, id: DaemonId) -> Result<()> {
        let mut states = self.states.write().await;

        let state = states
            .get_mut(&id)
            .ok_or_else(|| ObserveError::NotFound(format!("daemon {} not registered", id)))?;

        state.record_failure(None);

        let _ = self.event_tx.send(HealthEvent::Timeout {
            id,
            timeout: self.config.check_timeout,
        });

        // Check failure threshold
        if state.is_healthy && state.consecutive_failures >= self.config.failure_threshold {
            state.is_healthy = false;

            tracing::warn!(
                id = %id,
                failures = state.consecutive_failures,
                "daemon marked unhealthy due to timeouts"
            );
        }

        Ok(())
    }

    /// Returns aggregate health statistics.
    pub async fn statistics(&self) -> HealthStatistics {
        let states = self.states.read().await;

        let total = states.len();
        let healthy = states.values().filter(|s| s.is_healthy).count();
        let unhealthy = total - healthy;

        let total_checks: u64 = states.values().map(|s| s.total_checks).sum();
        let total_failures: u64 = states.values().map(|s| s.total_failures).sum();

        let avg_failure_rate = if !states.is_empty() {
            states.values().map(|s| s.failure_rate()).sum::<f64>() / states.len() as f64
        } else {
            0.0
        };

        HealthStatistics {
            total_daemons: total,
            healthy_daemons: healthy,
            unhealthy_daemons: unhealthy,
            total_checks,
            total_failures,
            average_failure_rate: avg_failure_rate,
        }
    }
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// =============================================================================
// HealthStatistics
// =============================================================================

/// Aggregate health statistics.
#[derive(Debug, Clone)]
pub struct HealthStatistics {
    /// Total number of monitored daemons.
    pub total_daemons: usize,
    /// Number of healthy daemons.
    pub healthy_daemons: usize,
    /// Number of unhealthy daemons.
    pub unhealthy_daemons: usize,
    /// Total health checks performed.
    pub total_checks: u64,
    /// Total failed health checks.
    pub total_failures: u64,
    /// Average failure rate across all daemons.
    pub average_failure_rate: f64,
}

impl HealthStatistics {
    /// Returns the health ratio (healthy / total).
    #[must_use]
    pub fn health_ratio(&self) -> f64 {
        if self.total_daemons == 0 {
            1.0
        } else {
            self.healthy_daemons as f64 / self.total_daemons as f64
        }
    }

    /// Returns true if all daemons are healthy.
    #[must_use]
    pub fn all_healthy(&self) -> bool {
        self.unhealthy_daemons == 0
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // HealthConfig Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_health_config_default() {
        let config = HealthConfig::default();
        assert_eq!(config.check_interval, Duration::from_secs(30));
        assert_eq!(config.check_timeout, Duration::from_secs(5));
        assert_eq!(config.failure_threshold, 3);
        assert_eq!(config.recovery_threshold, 2);
    }

    #[test]
    fn test_health_config_builder() {
        let config = HealthConfig::new()
            .with_check_interval(Duration::from_secs(10))
            .with_check_timeout(Duration::from_secs(2))
            .with_failure_threshold(5)
            .with_recovery_threshold(3);

        assert_eq!(config.check_interval, Duration::from_secs(10));
        assert_eq!(config.check_timeout, Duration::from_secs(2));
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.recovery_threshold, 3);
    }

    // -------------------------------------------------------------------------
    // DaemonHealthState Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_health_state_new() {
        let id = DaemonId::new();
        let state = DaemonHealthState::new(id);

        assert_eq!(state.id, id);
        assert!(state.last_status.is_none());
        assert!(state.last_check.is_none());
        assert_eq!(state.consecutive_failures, 0);
        assert_eq!(state.consecutive_successes, 0);
        assert!(state.is_healthy);
        assert_eq!(state.total_checks, 0);
        assert_eq!(state.total_failures, 0);
    }

    #[test]
    fn test_health_state_record_success() {
        let id = DaemonId::new();
        let mut state = DaemonHealthState::new(id);

        let status = HealthStatus::healthy(5);
        state.record_success(status);

        assert!(state.last_status.is_some());
        assert!(state.last_check.is_some());
        assert_eq!(state.consecutive_failures, 0);
        assert_eq!(state.consecutive_successes, 1);
        assert_eq!(state.total_checks, 1);
        assert_eq!(state.total_failures, 0);
    }

    #[test]
    fn test_health_state_record_failure() {
        let id = DaemonId::new();
        let mut state = DaemonHealthState::new(id);

        let status = HealthStatus::unhealthy("timeout", 100);
        state.record_failure(Some(status));

        assert!(state.last_status.is_some());
        assert!(state.last_check.is_some());
        assert_eq!(state.consecutive_failures, 1);
        assert_eq!(state.consecutive_successes, 0);
        assert_eq!(state.total_checks, 1);
        assert_eq!(state.total_failures, 1);
    }

    #[test]
    fn test_health_state_failure_rate() {
        let id = DaemonId::new();
        let mut state = DaemonHealthState::new(id);

        // No checks yet
        assert_eq!(state.failure_rate(), 0.0);

        // 2 successes, 1 failure = 33.33%
        state.record_success(HealthStatus::healthy(1));
        state.record_success(HealthStatus::healthy(1));
        state.record_failure(Some(HealthStatus::unhealthy("fail", 1)));

        let rate = state.failure_rate();
        assert!((rate - 33.333).abs() < 1.0);
    }

    // -------------------------------------------------------------------------
    // HealthMonitor Tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_monitor_creation() {
        let monitor = HealthMonitor::with_defaults();
        assert_eq!(monitor.count().await, 0);
    }

    #[tokio::test]
    async fn test_monitor_register() {
        let monitor = HealthMonitor::with_defaults();
        let id = DaemonId::new();

        monitor.register(id).await;
        assert_eq!(monitor.count().await, 1);

        let state = monitor.get_state(id).await;
        assert!(state.is_some());
    }

    #[tokio::test]
    async fn test_monitor_unregister() {
        let monitor = HealthMonitor::with_defaults();
        let id = DaemonId::new();

        monitor.register(id).await;
        monitor.unregister(id).await;
        assert_eq!(monitor.count().await, 0);
    }

    #[tokio::test]
    async fn test_monitor_record_healthy() {
        let monitor = HealthMonitor::with_defaults();
        let id = DaemonId::new();

        monitor.register(id).await;

        let status = HealthStatus::healthy(5);
        monitor.record_check(id, status).await.unwrap();

        let state = monitor.get_state(id).await.unwrap();
        assert!(state.is_healthy);
        assert_eq!(state.consecutive_successes, 1);
    }

    #[tokio::test]
    async fn test_monitor_record_unhealthy() {
        let config = HealthConfig::new().with_failure_threshold(2);
        let monitor = HealthMonitor::new(config);
        let id = DaemonId::new();

        monitor.register(id).await;

        // First failure
        let status = HealthStatus::unhealthy("error", 5);
        monitor.record_check(id, status.clone()).await.unwrap();

        let state = monitor.get_state(id).await.unwrap();
        assert!(state.is_healthy); // Still healthy (threshold not met)
        assert_eq!(state.consecutive_failures, 1);

        // Second failure - should become unhealthy
        monitor.record_check(id, status).await.unwrap();

        let state = monitor.get_state(id).await.unwrap();
        assert!(!state.is_healthy);
        assert_eq!(state.consecutive_failures, 2);
    }

    #[tokio::test]
    async fn test_monitor_recovery() {
        let config = HealthConfig::new()
            .with_failure_threshold(1)
            .with_recovery_threshold(2);
        let monitor = HealthMonitor::new(config);
        let id = DaemonId::new();

        monitor.register(id).await;

        // Fail
        let unhealthy = HealthStatus::unhealthy("error", 5);
        monitor.record_check(id, unhealthy).await.unwrap();

        let state = monitor.get_state(id).await.unwrap();
        assert!(!state.is_healthy);

        // First success
        let healthy = HealthStatus::healthy(5);
        monitor.record_check(id, healthy.clone()).await.unwrap();

        let state = monitor.get_state(id).await.unwrap();
        assert!(!state.is_healthy); // Not recovered yet

        // Second success - should recover
        monitor.record_check(id, healthy).await.unwrap();

        let state = monitor.get_state(id).await.unwrap();
        assert!(state.is_healthy);
    }

    #[tokio::test]
    async fn test_monitor_timeout() {
        let config = HealthConfig::new().with_failure_threshold(1);
        let monitor = HealthMonitor::new(config);
        let id = DaemonId::new();

        monitor.register(id).await;

        monitor.record_timeout(id).await.unwrap();

        let state = monitor.get_state(id).await.unwrap();
        assert!(!state.is_healthy);
        assert_eq!(state.consecutive_failures, 1);
    }

    #[tokio::test]
    async fn test_monitor_statistics() {
        let monitor = HealthMonitor::with_defaults();

        let id1 = DaemonId::new();
        let id2 = DaemonId::new();

        monitor.register(id1).await;
        monitor.register(id2).await;

        // Make id1 unhealthy
        let config = monitor.config().clone();
        for _ in 0..config.failure_threshold {
            let unhealthy = HealthStatus::unhealthy("error", 1);
            monitor.record_check(id1, unhealthy).await.unwrap();
        }

        // id2 stays healthy
        let healthy = HealthStatus::healthy(1);
        monitor.record_check(id2, healthy).await.unwrap();

        let stats = monitor.statistics().await;
        assert_eq!(stats.total_daemons, 2);
        assert_eq!(stats.healthy_daemons, 1);
        assert_eq!(stats.unhealthy_daemons, 1);
        assert!(!stats.all_healthy());
    }

    #[tokio::test]
    async fn test_monitor_healthy_unhealthy_counts() {
        let config = HealthConfig::new().with_failure_threshold(1);
        let monitor = HealthMonitor::new(config);

        let id1 = DaemonId::new();
        let id2 = DaemonId::new();
        let id3 = DaemonId::new();

        monitor.register(id1).await;
        monitor.register(id2).await;
        monitor.register(id3).await;

        // Make id1 unhealthy
        let unhealthy = HealthStatus::unhealthy("error", 1);
        monitor.record_check(id1, unhealthy).await.unwrap();

        assert_eq!(monitor.healthy_count().await, 2);
        assert_eq!(monitor.unhealthy_count().await, 1);
    }

    // -------------------------------------------------------------------------
    // HealthStatistics Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_health_statistics_ratio() {
        let stats = HealthStatistics {
            total_daemons: 4,
            healthy_daemons: 3,
            unhealthy_daemons: 1,
            total_checks: 100,
            total_failures: 10,
            average_failure_rate: 10.0,
        };

        assert_eq!(stats.health_ratio(), 0.75);
        assert!(!stats.all_healthy());
    }

    #[test]
    fn test_health_statistics_all_healthy() {
        let stats = HealthStatistics {
            total_daemons: 4,
            healthy_daemons: 4,
            unhealthy_daemons: 0,
            total_checks: 100,
            total_failures: 0,
            average_failure_rate: 0.0,
        };

        assert_eq!(stats.health_ratio(), 1.0);
        assert!(stats.all_healthy());
    }

    #[test]
    fn test_health_statistics_empty() {
        let stats = HealthStatistics {
            total_daemons: 0,
            healthy_daemons: 0,
            unhealthy_daemons: 0,
            total_checks: 0,
            total_failures: 0,
            average_failure_rate: 0.0,
        };

        assert_eq!(stats.health_ratio(), 1.0); // Empty is considered healthy
        assert!(stats.all_healthy());
    }

    // -------------------------------------------------------------------------
    // HealthEvent Tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_health_event_subscription() {
        let monitor = HealthMonitor::with_defaults();
        let mut rx = monitor.subscribe();

        let id = DaemonId::new();
        monitor.register(id).await;

        let status = HealthStatus::healthy(5);
        monitor.record_check(id, status).await.unwrap();

        // Should receive the healthy event
        let event = rx.try_recv();
        assert!(event.is_ok());

        match event.unwrap() {
            HealthEvent::Healthy { id: event_id, .. } => {
                assert_eq!(event_id, id);
            }
            _ => panic!("Expected Healthy event"),
        }
    }
}
