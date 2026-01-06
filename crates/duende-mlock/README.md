# duende-mlock

Memory locking for swap-critical daemons.

[![Crates.io](https://img.shields.io/crates/v/duende-mlock.svg)](https://crates.io/crates/duende-mlock)
[![Documentation](https://docs.rs/duende-mlock/badge.svg)](https://docs.rs/duende-mlock)
[![License](https://img.shields.io/crates/l/duende-mlock.svg)](LICENSE)

## Problem: Swap Deadlock (DT-007)

When a daemon serves as a swap device (e.g., `trueno-ublk`), a deadlock occurs if:

1. Kernel needs memory → initiates swap-out to the daemon
2. Daemon needs memory to process I/O
3. Kernel tries to swap daemon's pages → to the same daemon
4. **Deadlock**: daemon blocked waiting for itself

```
INFO: task trueno-ublk:59497 blocked for more than 122 seconds.
task:trueno-ublk state:D (uninterruptible sleep)
__swap_writepage+0x111/0x1a0
```

## Solution

```rust
use duende_mlock::lock_all;

fn main() -> Result<(), duende_mlock::MlockError> {
    // Lock all current and future memory allocations
    let status = lock_all()?;
    println!("Memory locked: {status}");

    // Daemon is now safe from swap deadlock
    run_daemon();

    Ok(())
}
```

## API

### Quick Start

```rust
use duende_mlock::{lock_all, is_locked, locked_bytes};

// Lock all memory
let status = lock_all()?;
assert!(status.is_locked());

// Verify
assert!(is_locked());
println!("Locked {} KB", locked_bytes() / 1024);
```

### Configuration

```rust
use duende_mlock::{MlockConfig, lock_with_config};

let config = MlockConfig::builder()
    .current(true)      // Lock existing pages
    .future(true)       // Lock future allocations
    .required(false)    // Don't fail if mlock fails
    .onfault(false)     // Lock immediately (not on fault)
    .build();

match lock_with_config(&config) {
    Ok(status) if status.is_locked() => {
        println!("Locked {} bytes", status.bytes_locked());
    }
    Ok(status) if status.is_failed() => {
        eprintln!("Warning: mlock failed, continuing without memory lock");
    }
    Ok(_) => {
        println!("Platform does not support mlock");
    }
    Err(e) => {
        eprintln!("Fatal: {e}");
        std::process::exit(1);
    }
}
```

### Error Handling

```rust
use duende_mlock::{lock_all, MlockError};

match lock_all() {
    Ok(status) => println!("Success: {status}"),
    Err(MlockError::PermissionDenied) => {
        eprintln!("Need CAP_IPC_LOCK capability");
        eprintln!("  sudo setcap cap_ipc_lock=+ep ./daemon");
        eprintln!("  docker run --cap-add=IPC_LOCK ...");
    }
    Err(MlockError::ResourceLimit) => {
        eprintln!("RLIMIT_MEMLOCK too low");
        eprintln!("  ulimit -l unlimited");
        eprintln!("  docker run --ulimit memlock=-1:-1 ...");
    }
    Err(e) => eprintln!("Unexpected error: {e}"),
}
```

## Platform Support

| Platform | Support | Notes |
|----------|---------|-------|
| Linux    | Full    | Requires `CAP_IPC_LOCK` or root |
| macOS    | Limited | Requires entitlements |
| Windows  | None    | Returns `Unsupported` |
| WASM     | None    | Returns `Unsupported` |

## Container Requirements

### Docker

```bash
docker run --cap-add=IPC_LOCK --ulimit memlock=-1:-1 your-image
```

### docker-compose.yml

```yaml
services:
  daemon:
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
        add: ["IPC_LOCK"]
```

### Systemd

```ini
[Service]
CapabilityBoundingSet=CAP_IPC_LOCK
AmbientCapabilities=CAP_IPC_LOCK
LimitMEMLOCK=infinity
```

## Minimal Dependencies

This crate has minimal dependencies for maximum compatibility:

- `libc` - For `mlockall(2)` and `munlockall(2)` syscalls
- No async runtime (no tokio)
- No heavy frameworks

## License

MIT OR Apache-2.0
