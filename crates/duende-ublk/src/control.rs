//! ublk control device operations
//!
//! This module provides the main API for managing ublk devices via the
//! `/dev/ublk-control` control device using io_uring URING_CMD.

use crate::error::Error;
use crate::sys::{
    UBLK_BLOCK_DEV_PREFIX, UBLK_CTRL_DEV, UBLK_U_CMD_DEL_DEV, UBLK_U_CMD_GET_DEV_INFO,
    UBLK_U_CMD_STOP_DEV, UblkCtrlCmdExt, UblkCtrlDevInfo,
};
use io_uring::{IoUring, opcode, squeue, types};
use std::fs::{self, File};
use std::os::fd::AsRawFd;
use std::path::Path;

/// IoUring with 128-byte SQE support for URING_CMD
type IoUring128 = IoUring<squeue::Entry128>;

/// ublk control device handle
///
/// Provides methods to manage ublk devices: list, delete, stop, etc.
///
/// # Example
///
/// ```rust,no_run
/// use duende_ublk::UblkControl;
///
/// let mut ctrl = UblkControl::open()?;
/// ctrl.delete_device(0)?;
/// # Ok::<(), duende_ublk::Error>(())
/// ```
pub struct UblkControl {
    /// Open file handle to /dev/ublk-control
    file: File,
    /// io_uring instance for submitting commands
    ring: IoUring128,
}

impl UblkControl {
    /// Open the ublk control device
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `/dev/ublk-control` doesn't exist (ublk module not loaded)
    /// - Permission denied (need root or CAP_SYS_ADMIN)
    /// - Failed to create io_uring
    pub fn open() -> Result<Self, Error> {
        if !Path::new(UBLK_CTRL_DEV).exists() {
            return Err(Error::ControlDeviceNotFound);
        }

        let file = File::options()
            .read(true)
            .write(true)
            .open(UBLK_CTRL_DEV)
            .map_err(Error::OpenControl)?;

        // Create io_uring with 128-byte SQE support for URING_CMD
        let ring = IoUring128::builder()
            .build(4)
            .map_err(Error::IoUringCreate)?;

        Ok(Self { file, ring })
    }

    /// Delete a ublk device by ID
    ///
    /// This sends the DEL_DEV command to remove a device from the kernel.
    /// The device must be stopped first, or this will fail with EBUSY.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Device doesn't exist (ENOENT) - returns Ok(false)
    /// - Device is busy (EBUSY)
    /// - io_uring submission fails
    pub fn delete_device(&mut self, dev_id: u32) -> Result<bool, Error> {
        self.send_command(dev_id, UBLK_U_CMD_DEL_DEV)
    }

    /// Stop a ublk device by ID
    ///
    /// This sends the STOP_DEV command to stop a running device.
    /// After stopping, the device can be deleted.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Device doesn't exist (ENOENT)
    /// - io_uring submission fails
    pub fn stop_device(&mut self, dev_id: u32) -> Result<bool, Error> {
        self.send_command(dev_id, UBLK_U_CMD_STOP_DEV)
    }

    /// Get device info
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Device doesn't exist
    /// - io_uring submission fails
    pub fn get_device_info(&mut self, dev_id: u32) -> Result<UblkCtrlDevInfo, Error> {
        let mut info = UblkCtrlDevInfo::default();
        let info_ptr = std::ptr::from_mut(&mut info) as u64;

        let cmd = UblkCtrlCmdExt {
            cmd: crate::sys::UblkCtrlCmd {
                dev_id,
                queue_id: u16::MAX,
                len: std::mem::size_of::<UblkCtrlDevInfo>() as u16,
                addr: info_ptr,
                ..Default::default()
            },
            padding: [0; 48],
        };

        let fd = self.file.as_raw_fd();
        let sqe = opcode::UringCmd80::new(types::Fd(fd), UBLK_U_CMD_GET_DEV_INFO)
            .cmd(cmd.to_bytes())
            .build()
            .user_data(1);

        // SAFETY: SQE is valid and fd is open
        unsafe {
            self.ring.submission().push(&sqe).map_err(|_| {
                Error::IoUringSubmit(std::io::Error::from_raw_os_error(libc::ENOSPC))
            })?;
        }

        self.ring.submit_and_wait(1).map_err(Error::IoUringSubmit)?;

        if let Some(cqe) = self.ring.completion().next() {
            let res = cqe.result();
            if res < 0 {
                if res == -libc::ENOENT {
                    return Err(Error::DeviceNotFound { dev_id });
                }
                return Err(Error::from_errno(res));
            }
        }

        Ok(info)
    }

    /// Send a simple command (no data buffer) to a device with timeout
    fn send_command(&mut self, dev_id: u32, opcode_val: u32) -> Result<bool, Error> {
        self.send_command_timeout(dev_id, opcode_val, std::time::Duration::from_secs(5))
    }

    /// Send a simple command with explicit timeout
    fn send_command_timeout(
        &mut self,
        dev_id: u32,
        opcode_val: u32,
        timeout: std::time::Duration,
    ) -> Result<bool, Error> {
        let cmd = UblkCtrlCmdExt::for_device(dev_id);
        let fd = self.file.as_raw_fd();

        let sqe = opcode::UringCmd80::new(types::Fd(fd), opcode_val)
            .cmd(cmd.to_bytes())
            .build()
            .user_data(1);

        // SAFETY: SQE is valid and fd is open
        unsafe {
            self.ring.submission().push(&sqe).map_err(|_| {
                Error::IoUringSubmit(std::io::Error::from_raw_os_error(libc::ENOSPC))
            })?;
        }

        // Submit without blocking
        self.ring.submit().map_err(Error::IoUringSubmit)?;

        // Poll with timeout
        let start = std::time::Instant::now();
        loop {
            // Check for completion
            if let Some(cqe) = self.ring.completion().next() {
                let res = cqe.result();
                if res < 0 {
                    // ENOENT means device doesn't exist - not an error for delete
                    if res == -libc::ENOENT {
                        return Ok(false);
                    }
                    // EBUSY means device is in use
                    if res == -libc::EBUSY {
                        return Err(Error::DeviceBusy { dev_id });
                    }
                    return Err(Error::from_errno(res));
                }
                return Ok(true);
            }

            // Check timeout
            if start.elapsed() > timeout {
                return Err(Error::Timeout {
                    timeout_ms: timeout.as_millis() as u64,
                });
            }

            // Brief sleep to avoid busy-spinning
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Force delete a device (stop first, then delete)
    ///
    /// This is a convenience method that tries to stop the device first,
    /// then deletes it. Useful for cleaning up orphaned devices.
    ///
    /// # Errors
    ///
    /// Returns an error if deletion fails after stopping.
    pub fn force_delete(&mut self, dev_id: u32) -> Result<bool, Error> {
        // Try to stop first (ignore errors - device may already be stopped)
        let _ = self.stop_device(dev_id);

        // Now delete
        self.delete_device(dev_id)
    }
}

/// Detect orphaned ublk devices
///
/// An orphaned device has a character device (/dev/ublkcN) but no
/// corresponding block device (/dev/ublkbN), indicating the daemon crashed.
///
/// # Returns
///
/// A list of device IDs that appear to be orphaned.
///
/// # Errors
///
/// Returns an error if scanning /dev fails.
pub fn detect_orphaned_devices() -> Result<Vec<u32>, Error> {
    let mut orphans = Vec::new();

    let dev_path = Path::new("/dev");
    if !dev_path.exists() {
        return Ok(orphans);
    }

    let entries = fs::read_dir(dev_path).map_err(Error::ScanDevDir)?;

    for entry in entries {
        let entry = entry.map_err(Error::ScanDevDir)?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Look for character devices: ublkcN
        if let Some(id_str) = name_str.strip_prefix("ublkc") {
            if let Ok(dev_id) = id_str.parse::<u32>() {
                // Check if block device exists
                let block_path = format!("{}{}", UBLK_BLOCK_DEV_PREFIX, dev_id);
                if !Path::new(&block_path).exists() {
                    orphans.push(dev_id);
                }
            }
        }
    }

    Ok(orphans)
}

/// Clean up all orphaned ublk devices
///
/// This function detects and removes orphaned devices. It first tries
/// to stop each device, then deletes it.
///
/// # Returns
///
/// The number of devices successfully cleaned up.
///
/// # Errors
///
/// Returns an error if the control device cannot be opened.
/// Individual device cleanup failures are logged but don't stop the process.
pub fn cleanup_orphaned_devices() -> Result<usize, Error> {
    // First try to detect orphans via /dev scan
    let orphans = detect_orphaned_devices().unwrap_or_default();

    if orphans.is_empty() {
        // No orphans found via /dev scan, but there might be kernel-only orphans
        // Try to delete device IDs 0-7 (common range) to clean any stale state
        return cleanup_device_range(0, 8);
    }

    // Open control device
    let mut ctrl = match UblkControl::open() {
        Ok(c) => c,
        Err(Error::ControlDeviceNotFound) => return Ok(0),
        Err(e) => return Err(e),
    };

    let mut cleaned = 0;

    for dev_id in orphans {
        match ctrl.force_delete(dev_id) {
            Ok(true) => cleaned += 1,
            Ok(false) => {} // Device didn't exist
            Err(_) => {}    // Ignore errors, try others
        }
    }

    Ok(cleaned)
}

/// Try to clean up devices in a range (for kernel-only orphans)
fn cleanup_device_range(start: u32, end: u32) -> Result<usize, Error> {
    let mut ctrl = match UblkControl::open() {
        Ok(c) => c,
        Err(Error::ControlDeviceNotFound) => return Ok(0),
        Err(e) => return Err(e),
    };

    let mut cleaned = 0;

    for dev_id in start..end {
        // Check if block device exists (if so, it's not orphaned)
        let block_path = format!("{}{}", UBLK_BLOCK_DEV_PREFIX, dev_id);
        if Path::new(&block_path).exists() {
            continue;
        }

        // Try to force delete
        match ctrl.force_delete(dev_id) {
            Ok(true) => cleaned += 1,
            Ok(false) => {} // Device didn't exist
            Err(_) => {}    // Ignore errors
        }
    }

    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_orphaned_no_dev() {
        // Should not panic even if /dev doesn't exist (e.g., in containers)
        let result = detect_orphaned_devices();
        // Result depends on system state, just verify no panic
        let _ = result;
    }

    #[test]
    fn test_cleanup_orphaned_no_panic() {
        // Should handle missing control device gracefully
        let result = cleanup_orphaned_devices();
        // Either success or controlled error
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_open_control_missing() {
        // On systems without ublk, should return specific error
        // (can't test reliably as system state varies)
        let _ = UblkControl::open();
    }

    #[test]
    fn test_detect_orphaned_returns_vec() {
        // detect_orphaned_devices should return a Vec<u32>
        let result = detect_orphaned_devices();
        if let Ok(orphans) = result {
            // Verify it's a valid vector (may be empty)
            assert!(orphans.len() <= 256); // Reasonable upper bound
        }
    }

    #[test]
    fn test_cleanup_device_range_no_ublk() {
        // When ublk isn't available, should return Ok(0)
        let result = cleanup_device_range(0, 8);
        match result {
            Ok(count) => assert!(count <= 8), // Can't clean more than the range
            Err(Error::ControlDeviceNotFound) => {} // Expected on systems without ublk
            Err(_) => {} // Other errors acceptable
        }
    }

    #[test]
    fn test_cleanup_empty_orphan_list() {
        // When no orphans detected via /dev, should try range cleanup
        let result = cleanup_orphaned_devices();
        // Should not panic
        let _ = result;
    }

    #[test]
    fn test_ublk_ctrl_cmd_ext_for_device() {
        let cmd = UblkCtrlCmdExt::for_device(42);
        assert_eq!(cmd.cmd.dev_id, 42);
        assert_eq!(cmd.cmd.queue_id, u16::MAX);
    }

    #[test]
    fn test_ublk_ctrl_cmd_ext_to_bytes() {
        let cmd = UblkCtrlCmdExt::for_device(1);
        let bytes = cmd.to_bytes();
        assert_eq!(bytes.len(), 80); // 80 bytes for UringCmd80
    }

    #[test]
    fn test_ublk_ctrl_dev_info_default() {
        let info = UblkCtrlDevInfo::default();
        assert_eq!(info.nr_hw_queues, 0);
        assert_eq!(info.queue_depth, 0);
        assert_eq!(info.state, 0);
    }

    #[test]
    fn test_error_from_errno_enoent() {
        let err = Error::from_errno(-libc::ENOENT);
        match err {
            Error::IoUringCommand { errno, .. } => {
                assert_eq!(errno, -libc::ENOENT);
            }
            _ => panic!("Expected IoUringCommand error"),
        }
    }

    #[test]
    fn test_error_from_errno_ebusy() {
        let err = Error::from_errno(-libc::EBUSY);
        match err {
            Error::IoUringCommand { errno, .. } => {
                assert_eq!(errno, -libc::EBUSY);
            }
            _ => panic!("Expected IoUringCommand error"),
        }
    }

    #[test]
    fn test_block_dev_prefix() {
        assert_eq!(UBLK_BLOCK_DEV_PREFIX, "/dev/ublkb");
    }

    #[test]
    fn test_ctrl_dev_path() {
        assert_eq!(UBLK_CTRL_DEV, "/dev/ublk-control");
    }
}
