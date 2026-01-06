//! Mock implementations for testing.
//!
//! Provides configurable mock daemons for falsification tests.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::config::DaemonConfig;
use crate::daemon::{Daemon, DaemonContext};
use crate::error::{DaemonError, Result};
use crate::metrics::DaemonMetrics;
use crate::types::{DaemonId, ExitReason, HealthStatus};

/// Mock daemon for testing.
///
/// Configurable behavior for testing various scenarios:
/// - Init success/failure
/// - Health check responses
/// - Shutdown behavior
/// - Resource consumption simulation
pub struct MockDaemon {
    id: DaemonId,
    name: String,
    metrics: DaemonMetrics,
    state: Arc<MockState>,
}

/// Internal state for mock daemon.
struct MockState {
    /// Whether init should fail.
    init_should_fail: AtomicBool,
    /// Error message for init failure.
    init_error_msg: parking_lot::RwLock<String>,

    /// Whether health check should return healthy.
    is_healthy: AtomicBool,
    /// Health check latency in ms.
    health_latency_ms: AtomicU32,

    /// Whether shutdown should fail.
    shutdown_should_fail: AtomicBool,
    /// Shutdown error message.
    shutdown_error_msg: parking_lot::RwLock<String>,

    /// Number of init calls.
    init_count: AtomicU32,
    /// Number of shutdown calls.
    shutdown_count: AtomicU32,
    /// Number of health check calls.
    health_check_count: AtomicU32,

    /// Run iterations before self-exit.
    run_iterations: AtomicU32,
    /// Current iteration count.
    current_iteration: AtomicU32,

    /// Whether run should return error.
    run_should_fail: AtomicBool,
    /// Run error message.
    run_error_msg: parking_lot::RwLock<String>,
}

impl MockDaemon {
    /// Creates a new mock daemon with default behavior.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: DaemonId::new(),
            name: name.into(),
            metrics: DaemonMetrics::new(),
            state: Arc::new(MockState {
                init_should_fail: AtomicBool::new(false),
                init_error_msg: parking_lot::RwLock::new(String::new()),
                is_healthy: AtomicBool::new(true),
                health_latency_ms: AtomicU32::new(1),
                shutdown_should_fail: AtomicBool::new(false),
                shutdown_error_msg: parking_lot::RwLock::new(String::new()),
                init_count: AtomicU32::new(0),
                shutdown_count: AtomicU32::new(0),
                health_check_count: AtomicU32::new(0),
                run_iterations: AtomicU32::new(u32::MAX),
                current_iteration: AtomicU32::new(0),
                run_should_fail: AtomicBool::new(false),
                run_error_msg: parking_lot::RwLock::new(String::new()),
            }),
        }
    }

    /// Creates a mock daemon with a specific ID.
    #[must_use]
    pub fn with_id(mut self, id: DaemonId) -> Self {
        self.id = id;
        self
    }

    /// Configures init to fail with the given message.
    #[must_use]
    pub fn fail_init(self, msg: impl Into<String>) -> Self {
        self.state.init_should_fail.store(true, Ordering::SeqCst);
        *self.state.init_error_msg.write() = msg.into();
        self
    }

    /// Configures health check to return unhealthy.
    #[must_use]
    pub fn unhealthy(self) -> Self {
        self.state.is_healthy.store(false, Ordering::SeqCst);
        self
    }

    /// Configures health check latency.
    #[must_use]
    pub fn health_latency(self, ms: u32) -> Self {
        self.state.health_latency_ms.store(ms, Ordering::SeqCst);
        self
    }

    /// Configures shutdown to fail with the given message.
    #[must_use]
    pub fn fail_shutdown(self, msg: impl Into<String>) -> Self {
        self.state.shutdown_should_fail.store(true, Ordering::SeqCst);
        *self.state.shutdown_error_msg.write() = msg.into();
        self
    }

    /// Configures run to exit after N iterations.
    #[must_use]
    pub fn exit_after(self, iterations: u32) -> Self {
        self.state.run_iterations.store(iterations, Ordering::SeqCst);
        self
    }

    /// Configures run to fail with the given message.
    #[must_use]
    pub fn fail_run(self, msg: impl Into<String>) -> Self {
        self.state.run_should_fail.store(true, Ordering::SeqCst);
        *self.state.run_error_msg.write() = msg.into();
        self
    }

    /// Returns the number of init calls.
    #[must_use]
    pub fn init_count(&self) -> u32 {
        self.state.init_count.load(Ordering::SeqCst)
    }

    /// Returns the number of shutdown calls.
    #[must_use]
    pub fn shutdown_count(&self) -> u32 {
        self.state.shutdown_count.load(Ordering::SeqCst)
    }

    /// Returns the number of health check calls.
    #[must_use]
    pub fn health_check_count(&self) -> u32 {
        self.state.health_check_count.load(Ordering::SeqCst)
    }

    /// Returns the current run iteration.
    #[must_use]
    pub fn current_iteration(&self) -> u32 {
        self.state.current_iteration.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Daemon for MockDaemon {
    fn id(&self) -> DaemonId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    async fn init(&mut self, _config: &DaemonConfig) -> Result<()> {
        self.state.init_count.fetch_add(1, Ordering::SeqCst);

        if self.state.init_should_fail.load(Ordering::SeqCst) {
            let msg = self.state.init_error_msg.read().clone();
            return Err(DaemonError::init(msg));
        }

        Ok(())
    }

    async fn run(&mut self, ctx: &mut DaemonContext) -> Result<ExitReason> {
        if self.state.run_should_fail.load(Ordering::SeqCst) {
            let msg = self.state.run_error_msg.read().clone();
            return Err(DaemonError::runtime(msg));
        }

        let max_iterations = self.state.run_iterations.load(Ordering::SeqCst);

        loop {
            if ctx.should_shutdown() {
                return Ok(ExitReason::Graceful);
            }

            // Check for signals
            if let Some(sig) = ctx.try_recv_signal() {
                return Ok(ExitReason::Signal(sig));
            }

            // Check iteration limit
            let iter = self.state.current_iteration.fetch_add(1, Ordering::SeqCst);
            if iter >= max_iterations {
                return Ok(ExitReason::Graceful);
            }

            // Record metrics
            self.metrics.record_request();

            // Small sleep to prevent busy loop
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn shutdown(&mut self, _timeout: Duration) -> Result<()> {
        self.state.shutdown_count.fetch_add(1, Ordering::SeqCst);

        if self.state.shutdown_should_fail.load(Ordering::SeqCst) {
            let msg = self.state.shutdown_error_msg.read().clone();
            return Err(DaemonError::shutdown(msg));
        }

        Ok(())
    }

    async fn health_check(&self) -> HealthStatus {
        self.state.health_check_count.fetch_add(1, Ordering::SeqCst);

        let latency_ms = self.state.health_latency_ms.load(Ordering::SeqCst) as u64;

        if self.state.is_healthy.load(Ordering::SeqCst) {
            HealthStatus::healthy(latency_ms)
        } else {
            HealthStatus::unhealthy("mock unhealthy", latency_ms)
        }
    }

    fn metrics(&self) -> &DaemonMetrics {
        &self.metrics
    }
}

/// Factory for creating mock daemons with common configurations.
pub struct MockDaemonFactory;

impl MockDaemonFactory {
    /// Creates a simple healthy daemon.
    #[must_use]
    pub fn healthy(name: &str) -> MockDaemon {
        MockDaemon::new(name)
    }

    /// Creates a daemon that fails during init.
    #[must_use]
    pub fn failing_init(name: &str, msg: &str) -> MockDaemon {
        MockDaemon::new(name).fail_init(msg)
    }

    /// Creates a daemon that fails health checks.
    #[must_use]
    pub fn unhealthy(name: &str) -> MockDaemon {
        MockDaemon::new(name).unhealthy()
    }

    /// Creates a daemon that exits after N iterations.
    #[must_use]
    pub fn short_lived(name: &str, iterations: u32) -> MockDaemon {
        MockDaemon::new(name).exit_after(iterations)
    }

    /// Creates a daemon with slow health checks.
    #[must_use]
    pub fn slow_health(name: &str, latency_ms: u32) -> MockDaemon {
        MockDaemon::new(name).health_latency(latency_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::DaemonContext;
    use crate::types::Signal;

    #[test]
    fn test_mock_daemon_creation() {
        let daemon = MockDaemon::new("test");
        assert_eq!(daemon.name(), "test");
        assert_eq!(daemon.init_count(), 0);
    }

    #[tokio::test]
    async fn test_mock_daemon_init_success() {
        let mut daemon = MockDaemon::new("test");
        let config = DaemonConfig::new("test", "/bin/test");

        let result = daemon.init(&config).await;
        assert!(result.is_ok());
        assert_eq!(daemon.init_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_daemon_init_failure() {
        let mut daemon = MockDaemon::new("test").fail_init("test error");
        let config = DaemonConfig::new("test", "/bin/test");

        let result = daemon.init(&config).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("test error"));
    }

    #[tokio::test]
    async fn test_mock_daemon_health_check() {
        let daemon = MockDaemon::new("test");

        let health = daemon.health_check().await;
        assert!(health.is_healthy());
        assert_eq!(daemon.health_check_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_daemon_unhealthy() {
        let daemon = MockDaemon::new("test").unhealthy();

        let health = daemon.health_check().await;
        assert!(!health.is_healthy());
    }

    #[tokio::test]
    async fn test_mock_daemon_shutdown() {
        let mut daemon = MockDaemon::new("test");

        let result = daemon.shutdown(Duration::from_secs(5)).await;
        assert!(result.is_ok());
        assert_eq!(daemon.shutdown_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_daemon_shutdown_failure() {
        let mut daemon = MockDaemon::new("test").fail_shutdown("shutdown error");

        let result = daemon.shutdown(Duration::from_secs(5)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_daemon_exit_after_iterations() {
        let mut daemon = MockDaemon::new("test").exit_after(5);
        let config = DaemonConfig::new("test", "/bin/test");
        let (mut ctx, _handle) = DaemonContext::new(config);

        let result = daemon.run(&mut ctx).await;
        assert!(matches!(result, Ok(ExitReason::Graceful)));
        // Should be at least 5 iterations (may be more due to timing)
        assert!(daemon.current_iteration() >= 5);
    }

    #[tokio::test]
    async fn test_mock_daemon_signal_response() {
        let mut daemon = MockDaemon::new("test");
        let config = DaemonConfig::new("test", "/bin/test");
        let (mut ctx, handle) = DaemonContext::new(config);

        // Send signal in background
        let send_handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            handle.send_signal(Signal::Hup).await
        });

        let result = daemon.run(&mut ctx).await;
        assert!(matches!(result, Ok(ExitReason::Signal(Signal::Hup))));

        send_handle.await.ok();
    }

    #[tokio::test]
    async fn test_mock_daemon_factory() {
        let healthy = MockDaemonFactory::healthy("h");
        assert!(healthy.health_check().await.is_healthy());

        let unhealthy = MockDaemonFactory::unhealthy("u");
        assert!(!unhealthy.health_check().await.is_healthy());

        let mut failing = MockDaemonFactory::failing_init("f", "fail");
        let config = DaemonConfig::new("test", "/bin/test");
        assert!(failing.init(&config).await.is_err());
    }
}
