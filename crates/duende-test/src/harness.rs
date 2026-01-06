//! Daemon test harness.
//!
//! # Toyota Way: Built-in Quality (品質の作り込み)
//! Quality cannot be inspected in; it must be built in.

use std::time::{Duration, Instant};

use duende_core::types::HealthCheck;
use duende_core::{Daemon, DaemonStatus, HealthStatus, Signal};
use duende_platform::{DaemonHandle, NativeAdapter, Platform, PlatformAdapter, detect_platform};

use crate::chaos::ChaosConfig;
use crate::error::{Result, TestError};

/// Test harness for daemon lifecycle testing.
pub struct DaemonTestHarness {
    platform: Platform,
    chaos: Option<ChaosConfig>,
    adapter: Box<dyn PlatformAdapter>,
}

impl DaemonTestHarness {
    /// Creates a new test harness builder.
    #[must_use]
    pub fn builder() -> DaemonTestHarnessBuilder {
        DaemonTestHarnessBuilder::default()
    }

    /// Creates a new test harness with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::builder().build()
    }

    /// Spawns a daemon for testing.
    ///
    /// # Errors
    /// Returns an error if spawning fails.
    pub async fn spawn(&self, daemon: impl Daemon + 'static) -> Result<TestDaemonHandle> {
        let handle = self.adapter.spawn(Box::new(daemon)).await?;

        Ok(TestDaemonHandle {
            inner: handle,
            chaos: self.chaos.clone(),
            // Create own adapter for health/signal operations
            adapter: Box::new(NativeAdapter::new()),
        })
    }

    /// Returns the platform being tested.
    #[must_use]
    pub const fn platform(&self) -> Platform {
        self.platform
    }
}

impl Default for DaemonTestHarness {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for test harness.
#[derive(Default)]
pub struct DaemonTestHarnessBuilder {
    platform: Option<Platform>,
    chaos: Option<ChaosConfig>,
}

impl DaemonTestHarnessBuilder {
    /// Sets the platform to test on.
    #[must_use]
    pub const fn with_platform(mut self, platform: Platform) -> Self {
        self.platform = Some(platform);
        self
    }

    /// Enables chaos injection.
    #[must_use]
    pub fn with_chaos(mut self, config: ChaosConfig) -> Self {
        self.chaos = Some(config);
        self
    }

    /// Builds the test harness.
    #[must_use]
    pub fn build(self) -> DaemonTestHarness {
        let platform = self.platform.unwrap_or_else(detect_platform);

        // For now, always use native adapter for testing
        let adapter: Box<dyn PlatformAdapter> = Box::new(NativeAdapter::new());

        DaemonTestHarness {
            platform,
            chaos: self.chaos,
            adapter,
        }
    }
}

/// Handle to a daemon spawned for testing.
pub struct TestDaemonHandle {
    inner: DaemonHandle,
    chaos: Option<ChaosConfig>,
    adapter: Box<dyn PlatformAdapter>,
}

impl TestDaemonHandle {
    /// Performs a health check on the daemon.
    ///
    /// Checks:
    /// 1. Process is running (via adapter status)
    /// 2. Process is responsive (can receive signal 0)
    /// 3. Optional: memory/CPU within limits
    ///
    /// # Errors
    /// Returns an error if health check encounters an error.
    pub async fn health_check(&self) -> Result<HealthStatus> {
        let start = Instant::now();
        let mut checks = Vec::new();

        // Check 1: Process status via adapter
        let status_result = self.adapter.status(&self.inner).await;
        let process_running = match &status_result {
            Ok(DaemonStatus::Running) => {
                checks.push(HealthCheck {
                    name: "process_status".to_string(),
                    passed: true,
                    message: Some("Process is running".to_string()),
                });
                true
            }
            Ok(status) => {
                checks.push(HealthCheck {
                    name: "process_status".to_string(),
                    passed: false,
                    message: Some(format!("Process status: {status:?}")),
                });
                false
            }
            Err(e) => {
                checks.push(HealthCheck {
                    name: "process_status".to_string(),
                    passed: false,
                    message: Some(format!("Failed to check status: {e}")),
                });
                false
            }
        };

        // Check 2: Process has valid PID
        let has_pid = self.inner.pid.is_some();
        checks.push(HealthCheck {
            name: "pid_valid".to_string(),
            passed: has_pid,
            message: self.inner.pid.map_or_else(
                || Some("No PID assigned".to_string()),
                |pid| Some(format!("PID: {pid}")),
            ),
        });

        // Check 3: Optional Linux-specific checks via /proc
        #[cfg(target_os = "linux")]
        if let Some(pid) = self.inner.pid {
            // Check if /proc/{pid} exists and is readable
            let proc_path = format!("/proc/{pid}/stat");
            let proc_exists = std::path::Path::new(&proc_path).exists();
            checks.push(HealthCheck {
                name: "proc_accessible".to_string(),
                passed: proc_exists,
                message: Some(if proc_exists {
                    "Process info accessible via /proc".to_string()
                } else {
                    "Cannot access /proc info".to_string()
                }),
            });
        }

        let latency_ms = start.elapsed().as_millis() as u64;
        let healthy = process_running && has_pid;

        Ok(HealthStatus {
            healthy,
            checks,
            latency_ms,
            last_check_epoch_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        })
    }

    /// Shuts down the daemon gracefully.
    ///
    /// # Shutdown sequence (Toyota Way: Jidoka - stop-on-error)
    /// 1. Send SIGTERM for graceful shutdown
    /// 2. Wait for process to exit (up to timeout)
    /// 3. If still running, send SIGKILL
    /// 4. Verify process terminated
    ///
    /// # Errors
    /// Returns an error if shutdown fails completely.
    pub async fn shutdown(&self, timeout: Duration) -> Result<()> {
        let Some(pid) = self.inner.pid else {
            return Ok(()); // No process to shutdown
        };

        tracing::info!(pid = pid, timeout = ?timeout, "initiating graceful shutdown");

        // Step 1: Send SIGTERM for graceful shutdown
        if let Err(e) = self.adapter.signal(&self.inner, Signal::Term).await {
            tracing::warn!(pid = pid, error = %e, "failed to send SIGTERM, trying SIGKILL");
            // Process might already be dead, continue
        }

        // Step 2: Wait for process to exit with polling
        let poll_interval = Duration::from_millis(50);
        let start = Instant::now();

        loop {
            match self.adapter.status(&self.inner).await {
                Ok(DaemonStatus::Stopped | DaemonStatus::Failed(_)) => {
                    tracing::info!(pid = pid, elapsed = ?start.elapsed(), "daemon stopped gracefully");
                    return Ok(());
                }
                Ok(_) => {
                    // Still running
                    if start.elapsed() >= timeout {
                        break; // Timeout reached
                    }
                    tokio::time::sleep(poll_interval).await;
                }
                Err(e) => {
                    // Error checking status - process likely dead
                    tracing::debug!(pid = pid, error = %e, "status check failed, assuming stopped");
                    return Ok(());
                }
            }
        }

        // Step 3: Timeout reached, send SIGKILL
        tracing::warn!(pid = pid, "graceful shutdown timed out, sending SIGKILL");
        if let Err(e) = self.adapter.signal(&self.inner, Signal::Kill).await {
            tracing::debug!(pid = pid, error = %e, "SIGKILL failed, process may be dead");
            return Ok(()); // Process likely already dead
        }

        // Step 4: Final verification with short wait
        tokio::time::sleep(Duration::from_millis(100)).await;
        match self.adapter.status(&self.inner).await {
            Ok(DaemonStatus::Stopped | DaemonStatus::Failed(_)) => {
                tracing::info!(pid = pid, "daemon killed");
                Ok(())
            }
            Ok(status) => {
                tracing::error!(pid = pid, status = ?status, "daemon failed to terminate");
                Err(TestError::Shutdown(format!(
                    "daemon PID {pid} failed to terminate after SIGKILL"
                )))
            }
            Err(_) => {
                // Error checking status - process likely dead
                Ok(())
            }
        }
    }

    /// Returns the inner daemon handle.
    #[must_use]
    pub const fn handle(&self) -> &DaemonHandle {
        &self.inner
    }

    /// Returns the chaos config, if any.
    #[must_use]
    pub const fn chaos(&self) -> Option<&ChaosConfig> {
        self.chaos.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use duende_core::{DaemonConfig, DaemonContext, DaemonId, DaemonMetrics, ExitReason};

    /// Mock daemon for testing.
    struct MockDaemon {
        id: DaemonId,
        name: String,
        metrics: DaemonMetrics,
    }

    impl MockDaemon {
        fn new(name: &str) -> Self {
            Self {
                id: DaemonId::new(),
                name: name.to_string(),
                metrics: DaemonMetrics::new(),
            }
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

        async fn init(&mut self, _config: &DaemonConfig) -> duende_core::error::Result<()> {
            Ok(())
        }

        async fn run(
            &mut self,
            _ctx: &mut DaemonContext,
        ) -> duende_core::error::Result<ExitReason> {
            Ok(ExitReason::Graceful)
        }

        async fn shutdown(&mut self, _timeout: Duration) -> duende_core::error::Result<()> {
            Ok(())
        }

        async fn health_check(&self) -> HealthStatus {
            HealthStatus::healthy(1)
        }

        fn metrics(&self) -> &DaemonMetrics {
            &self.metrics
        }
    }

    #[test]
    fn test_harness_builder() {
        let harness = DaemonTestHarness::builder()
            .with_platform(Platform::Native)
            .build();

        assert_eq!(harness.platform(), Platform::Native);
    }

    #[test]
    fn test_harness_default() {
        let harness = DaemonTestHarness::default();
        // Platform should be detected
        let platform = harness.platform();
        assert!(matches!(
            platform,
            Platform::Native | Platform::Linux | Platform::MacOS | Platform::Container
        ));
    }

    #[test]
    fn test_harness_new() {
        let harness = DaemonTestHarness::new();
        // Should work the same as default
        let _ = harness.platform();
    }

    #[test]
    fn test_harness_with_chaos() {
        let harness = DaemonTestHarness::builder()
            .with_chaos(ChaosConfig::default())
            .build();

        // Should have chaos config (harness doesn't expose it directly)
        assert!(harness.chaos.is_some());
    }

    #[test]
    fn test_builder_default() {
        let builder = DaemonTestHarnessBuilder::default();
        let harness = builder.build();
        // Should work with all defaults
        let _ = harness.platform();
    }

    #[tokio::test]
    async fn test_test_daemon_handle_health_check_running() {
        // Use our own PID which should be running
        let pid = std::process::id();
        let handle = TestDaemonHandle {
            inner: DaemonHandle::native(pid),
            chaos: None,
            adapter: Box::new(NativeAdapter::new()),
        };

        let health = handle.health_check().await;
        assert!(health.is_ok());
        let status = health.expect("health status");
        assert!(status.healthy, "Our own process should be healthy");
        assert!(!status.checks.is_empty(), "Should have checks");
    }

    #[tokio::test]
    async fn test_test_daemon_handle_health_check_not_running() {
        // Use a very high PID that shouldn't exist
        let handle = TestDaemonHandle {
            inner: DaemonHandle::native(4000000),
            chaos: None,
            adapter: Box::new(NativeAdapter::new()),
        };

        let health = handle.health_check().await;
        assert!(health.is_ok());
        let status = health.expect("health status");
        assert!(!status.healthy, "Non-existent process should be unhealthy");
    }

    #[tokio::test]
    async fn test_test_daemon_handle_health_check_no_pid() {
        let handle = TestDaemonHandle {
            inner: DaemonHandle {
                platform: Platform::Native,
                pid: None,
                id: "no-pid".to_string(),
            },
            chaos: None,
            adapter: Box::new(NativeAdapter::new()),
        };

        let health = handle.health_check().await;
        assert!(health.is_ok());
        let status = health.expect("health status");
        assert!(!status.healthy, "No PID should be unhealthy");
    }

    #[tokio::test]
    async fn test_test_daemon_handle_shutdown_no_pid() {
        // Shutdown with no PID should succeed (nothing to do)
        let handle = TestDaemonHandle {
            inner: DaemonHandle {
                platform: Platform::Native,
                pid: None,
                id: "no-pid".to_string(),
            },
            chaos: None,
            adapter: Box::new(NativeAdapter::new()),
        };

        let result = handle.shutdown(Duration::from_secs(1)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_test_daemon_handle_shutdown_nonexistent() {
        // Shutdown of non-existent process should succeed
        let handle = TestDaemonHandle {
            inner: DaemonHandle::native(4000000),
            chaos: None,
            adapter: Box::new(NativeAdapter::new()),
        };

        let result = handle.shutdown(Duration::from_millis(100)).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_test_daemon_handle_accessors() {
        let chaos = ChaosConfig::default();
        let handle = TestDaemonHandle {
            inner: DaemonHandle::native(12345),
            chaos: Some(chaos),
            adapter: Box::new(NativeAdapter::new()),
        };

        assert_eq!(handle.handle().pid, Some(12345));
        assert!(handle.chaos().is_some());
    }

    #[test]
    fn test_test_daemon_handle_no_chaos() {
        let handle = TestDaemonHandle {
            inner: DaemonHandle::native(12345),
            chaos: None,
            adapter: Box::new(NativeAdapter::new()),
        };

        assert!(handle.chaos().is_none());
    }

    #[tokio::test]
    async fn test_mock_daemon_lifecycle() {
        use duende_core::Daemon;

        let mut daemon = MockDaemon::new("test-daemon");
        assert_eq!(daemon.name(), "test-daemon");
        assert!(!daemon.id().as_uuid().is_nil());

        // Init should succeed
        let config = DaemonConfig::new("test", "/bin/test");
        let result = daemon.init(&config).await;
        assert!(result.is_ok());

        // Health check should return healthy
        let health = daemon.health_check().await;
        assert!(health.is_healthy());

        // Shutdown should succeed
        let result = daemon.shutdown(Duration::from_secs(5)).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_mock_daemon_metrics() {
        use duende_core::Daemon;

        let daemon = MockDaemon::new("metrics-test");
        let metrics = daemon.metrics();

        // Metrics should be accessible
        assert_eq!(metrics.requests_total(), 0);
    }
}
