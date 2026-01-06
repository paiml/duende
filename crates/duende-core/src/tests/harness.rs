//! Test harness for daemon lifecycle testing.
//!
//! Provides controlled environment for falsification tests.

use std::time::Duration;

use crate::adapter::PlatformAdapter;
use crate::adapters::NativeAdapter;
use crate::daemon::{Daemon, DaemonContext};
use crate::manager::DaemonManager;

/// Test harness for daemon lifecycle tests.
///
/// Provides:
/// - Managed timeout enforcement
/// - Platform adapter selection
/// - Daemon context management
/// - Cleanup on drop
pub struct TestHarness {
    /// The platform adapter.
    adapter: Box<dyn PlatformAdapter>,
    /// The daemon manager.
    manager: DaemonManager,
    /// Default test timeout.
    timeout: Duration,
}

impl TestHarness {
    /// Creates a new test harness with native adapter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            adapter: Box::new(NativeAdapter::new()),
            manager: DaemonManager::new(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Creates a test harness with a custom adapter.
    #[must_use]
    pub fn with_adapter(adapter: Box<dyn PlatformAdapter>) -> Self {
        Self {
            adapter,
            manager: DaemonManager::new(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Sets the default timeout.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Returns a reference to the platform adapter.
    #[must_use]
    pub fn adapter(&self) -> &dyn PlatformAdapter {
        self.adapter.as_ref()
    }

    /// Returns a reference to the daemon manager.
    #[must_use]
    pub const fn manager(&self) -> &DaemonManager {
        &self.manager
    }

    /// Returns the default timeout.
    #[must_use]
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Creates a daemon context for testing.
    #[must_use]
    pub fn create_context(
        &self,
        name: impl Into<String>,
    ) -> (DaemonContext, crate::daemon::DaemonContextHandle) {
        let config = crate::config::DaemonConfig::new(name, "/bin/test");
        DaemonContext::new(config)
    }

    /// Spawns a daemon and returns its handle.
    ///
    /// # Errors
    /// Returns an error if the daemon cannot be spawned.
    pub async fn spawn(
        &self,
        daemon: Box<dyn Daemon>,
    ) -> crate::adapter::PlatformResult<crate::adapter::DaemonHandle> {
        self.adapter.spawn(daemon).await
    }

    /// Runs a test with timeout.
    ///
    /// # Errors
    /// Returns an error if the test times out or fails.
    pub async fn run_with_timeout<F, Fut, T>(
        &self,
        test: F,
    ) -> Result<T, TestError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, TestError>>,
    {
        tokio::time::timeout(self.timeout, test())
            .await
            .map_err(|_| TestError::Timeout(self.timeout))?
    }
}

impl Default for TestHarness {
    fn default() -> Self {
        Self::new()
    }
}

/// Test error type.
#[derive(Debug, thiserror::Error)]
pub enum TestError {
    /// Test timed out.
    #[error("test timed out after {0:?}")]
    Timeout(Duration),

    /// Assertion failed.
    #[error("assertion failed: {0}")]
    AssertionFailed(String),

    /// Platform error.
    #[error("platform error: {0}")]
    Platform(#[from] crate::adapter::PlatformError),

    /// Daemon error.
    #[error("daemon error: {0}")]
    Daemon(#[from] crate::error::DaemonError),
}

impl TestError {
    /// Creates an assertion failure.
    #[must_use]
    pub fn assertion(msg: impl Into<String>) -> Self {
        Self::AssertionFailed(msg.into())
    }
}

/// Assertion helper for tests.
#[macro_export]
macro_rules! assert_test {
    ($cond:expr) => {
        if !$cond {
            return Err($crate::tests::harness::TestError::assertion(
                stringify!($cond),
            ));
        }
    };
    ($cond:expr, $msg:expr) => {
        if !$cond {
            return Err($crate::tests::harness::TestError::assertion($msg));
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Platform;

    #[test]
    fn test_harness_creation() {
        let harness = TestHarness::new();
        assert_eq!(harness.adapter().platform(), Platform::Native);
        assert_eq!(harness.timeout(), Duration::from_secs(30));
    }

    #[test]
    fn test_harness_with_timeout() {
        let harness = TestHarness::new().with_timeout(Duration::from_secs(60));
        assert_eq!(harness.timeout(), Duration::from_secs(60));
    }

    #[test]
    fn test_harness_create_context() {
        let harness = TestHarness::new();
        let (ctx, _handle) = harness.create_context("test");
        assert!(!ctx.should_shutdown());
    }

    #[tokio::test]
    async fn test_run_with_timeout_success() {
        let harness = TestHarness::new().with_timeout(Duration::from_secs(1));

        let result = harness
            .run_with_timeout(|| async { Ok::<i32, TestError>(42) })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_run_with_timeout_timeout() {
        let harness = TestHarness::new().with_timeout(Duration::from_millis(10));

        let result = harness
            .run_with_timeout(|| async {
                tokio::time::sleep(Duration::from_secs(1)).await;
                Ok::<i32, TestError>(42)
            })
            .await;

        assert!(matches!(result, Err(TestError::Timeout(_))));
    }

    #[test]
    fn test_error_display() {
        let err = TestError::assertion("expected true");
        assert!(err.to_string().contains("expected true"));

        let err = TestError::Timeout(Duration::from_secs(30));
        assert!(err.to_string().contains("30"));
    }
}
