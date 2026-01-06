# Linux (systemd) Adapter

The `SystemdAdapter` manages daemons as systemd transient units on Linux systems.

## Features

- Transient units via `systemd-run` (no unit files needed)
- User and system mode support
- Signal forwarding via `systemctl kill`
- Status queries via `systemctl is-active`
- Journal logging integration

## Usage

```rust
use duende_core::adapters::SystemdAdapter;
use duende_core::types::Signal;

// User mode (default) - no root required
let adapter = SystemdAdapter::new();

// System mode - requires root
let adapter = SystemdAdapter::system();

// Spawn daemon as transient unit
let handle = adapter.spawn(Box::new(my_daemon)).await?;
println!("Unit: {}", handle.systemd_unit().unwrap());

// Check status
let status = adapter.status(&handle).await?;

// Send signal
adapter.signal(&handle, Signal::Term).await?;
```

## How It Works

1. **Spawn**: Runs `systemd-run --user --unit=duende-<name>-<uuid> <binary>`
2. **Signal**: Runs `systemctl --user kill --signal=<sig> <unit>`
3. **Status**: Runs `systemctl --user is-active <unit>`
4. **Cleanup**: Unit is transient - removed when process exits

## Verification

```bash
# List duende units
systemctl --user list-units 'duende-*'

# Check specific unit
systemctl --user status duende-my-daemon-abc123

# View logs
journalctl --user -u duende-my-daemon-abc123
```

## mlock Requirements

For swap device daemons (DT-007), grant memory locking capability:

```bash
# Via setcap (preferred)
sudo setcap cap_ipc_lock+ep /usr/bin/my-daemon

# Or via systemd unit override
systemctl --user edit duende-my-daemon
# Add:
# [Service]
# AmbientCapabilities=CAP_IPC_LOCK
# LimitMEMLOCK=infinity
```

## Platform Detection

The adapter is automatically selected when:
- Running on Linux
- Not in a container
- systemd is the init system

```rust
use duende_core::platform::detect_platform;
use duende_core::adapters::select_adapter;

let platform = detect_platform();  // Returns Platform::Linux
let adapter = select_adapter(platform);  // Returns SystemdAdapter
```

## Requirements

- Linux with systemd 232+
- `systemd-run` and `systemctl` in PATH
- User session (user mode) or root (system mode)
