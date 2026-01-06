# duende-platform

Platform adapters for the Duende daemon framework.

[![Crates.io](https://img.shields.io/crates/v/duende-platform.svg)](https://crates.io/crates/duende-platform)
[![Documentation](https://docs.rs/duende-platform/badge.svg)](https://docs.rs/duende-platform)
[![License](https://img.shields.io/crates/l/duende-platform.svg)](LICENSE)

## Overview

Platform-specific implementations for spawning, signaling, and monitoring daemons across:

- **Linux** (systemd): Service units with cgroup resource control
- **macOS** (launchd): Property lists with keep-alive support
- **Container** (Docker/OCI): Container runtime integration
- **pepita** (MicroVM): Virtio-vsock communication
- **WOS** (WebAssembly OS): Process scheduling with priority levels
- **Native** (fallback): Direct process spawning

## Usage

```rust
use duende_platform::{detect_platform, NativeAdapter, Platform, PlatformAdapter};

// Auto-detect platform
let platform = detect_platform();

// Create appropriate adapter
let adapter = NativeAdapter::new();

// Spawn daemon
let handle = adapter.spawn(my_daemon).await?;

// Signal daemon
adapter.signal(&handle, Signal::Term).await?;
```

## Memory Locking (DT-007)

Prevents swap deadlock for daemons serving as swap devices:

```rust
use duende_platform::{lock_daemon_memory, is_memory_locked};

// Lock all current and future memory allocations
let result = lock_daemon_memory()?;
assert!(is_memory_locked());
```

## Feature Flags

| Feature | Description |
|---------|-------------|
| `native` (default) | Native process adapter |
| `linux` | systemd integration |
| `macos` | launchd integration |
| `container` | Docker/OCI support |
| `pepita` | MicroVM support |
| `wos` | WebAssembly OS support |

## License

MIT OR Apache-2.0
