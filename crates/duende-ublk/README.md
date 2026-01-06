# duende-ublk

ublk device lifecycle management for swap-critical daemons.

[![Crates.io](https://img.shields.io/crates/v/duende-ublk.svg)](https://crates.io/crates/duende-ublk)
[![Documentation](https://docs.rs/duende-ublk/badge.svg)](https://docs.rs/duende-ublk)
[![License](https://img.shields.io/crates/l/duende-ublk.svg)](LICENSE)

## Problem: Orphaned ublk Devices

When a ublk daemon crashes or is killed, the kernel may retain device state
even after the `/dev/ublkbN` block device disappears. This causes:

- "File exists" errors when creating new devices
- Device ID conflicts
- System requires reboot to clear stale state

```
ERROR: Failed to add device 0: File exists (os error 17)
```

## Solution

```rust
use duende_ublk::{cleanup_orphaned_devices, UblkControl};

fn main() -> Result<(), duende_ublk::Error> {
    // Clean up any orphaned devices from previous crashes
    let cleaned = cleanup_orphaned_devices()?;
    println!("Cleaned {} orphaned devices", cleaned);

    // Now safe to create new devices
    Ok(())
}
```

## API

### Quick Start

```rust
use duende_ublk::{cleanup_orphaned_devices, detect_orphaned_devices, UblkControl};

// Detect orphaned devices
let orphans = detect_orphaned_devices()?;
println!("Found {} orphans: {:?}", orphans.len(), orphans);

// Clean up all orphaned devices
let cleaned = cleanup_orphaned_devices()?;
println!("Cleaned {} devices", cleaned);
```

### Manual Control

```rust
use duende_ublk::UblkControl;

let mut ctrl = UblkControl::open()?;

// Stop then delete a specific device
ctrl.stop_device(0)?;
ctrl.delete_device(0)?;

// Or use force_delete (stop + delete)
ctrl.force_delete(1)?;

// Get device info
let info = ctrl.get_device_info(0)?;
println!("Device {} has {} queues", info.dev_id, info.nr_hw_queues);
```

## Kernel Interface

This crate uses io_uring `URING_CMD` to communicate with the ublk kernel driver.
Requires Linux 6.0+.

### Requirements

- Linux 6.0+ kernel
- `ublk` kernel module loaded
- Root privileges or `CAP_SYS_ADMIN`

### io_uring Commands

| Command | Description |
|---------|-------------|
| `UBLK_U_CMD_DEL_DEV` | Delete a device |
| `UBLK_U_CMD_STOP_DEV` | Stop a running device |
| `UBLK_U_CMD_GET_DEV_INFO` | Get device information |

## Integration with duende-mlock

For swap-critical daemons, use with `duende-mlock`:

```rust
use duende_mlock::lock_all;
use duende_ublk::cleanup_orphaned_devices;

fn main() -> anyhow::Result<()> {
    // 1. Lock memory first (DT-007)
    lock_all()?;

    // 2. Clean up orphaned devices
    cleanup_orphaned_devices()?;

    // 3. Create and run daemon
    run_ublk_daemon()?;

    Ok(())
}
```

## License

MIT OR Apache-2.0
