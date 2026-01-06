//! Policy error types.

/// Result type alias for policy operations.
pub type Result<T> = std::result::Result<T, PolicyError>;

/// Policy enforcement errors.
#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    /// Circuit breaker open.
    #[error("circuit breaker open")]
    CircuitOpen,

    /// Quality gate failed.
    #[error("quality gate failed: {0}")]
    GateFailed(String),

    /// Resource limit error.
    #[error("resource limit error: {0}")]
    ResourceLimit(String),

    /// Jidoka violation.
    #[error("jidoka violation: {0}")]
    JidokaViolation(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl PolicyError {
    /// Creates a gate failed error.
    #[must_use]
    pub fn gate_failed(msg: impl Into<String>) -> Self {
        Self::GateFailed(msg.into())
    }

    /// Creates a jidoka violation error.
    #[must_use]
    pub fn jidoka_violation(msg: impl Into<String>) -> Self {
        Self::JidokaViolation(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_open_error() {
        let err = PolicyError::CircuitOpen;
        assert!(err.to_string().contains("circuit breaker open"));
    }

    #[test]
    fn test_gate_failed_error() {
        let err = PolicyError::gate_failed("complexity threshold exceeded");
        assert!(err.to_string().contains("quality gate failed"));
        assert!(err.to_string().contains("complexity"));
    }

    #[test]
    fn test_jidoka_violation_error() {
        let err = PolicyError::jidoka_violation("invariant violated");
        assert!(err.to_string().contains("jidoka violation"));
        assert!(err.to_string().contains("invariant"));
    }

    #[test]
    fn test_resource_limit_error() {
        let err = PolicyError::ResourceLimit("memory exceeded".into());
        assert!(err.to_string().contains("resource limit"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err: PolicyError = io_err.into();
        assert!(err.to_string().contains("I/O error"));
    }
}
