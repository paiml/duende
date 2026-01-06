//! Status types for memory locking results.

use std::fmt;

/// Result of a memory locking operation.
///
/// This type captures both successful and unsuccessful (but non-fatal) outcomes.
/// Fatal errors are returned as [`Err(MlockError)`](crate::MlockError).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MlockStatus {
    /// Memory was successfully locked.
    ///
    /// Contains the number of bytes locked at the time of the call.
    /// Note: This may increase as more memory is allocated (with `MCL_FUTURE`).
    Locked {
        /// Bytes locked at time of call (from /proc/self/status `VmLck`).
        bytes_locked: usize,
    },

    /// Memory locking failed but was not required.
    ///
    /// Only returned when `MlockConfig::required(false)` is set.
    /// The daemon should continue with a warning.
    Failed {
        /// The errno that caused the failure.
        errno: i32,
    },

    /// Memory locking is not supported on this platform.
    ///
    /// Returned on non-Unix platforms (Windows, WASM, etc.).
    Unsupported,
}

impl MlockStatus {
    /// Check if memory is currently locked.
    ///
    /// Returns `true` for [`MlockStatus::Locked`], `false` otherwise.
    #[must_use]
    pub const fn is_locked(&self) -> bool {
        matches!(self, Self::Locked { .. })
    }

    /// Check if locking failed.
    ///
    /// Returns `true` for [`MlockStatus::Failed`], `false` otherwise.
    #[must_use]
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    /// Check if mlock is unsupported on this platform.
    ///
    /// Returns `true` for [`MlockStatus::Unsupported`], `false` otherwise.
    #[must_use]
    pub const fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported)
    }

    /// Get the number of bytes locked.
    ///
    /// Returns the bytes locked for [`MlockStatus::Locked`], `0` otherwise.
    #[must_use]
    pub const fn bytes_locked(&self) -> usize {
        match self {
            Self::Locked { bytes_locked } => *bytes_locked,
            Self::Failed { .. } | Self::Unsupported => 0,
        }
    }

    /// Get the failure errno, if any.
    ///
    /// Returns `Some(errno)` for [`MlockStatus::Failed`], `None` otherwise.
    #[must_use]
    pub const fn failure_errno(&self) -> Option<i32> {
        match self {
            Self::Failed { errno } => Some(*errno),
            Self::Locked { .. } | Self::Unsupported => None,
        }
    }
}

impl fmt::Display for MlockStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Locked { bytes_locked } => {
                if *bytes_locked >= 1024 * 1024 {
                    write!(f, "locked ({} MB)", bytes_locked / (1024 * 1024))
                } else if *bytes_locked >= 1024 {
                    write!(f, "locked ({} KB)", bytes_locked / 1024)
                } else {
                    write!(f, "locked ({bytes_locked} bytes)")
                }
            }
            Self::Failed { errno } => {
                write!(f, "failed (errno={errno})")
            }
            Self::Unsupported => {
                write!(f, "unsupported platform")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locked_status() {
        let status = MlockStatus::Locked {
            bytes_locked: 4096,
        };
        assert!(status.is_locked());
        assert!(!status.is_failed());
        assert!(!status.is_unsupported());
        assert_eq!(status.bytes_locked(), 4096);
        assert_eq!(status.failure_errno(), None);
    }

    #[test]
    fn test_failed_status() {
        let status = MlockStatus::Failed { errno: 1 };
        assert!(!status.is_locked());
        assert!(status.is_failed());
        assert!(!status.is_unsupported());
        assert_eq!(status.bytes_locked(), 0);
        assert_eq!(status.failure_errno(), Some(1));
    }

    #[test]
    fn test_unsupported_status() {
        let status = MlockStatus::Unsupported;
        assert!(!status.is_locked());
        assert!(!status.is_failed());
        assert!(status.is_unsupported());
        assert_eq!(status.bytes_locked(), 0);
        assert_eq!(status.failure_errno(), None);
    }

    #[test]
    fn test_display_locked_bytes() {
        let status = MlockStatus::Locked { bytes_locked: 512 };
        assert_eq!(format!("{status}"), "locked (512 bytes)");
    }

    #[test]
    fn test_display_locked_kb() {
        let status = MlockStatus::Locked {
            bytes_locked: 4096,
        };
        assert_eq!(format!("{status}"), "locked (4 KB)");
    }

    #[test]
    fn test_display_locked_mb() {
        let status = MlockStatus::Locked {
            bytes_locked: 10 * 1024 * 1024,
        };
        assert_eq!(format!("{status}"), "locked (10 MB)");
    }

    #[test]
    fn test_display_failed() {
        let status = MlockStatus::Failed { errno: 12 };
        assert_eq!(format!("{status}"), "failed (errno=12)");
    }

    #[test]
    fn test_display_unsupported() {
        let status = MlockStatus::Unsupported;
        assert_eq!(format!("{status}"), "unsupported platform");
    }
}
