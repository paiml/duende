# Memory Locking (mlock)

**DT-007: Swap Deadlock Prevention**

Memory locking is **CRITICAL** for daemons that serve as swap devices. Without it, your daemon can deadlock under memory pressure.

## The Problem

When a daemon serves as a swap device (e.g., `trueno-ublk`), a deadly cycle can occur:

```
┌─────────────────────────────────────────────────────────────┐
│                    DEADLOCK SCENARIO                        │
├─────────────────────────────────────────────────────────────┤
│  1. Kernel needs to swap pages OUT to daemon's device       │
│                           ↓                                 │
│  2. Daemon needs memory to process I/O request              │
│                           ↓                                 │
│  3. Kernel tries to swap OUT daemon's pages to free memory  │
│                           ↓                                 │
│  4. Swap request goes to... the same daemon                 │
│                           ↓                                 │
│  5. Daemon waiting for itself → DEADLOCK                    │
└─────────────────────────────────────────────────────────────┘
```

### Real-World Evidence

Kernel log from 2026-01-06 stress test:

```
INFO: task trueno-ublk:59497 blocked for more than 122 seconds.
task:trueno-ublk state:D (uninterruptible sleep)
__swap_writepage+0x111/0x1a0
swap_writepage+0x5f/0xe0
```

## The Solution

Use `mlockall(MCL_CURRENT | MCL_FUTURE)` to **pin all daemon memory**, preventing it from ever being swapped out.

## Configuration

### Via ResourceConfig

```rust
use duende_core::ResourceConfig;
use duende_platform::apply_memory_config;

let mut config = ResourceConfig::default();
config.lock_memory = true;           // Enable mlock
config.lock_memory_required = true;  // Fail if mlock fails

// Apply during daemon initialization
apply_memory_config(&config)?;
```

### Via TOML

```toml
[resources]
lock_memory = true
lock_memory_required = true
```

### Direct API

```rust
use duende_platform::{lock_daemon_memory, MlockResult};

match lock_daemon_memory(true) {  // true = required
    Ok(MlockResult::Success) => println!("Memory locked"),
    Ok(MlockResult::Failed(errno)) => println!("Failed: {}", errno),
    Err(e) => panic!("Fatal: {}", e),
}
```

## Running the Example

```bash
# Basic test
cargo run -p duende-platform --example mlock

# With mlock required (fails without privileges)
cargo run -p duende-platform --example mlock -- --required

# Check current status
cargo run -p duende-platform --example mlock -- --status
```

## Container Configuration

Containers require special configuration for mlock.

### Docker

```bash
# Minimum required
docker run --cap-add=IPC_LOCK your-image

# Recommended (unlimited memlock)
docker run --cap-add=IPC_LOCK --ulimit memlock=-1:-1 your-image
```

### Docker Compose

```yaml
version: '3.8'
services:
  trueno-ublk:
    image: your-image
    cap_add:
      - IPC_LOCK
    ulimits:
      memlock:
        soft: -1
        hard: -1
```

### Kubernetes

```yaml
apiVersion: v1
kind: Pod
spec:
  containers:
  - name: daemon
    securityContext:
      capabilities:
        add:
        - IPC_LOCK
```

## Capability Requirements

mlock requires one of:

| Method | Notes |
|--------|-------|
| `CAP_IPC_LOCK` | Preferred - grants mlock capability |
| Root privileges | Works but not recommended |
| Sufficient `RLIMIT_MEMLOCK` | Default is often 64KB-8MB |

### Checking Capabilities

```bash
# In container
cat /proc/self/status | grep Cap

# Check CAP_IPC_LOCK specifically (bit 14)
# CapEff: 00000000a80465fb  (bit 14 set = has IPC_LOCK)
# CapEff: 00000000a80425fb  (bit 14 not set)
```

## API Reference

### `lock_daemon_memory(required: bool)`

Locks all current and future memory allocations.

**Arguments:**
- `required`: If `true`, returns `Err` on failure. If `false`, returns `Ok(MlockResult::Failed)`.

**Returns:**
- `Ok(MlockResult::Success)` - Memory locked successfully
- `Ok(MlockResult::Failed(errno))` - Failed but continuing (when `required=false`)
- `Ok(MlockResult::Disabled)` - Platform doesn't support mlock
- `Err(PlatformError)` - Failed and `required=true`

### `is_memory_locked()`

Checks if memory is currently locked by reading `/proc/self/status`.

```rust
if is_memory_locked() {
    println!("Memory is locked");
}
```

### `apply_memory_config(config: &ResourceConfig)`

Convenience function that checks `config.lock_memory` and calls `lock_daemon_memory` if enabled.

```rust
let config = ResourceConfig {
    lock_memory: true,
    lock_memory_required: true,
    ..Default::default()
};

apply_memory_config(&config)?;
```

## Testing

### Docker Test Suite

```bash
cd duende
./docker/test-mlock.sh --build
```

This runs mlock tests across different privilege configurations:

| Test | Expected Result |
|------|-----------------|
| No capabilities | Fails with EPERM (or succeeds if within ulimit) |
| With CAP_IPC_LOCK | Succeeds |
| With unlimited memlock | Succeeds |
| Privileged container | Succeeds |

### Unit Tests

```bash
cargo test -p duende-platform memory
```

## Troubleshooting

### mlock fails with EPERM

**Cause:** Missing CAP_IPC_LOCK capability.

**Solution:**
```bash
# Docker
docker run --cap-add=IPC_LOCK ...

# Native Linux
sudo setcap cap_ipc_lock+ep ./your-daemon
```

### mlock fails with ENOMEM

**Cause:** Memlock ulimit exhausted.

**Solution:**
```bash
# Docker
docker run --ulimit memlock=-1:-1 ...

# Native Linux
ulimit -l unlimited
```

### Memory locked but daemon still deadlocks

**Possible causes:**
1. Container memory limit too restrictive
2. Swap enabled for container
3. cgroup memory controller limiting

**Solution:**
```bash
# Disable swap for container
docker run --memory=2g --memory-swap=2g ...
```

## Security Considerations

- `CAP_IPC_LOCK` allows locking arbitrary amounts of memory
- This can impact host performance if abused
- In production, set reasonable memlock limits
- Monitor daemon memory usage for leaks
