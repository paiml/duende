# Linux (systemd)

The Linux adapter integrates with systemd for daemon management.

## Features

- systemd unit file generation
- cgroup resource limits
- Journal logging
- Restart policies via `Restart=`
- Socket activation (planned)

## Configuration

```toml
[platform]
# No Linux-specific config needed for systemd
```

## systemd Unit File

Generated unit file example:

```ini
[Unit]
Description=My Daemon
After=network.target

[Service]
Type=simple
ExecStart=/usr/bin/my-daemon
Restart=on-failure
MemoryLimit=512M
CPUQuota=100%

# For swap device daemons
AmbientCapabilities=CAP_IPC_LOCK
LimitMEMLOCK=infinity

[Install]
WantedBy=multi-user.target
```

## mlock Requirements

For swap device daemons, add to the unit file:

```ini
[Service]
AmbientCapabilities=CAP_IPC_LOCK
LimitMEMLOCK=infinity
```

Or grant capability to the binary:

```bash
sudo setcap cap_ipc_lock+ep /usr/bin/my-daemon
```
