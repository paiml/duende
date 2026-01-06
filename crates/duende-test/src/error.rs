//! Test error types.

/// Result type alias for test operations.
pub type Result<T> = std::result::Result<T, TestError>;

/// Testing errors.
#[derive(Debug, thiserror::Error)]
pub enum TestError {
    /// Harness error.
    #[error("harness error: {0}")]
    Harness(String),

    /// Chaos injection error.
    #[error("chaos injection error: {0}")]
    Chaos(String),

    /// Load test error.
    #[error("load test error: {0}")]
    LoadTest(String),

    /// Assertion failed.
    #[error("assertion failed: {0}")]
    Assertion(String),

    /// Timeout.
    #[error("timeout after {0:?}")]
    Timeout(std::time::Duration),

    /// Shutdown error.
    #[error("shutdown error: {0}")]
    Shutdown(String),

    /// Core daemon error.
    #[error("daemon error: {0}")]
    Daemon(#[from] duende_core::DaemonError),

    /// Platform error.
    #[error("platform error: {0}")]
    Platform(#[from] duende_platform::PlatformError),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl TestError {
    /// Creates a harness error.
    #[must_use]
    pub fn harness(msg: impl Into<String>) -> Self {
        Self::Harness(msg.into())
    }

    /// Creates an assertion error.
    #[must_use]
    pub fn assertion(msg: impl Into<String>) -> Self {
        Self::Assertion(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_harness_error() {
        let err = TestError::harness("setup failed");
        assert!(err.to_string().contains("harness error"));
        assert!(err.to_string().contains("setup failed"));
    }

    #[test]
    fn test_assertion_error() {
        let err = TestError::assertion("expected 5, got 10");
        assert!(err.to_string().contains("assertion failed"));
        assert!(err.to_string().contains("expected 5"));
    }

    #[test]
    fn test_chaos_error() {
        let err = TestError::Chaos("injection failed".into());
        assert!(err.to_string().contains("chaos injection"));
    }

    #[test]
    fn test_load_test_error() {
        let err = TestError::LoadTest("connection pool exhausted".into());
        assert!(err.to_string().contains("load test error"));
    }

    #[test]
    fn test_timeout_error() {
        let err = TestError::Timeout(Duration::from_secs(30));
        assert!(err.to_string().contains("timeout"));
        assert!(err.to_string().contains("30"));
    }

    #[test]
    fn test_shutdown_error() {
        let err = TestError::Shutdown("daemon failed to terminate".into());
        assert!(err.to_string().contains("shutdown error"));
        assert!(err.to_string().contains("failed to terminate"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out");
        let err: TestError = io_err.into();
        assert!(err.to_string().contains("I/O error"));
    }
}
