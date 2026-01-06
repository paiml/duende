//! Error types for duende-ublk

/// Errors that can occur during ublk operations
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Control device (/dev/ublk-control) not found
    #[error("ublk control device not found at /dev/ublk-control (is ublk module loaded?)")]
    ControlDeviceNotFound,

    /// Failed to open control device
    #[error("failed to open control device: {0}")]
    OpenControl(std::io::Error),

    /// Device not found
    #[error("ublk device {dev_id} not found")]
    DeviceNotFound {
        /// Device ID that was not found
        dev_id: u32,
    },

    /// Device is busy (in use)
    #[error("ublk device {dev_id} is busy")]
    DeviceBusy {
        /// Device ID that is busy
        dev_id: u32,
    },

    /// Failed to create io_uring
    #[error("failed to create io_uring: {0}")]
    IoUringCreate(std::io::Error),

    /// Failed to submit io_uring command
    #[error("failed to submit io_uring command: {0}")]
    IoUringSubmit(std::io::Error),

    /// io_uring command failed
    #[error("io_uring command failed with error code {errno}: {message}")]
    IoUringCommand {
        /// Error number from kernel
        errno: i32,
        /// Human-readable message
        message: String,
    },

    /// Failed to read /dev directory
    #[error("failed to scan /dev directory: {0}")]
    ScanDevDir(std::io::Error),

    /// Operation timed out
    #[error("operation timed out after {timeout_ms}ms")]
    Timeout {
        /// Timeout in milliseconds
        timeout_ms: u64,
    },
}

impl Error {
    /// Create an IoUringCommand error from errno
    #[must_use]
    pub fn from_errno(errno: i32) -> Self {
        let message = if errno == -libc::ENOENT {
            "device not found".to_string()
        } else if errno == -libc::EEXIST {
            "device already exists".to_string()
        } else if errno == -libc::EBUSY {
            "device is busy".to_string()
        } else if errno == -libc::EPERM {
            "permission denied".to_string()
        } else if errno == -libc::EINVAL {
            "invalid argument".to_string()
        } else {
            format!("unknown error ({})", errno)
        };
        Self::IoUringCommand { errno, message }
    }

    /// Check if this error indicates the device was not found
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::DeviceNotFound { .. } => true,
            Self::IoUringCommand { errno, .. } => *errno == -libc::ENOENT,
            _ => false,
        }
    }

    /// Check if this error indicates permission was denied
    #[must_use]
    pub fn is_permission_denied(&self) -> bool {
        match self {
            Self::IoUringCommand { errno, .. } => *errno == -libc::EPERM,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::ControlDeviceNotFound;
        let msg = err.to_string();
        assert!(msg.contains("control"));
        assert!(msg.contains("ublk"));

        let err = Error::DeviceNotFound { dev_id: 42 };
        let msg = err.to_string();
        assert!(msg.contains("42"));
    }

    #[test]
    fn test_from_errno() {
        let err = Error::from_errno(-libc::ENOENT);
        assert!(err.to_string().contains("not found"));

        let err = Error::from_errno(-libc::EBUSY);
        assert!(err.to_string().contains("busy"));

        let err = Error::from_errno(-libc::EPERM);
        assert!(err.to_string().contains("permission"));
    }

    #[test]
    fn test_is_not_found() {
        let err = Error::DeviceNotFound { dev_id: 0 };
        assert!(err.is_not_found());

        let err = Error::from_errno(-libc::ENOENT);
        assert!(err.is_not_found());

        let err = Error::from_errno(-libc::EBUSY);
        assert!(!err.is_not_found());
    }

    #[test]
    fn test_is_permission_denied() {
        let err = Error::from_errno(-libc::EPERM);
        assert!(err.is_permission_denied());

        let err = Error::from_errno(-libc::ENOENT);
        assert!(!err.is_permission_denied());
    }
}
