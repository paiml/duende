//! Error types for duende-core.
//!
//! Per Iron Lotus Framework: All errors are explicit, no panics allowed.

use std::time::Duration;

/// Result type alias for daemon operations.
pub type Result<T> = std::result::Result<T, DaemonError>;

/// Comprehensive error type for daemon operations.
///
/// Following Iron Lotus principle of explicit error handling,
/// this enum covers all failure modes without panics.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    /// Configuration error during daemon initialization.
    #[error("configuration error: {0}")]
    Config(String),

    /// Initialization failed.
    #[error("initialization failed: {0}")]
    Init(String),

    /// Runtime error during daemon execution.
    #[error("runtime error: {0}")]
    Runtime(String),

    /// Shutdown error.
    #[error("shutdown error: {0}")]
    Shutdown(String),

    /// Shutdown timed out.
    #[error("shutdown timed out after {0:?}")]
    ShutdownTimeout(Duration),

    /// Health check failed.
    #[error("health check failed: {0}")]
    HealthCheck(String),

    /// Resource limit exceeded.
    #[error("resource limit exceeded: {resource} (limit: {limit}, actual: {actual})")]
    ResourceLimit {
        /// The resource that was exceeded.
        resource: String,
        /// The configured limit.
        limit: u64,
        /// The actual value.
        actual: u64,
    },

    /// Policy violation.
    #[error("policy violation: {0}")]
    PolicyViolation(String),

    /// Signal handling error.
    #[error("signal error: {0}")]
    Signal(String),

    /// Daemon not found.
    #[error("daemon not found: {0}")]
    NotFound(String),

    /// Invalid state for operation.
    #[error("invalid state: {0}")]
    State(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Internal error (should not occur in production).
    #[error("internal error: {0}")]
    Internal(String),
}

impl DaemonError {
    /// Creates a configuration error.
    #[must_use]
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Creates an initialization error.
    #[must_use]
    pub fn init(msg: impl Into<String>) -> Self {
        Self::Init(msg.into())
    }

    /// Creates a runtime error.
    #[must_use]
    pub fn runtime(msg: impl Into<String>) -> Self {
        Self::Runtime(msg.into())
    }

    /// Creates a shutdown error.
    #[must_use]
    pub fn shutdown(msg: impl Into<String>) -> Self {
        Self::Shutdown(msg.into())
    }

    /// Creates a health check error.
    #[must_use]
    pub fn health_check(msg: impl Into<String>) -> Self {
        Self::HealthCheck(msg.into())
    }

    /// Creates a policy violation error.
    #[must_use]
    pub fn policy_violation(msg: impl Into<String>) -> Self {
        Self::PolicyViolation(msg.into())
    }

    /// Returns true if this error is recoverable (daemon can continue).
    #[must_use]
    pub const fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::HealthCheck(_) | Self::ResourceLimit { .. } | Self::PolicyViolation(_)
        )
    }

    /// Returns true if this error requires immediate shutdown.
    #[must_use]
    pub const fn is_fatal(&self) -> bool {
        matches!(self, Self::Init(_) | Self::Internal(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = DaemonError::config("invalid port");
        assert_eq!(err.to_string(), "configuration error: invalid port");
    }

    #[test]
    fn test_error_recoverable() {
        assert!(DaemonError::health_check("timeout").is_recoverable());
        assert!(!DaemonError::init("failed").is_recoverable());
    }

    #[test]
    fn test_error_fatal() {
        assert!(DaemonError::init("failed").is_fatal());
        assert!(!DaemonError::runtime("transient").is_fatal());
    }

    #[test]
    fn test_resource_limit_error() {
        let err = DaemonError::ResourceLimit {
            resource: "memory".to_string(),
            limit: 1024,
            actual: 2048,
        };
        assert!(err.to_string().contains("memory"));
        assert!(err.is_recoverable());
    }
}
