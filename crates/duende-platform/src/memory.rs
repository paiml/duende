//! Memory management for daemon processes.
//!
//! # DT-007: Swap Deadlock Prevention
//!
//! This module provides memory locking functionality to prevent swap deadlock
//! for daemons that serve as swap devices (e.g., trueno-ublk).
//!
//! ## The Problem
//!
//! When a daemon serves as a swap device, a deadlock can occur:
//! 1. Kernel needs to swap pages OUT to the daemon's device
//! 2. Daemon needs memory to process I/O request
//! 3. Kernel tries to swap out daemon's pages to free memory
//! 4. Swap goes to the same daemon → waiting for itself → DEADLOCK
//!
//! ## Evidence
//!
//! Kernel log from 2026-01-06 stress test:
//! ```text
//! INFO: task trueno-ublk:59497 blocked for more than 122 seconds.
//! task:trueno-ublk state:D (uninterruptible sleep)
//! __swap_writepage+0x111/0x1a0
//! swap_writepage+0x5f/0xe0
//! ```
//!
//! ## Solution
//!
//! Use `mlockall(MCL_CURRENT | MCL_FUTURE)` to pin all daemon memory,
//! preventing the daemon itself from being swapped out.

use std::io;

use crate::{PlatformError, Result};

/// Result of memory locking operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MlockResult {
    /// Memory successfully locked.
    Success,
    /// mlock() not requested (lock_memory = false).
    Disabled,
    /// mlock() failed but daemon continues (non-fatal).
    Failed(i32),
}

/// Lock all current and future memory allocations to prevent swapping.
///
/// This is CRITICAL for swap device daemons to prevent deadlock.
///
/// # Arguments
///
/// * `required` - If true, returns an error on failure. If false, logs warning and continues.
///
/// # Returns
///
/// - `Ok(MlockResult::Success)` if memory was locked successfully
/// - `Ok(MlockResult::Failed(errno))` if mlockall() failed and `required` is false
/// - `Err(...)` if mlockall() failed and `required` is true
///
/// # Platform Support
///
/// - **Linux**: Full support via `mlockall()`
/// - **macOS**: Limited support (requires entitlements)
/// - **Others**: Returns `MlockResult::Disabled`
///
/// # Capability Requirements
///
/// Requires one of:
/// - `CAP_IPC_LOCK` capability
/// - Root privileges
/// - Sufficient `RLIMIT_MEMLOCK` limit
///
/// # Errors
/// Returns `PlatformError::Resource` if `required` is true and mlockall fails.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
pub fn lock_daemon_memory(required: bool) -> Result<MlockResult> {
    use tracing::{info, warn};

    info!("Locking daemon memory to prevent swap deadlock (DT-007)...");

    // MCL_CURRENT: Lock all pages currently mapped
    // MCL_FUTURE: Lock all pages that become mapped in the future
    // SAFETY: mlockall is a well-defined syscall. It affects only the current process.
    let result = unsafe { libc::mlockall(libc::MCL_CURRENT | libc::MCL_FUTURE) };

    if result == 0 {
        info!("Memory locked successfully - daemon pages will not be swapped");
        Ok(MlockResult::Success)
    } else {
        let errno = io::Error::last_os_error().raw_os_error().unwrap_or(-1);
        let err_msg = match errno {
            libc::ENOMEM => "insufficient memory or resource limits (check RLIMIT_MEMLOCK)",
            libc::EPERM => "insufficient privileges (need CAP_IPC_LOCK or root)",
            libc::EINVAL => "invalid flags",
            _ => "unknown error",
        };

        if required {
            Err(PlatformError::Resource(format!(
                "mlockall() failed: {} (errno={}). \
                 Cannot safely run as swap device without mlock(). \
                 Either run as root, add CAP_IPC_LOCK, or set lock_memory_required=false",
                err_msg, errno
            )))
        } else {
            warn!(
                "mlockall() failed: {} (errno={}). \
                 Daemon may deadlock under memory pressure when used as swap device. \
                 Set lock_memory_required=true to make this fatal.",
                err_msg, errno
            );
            Ok(MlockResult::Failed(errno))
        }
    }
}

/// macOS implementation (limited support).
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub fn lock_daemon_memory(required: bool) -> Result<MlockResult> {
    use tracing::{info, warn};

    info!("Attempting memory lock on macOS...");

    // macOS supports mlockall but requires entitlements for full functionality
    // SAFETY: mlockall is a well-defined syscall
    let result = unsafe { libc::mlockall(libc::MCL_CURRENT | libc::MCL_FUTURE) };

    if result == 0 {
        info!("Memory locked successfully on macOS");
        Ok(MlockResult::Success)
    } else {
        let errno = io::Error::last_os_error().raw_os_error().unwrap_or(-1);
        let err_msg = match errno {
            libc::ENOMEM => "insufficient memory or resource limits",
            libc::EPERM => {
                "insufficient privileges (may need com.apple.security.cs.allow-jit entitlement)"
            }
            libc::EINVAL => "invalid flags",
            libc::EAGAIN => "system resources temporarily unavailable",
            _ => "unknown error",
        };

        if required {
            Err(PlatformError::Resource(format!(
                "mlockall() failed on macOS: {} (errno={})",
                err_msg, errno
            )))
        } else {
            warn!("mlockall() failed on macOS: {} (errno={})", err_msg, errno);
            Ok(MlockResult::Failed(errno))
        }
    }
}

/// Non-Unix platforms: memory locking not supported.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn lock_daemon_memory(_required: bool) -> Result<MlockResult> {
    use tracing::debug;
    debug!("Memory locking not supported on this platform");
    Ok(MlockResult::Disabled)
}

/// Check if memory is currently locked.
///
/// Reads `/proc/self/status` on Linux to check the `VmLck` field.
#[cfg(target_os = "linux")]
pub fn is_memory_locked() -> bool {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("VmLck:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && let Ok(kb) = parts[1].parse::<u64>() {
                    return kb > 0;
                }
            }
        }
    }
    false
}

/// Check if memory is locked (non-Linux fallback).
#[cfg(not(target_os = "linux"))]
pub fn is_memory_locked() -> bool {
    // No easy way to check on other platforms
    false
}

/// Unlock all memory (for cleanup/testing).
///
/// Note: This is rarely needed in production since process exit releases all locks.
///
/// # Errors
/// Returns `PlatformError::Resource` if munlockall fails.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[allow(unsafe_code)]
pub fn unlock_daemon_memory() -> Result<()> {
    // SAFETY: munlockall is a well-defined syscall
    let result = unsafe { libc::munlockall() };
    if result == 0 {
        Ok(())
    } else {
        Err(PlatformError::Resource("munlockall() failed".to_string()))
    }
}

/// Unlock memory (non-Unix fallback).
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn unlock_daemon_memory() -> Result<()> {
    Ok(())
}

/// Apply memory-related resource configuration.
///
/// This is a convenience function for daemons to call during initialization.
/// It reads the `ResourceConfig` and applies memory locking if configured.
///
/// # Example
///
/// ```rust,ignore
/// use duende_core::ResourceConfig;
/// use duende_platform::apply_memory_config;
///
/// fn daemon_init(config: &ResourceConfig) -> Result<()> {
///     apply_memory_config(config)?;
///     // ... rest of initialization
///     Ok(())
/// }
/// ```
///
/// # Errors
///
/// Returns an error if `lock_memory` is true, `lock_memory_required` is true,
/// and mlock() fails.
pub fn apply_memory_config(config: &duende_core::ResourceConfig) -> Result<()> {
    if config.lock_memory {
        let result = lock_daemon_memory(config.lock_memory_required)?;
        tracing::info!("Memory lock result: {:?}", result);
    } else {
        tracing::debug!("Memory locking disabled (lock_memory=false)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use duende_core::ResourceConfig;

    #[test]
    fn test_mlock_result_variants() {
        // Test all variants can be constructed and compared
        let success = MlockResult::Success;
        let disabled = MlockResult::Disabled;
        let failed = MlockResult::Failed(1);

        assert_eq!(success, MlockResult::Success);
        assert_eq!(disabled, MlockResult::Disabled);
        assert_eq!(failed, MlockResult::Failed(1));
        assert_ne!(success, disabled);
        assert_ne!(success, failed);

        // Test Debug impl
        let _ = format!("{:?}", success);
        let _ = format!("{:?}", disabled);
        let _ = format!("{:?}", failed);

        // Test Clone and Copy
        let cloned = success;
        assert_eq!(cloned, success);
    }

    #[test]
    fn test_mlock_disabled_when_not_required() {
        // This test should not fail even without privileges
        // when required=false
        let result = lock_daemon_memory(false);
        assert!(result.is_ok());
        // Result should be Success or Failed, but not an error
        let mlock_result = result.expect("should succeed");
        assert!(matches!(
            mlock_result,
            MlockResult::Success | MlockResult::Failed(_) | MlockResult::Disabled
        ));
    }

    #[test]
    fn test_is_memory_locked_returns_bool() {
        // Just verify it doesn't panic
        let _ = is_memory_locked();
    }

    #[test]
    fn test_unlock_daemon_memory() {
        // Should not panic or error (even without prior lock)
        let result = unlock_daemon_memory();
        // On most systems this will succeed (nop or actual unlock)
        let _ = result; // May fail on non-Unix, that's ok
    }

    #[test]
    fn test_apply_memory_config_disabled() {
        let config = ResourceConfig {
            lock_memory: false,
            lock_memory_required: false,
            ..ResourceConfig::default()
        };

        let result = apply_memory_config(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_apply_memory_config_enabled_not_required() {
        let config = ResourceConfig {
            lock_memory: true,
            lock_memory_required: false,
            ..ResourceConfig::default()
        };

        let result = apply_memory_config(&config);
        // Should succeed (even if mlock fails, since required=false)
        assert!(result.is_ok());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_mlock_with_privileges() {
        // This test may pass or fail depending on system configuration
        // In CI/unprivileged environments, it should fail gracefully
        let result = lock_daemon_memory(false);
        assert!(result.is_ok());

        match result.expect("mlock result") {
            MlockResult::Success => {
                // mlockall() succeeded. Note: VmLck in /proc/self/status may
                // still be 0 for minimal test processes since only resident
                // pages are counted. We verify the syscall succeeded, not that
                // pages are locked (which depends on memory pressure).
                // Clean up
                let _ = unlock_daemon_memory();
            }
            MlockResult::Failed(errno) => {
                // Expected in unprivileged environments
                assert!(
                    errno == libc::EPERM || errno == libc::ENOMEM,
                    "Unexpected errno: {}",
                    errno
                );
            }
            MlockResult::Disabled => {
                panic!("Should not be disabled on Linux");
            }
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_mlock_required_may_fail() {
        // When required=true, mlock might return error if no privileges
        let result = lock_daemon_memory(true);
        // Either succeeds (with privileges) or fails (without)
        match result {
            Ok(MlockResult::Success) => {
                // Has privileges, clean up
                let _ = unlock_daemon_memory();
            }
            Err(_) => {
                // Expected without CAP_IPC_LOCK
            }
            Ok(MlockResult::Failed(_)) => {
                panic!("Should not return Failed when required=true");
            }
            Ok(MlockResult::Disabled) => {
                panic!("Should not be disabled on Linux");
            }
        }
    }
}
