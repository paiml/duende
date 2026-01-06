//! duende-ublk - ublk device lifecycle management
//!
//! This crate provides tools for managing ublk (userspace block device) lifecycle,
//! particularly for swap-critical daemons that need to clean up orphaned devices.
//!
//! # Problem: Orphaned ublk Devices
//!
//! When a ublk daemon crashes or is killed, the kernel may retain device state
//! even after the `/dev/ublkbN` block device disappears. This causes:
//!
//! - "File exists" errors when creating new devices
//! - Device ID conflicts
//! - System requires reboot to clear stale state
//!
//! # Solution
//!
//! ```rust,no_run
//! use duende_ublk::{UblkControl, cleanup_orphaned_devices};
//!
//! fn main() -> Result<(), duende_ublk::Error> {
//!     // Clean up any orphaned devices from previous crashes
//!     let cleaned = cleanup_orphaned_devices()?;
//!     println!("Cleaned {} orphaned devices", cleaned);
//!
//!     // Now safe to create new devices
//!     Ok(())
//! }
//! ```
//!
//! # Kernel Interface
//!
//! This crate uses io_uring URING_CMD to communicate with the ublk kernel driver.
//! Linux 6.0+ is required.

#![forbid(unsafe_op_in_unsafe_fn)]

mod control;
mod error;
mod sys;

pub use control::{UblkControl, cleanup_orphaned_devices, detect_orphaned_devices};
pub use error::Error;
pub use sys::{UBLK_CTRL_DEV, UblkCtrlCmd, UblkCtrlDevInfo};

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // A. Struct Layout Tests (Protocol Correctness)
    // ============================================================================

    #[test]
    fn test_ctrl_cmd_size() {
        // Kernel ublksrv_ctrl_cmd is exactly 32 bytes
        assert_eq!(std::mem::size_of::<sys::UblkCtrlCmd>(), 32);
    }

    #[test]
    fn test_ctrl_cmd_ext_size() {
        // Extended command for io_uring SQE cmd field is 80 bytes
        assert_eq!(std::mem::size_of::<sys::UblkCtrlCmdExt>(), 80);
    }

    #[test]
    fn test_ctrl_dev_info_size() {
        // Kernel ublksrv_ctrl_dev_info is exactly 64 bytes
        assert_eq!(std::mem::size_of::<sys::UblkCtrlDevInfo>(), 64);
    }

    // ============================================================================
    // B. ioctl Encoding Tests
    // ============================================================================

    #[test]
    fn test_del_dev_ioctl_value() {
        // UBLK_U_CMD_DEL_DEV = _IOWR('u', 0x05, struct ublksrv_ctrl_cmd)
        // = (3 << 30) | (32 << 16) | (0x75 << 8) | 0x05
        let expected = (3u32 << 30) | (32u32 << 16) | (0x75u32 << 8) | 0x05;
        assert_eq!(sys::UBLK_U_CMD_DEL_DEV, expected);
    }

    #[test]
    fn test_stop_dev_ioctl_value() {
        // UBLK_U_CMD_STOP_DEV = _IOWR('u', 0x07, struct ublksrv_ctrl_cmd)
        let expected = (3u32 << 30) | (32u32 << 16) | (0x75u32 << 8) | 0x07;
        assert_eq!(sys::UBLK_U_CMD_STOP_DEV, expected);
    }

    #[test]
    fn test_get_dev_info_ioctl_value() {
        // UBLK_U_CMD_GET_DEV_INFO = _IOR('u', 0x02, struct ublksrv_ctrl_cmd)
        let expected = (2u32 << 30) | (32u32 << 16) | (0x75u32 << 8) | 0x02;
        assert_eq!(sys::UBLK_U_CMD_GET_DEV_INFO, expected);
    }

    // ============================================================================
    // C. Error Type Tests
    // ============================================================================

    #[test]
    fn test_error_display() {
        let err = Error::ControlDeviceNotFound;
        assert!(err.to_string().contains("control"));

        let err = Error::DeviceNotFound { dev_id: 5 };
        assert!(err.to_string().contains("5"));
    }

    // ============================================================================
    // D. Orphan Detection Tests (requires /dev access)
    // ============================================================================

    #[test]
    fn test_detect_orphaned_devices_no_panic() {
        // Should not panic on systems without ublk
        let result = detect_orphaned_devices();
        // Either Ok with empty vec or error is acceptable
        if let Ok(orphans) = result {
            // On a clean system, should be empty
            // (can't assert this as system state varies)
            let _ = orphans;
        }
        // Error is also acceptable if /dev/ublk-control doesn't exist
    }

    #[test]
    fn test_cleanup_orphaned_devices_no_panic() {
        // Should not panic on systems without ublk
        let result = cleanup_orphaned_devices();
        // Either Ok or error is acceptable
        assert!(result.is_ok() || result.is_err());
    }
}
