//! Error types for memory locking operations.

use std::fmt;

/// Error type for memory locking operations.
///
/// Each variant includes remediation guidance in its `Display` implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MlockError {
    /// Permission denied (EPERM).
    ///
    /// The process lacks the `CAP_IPC_LOCK` capability.
    ///
    /// # Remediation
    ///
    /// - Run as root: `sudo ./daemon`
    /// - Add capability: `sudo setcap cap_ipc_lock=+ep ./daemon`
    /// - Docker: `docker run --cap-add=IPC_LOCK ...`
    PermissionDenied,

    /// Resource limit exceeded (ENOMEM).
    ///
    /// Either:
    /// - `RLIMIT_MEMLOCK` is too low
    /// - Insufficient physical memory available
    /// - Kernel cannot allocate tracking structures
    ///
    /// # Remediation
    ///
    /// - Raise limit: `ulimit -l unlimited`
    /// - Docker: `docker run --ulimit memlock=-1:-1 ...`
    /// - Systemd: `LimitMEMLOCK=infinity` in unit file
    ResourceLimit,

    /// Invalid argument (EINVAL).
    ///
    /// Shouldn't occur with this library unless using experimental flags
    /// on unsupported kernels.
    InvalidArgument,

    /// Operation would block (EAGAIN).
    ///
    /// Some pages could not be locked. Typically occurs on macOS.
    WouldBlock,

    /// Unknown error with errno value.
    ///
    /// Contains the raw errno for debugging.
    Unknown(i32),
}

impl MlockError {
    /// Create an error from a raw errno value.
    #[must_use]
    pub const fn from_errno(errno: i32) -> Self {
        match errno {
            libc::EPERM => Self::PermissionDenied,
            libc::ENOMEM => Self::ResourceLimit,
            libc::EINVAL => Self::InvalidArgument,
            libc::EAGAIN => Self::WouldBlock,
            _ => Self::Unknown(errno),
        }
    }

    /// Get the raw errno value, if available.
    #[must_use]
    pub const fn errno(&self) -> Option<i32> {
        match self {
            Self::PermissionDenied => Some(libc::EPERM),
            Self::ResourceLimit => Some(libc::ENOMEM),
            Self::InvalidArgument => Some(libc::EINVAL),
            Self::WouldBlock => Some(libc::EAGAIN),
            Self::Unknown(e) => Some(*e),
        }
    }

    /// Check if this error indicates a permission issue.
    #[must_use]
    pub const fn is_permission_error(&self) -> bool {
        matches!(self, Self::PermissionDenied)
    }

    /// Check if this error indicates a resource limit issue.
    #[must_use]
    pub const fn is_resource_limit(&self) -> bool {
        matches!(self, Self::ResourceLimit)
    }
}

impl fmt::Display for MlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PermissionDenied => write!(
                f,
                "permission denied: need CAP_IPC_LOCK capability or root \
                 (docker: --cap-add=IPC_LOCK)"
            ),
            Self::ResourceLimit => write!(
                f,
                "resource limit: RLIMIT_MEMLOCK too low or insufficient memory \
                 (docker: --ulimit memlock=-1:-1)"
            ),
            Self::InvalidArgument => {
                write!(f, "invalid argument: unsupported flags for this kernel")
            }
            Self::WouldBlock => write!(f, "would block: some pages could not be locked"),
            Self::Unknown(errno) => write!(f, "mlock failed with errno={errno}"),
        }
    }
}

impl std::error::Error for MlockError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_errno_eperm() {
        let err = MlockError::from_errno(libc::EPERM);
        assert_eq!(err, MlockError::PermissionDenied);
        assert!(err.is_permission_error());
        assert!(!err.is_resource_limit());
    }

    #[test]
    fn test_from_errno_enomem() {
        let err = MlockError::from_errno(libc::ENOMEM);
        assert_eq!(err, MlockError::ResourceLimit);
        assert!(!err.is_permission_error());
        assert!(err.is_resource_limit());
    }

    #[test]
    fn test_from_errno_einval() {
        let err = MlockError::from_errno(libc::EINVAL);
        assert_eq!(err, MlockError::InvalidArgument);
        assert!(!err.is_permission_error());
        assert!(!err.is_resource_limit());
    }

    #[test]
    fn test_from_errno_eagain() {
        let err = MlockError::from_errno(libc::EAGAIN);
        assert_eq!(err, MlockError::WouldBlock);
        assert!(!err.is_permission_error());
        assert!(!err.is_resource_limit());
    }

    #[test]
    fn test_from_errno_unknown() {
        let err = MlockError::from_errno(999);
        assert_eq!(err, MlockError::Unknown(999));
        assert_eq!(err.errno(), Some(999));
    }

    #[test]
    fn test_errno_all_variants() {
        assert_eq!(MlockError::PermissionDenied.errno(), Some(libc::EPERM));
        assert_eq!(MlockError::ResourceLimit.errno(), Some(libc::ENOMEM));
        assert_eq!(MlockError::InvalidArgument.errno(), Some(libc::EINVAL));
        assert_eq!(MlockError::WouldBlock.errno(), Some(libc::EAGAIN));
        assert_eq!(MlockError::Unknown(42).errno(), Some(42));
    }

    #[test]
    fn test_display_permission_denied() {
        let err = MlockError::PermissionDenied;
        let msg = format!("{err}");
        assert!(msg.contains("CAP_IPC_LOCK"));
        assert!(msg.contains("--cap-add=IPC_LOCK"));
    }

    #[test]
    fn test_display_resource_limit() {
        let err = MlockError::ResourceLimit;
        let msg = format!("{err}");
        assert!(msg.contains("RLIMIT_MEMLOCK"));
        assert!(msg.contains("--ulimit memlock=-1:-1"));
    }

    #[test]
    fn test_display_invalid_argument() {
        let err = MlockError::InvalidArgument;
        let msg = format!("{err}");
        assert!(msg.contains("invalid argument"));
    }

    #[test]
    fn test_display_would_block() {
        let err = MlockError::WouldBlock;
        let msg = format!("{err}");
        assert!(msg.contains("would block"));
    }

    #[test]
    fn test_display_unknown() {
        let err = MlockError::Unknown(42);
        let msg = format!("{err}");
        assert!(msg.contains("errno=42"));
    }

    #[test]
    fn test_error_trait() {
        let err: &dyn std::error::Error = &MlockError::PermissionDenied;
        // Verify Error trait is implemented
        assert!(!err.to_string().is_empty());
    }
}
