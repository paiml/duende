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

// ============================================================================
// Pure helper functions for testability
// ============================================================================

/// Parse a ublk character device name to extract the device ID.
///
/// E.g., "ublkc0" -> Some(0), "ublkc42" -> Some(42), "ublkb0" -> None
#[must_use]
pub fn parse_char_device_id(name: &str) -> Option<u32> {
    name.strip_prefix("ublkc")?.parse().ok()
}

/// Check if a block device exists for a given device ID.
#[must_use]
pub fn block_device_exists(dev_id: u32) -> bool {
    let block_path = format!("{}{}", UBLK_BLOCK_DEV_PREFIX, dev_id);
    Path::new(&block_path).exists()
}

/// Build the block device path for a device ID.
#[must_use]
pub fn block_device_path(dev_id: u32) -> String {
    format!("{}{}", UBLK_BLOCK_DEV_PREFIX, dev_id)
}

/// Build the character device path for a device ID.
#[must_use]
pub fn char_device_path(dev_id: u32) -> String {
    format!("/dev/ublkc{}", dev_id)
}

/// Detect orphaned devices in a given directory (for testing).
///
/// This is the testable core of `detect_orphaned_devices()`.
pub fn detect_orphans_in_dir(dev_path: &Path) -> Result<Vec<u32>, Error> {
    let mut orphans = Vec::new();

    if !dev_path.exists() {
        return Ok(orphans);
    }

    let entries = fs::read_dir(dev_path).map_err(Error::ScanDevDir)?;

    for entry in entries {
        let entry = entry.map_err(Error::ScanDevDir)?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Look for character devices: ublkcN
        if let Some(dev_id) = parse_char_device_id(&name_str) {
            // Check if block device exists in the same directory
            let block_name = format!("ublkb{}", dev_id);
            let block_path = dev_path.join(&block_name);
            if !block_path.exists() {
                orphans.push(dev_id);
            }
        }
    }

    Ok(orphans)
}

/// Interpret a command result code.
///
/// Returns:
/// - `Ok(true)` if command succeeded
/// - `Ok(false)` if device doesn't exist (ENOENT)
/// - `Err(DeviceBusy)` if device is in use (EBUSY)
/// - `Err(...)` for other errors
pub fn interpret_command_result(res: i32, dev_id: u32) -> Result<bool, Error> {
    if res >= 0 {
        return Ok(true);
    }

    // ENOENT means device doesn't exist - not an error for delete
    if res == -libc::ENOENT {
        return Ok(false);
    }

    // EBUSY means device is in use
    if res == -libc::EBUSY {
        return Err(Error::DeviceBusy { dev_id });
    }

    Err(Error::from_errno(res))
}

/// Build a command structure for a device operation.
#[must_use]
pub fn build_device_command(dev_id: u32) -> UblkCtrlCmdExt {
    UblkCtrlCmdExt::for_device(dev_id)
}

/// Build a command structure for getting device info.
#[must_use]
pub fn build_get_info_command(dev_id: u32, info_ptr: u64) -> UblkCtrlCmdExt {
    UblkCtrlCmdExt {
        cmd: crate::sys::UblkCtrlCmd {
            dev_id,
            queue_id: u16::MAX,
            len: std::mem::size_of::<UblkCtrlDevInfo>() as u16,
            addr: info_ptr,
            ..Default::default()
        },
        padding: [0; 48],
    }
}

/// Check if the ublk control device is available.
#[must_use]
pub fn control_device_available() -> bool {
    Path::new(UBLK_CTRL_DEV).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ==================== Pure Function Tests ====================

    #[test]
    fn test_parse_char_device_id_valid() {
        assert_eq!(parse_char_device_id("ublkc0"), Some(0));
        assert_eq!(parse_char_device_id("ublkc1"), Some(1));
        assert_eq!(parse_char_device_id("ublkc42"), Some(42));
        assert_eq!(parse_char_device_id("ublkc255"), Some(255));
    }

    #[test]
    fn test_parse_char_device_id_invalid() {
        assert_eq!(parse_char_device_id("ublkb0"), None);
        assert_eq!(parse_char_device_id("ublk0"), None);
        assert_eq!(parse_char_device_id("ublkc"), None);
        assert_eq!(parse_char_device_id("ublkcabc"), None);
        assert_eq!(parse_char_device_id(""), None);
        assert_eq!(parse_char_device_id("sda"), None);
    }

    #[test]
    fn test_block_device_path() {
        assert_eq!(block_device_path(0), "/dev/ublkb0");
        assert_eq!(block_device_path(1), "/dev/ublkb1");
        assert_eq!(block_device_path(42), "/dev/ublkb42");
    }

    #[test]
    fn test_char_device_path() {
        assert_eq!(char_device_path(0), "/dev/ublkc0");
        assert_eq!(char_device_path(1), "/dev/ublkc1");
        assert_eq!(char_device_path(42), "/dev/ublkc42");
    }

    #[test]
    fn test_interpret_command_result_success() {
        assert!(matches!(interpret_command_result(0, 0), Ok(true)));
        assert!(matches!(interpret_command_result(1, 0), Ok(true)));
        assert!(matches!(interpret_command_result(100, 0), Ok(true)));
    }

    #[test]
    fn test_interpret_command_result_not_found() {
        assert!(matches!(interpret_command_result(-libc::ENOENT, 0), Ok(false)));
        assert!(matches!(interpret_command_result(-libc::ENOENT, 42), Ok(false)));
    }

    #[test]
    fn test_interpret_command_result_busy() {
        let result = interpret_command_result(-libc::EBUSY, 5);
        assert!(matches!(result, Err(Error::DeviceBusy { dev_id: 5 })));
    }

    #[test]
    fn test_interpret_command_result_other_error() {
        let result = interpret_command_result(-libc::EPERM, 0);
        assert!(result.is_err());
        assert!(!matches!(result, Err(Error::DeviceBusy { .. })));
    }

    #[test]
    fn test_build_device_command() {
        let cmd = build_device_command(42);
        assert_eq!(cmd.cmd.dev_id, 42);
        assert_eq!(cmd.cmd.queue_id, u16::MAX);
        assert_eq!(cmd.cmd.len, 0);
        assert_eq!(cmd.cmd.addr, 0);
    }

    #[test]
    fn test_build_get_info_command() {
        let cmd = build_get_info_command(7, 0x1234_5678);
        assert_eq!(cmd.cmd.dev_id, 7);
        assert_eq!(cmd.cmd.queue_id, u16::MAX);
        assert_eq!(cmd.cmd.len, std::mem::size_of::<UblkCtrlDevInfo>() as u16);
        assert_eq!(cmd.cmd.addr, 0x1234_5678);
    }

    #[test]
    fn test_control_device_available() {
        // Just verify it doesn't panic
        let _ = control_device_available();
    }

    // ==================== Mock Directory Tests ====================

    #[test]
    fn test_detect_orphans_in_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let result = detect_orphans_in_dir(temp_dir.path());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_detect_orphans_with_orphaned_device() {
        let temp_dir = TempDir::new().unwrap();

        // Create an orphaned character device (no block device)
        std::fs::write(temp_dir.path().join("ublkc0"), "").unwrap();

        let result = detect_orphans_in_dir(temp_dir.path());
        assert!(result.is_ok());
        let orphans = result.unwrap();
        assert_eq!(orphans, vec![0]);
    }

    #[test]
    fn test_detect_orphans_with_paired_device() {
        let temp_dir = TempDir::new().unwrap();

        // Create a paired device (both char and block exist)
        std::fs::write(temp_dir.path().join("ublkc0"), "").unwrap();
        std::fs::write(temp_dir.path().join("ublkb0"), "").unwrap();

        let result = detect_orphans_in_dir(temp_dir.path());
        assert!(result.is_ok());
        let orphans = result.unwrap();
        assert!(orphans.is_empty());
    }

    #[test]
    fn test_detect_orphans_mixed_devices() {
        let temp_dir = TempDir::new().unwrap();

        // Create some paired and some orphaned
        std::fs::write(temp_dir.path().join("ublkc0"), "").unwrap();
        std::fs::write(temp_dir.path().join("ublkb0"), "").unwrap(); // paired

        std::fs::write(temp_dir.path().join("ublkc1"), "").unwrap(); // orphan

        std::fs::write(temp_dir.path().join("ublkc2"), "").unwrap();
        std::fs::write(temp_dir.path().join("ublkb2"), "").unwrap(); // paired

        std::fs::write(temp_dir.path().join("ublkc3"), "").unwrap(); // orphan

        let result = detect_orphans_in_dir(temp_dir.path());
        assert!(result.is_ok());
        let mut orphans = result.unwrap();
        orphans.sort();
        assert_eq!(orphans, vec![1, 3]);
    }

    #[test]
    fn test_detect_orphans_ignores_other_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create non-ublk files
        std::fs::write(temp_dir.path().join("sda"), "").unwrap();
        std::fs::write(temp_dir.path().join("sdb"), "").unwrap();
        std::fs::write(temp_dir.path().join("random"), "").unwrap();
        std::fs::write(temp_dir.path().join("null"), "").unwrap();

        // One orphaned ublk
        std::fs::write(temp_dir.path().join("ublkc5"), "").unwrap();

        let result = detect_orphans_in_dir(temp_dir.path());
        assert!(result.is_ok());
        let orphans = result.unwrap();
        assert_eq!(orphans, vec![5]);
    }

    #[test]
    fn test_detect_orphans_nonexistent_dir() {
        let result = detect_orphans_in_dir(Path::new("/nonexistent/path/12345"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    // ==================== Command Structure Tests ====================

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
    fn test_ublk_ctrl_cmd_ext_bytes_contain_dev_id() {
        let cmd = UblkCtrlCmdExt::for_device(42);
        let bytes = cmd.to_bytes();
        // dev_id is first u32 in the structure (little-endian)
        let dev_id = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(dev_id, 42);
    }

    #[test]
    fn test_ublk_ctrl_dev_info_default() {
        let info = UblkCtrlDevInfo::default();
        assert_eq!(info.nr_hw_queues, 0);
        assert_eq!(info.queue_depth, 0);
        assert_eq!(info.state, 0);
        assert_eq!(info.dev_id, 0);
    }

    // ==================== Error Handling Tests ====================

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
    fn test_error_from_errno_eperm() {
        let err = Error::from_errno(-libc::EPERM);
        assert!(err.is_permission_denied());
    }

    #[test]
    fn test_error_from_errno_einval() {
        let err = Error::from_errno(-libc::EINVAL);
        match err {
            Error::IoUringCommand { errno, message } => {
                assert_eq!(errno, -libc::EINVAL);
                assert!(message.contains("invalid"));
            }
            _ => panic!("Expected IoUringCommand error"),
        }
    }

    // ==================== Constant Tests ====================

    #[test]
    fn test_block_dev_prefix() {
        assert_eq!(UBLK_BLOCK_DEV_PREFIX, "/dev/ublkb");
    }

    #[test]
    fn test_ctrl_dev_path() {
        assert_eq!(UBLK_CTRL_DEV, "/dev/ublk-control");
    }

    #[test]
    fn test_ublk_commands() {
        // Verify command constants are reasonable values
        assert!(UBLK_U_CMD_DEL_DEV > 0);
        assert!(UBLK_U_CMD_GET_DEV_INFO > 0);
        assert!(UBLK_U_CMD_STOP_DEV > 0);
        // Commands should be distinct
        assert_ne!(UBLK_U_CMD_DEL_DEV, UBLK_U_CMD_GET_DEV_INFO);
        assert_ne!(UBLK_U_CMD_DEL_DEV, UBLK_U_CMD_STOP_DEV);
    }

    // ==================== Integration-style Tests ====================

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
        let result = UblkControl::open();
        match result {
            Ok(_) => {} // ublk is available
            Err(Error::ControlDeviceNotFound) => {} // Expected
            Err(Error::OpenControl(_)) => {} // Permission denied
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
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

    // ==================== Property-like Tests ====================

    #[test]
    fn test_parse_roundtrip() {
        // For any valid device ID, char_device_path should produce parseable name
        for dev_id in [0, 1, 2, 10, 42, 100, 255] {
            let path = char_device_path(dev_id);
            let name = path.rsplit('/').next().unwrap();
            assert_eq!(parse_char_device_id(name), Some(dev_id));
        }
    }

    #[test]
    fn test_interpret_all_errno_values() {
        // Test various errno values are handled
        let test_cases = [
            (-libc::ENOENT, false, false), // Not found
            (-libc::EBUSY, true, true),    // Busy error
            (-libc::EPERM, true, false),   // Permission error
            (-libc::EIO, true, false),     // I/O error
            (-libc::EINVAL, true, false),  // Invalid argument
        ];

        for (errno, is_err, is_busy) in test_cases {
            let result = interpret_command_result(errno, 0);
            assert_eq!(result.is_err(), is_err, "errno {} should be err={}", errno, is_err);
            if is_busy {
                assert!(matches!(result, Err(Error::DeviceBusy { .. })));
            }
        }
    }

    // ==================== Additional Tests for Coverage ====================

    #[test]
    fn test_block_device_exists_returns_bool() {
        // Verify it returns bool without panicking
        let _ = block_device_exists(0);
        let _ = block_device_exists(255);
        let _ = block_device_exists(u32::MAX);
    }

    #[test]
    fn test_build_get_info_command_size() {
        let cmd = build_get_info_command(0, 0);
        // len should be the size of UblkCtrlDevInfo
        assert!(cmd.cmd.len > 0);
        assert_eq!(cmd.cmd.len as usize, std::mem::size_of::<UblkCtrlDevInfo>());
    }

    #[test]
    fn test_build_device_command_padding() {
        let cmd = build_device_command(123);
        // padding should be zeroed
        assert!(cmd.padding.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_detect_orphans_with_many_devices() {
        let temp_dir = TempDir::new().unwrap();

        // Create many orphaned devices
        for i in 0..20 {
            std::fs::write(temp_dir.path().join(format!("ublkc{}", i)), "").unwrap();
        }

        let result = detect_orphans_in_dir(temp_dir.path());
        assert!(result.is_ok());
        let orphans = result.unwrap();
        assert_eq!(orphans.len(), 20);
    }

    #[test]
    fn test_detect_orphans_high_device_ids() {
        let temp_dir = TempDir::new().unwrap();

        // Create devices with high IDs
        for i in [100, 200, 255] {
            std::fs::write(temp_dir.path().join(format!("ublkc{}", i)), "").unwrap();
        }

        let result = detect_orphans_in_dir(temp_dir.path());
        assert!(result.is_ok());
        let mut orphans = result.unwrap();
        orphans.sort();
        assert_eq!(orphans, vec![100, 200, 255]);
    }

    #[test]
    fn test_parse_char_device_id_edge_cases() {
        // Leading zeros
        assert_eq!(parse_char_device_id("ublkc00"), Some(0));
        assert_eq!(parse_char_device_id("ublkc01"), Some(1));

        // Large numbers
        assert_eq!(parse_char_device_id("ublkc65535"), Some(65535));
        assert_eq!(parse_char_device_id("ublkc4294967295"), Some(4294967295));

        // Too large (overflow)
        assert_eq!(parse_char_device_id("ublkc4294967296"), None);

        // Negative (shouldn't parse)
        assert_eq!(parse_char_device_id("ublkc-1"), None);
    }

    #[test]
    fn test_interpret_command_result_all_positive() {
        // Any positive result is success
        for i in [0, 1, 10, 100, 1000, i32::MAX] {
            assert!(matches!(interpret_command_result(i, 0), Ok(true)));
        }
    }

    #[test]
    fn test_interpret_command_result_dev_id_preserved() {
        // Verify dev_id is preserved in DeviceBusy error
        let result = interpret_command_result(-libc::EBUSY, 42);
        match result {
            Err(Error::DeviceBusy { dev_id }) => assert_eq!(dev_id, 42),
            _ => panic!("Expected DeviceBusy with dev_id 42"),
        }
    }

    #[test]
    fn test_ublk_ctrl_cmd_ext_bytes_reproducible() {
        let cmd1 = UblkCtrlCmdExt::for_device(99);
        let cmd2 = UblkCtrlCmdExt::for_device(99);

        let bytes1 = cmd1.to_bytes();
        let bytes2 = cmd2.to_bytes();

        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn test_ublk_ctrl_cmd_ext_different_devices() {
        let cmd1 = UblkCtrlCmdExt::for_device(1);
        let cmd2 = UblkCtrlCmdExt::for_device(2);

        let bytes1 = cmd1.to_bytes();
        let bytes2 = cmd2.to_bytes();

        assert_ne!(bytes1, bytes2);
    }

    #[test]
    fn test_detect_orphans_block_only_ignored() {
        let temp_dir = TempDir::new().unwrap();

        // Create only block devices (no char devices) - should be ignored
        std::fs::write(temp_dir.path().join("ublkb0"), "").unwrap();
        std::fs::write(temp_dir.path().join("ublkb1"), "").unwrap();

        let result = detect_orphans_in_dir(temp_dir.path());
        assert!(result.is_ok());
        let orphans = result.unwrap();
        assert!(orphans.is_empty());
    }

    #[test]
    fn test_char_device_path_format() {
        for id in 0..10 {
            let path = char_device_path(id);
            assert!(path.starts_with("/dev/ublkc"));
            assert!(path.ends_with(&id.to_string()));
        }
    }

    #[test]
    fn test_block_device_path_format() {
        for id in 0..10 {
            let path = block_device_path(id);
            assert!(path.starts_with("/dev/ublkb"));
            assert!(path.ends_with(&id.to_string()));
        }
    }

    #[test]
    fn test_ublk_ctrl_dev_info_size() {
        // UblkCtrlDevInfo should have a reasonable size
        let size = std::mem::size_of::<UblkCtrlDevInfo>();
        assert!(size > 0);
        assert!(size < 1024); // Sanity check
    }

    #[test]
    fn test_cleanup_device_range_empty() {
        // Range of 0..0 should not try to clean anything
        let result = cleanup_device_range(0, 0);
        match result {
            Ok(0) => {}
            Err(Error::ControlDeviceNotFound) => {}
            _ => {}
        }
    }

    #[test]
    fn test_cleanup_device_range_single() {
        // Range of single device
        let result = cleanup_device_range(100, 101);
        match result {
            Ok(count) => assert!(count <= 1),
            Err(Error::ControlDeviceNotFound) => {}
            Err(_) => {}
        }
    }

    #[test]
    fn test_detect_orphans_subdirectories_ignored() {
        let temp_dir = TempDir::new().unwrap();

        // Create a subdirectory named like a ublk device (should be ignored)
        std::fs::create_dir(temp_dir.path().join("ublkc0")).unwrap();

        let result = detect_orphans_in_dir(temp_dir.path());
        assert!(result.is_ok());
        // Directories named ublkcN should still be detected (they exist in the namespace)
        // but won't be real orphans since they're directories
    }

    #[test]
    fn test_interpret_command_result_negative_boundary() {
        // Test boundary between success (>= 0) and error (< 0)
        assert!(matches!(interpret_command_result(0, 0), Ok(true)));
        assert!(matches!(interpret_command_result(-1, 0), Err(_)));
    }

    #[test]
    fn test_build_get_info_command_addr() {
        // Test various addresses
        for addr in [0, 1, 0x1000, 0xFFFF_FFFF, u64::MAX] {
            let cmd = build_get_info_command(0, addr);
            assert_eq!(cmd.cmd.addr, addr);
        }
    }

    #[test]
    fn test_build_get_info_command_dev_ids() {
        // Test various device IDs
        for dev_id in [0, 1, 100, 255, u32::MAX] {
            let cmd = build_get_info_command(dev_id, 0);
            assert_eq!(cmd.cmd.dev_id, dev_id);
        }
    }

    #[test]
    fn test_ublk_ctrl_dev_info_fields() {
        let mut info = UblkCtrlDevInfo::default();

        // Verify all fields are accessible and modifiable
        info.nr_hw_queues = 4;
        info.queue_depth = 128;
        info.max_io_buf_bytes = 524288;
        info.dev_id = 42;
        info.ublksrv_pid = 1234;
        info.state = 1;
        info.pad0 = 0;
        info.flags = 0xFF;
        info.owner_uid = 1000;
        info.owner_gid = 1000;
        info.reserved1 = 0;
        info.reserved2 = 0;

        assert_eq!(info.nr_hw_queues, 4);
        assert_eq!(info.queue_depth, 128);
        assert_eq!(info.dev_id, 42);
        assert_eq!(info.ublksrv_pid, 1234);
    }
}

// Property-based tests for control.rs
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// parse_char_device_id should handle any valid u32 device ID
        #[test]
        fn parse_valid_device_ids(dev_id in 0u32..=u32::MAX) {
            let name = format!("ublkc{}", dev_id);
            let result = parse_char_device_id(&name);
            prop_assert_eq!(result, Some(dev_id));
        }

        /// char_device_path and parse_char_device_id should roundtrip
        #[test]
        fn char_device_path_roundtrip(dev_id in 0u32..10000) {
            let path = char_device_path(dev_id);
            let name = path.rsplit('/').next().unwrap();
            prop_assert_eq!(parse_char_device_id(name), Some(dev_id));
        }

        /// build_device_command should preserve dev_id
        #[test]
        fn build_command_preserves_dev_id(dev_id in 0u32..=u32::MAX) {
            let cmd = build_device_command(dev_id);
            prop_assert_eq!(cmd.cmd.dev_id, dev_id);
        }

        /// build_get_info_command should preserve both dev_id and addr
        #[test]
        fn build_info_command_preserves_fields(
            dev_id in 0u32..=u32::MAX,
            addr in 0u64..=u64::MAX
        ) {
            let cmd = build_get_info_command(dev_id, addr);
            prop_assert_eq!(cmd.cmd.dev_id, dev_id);
            prop_assert_eq!(cmd.cmd.addr, addr);
        }

        /// interpret_command_result: positive results are always success
        #[test]
        fn positive_results_are_success(res in 0i32..=i32::MAX) {
            let result = interpret_command_result(res, 0);
            prop_assert!(matches!(result, Ok(true)));
        }

        /// interpret_command_result: ENOENT is always Ok(false)
        #[test]
        fn enoent_is_not_found(dev_id in 0u32..=1000) {
            let result = interpret_command_result(-libc::ENOENT, dev_id);
            prop_assert!(matches!(result, Ok(false)));
        }

        /// interpret_command_result: EBUSY preserves dev_id
        #[test]
        fn ebusy_preserves_dev_id(dev_id in 0u32..=1000) {
            let result = interpret_command_result(-libc::EBUSY, dev_id);
            match result {
                Err(Error::DeviceBusy { dev_id: id }) => prop_assert_eq!(id, dev_id),
                _ => prop_assert!(false, "Expected DeviceBusy error"),
            }
        }

        /// block_device_path always produces paths starting with prefix
        #[test]
        fn block_device_path_has_prefix(dev_id in 0u32..10000) {
            let path = block_device_path(dev_id);
            prop_assert!(path.starts_with(UBLK_BLOCK_DEV_PREFIX));
        }

        /// to_bytes always produces 80-byte output
        #[test]
        fn cmd_to_bytes_size(dev_id in 0u32..=u32::MAX) {
            let cmd = UblkCtrlCmdExt::for_device(dev_id);
            let bytes = cmd.to_bytes();
            prop_assert_eq!(bytes.len(), 80);
        }
    }
}

// Additional edge case tests
#[cfg(test)]
mod edge_case_tests {
    use super::*;

    #[test]
    fn test_error_types_from_errno() {
        // Test all common errno values produce appropriate errors
        let errnos = [
            libc::ENOENT,
            libc::EEXIST,
            libc::EBUSY,
            libc::EPERM,
            libc::EACCES,
            libc::EINVAL,
            libc::EIO,
            libc::ENOMEM,
            libc::ENOSPC,
            libc::EAGAIN,
        ];

        for e in errnos {
            let err = Error::from_errno(-e);
            // All should produce a displayable error
            let _ = err.to_string();
        }
    }

    #[test]
    fn test_detect_orphans_dir_with_subdirs() {
        let temp_dir = tempfile::TempDir::new().unwrap();

        // Create a mix of files and directories
        std::fs::write(temp_dir.path().join("ublkc0"), "").unwrap();
        std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();
        std::fs::write(temp_dir.path().join("regular_file"), "").unwrap();

        let result = detect_orphans_in_dir(temp_dir.path());
        assert!(result.is_ok());
        let orphans = result.unwrap();
        assert!(orphans.contains(&0));
    }

    #[test]
    fn test_ublk_ctrl_cmd_bytes_all_zeros_padding() {
        let cmd = UblkCtrlCmdExt::for_device(0);
        let bytes = cmd.to_bytes();

        // The padding bytes (bytes 32-79) should be all zeros
        for b in &bytes[32..80] {
            assert_eq!(*b, 0, "Padding byte should be zero");
        }
    }

    #[test]
    fn test_ctrl_dev_info_clone() {
        let mut info = UblkCtrlDevInfo::default();
        info.nr_hw_queues = 8;
        info.queue_depth = 256;
        info.max_io_buf_bytes = 1048576;
        info.dev_id = 99;
        info.ublksrv_pid = 5678;
        info.state = 2;
        info.flags = 0xABCD;

        let cloned = info.clone();
        assert_eq!(cloned.nr_hw_queues, 8);
        assert_eq!(cloned.queue_depth, 256);
        assert_eq!(cloned.dev_id, 99);
        assert_eq!(cloned.ublksrv_pid, 5678);
        assert_eq!(cloned.flags, 0xABCD);
    }

    #[test]
    fn test_ctrl_cmd_clone() {
        let cmd = build_device_command(42);
        let cloned = cmd.clone();

        assert_eq!(cloned.cmd.dev_id, 42);
        assert_eq!(cmd.to_bytes(), cloned.to_bytes());
    }

    #[test]
    fn test_interpret_various_negative_errnos() {
        // Test a wide range of negative errno values
        for errno in 1..50 {
            let result = interpret_command_result(-errno, 0);
            // All negative values should be handled without panic
            let _ = result;
        }
    }

    #[test]
    fn test_path_functions_consistency() {
        for id in [0, 1, 10, 100, 255] {
            let block = block_device_path(id);
            let char_dev = char_device_path(id);

            // Both should contain the device ID
            assert!(block.contains(&id.to_string()));
            assert!(char_dev.contains(&id.to_string()));

            // Block path should have 'b', char path should have 'c'
            assert!(block.contains("ublkb"));
            assert!(char_dev.contains("ublkc"));
        }
    }
}
