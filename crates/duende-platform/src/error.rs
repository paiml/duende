//! Platform error types.

/// Result type alias for platform operations.
pub type Result<T> = std::result::Result<T, PlatformError>;

/// Platform-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    /// Platform not supported.
    #[error("platform not supported: {0}")]
    NotSupported(String),

    /// Feature not implemented.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),

    /// Spawn failed.
    #[error("failed to spawn daemon: {0}")]
    Spawn(String),

    /// Signal failed.
    #[error("failed to send signal: {0}")]
    Signal(String),

    /// Status query failed.
    #[error("failed to get status: {0}")]
    Status(String),

    /// Tracer attachment failed.
    #[error("failed to attach tracer: {0}")]
    Tracer(String),

    /// Resource allocation or configuration failed.
    #[error("resource error: {0}")]
    Resource(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Core daemon error.
    #[error("daemon error: {0}")]
    Daemon(#[from] duende_core::DaemonError),
}

impl PlatformError {
    /// Creates a not supported error.
    #[must_use]
    pub fn not_supported(msg: impl Into<String>) -> Self {
        Self::NotSupported(msg.into())
    }

    /// Creates a spawn error.
    #[must_use]
    pub fn spawn(msg: impl Into<String>) -> Self {
        Self::Spawn(msg.into())
    }

    /// Creates a signal error.
    #[must_use]
    pub fn signal(msg: impl Into<String>) -> Self {
        Self::Signal(msg.into())
    }

    /// Creates a resource error.
    #[must_use]
    pub fn resource(msg: impl Into<String>) -> Self {
        Self::Resource(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_supported_error() {
        let err = PlatformError::not_supported("macOS cgroups");
        assert!(err.to_string().contains("not supported"));
        assert!(err.to_string().contains("macOS cgroups"));
    }

    #[test]
    fn test_spawn_error() {
        let err = PlatformError::spawn("binary not found");
        assert!(err.to_string().contains("spawn"));
        assert!(err.to_string().contains("binary not found"));
    }

    #[test]
    fn test_signal_error() {
        let err = PlatformError::signal("process not found");
        assert!(err.to_string().contains("signal"));
    }

    #[test]
    fn test_resource_error() {
        let err = PlatformError::resource("memory exhausted");
        assert!(err.to_string().contains("resource"));
    }

    #[test]
    fn test_not_implemented_error() {
        let err = PlatformError::NotImplemented("systemd integration");
        assert!(err.to_string().contains("not implemented"));
    }

    #[test]
    fn test_status_error() {
        let err = PlatformError::Status("connection refused".into());
        assert!(err.to_string().contains("status"));
    }

    #[test]
    fn test_tracer_error() {
        let err = PlatformError::Tracer("ptrace denied".into());
        assert!(err.to_string().contains("tracer"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: PlatformError = io_err.into();
        assert!(err.to_string().contains("I/O error"));
    }
}
