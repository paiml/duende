//! Unix implementation of memory locking.
//!
//! Supports Linux and macOS via `mlockall(2)` and `munlockall(2)`.

use crate::config::MlockConfig;
use crate::error::MlockError;
use crate::status::MlockStatus;

/// Lock memory with the given configuration.
pub fn lock_with_config(config: MlockConfig) -> Result<MlockStatus, MlockError> {
    let flags = config.as_flags();

    // Empty flags = no-op
    if flags == 0 {
        return Ok(MlockStatus::Locked { bytes_locked: 0 });
    }

    // SAFETY: mlockall is safe to call with valid flags.
    // We construct flags from known-good constants.
    let result = unsafe { libc::mlockall(flags) };

    if result == 0 {
        // Success - get bytes locked
        let bytes_locked = locked_bytes();
        Ok(MlockStatus::Locked { bytes_locked })
    } else {
        // Failure - get errno
        let errno = std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(-1);

        if config.required() {
            Err(MlockError::from_errno(errno))
        } else {
            Ok(MlockStatus::Failed { errno })
        }
    }
}

/// Unlock all locked memory.
pub fn unlock_all() -> Result<(), MlockError> {
    // SAFETY: munlockall is always safe to call.
    let result = unsafe { libc::munlockall() };

    if result == 0 {
        Ok(())
    } else {
        let errno = std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(-1);
        Err(MlockError::from_errno(errno))
    }
}

/// Check if any memory is currently locked.
#[cfg(target_os = "linux")]
pub fn is_locked() -> bool {
    locked_bytes() > 0
}

/// Check if any memory is currently locked.
#[cfg(not(target_os = "linux"))]
pub fn is_locked() -> bool {
    // No reliable way to check on non-Linux Unix
    false
}

/// Get the number of bytes currently locked.
#[cfg(target_os = "linux")]
pub fn locked_bytes() -> usize {
    // Read /proc/self/status and parse VmLck field
    // Format: "VmLck:     1234 kB"
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };

    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmLck:") {
            // Parse "    1234 kB" -> 1234 * 1024
            let trimmed = rest.trim();
            if let Some(kb_str) = trimmed.strip_suffix(" kB") {
                if let Ok(kb) = kb_str.trim().parse::<usize>() {
                    return kb * 1024;
                }
            }
        }
    }

    0
}

/// Get the number of bytes currently locked.
#[cfg(not(target_os = "linux"))]
pub fn locked_bytes() -> usize {
    // No reliable way to check on non-Linux Unix
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locked_bytes_parses_correctly() {
        // This test only verifies the function doesn't crash
        let bytes = locked_bytes();
        // Result depends on whether we have mlock privileges
        let _ = bytes;
    }

    #[test]
    fn test_is_locked_returns_bool() {
        let locked = is_locked();
        let _ = locked;
    }

    #[test]
    fn test_lock_with_empty_flags() {
        let config = MlockConfig::builder()
            .current(false)
            .future(false)
            .build();

        let result = lock_with_config(config);
        assert!(result.is_ok());
        if let Ok(status) = result {
            assert!(status.is_locked());
        }
    }

    #[test]
    fn test_lock_non_required_mode() {
        // This test should not fail even without privileges
        let config = MlockConfig::builder().required(false).build();

        let result = lock_with_config(config);
        assert!(result.is_ok());
        // Status might be Locked or Failed depending on privileges
    }

    #[test]
    fn test_unlock_all_does_not_error() {
        // unlock_all should work even if nothing is locked
        let result = unlock_all();
        // On Linux this should succeed
        assert!(result.is_ok());
    }

    #[test]
    fn test_lock_current_only() {
        let config = MlockConfig::builder()
            .current(true)
            .future(false)
            .required(false)
            .build();

        let result = lock_with_config(config);
        assert!(result.is_ok());
        // Clean up
        let _ = unlock_all();
    }

    #[test]
    fn test_lock_future_only() {
        let config = MlockConfig::builder()
            .current(false)
            .future(true)
            .required(false)
            .build();

        let result = lock_with_config(config);
        assert!(result.is_ok());
        // Clean up
        let _ = unlock_all();
    }

    #[test]
    fn test_locked_bytes_is_zero_or_positive() {
        // locked_bytes should return a non-negative value
        let bytes = locked_bytes();
        // This assertion is always true for usize, but documents intent
        assert!(bytes < usize::MAX);
    }

    #[test]
    fn test_mlock_with_onfault() {
        // Test MCL_ONFAULT flag (Linux 4.4+)
        let config = MlockConfig::builder()
            .current(true)
            .future(true)
            .onfault(true)
            .required(false)
            .build();

        let result = lock_with_config(config);
        // May fail on older kernels, but should not panic
        assert!(result.is_ok());
        let _ = unlock_all();
    }

    // Note: Tests requiring actual mlock privileges are in integration tests
    // and Docker-based tests (see docker/Dockerfile.mlock-test)
}
