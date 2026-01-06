# duende

[![Crates.io](https://img.shields.io/crates/v/duende-core.svg)](https://crates.io/crates/duende-core)
[![Documentation](https://docs.rs/duende-core/badge.svg)](https://docs.rs/duende-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

<p align="center">
  <img src="assets/hero.svg" alt="Duende - Cross-Platform Daemon Orchestration" width="800">
</p>

Cross-platform daemon tooling framework for the **PAIML Sovereign AI Stack**.

## Overview

Duende (Spanish: spirit/daemon) provides unified lifecycle management, observability, and policy enforcement for long-running processes across multiple platforms.

## Features

- **Daemon Trait**: Standard lifecycle with `init()`, `run()`, `shutdown()`, `health_check()`
- **DaemonManager**: Orchestration with auto-restart and exponential backoff
- **Platform Adapters**: Linux (systemd), macOS (launchd), Container, pepita, WOS
- **Toyota Way Principles**: Jidoka, Heijunka, Poka-Yoke, Standardized Work
- **Memory Locking**: `mlock()` support for swap device daemons (critical for trueno-ublk)

## Platform Support

| Platform | Adapter | Status |
|----------|---------|--------|
| Linux (systemd) | `LinuxAdapter` | Stub (DP-002) |
| macOS (launchd) | `MacosAdapter` | Stub (DP-004) |
| Container (Docker/OCI) | `ContainerAdapter` | Stub (DP-005) |
| pepita MicroVM | `PepitaAdapter` | Stub (DP-006) |
| WOS (WebAssembly OS) | `WosAdapter` | Stub (DP-007) |
| Native Process | `NativeAdapter` | Implemented |

## Crate Structure

```
duende/
├── crates/
│   ├── duende-core/       # Daemon trait, config, manager, metrics
│   ├── duende-platform/   # Platform adapters (systemd, launchd, etc.)
│   ├── duende-observe/    # Observability (renacer, ttop integration)
│   ├── duende-policy/     # Policy enforcement (Jidoka gates, circuit breakers)
│   └── duende-test/       # Testing infrastructure (chaos, load testing)
```

## Quick Start

```rust
use duende_core::{Daemon, DaemonConfig, DaemonContext, DaemonId, DaemonMetrics, ExitReason, HealthStatus};
use async_trait::async_trait;
use std::time::Duration;

struct MyDaemon {
    id: DaemonId,
    metrics: DaemonMetrics,
}

#[async_trait]
impl Daemon for MyDaemon {
    fn id(&self) -> DaemonId { self.id }
    fn name(&self) -> &str { "my-daemon" }

    async fn init(&mut self, _config: &DaemonConfig) -> Result<(), DaemonError> {
        Ok(())
    }

    async fn run(&mut self, ctx: &mut DaemonContext) -> Result<ExitReason, DaemonError> {
        while !ctx.should_shutdown() {
            // Do work...
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(ExitReason::Graceful)
    }

    async fn shutdown(&mut self, _timeout: Duration) -> Result<(), DaemonError> {
        Ok(())
    }

    async fn health_check(&self) -> HealthStatus {
        HealthStatus::healthy(5)
    }

    fn metrics(&self) -> &DaemonMetrics {
        &self.metrics
    }
}
```

## Memory Locking (Swap Device Safety)

For daemons that serve as swap device backends (like trueno-ublk), memory locking is critical to prevent deadlocks:

```toml
[resources]
lock_memory = true           # Pin daemon memory via mlockall()
lock_memory_required = true  # Fail if mlock() fails (recommended for swap daemons)
```

See DP-001 in the roadmap for implementation details.

## Development

```bash
# Build
cargo build

# Test
make tier1   # On-save: fmt, clippy, check (<3s)
make tier2   # Pre-commit: tests, coverage (1-5min)
make tier3   # Pre-merge: mutants, falsification (1-6h)

# Individual commands
cargo test --workspace
make coverage
make mutants-fast
```

## Roadmap

See `docs/roadmaps/roadmap.yaml` for the full roadmap:

| ID | Title | Priority | Status |
|----|-------|----------|--------|
| DP-001 | mlock() Memory Locking | Critical | In Progress |
| DP-002 | Linux systemd Adapter | High | Planned |
| DP-003 | trueno-ublk Integration | High | Planned |
| DP-004 | macOS launchd Adapter | Medium | Planned |
| DP-005 | Container Adapter | Medium | Planned |
| DP-006 | pepita MicroVM Adapter | Low | Planned |
| DP-007 | WOS Adapter | Low | Planned |

## PAIML Stack Integration

Duende integrates with the PAIML Sovereign AI Stack:

| Component | Integration |
|-----------|-------------|
| trueno | SIMD/GPU primitives |
| trueno-viz (ttop) | Real-time monitoring |
| trueno-zram | Compressed state storage |
| renacer | Syscall tracing |
| repartir | Work-stealing scheduler |
| pacha | State registry |

## Specification

Full specification: `docs/specifications/daemon-tools-spec.md`

## License

MIT
