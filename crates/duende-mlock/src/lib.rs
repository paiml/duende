// Iron Lotus: Allow unwrap/expect in tests for clear failure messages
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! # duende-mlock
//!
//! Memory locking for swap-critical daemons.
//!
//! ## DT-007: Swap Deadlock Prevention
//!
//! When a daemon serves as a swap device (e.g., `trueno-ublk`), a deadlock occurs if:
//!
//! 1. Kernel needs memory → initiates swap-out to the daemon
//! 2. Daemon needs memory to process I/O
//! 3. Kernel tries to swap daemon's pages → to the same daemon
//! 4. **Deadlock**: daemon blocked waiting for itself
//!
//! This crate provides `mlockall()` to pin daemon memory, preventing the kernel
//! from swapping it out.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use duende_mlock::{lock_all, MlockError};
//!
//! fn main() -> Result<(), MlockError> {
//!     // Lock all current and future memory allocations
//!     let status = lock_all()?;
//!     println!("Locked {} bytes", status.bytes_locked());
//!     Ok(())
//! }
//! ```
//!
//! ## Configuration
//!
//! ```rust,no_run
//! use duende_mlock::{MlockConfig, lock_with_config};
//!
//! let config = MlockConfig::builder()
//!     .current(true)      // Lock existing pages
//!     .future(true)       // Lock future allocations
//!     .required(false)    // Don't fail if mlock fails
//!     .build();
//!
//! match lock_with_config(config) {
//!     Ok(status) => println!("Locked: {}", status.is_locked()),
//!     Err(e) => eprintln!("Warning: {}", e),
//! }
//! ```
//!
//! ## Platform Support
//!
//! | Platform | Support | Notes |
//! |----------|---------|-------|
//! | Linux    | Full    | Requires `CAP_IPC_LOCK` or root |
//! | macOS    | Limited | Requires entitlements |
//! | Others   | None    | Returns `MlockStatus::Unsupported` |
//!
//! ## Container Requirements
//!
//! ```bash
//! # Docker
//! docker run --cap-add=IPC_LOCK --ulimit memlock=-1:-1 ...
//!
//! # docker-compose.yml
//! cap_add:
//!   - IPC_LOCK
//! ulimits:
//!   memlock:
//!     soft: -1
//!     hard: -1
//! ```

#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs, rust_2018_idioms)]

mod config;
mod error;
mod status;

#[cfg(unix)]
mod unix;

#[cfg(not(unix))]
mod unsupported;

pub use config::{MlockConfig, MlockConfigBuilder};
pub use error::MlockError;
pub use status::MlockStatus;

/// Lock all current and future memory allocations.
///
/// This is the recommended function for daemon memory locking. It calls
/// `mlockall(MCL_CURRENT | MCL_FUTURE)` to pin all existing pages and
/// ensure future allocations are also locked.
///
/// # Errors
///
/// Returns [`MlockError`] if memory locking fails:
///
/// - [`MlockError::PermissionDenied`]: Need `CAP_IPC_LOCK` capability or root
/// - [`MlockError::ResourceLimit`]: `RLIMIT_MEMLOCK` too low
/// - [`MlockError::InvalidArgument`]: Invalid flags (shouldn't happen)
///
/// # Example
///
/// ```rust,no_run
/// use duende_mlock::lock_all;
///
/// let status = lock_all()?;
/// assert!(status.is_locked());
/// # Ok::<(), duende_mlock::MlockError>(())
/// ```
///
/// # Platform Behavior
///
/// - **Linux/macOS**: Calls `mlockall(MCL_CURRENT | MCL_FUTURE)`
/// - **Others**: Returns `Ok(MlockStatus::Unsupported)`
pub fn lock_all() -> Result<MlockStatus, MlockError> {
    lock_with_config(MlockConfig::default())
}

/// Lock memory with custom configuration.
///
/// Use [`MlockConfig::builder()`] to create a configuration:
///
/// ```rust,no_run
/// use duende_mlock::{MlockConfig, lock_with_config};
///
/// let config = MlockConfig::builder()
///     .current(true)
///     .future(true)
///     .required(false)  // Don't fail on error
///     .build();
///
/// let status = lock_with_config(config)?;
/// # Ok::<(), duende_mlock::MlockError>(())
/// ```
///
/// # Non-Required Mode
///
/// When `required(false)` is set, mlock failures return `Ok(MlockStatus::Failed { .. })`
/// instead of `Err`. This allows daemons to continue with a warning.
///
/// # Errors
///
/// Returns [`MlockError::PermissionDenied`] if the process lacks `CAP_IPC_LOCK`.
/// Returns [`MlockError::ResourceLimit`] if `RLIMIT_MEMLOCK` is exceeded.
/// Returns [`MlockError::Unsupported`] on non-Unix platforms.
pub fn lock_with_config(config: MlockConfig) -> Result<MlockStatus, MlockError> {
    #[cfg(unix)]
    {
        unix::lock_with_config(config)
    }

    #[cfg(not(unix))]
    {
        unsupported::lock_with_config(config)
    }
}

/// Unlock all locked memory.
///
/// Calls `munlockall()` to release all memory locks. This is rarely needed
/// in production since process exit automatically releases all locks.
///
/// # Example
///
/// ```rust,no_run
/// use duende_mlock::{lock_all, unlock_all};
///
/// let _ = lock_all()?;
/// // ... do work ...
/// unlock_all()?;
/// # Ok::<(), duende_mlock::MlockError>(())
/// ```
///
/// # Errors
///
/// Returns [`MlockError::Unsupported`] if `munlockall()` fails.
pub fn unlock_all() -> Result<(), MlockError> {
    #[cfg(unix)]
    {
        unix::unlock_all()
    }

    #[cfg(not(unix))]
    {
        Ok(())
    }
}

/// Check if process memory is currently locked.
///
/// On Linux, reads `/proc/self/status` and checks the `VmLck` field.
/// On other platforms, returns `false`.
///
/// # Example
///
/// ```rust,no_run
/// use duende_mlock::{lock_all, is_locked};
///
/// assert!(!is_locked());
/// lock_all()?;
/// assert!(is_locked());
/// # Ok::<(), duende_mlock::MlockError>(())
/// ```
#[must_use]
pub fn is_locked() -> bool {
    #[cfg(unix)]
    {
        unix::is_locked()
    }

    #[cfg(not(unix))]
    {
        false
    }
}

/// Get the number of bytes currently locked.
///
/// On Linux, reads the `VmLck` field from `/proc/self/status`.
/// On other platforms, returns `0`.
///
/// # Example
///
/// ```rust,no_run
/// use duende_mlock::{lock_all, locked_bytes};
///
/// lock_all()?;
/// println!("Locked {} KB", locked_bytes() / 1024);
/// # Ok::<(), duende_mlock::MlockError>(())
/// ```
#[must_use]
pub fn locked_bytes() -> usize {
    #[cfg(unix)]
    {
        unix::locked_bytes()
    }

    #[cfg(not(unix))]
    {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MlockConfig::default();
        assert!(config.current());
        assert!(config.future());
        assert!(config.required());
        assert!(!config.onfault());
    }

    #[test]
    fn test_config_builder() {
        let config = MlockConfig::builder()
            .current(false)
            .future(true)
            .required(false)
            .onfault(true)
            .build();

        assert!(!config.current());
        assert!(config.future());
        assert!(!config.required());
        assert!(config.onfault());
    }

    #[test]
    fn test_is_locked_returns_bool() {
        // Should not panic regardless of privileges
        let _ = is_locked();
    }

    #[test]
    fn test_locked_bytes_returns_usize() {
        // Should not panic regardless of privileges
        let _ = locked_bytes();
    }

    #[test]
    fn test_unlock_all_does_not_panic() {
        // unlock_all should not panic even if nothing is locked
        let result = unlock_all();
        // Result depends on platform and privileges
        let _ = result;
    }

    #[test]
    fn test_lock_all_non_fatal() {
        // Test that lock_all works (may succeed or fail based on privileges)
        let config = MlockConfig::builder().required(false).build();
        let result = lock_with_config(config);
        assert!(result.is_ok());
        // Clean up
        let _ = unlock_all();
    }

    #[test]
    fn test_lock_with_config_empty_flags() {
        // With no flags, should succeed
        let config = MlockConfig::builder().current(false).future(false).build();
        let result = lock_with_config(config);
        assert!(result.is_ok());
        if let Ok(status) = result {
            assert!(status.is_locked());
            assert_eq!(status.bytes_locked(), 0);
        }
    }

    #[test]
    fn test_mlock_status_methods() {
        // Test MlockStatus methods
        let locked = MlockStatus::Locked { bytes_locked: 1024 };
        assert!(locked.is_locked());
        assert!(!locked.is_failed());
        assert!(!locked.is_unsupported());
        assert_eq!(locked.bytes_locked(), 1024);
        assert_eq!(locked.failure_errno(), None);

        let failed = MlockStatus::Failed { errno: 1 };
        assert!(!failed.is_locked());
        assert!(failed.is_failed());
        assert!(!failed.is_unsupported());
        assert_eq!(failed.bytes_locked(), 0);
        assert_eq!(failed.failure_errno(), Some(1));

        let unsupported = MlockStatus::Unsupported;
        assert!(!unsupported.is_locked());
        assert!(!unsupported.is_failed());
        assert!(unsupported.is_unsupported());
        assert_eq!(unsupported.bytes_locked(), 0);
        assert_eq!(unsupported.failure_errno(), None);
    }

    #[test]
    fn test_mlock_status_display() {
        let locked = MlockStatus::Locked { bytes_locked: 1024 };
        assert!(format!("{locked}").contains("locked"));

        let failed = MlockStatus::Failed { errno: 12 };
        assert!(format!("{failed}").contains("failed"));

        let unsupported = MlockStatus::Unsupported;
        assert!(format!("{unsupported}").contains("unsupported"));
    }
}
