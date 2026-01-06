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

    #[test]
    fn test_error_control_device_not_found_display() {
        let err = Error::ControlDeviceNotFound;
        let msg = err.to_string();
        assert!(msg.contains("/dev/ublk-control"));
        assert!(msg.contains("module loaded"));
    }

    #[test]
    fn test_error_open_control_display() {
        let io_err = std::io::Error::from_raw_os_error(libc::EACCES);
        let err = Error::OpenControl(io_err);
        let msg = err.to_string();
        assert!(msg.contains("open control device"));
    }

    #[test]
    fn test_error_device_not_found_display() {
        let err = Error::DeviceNotFound { dev_id: 123 };
        let msg = err.to_string();
        assert!(msg.contains("123"));
        assert!(msg.contains("not found"));
    }

    #[test]
    fn test_error_device_busy_display() {
        let err = Error::DeviceBusy { dev_id: 456 };
        let msg = err.to_string();
        assert!(msg.contains("456"));
        assert!(msg.contains("busy"));
    }

    #[test]
    fn test_error_io_uring_create_display() {
        let io_err = std::io::Error::from_raw_os_error(libc::ENOMEM);
        let err = Error::IoUringCreate(io_err);
        let msg = err.to_string();
        assert!(msg.contains("io_uring"));
    }

    #[test]
    fn test_error_io_uring_submit_display() {
        let io_err = std::io::Error::from_raw_os_error(libc::EIO);
        let err = Error::IoUringSubmit(io_err);
        let msg = err.to_string();
        assert!(msg.contains("submit"));
    }

    #[test]
    fn test_error_io_uring_command_display() {
        let err = Error::IoUringCommand {
            errno: -libc::EINVAL,
            message: "test message".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("test message"));
    }

    #[test]
    fn test_error_scan_dev_dir_display() {
        let io_err = std::io::Error::from_raw_os_error(libc::ENOENT);
        let err = Error::ScanDevDir(io_err);
        let msg = err.to_string();
        assert!(msg.contains("/dev"));
    }

    #[test]
    fn test_error_timeout_display() {
        let err = Error::Timeout { timeout_ms: 5000 };
        let msg = err.to_string();
        assert!(msg.contains("5000"));
        assert!(msg.contains("timed out"));
    }

    #[test]
    fn test_from_errno_eexist() {
        let err = Error::from_errno(-libc::EEXIST);
        let msg = err.to_string();
        assert!(msg.contains("already exists"));
    }

    #[test]
    fn test_from_errno_einval() {
        let err = Error::from_errno(-libc::EINVAL);
        let msg = err.to_string();
        assert!(msg.contains("invalid argument"));
    }

    #[test]
    fn test_from_errno_unknown() {
        let err = Error::from_errno(-999);
        match err {
            Error::IoUringCommand { errno, message } => {
                assert_eq!(errno, -999);
                assert!(message.contains("unknown"));
            }
            _ => panic!("Expected IoUringCommand error"),
        }
    }

    #[test]
    fn test_is_not_found_other_errors() {
        // Test that other error types return false
        let err = Error::ControlDeviceNotFound;
        assert!(!err.is_not_found());

        let err = Error::DeviceBusy { dev_id: 0 };
        assert!(!err.is_not_found());

        let err = Error::Timeout { timeout_ms: 1000 };
        assert!(!err.is_not_found());
    }

    #[test]
    fn test_is_permission_denied_other_errors() {
        // Test that other error types return false
        let err = Error::ControlDeviceNotFound;
        assert!(!err.is_permission_denied());

        let err = Error::DeviceNotFound { dev_id: 0 };
        assert!(!err.is_permission_denied());

        let err = Error::DeviceBusy { dev_id: 0 };
        assert!(!err.is_permission_denied());
    }

    #[test]
    fn test_error_debug() {
        let err = Error::ControlDeviceNotFound;
        let debug = format!("{:?}", err);
        assert!(debug.contains("ControlDeviceNotFound"));

        let err = Error::DeviceNotFound { dev_id: 42 };
        let debug = format!("{:?}", err);
        assert!(debug.contains("42"));

        let err = Error::Timeout { timeout_ms: 100 };
        let debug = format!("{:?}", err);
        assert!(debug.contains("100"));
    }

    #[test]
    fn test_from_errno_all_common_errors() {
        let cases = [
            (-libc::ENOENT, "not found"),
            (-libc::EEXIST, "exists"),
            (-libc::EBUSY, "busy"),
            (-libc::EPERM, "permission"),
            (-libc::EINVAL, "invalid"),
        ];

        for (errno, expected_text) in cases {
            let err = Error::from_errno(errno);
            let msg = err.to_string();
            assert!(
                msg.contains(expected_text),
                "Expected '{}' in error for errno {}: {}",
                expected_text,
                errno,
                msg
            );
        }
    }
}
