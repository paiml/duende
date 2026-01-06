//! Observability error types.

/// Result type alias for observe operations.
pub type Result<T> = std::result::Result<T, ObserveError>;

/// Observability errors.
#[derive(Debug, thiserror::Error)]
pub enum ObserveError {
    /// Tracer error.
    #[error("tracer error: {0}")]
    Tracer(String),

    /// Monitor error.
    #[error("monitor error: {0}")]
    Monitor(String),

    /// Export error.
    #[error("export error: {0}")]
    Export(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl ObserveError {
    /// Creates a tracer error.
    #[must_use]
    pub fn tracer(msg: impl Into<String>) -> Self {
        Self::Tracer(msg.into())
    }

    /// Creates a monitor error.
    #[must_use]
    pub fn monitor(msg: impl Into<String>) -> Self {
        Self::Monitor(msg.into())
    }

    /// Creates an export error.
    #[must_use]
    pub fn export(msg: impl Into<String>) -> Self {
        Self::Export(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracer_error() {
        let err = ObserveError::tracer("ptrace failed");
        assert!(err.to_string().contains("tracer error"));
        assert!(err.to_string().contains("ptrace failed"));
    }

    #[test]
    fn test_monitor_error() {
        let err = ObserveError::monitor("collection failed");
        assert!(err.to_string().contains("monitor error"));
        assert!(err.to_string().contains("collection failed"));
    }

    #[test]
    fn test_export_error() {
        let err = ObserveError::export("OTLP connection refused");
        assert!(err.to_string().contains("export error"));
        assert!(err.to_string().contains("OTLP"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broken");
        let err: ObserveError = io_err.into();
        assert!(err.to_string().contains("I/O error"));
    }
}
