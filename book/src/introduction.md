# Duende: Cross-Platform Daemon Framework

![Duende - Cross-Platform Daemon Orchestration](images/hero.svg)

**Duende** is a cross-platform daemon tooling framework for the PAIML Sovereign AI Stack. It provides a unified abstraction for daemon lifecycle management across:

- **Linux** (systemd) - Transient units via `systemd-run`
- **macOS** (launchd) - Plist files via `launchctl`
- **Containers** (Docker/Podman/containerd) - OCI runtime management
- **MicroVMs** (pepita) - Lightweight VMs with vsock communication
- **WebAssembly OS** (WOS) - 8-level priority scheduler

## Project Status

| Metric | Value |
|--------|-------|
| Tests | 683 passing |
| Platforms | 6/6 implemented |
| Falsification Tests | F001-F110 (110 tests) |

## Why Duende?

Managing daemons across different platforms is complex. Each platform has its own:

- Service management (systemd units, launchd plists, container specs)
- Signal handling conventions
- Resource limits and cgroups
- Health check mechanisms
- Logging and observability

Duende provides a **single Rust trait** that works everywhere:

```rust
use duende_core::{
    Daemon, DaemonConfig, DaemonContext, DaemonId,
    DaemonMetrics, ExitReason, HealthStatus, DaemonError
};
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

    async fn init(&mut self, config: &DaemonConfig) -> Result<(), DaemonError> {
        // Setup resources, validate config
        Ok(())
    }

    async fn run(&mut self, ctx: &mut DaemonContext) -> Result<ExitReason, DaemonError> {
        while !ctx.should_shutdown() {
            // Do work...
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        Ok(ExitReason::Graceful)
    }

    async fn shutdown(&mut self, timeout: Duration) -> Result<(), DaemonError> {
        // Cleanup
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

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Application                              │
├─────────────────────────────────────────────────────────────────┤
│                        duende-core                               │
│  ┌─────────────┐  ┌──────────────┐  ┌────────────────────────┐  │
│  │   Daemon    │  │ DaemonManager│  │    PlatformAdapter     │  │
│  │   Trait     │  │              │  │                        │  │
│  └─────────────┘  └──────────────┘  └────────────────────────┘  │
├─────────────────────────────────────────────────────────────────┤
│  Native │ Systemd │ Launchd │ Container │ Pepita │    WOS      │
│ (tokio) │ (Linux) │ (macOS) │(Docker/OCI)│(MicroVM)│ (WASM)    │
└─────────────────────────────────────────────────────────────────┘
```

## Design Principles

Duende follows the **Iron Lotus Framework** (Toyota Production System for Software):

| Principle | Application |
|-----------|-------------|
| **Jidoka** | Stop-on-error, no panics in production code |
| **Poka-Yoke** | Type-safe APIs prevent misuse |
| **Heijunka** | Load leveling via circuit breakers |
| **Muda** | Zero-waste resource allocation |
| **Kaizen** | Continuous metrics (RED method) |
| **Genchi Genbutsu** | Direct observation via syscall tracing |

## Crate Overview

| Crate | Tests | Purpose |
|-------|-------|---------|
| `duende-core` | 352 | Daemon trait, manager, platform adapters |
| `duende-mlock` | 44 | `mlockall()` for swap safety (DT-007) |
| `duende-observe` | 55 | `/proc` monitoring, syscall tracing |
| `duende-platform` | 29 | Platform detection, memory helpers |
| `duende-policy` | 45 | Circuit breaker, jidoka, cgroups |
| `duende-test` | 45 | Test harness, chaos injection |

## Quick Start

```bash
# Add to your project
cargo add duende-core

# Run the example daemon
cargo run --example daemon

# Run the mlock example
cargo run --example mlock
```

Or add to your `Cargo.toml`:

```toml
[dependencies]
duende-core = "0.1"
duende-platform = "0.1"
async-trait = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "time", "signal"] }
```

See [Getting Started](./getting-started.md) for a complete walkthrough.
