//! Raw ublk kernel interface definitions
//!
//! Direct port from Linux include/uapi/linux/ublk_cmd.h
//! Zero external dependencies - just libc types.
//!
//! IMPORTANT: Linux 6.0+ uses io_uring URING_CMD for control commands.
//! Commands use ioctl-encoded values (UBLK_U_CMD_*).

use std::mem::size_of;

// ============================================================================
// ioctl encoding helpers (match kernel _IO/_IOR/_IOW/_IOWR macros)
// ============================================================================

const UBLK_MAGIC: u32 = b'u' as u32;

const fn ior(ty: u32, nr: u32, sz: usize) -> u32 {
    (2 << 30) | ((sz as u32) << 16) | (ty << 8) | nr
}

const fn iowr(ty: u32, nr: u32, sz: usize) -> u32 {
    (3 << 30) | ((sz as u32) << 16) | (ty << 8) | nr
}

// ============================================================================
// Control Command Opcodes (ioctl-encoded for io_uring URING_CMD)
// ============================================================================

// Raw command numbers
const UBLK_CMD_GET_DEV_INFO: u32 = 0x02;
const UBLK_CMD_DEL_DEV: u32 = 0x05;
const UBLK_CMD_STOP_DEV: u32 = 0x07;

/// Get device info - _IOR('u', 0x02, struct ublksrv_ctrl_cmd)
pub const UBLK_U_CMD_GET_DEV_INFO: u32 =
    ior(UBLK_MAGIC, UBLK_CMD_GET_DEV_INFO, size_of::<UblkCtrlCmd>());

/// Delete device - _IOWR('u', 0x05, struct ublksrv_ctrl_cmd)
pub const UBLK_U_CMD_DEL_DEV: u32 = iowr(UBLK_MAGIC, UBLK_CMD_DEL_DEV, size_of::<UblkCtrlCmd>());

/// Stop device - _IOWR('u', 0x07, struct ublksrv_ctrl_cmd)
pub const UBLK_U_CMD_STOP_DEV: u32 = iowr(UBLK_MAGIC, UBLK_CMD_STOP_DEV, size_of::<UblkCtrlCmd>());

// ============================================================================
// Kernel Structures
// ============================================================================

/// Control command payload (32 bytes) - matches kernel ublksrv_ctrl_cmd
///
/// Used for UBLK_CMD_* operations via IORING_OP_URING_CMD
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UblkCtrlCmd {
    /// Device ID (u32::MAX for auto-assign)
    pub dev_id: u32,
    /// Queue ID (u16::MAX for device-level commands)
    pub queue_id: u16,
    /// Length of data buffer
    pub len: u16,
    /// Address of data buffer
    pub addr: u64,
    /// Command-specific data
    pub data: [u64; 1],
    /// Length of device path
    pub dev_path_len: u16,
    /// Padding
    pub pad: u16,
    /// Reserved
    pub reserved: u32,
}

impl Default for UblkCtrlCmd {
    fn default() -> Self {
        Self {
            dev_id: u32::MAX,
            queue_id: u16::MAX, // -1 means device-level command
            len: 0,
            addr: 0,
            data: [0; 1],
            dev_path_len: 0,
            pad: 0,
            reserved: 0,
        }
    }
}

/// Extended control command for io_uring 128-byte SQE (80 bytes total)
///
/// The io_uring SQE cmd field is 80 bytes. First 32 bytes is UblkCtrlCmd,
/// rest is padding.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UblkCtrlCmdExt {
    /// The actual command (32 bytes)
    pub cmd: UblkCtrlCmd,
    /// Padding to 80 bytes
    pub padding: [u8; 48],
}

impl Default for UblkCtrlCmdExt {
    fn default() -> Self {
        Self {
            cmd: UblkCtrlCmd::default(),
            padding: [0; 48],
        }
    }
}

impl UblkCtrlCmdExt {
    /// Create a new extended command for a specific device ID
    #[must_use]
    pub fn for_device(dev_id: u32) -> Self {
        Self {
            cmd: UblkCtrlCmd {
                dev_id,
                queue_id: u16::MAX,
                ..Default::default()
            },
            padding: [0; 48],
        }
    }

    /// Convert to raw bytes for io_uring SQE cmd field
    #[must_use]
    pub fn to_bytes(self) -> [u8; 80] {
        // SAFETY: UblkCtrlCmdExt is repr(C) and exactly 80 bytes
        // All bit patterns are valid for the struct
        unsafe { std::mem::transmute(self) }
    }
}

/// Device info structure (64 bytes) - matches kernel ublksrv_ctrl_dev_info
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct UblkCtrlDevInfo {
    /// Number of hardware queues
    pub nr_hw_queues: u16,
    /// Queue depth
    pub queue_depth: u16,
    /// Device state
    pub state: u16,
    /// Padding
    pub pad0: u16,
    /// Maximum I/O buffer size
    pub max_io_buf_bytes: u32,
    /// Device ID
    pub dev_id: u32,
    /// Server PID
    pub ublksrv_pid: i32,
    /// Padding
    pub pad1: u32,
    /// Device flags
    pub flags: u64,
    /// Server flags
    pub ublksrv_flags: u64,
    /// Owner UID
    pub owner_uid: u32,
    /// Owner GID
    pub owner_gid: u32,
    /// Reserved
    pub reserved1: u64,
    /// Reserved
    pub reserved2: u64,
}

// ============================================================================
// Constants
// ============================================================================

/// Path to ublk control device
pub const UBLK_CTRL_DEV: &str = "/dev/ublk-control";

/// Prefix for ublk character devices
#[allow(dead_code)]
pub const UBLK_CHAR_DEV_PREFIX: &str = "/dev/ublkc";

/// Prefix for ublk block devices
pub const UBLK_BLOCK_DEV_PREFIX: &str = "/dev/ublkb";

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    #[test]
    fn test_ctrl_cmd_layout() {
        // Verify field offsets match kernel layout
        assert_eq!(offset_of!(UblkCtrlCmd, dev_id), 0);
        assert_eq!(offset_of!(UblkCtrlCmd, queue_id), 4);
        assert_eq!(offset_of!(UblkCtrlCmd, len), 6);
        assert_eq!(offset_of!(UblkCtrlCmd, addr), 8);
        assert_eq!(offset_of!(UblkCtrlCmd, data), 16);
        assert_eq!(offset_of!(UblkCtrlCmd, dev_path_len), 24);
        assert_eq!(offset_of!(UblkCtrlCmd, pad), 26);
        assert_eq!(offset_of!(UblkCtrlCmd, reserved), 28);
    }

    #[test]
    fn test_ctrl_cmd_ext_for_device() {
        let ext = UblkCtrlCmdExt::for_device(42);
        assert_eq!(ext.cmd.dev_id, 42);
        assert_eq!(ext.cmd.queue_id, u16::MAX);
        assert_eq!(ext.padding, [0u8; 48]);
    }

    #[test]
    fn test_ctrl_cmd_ext_to_bytes() {
        let ext = UblkCtrlCmdExt::for_device(1);
        let bytes = ext.to_bytes();
        assert_eq!(bytes.len(), 80);
        // First 4 bytes should be dev_id (little-endian)
        assert_eq!(bytes[0], 1);
        assert_eq!(bytes[1], 0);
        assert_eq!(bytes[2], 0);
        assert_eq!(bytes[3], 0);
    }

    #[test]
    fn test_ctrl_dev_info_layout() {
        assert_eq!(offset_of!(UblkCtrlDevInfo, nr_hw_queues), 0);
        assert_eq!(offset_of!(UblkCtrlDevInfo, queue_depth), 2);
        assert_eq!(offset_of!(UblkCtrlDevInfo, state), 4);
        assert_eq!(offset_of!(UblkCtrlDevInfo, dev_id), 12);
        assert_eq!(offset_of!(UblkCtrlDevInfo, ublksrv_pid), 16);
    }

    #[test]
    fn test_default_values() {
        let cmd = UblkCtrlCmd::default();
        assert_eq!(cmd.dev_id, u32::MAX);
        assert_eq!(cmd.queue_id, u16::MAX);

        let info = UblkCtrlDevInfo::default();
        assert_eq!(info.dev_id, 0);
        assert_eq!(info.state, 0);
    }

    #[test]
    fn test_ctrl_cmd_all_fields_default() {
        let cmd = UblkCtrlCmd::default();
        assert_eq!(cmd.len, 0);
        assert_eq!(cmd.addr, 0);
        assert_eq!(cmd.data[0], 0);
        assert_eq!(cmd.dev_path_len, 0);
        assert_eq!(cmd.pad, 0);
        assert_eq!(cmd.reserved, 0);
    }

    #[test]
    fn test_ctrl_cmd_ext_default() {
        let ext = UblkCtrlCmdExt::default();
        assert_eq!(ext.cmd.dev_id, u32::MAX);
        assert_eq!(ext.padding, [0u8; 48]);
    }

    #[test]
    fn test_ctrl_cmd_clone() {
        let cmd = UblkCtrlCmd {
            dev_id: 123,
            queue_id: 456,
            len: 789,
            addr: 0xDEADBEEF,
            data: [0x12345678],
            dev_path_len: 10,
            pad: 0,
            reserved: 0,
        };
        let cloned = cmd;
        assert_eq!(cloned.dev_id, 123);
        assert_eq!(cloned.queue_id, 456);
        assert_eq!(cloned.len, 789);
        assert_eq!(cloned.addr, 0xDEADBEEF);
    }

    #[test]
    fn test_ctrl_cmd_ext_clone() {
        let ext = UblkCtrlCmdExt::for_device(99);
        let cloned = ext;
        assert_eq!(cloned.cmd.dev_id, 99);
    }

    #[test]
    fn test_ctrl_dev_info_all_fields() {
        let mut info = UblkCtrlDevInfo::default();
        info.nr_hw_queues = 4;
        info.queue_depth = 128;
        info.state = 1;
        info.max_io_buf_bytes = 1024 * 1024;
        info.dev_id = 42;
        info.ublksrv_pid = 12345;
        info.flags = 0xFF;
        info.ublksrv_flags = 0xAA;
        info.owner_uid = 1000;
        info.owner_gid = 1000;

        assert_eq!(info.nr_hw_queues, 4);
        assert_eq!(info.queue_depth, 128);
        assert_eq!(info.state, 1);
        assert_eq!(info.max_io_buf_bytes, 1024 * 1024);
        assert_eq!(info.dev_id, 42);
        assert_eq!(info.ublksrv_pid, 12345);
        assert_eq!(info.flags, 0xFF);
        assert_eq!(info.ublksrv_flags, 0xAA);
        assert_eq!(info.owner_uid, 1000);
        assert_eq!(info.owner_gid, 1000);
    }

    #[test]
    fn test_ctrl_dev_info_clone() {
        let info = UblkCtrlDevInfo {
            nr_hw_queues: 8,
            queue_depth: 256,
            state: 2,
            pad0: 0,
            max_io_buf_bytes: 2048,
            dev_id: 7,
            ublksrv_pid: 999,
            pad1: 0,
            flags: 0x1234,
            ublksrv_flags: 0x5678,
            owner_uid: 500,
            owner_gid: 500,
            reserved1: 0,
            reserved2: 0,
        };
        let cloned = info;
        assert_eq!(cloned.nr_hw_queues, 8);
        assert_eq!(cloned.dev_id, 7);
    }

    #[test]
    fn test_ioctl_values_correct() {
        // Verify ioctl encoding is correct
        // UBLK_U_CMD_GET_DEV_INFO = _IOR('u', 0x02, 32)
        let expected_get_info = (2u32 << 30) | (32u32 << 16) | (0x75u32 << 8) | 0x02;
        assert_eq!(UBLK_U_CMD_GET_DEV_INFO, expected_get_info);

        // UBLK_U_CMD_DEL_DEV = _IOWR('u', 0x05, 32)
        let expected_del = (3u32 << 30) | (32u32 << 16) | (0x75u32 << 8) | 0x05;
        assert_eq!(UBLK_U_CMD_DEL_DEV, expected_del);

        // UBLK_U_CMD_STOP_DEV = _IOWR('u', 0x07, 32)
        let expected_stop = (3u32 << 30) | (32u32 << 16) | (0x75u32 << 8) | 0x07;
        assert_eq!(UBLK_U_CMD_STOP_DEV, expected_stop);
    }

    #[test]
    fn test_constants() {
        assert_eq!(UBLK_CTRL_DEV, "/dev/ublk-control");
        assert_eq!(UBLK_CHAR_DEV_PREFIX, "/dev/ublkc");
        assert_eq!(UBLK_BLOCK_DEV_PREFIX, "/dev/ublkb");
    }

    #[test]
    fn test_ctrl_cmd_debug() {
        let cmd = UblkCtrlCmd::default();
        let debug_str = format!("{:?}", cmd);
        assert!(debug_str.contains("UblkCtrlCmd"));
        assert!(debug_str.contains("dev_id"));
    }

    #[test]
    fn test_ctrl_cmd_ext_debug() {
        let ext = UblkCtrlCmdExt::default();
        let debug_str = format!("{:?}", ext);
        assert!(debug_str.contains("UblkCtrlCmdExt"));
    }

    #[test]
    fn test_ctrl_dev_info_debug() {
        let info = UblkCtrlDevInfo::default();
        let debug_str = format!("{:?}", info);
        assert!(debug_str.contains("UblkCtrlDevInfo"));
    }

    #[test]
    fn test_ctrl_cmd_ext_to_bytes_roundtrip() {
        let ext = UblkCtrlCmdExt::for_device(0x12345678);
        let bytes = ext.to_bytes();

        // Verify the device ID in the byte representation
        let dev_id = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(dev_id, 0x12345678);

        // Verify queue_id is at bytes 4-5
        let queue_id = u16::from_le_bytes([bytes[4], bytes[5]]);
        assert_eq!(queue_id, u16::MAX);
    }

    #[test]
    fn test_ctrl_cmd_ext_zero_device() {
        let ext = UblkCtrlCmdExt::for_device(0);
        assert_eq!(ext.cmd.dev_id, 0);
        let bytes = ext.to_bytes();
        assert_eq!(bytes[0], 0);
        assert_eq!(bytes[1], 0);
        assert_eq!(bytes[2], 0);
        assert_eq!(bytes[3], 0);
    }

    #[test]
    fn test_ctrl_cmd_ext_max_device() {
        let ext = UblkCtrlCmdExt::for_device(u32::MAX);
        assert_eq!(ext.cmd.dev_id, u32::MAX);
        let bytes = ext.to_bytes();
        assert_eq!(bytes[0], 0xFF);
        assert_eq!(bytes[1], 0xFF);
        assert_eq!(bytes[2], 0xFF);
        assert_eq!(bytes[3], 0xFF);
    }
}
