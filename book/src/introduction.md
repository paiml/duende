# Duende: Cross-Platform Daemon Framework

**Duende** is a cross-platform daemon tooling framework for the PAIML Sovereign AI Stack. It provides a unified abstraction for daemon lifecycle management across:

- **Linux** (systemd)
- **macOS** (launchd)
- **Containers** (Docker/OCI)
- **MicroVMs** (pepita)
- **WebAssembly OS** (WOS)

## Why Duende?

Managing daemons across different platforms is complex. Each platform has its own:

- Service management (systemd units, launchd plists, container specs)
- Signal handling conventions
- Resource limits and cgroups
- Health check mechanisms
- Logging and observability

Duende provides a **single Rust trait** that works everywhere:

```rust
use duende_core::{Daemon, DaemonConfig, DaemonContext, ExitReason};
use async_trait::async_trait;

struct MyDaemon;

#[async_trait]
impl Daemon for MyDaemon {
    fn id(&self) -> DaemonId { /* ... */ }
    fn name(&self) -> &str { "my-daemon" }

    async fn init(&mut self, config: &DaemonConfig) -> Result<()> {
        // Setup resources, validate config
        Ok(())
    }

    async fn run(&mut self, ctx: &mut DaemonContext) -> Result<ExitReason> {
        loop {
            if ctx.should_shutdown() {
                return Ok(ExitReason::Graceful);
            }
            // Do work...
        }
    }

    async fn shutdown(&mut self, timeout: Duration) -> Result<()> {
        // Cleanup
        Ok(())
    }

    async fn health_check(&self) -> HealthStatus {
        HealthStatus::healthy(5)
    }
}
```

## Design Principles

Duende follows the **Toyota Production System** principles:

| Principle | Application |
|-----------|-------------|
| **Jidoka** (自働化) | Stop-on-error in pipelines, automatic failover |
| **Poka-Yoke** (ポカヨケ) | Privacy tiers prevent data leakage |
| **Heijunka** (平準化) | Load leveling via spillover routing |
| **Muda** (無駄) | Cost circuit breakers prevent waste |
| **Kaizen** (改善) | Continuous optimization via metrics |
| **Genchi Genbutsu** (現地現物) | Direct observation via renacer tracing |

## Stack Integration

Duende integrates with the PAIML Sovereign AI Stack:

```
┌─────────────────────────────────────────────────────────────┐
│                    duende (Daemon Framework)                │
├─────────────────────────────────────────────────────────────┤
│  duende-core  │  duende-platform  │  duende-observe        │
├───────────────┴───────────────────┴─────────────────────────┤
│  trueno-ublk (Block Device)  │  realizar (Inference)       │
├──────────────────────────────┴──────────────────────────────┤
│                 renacer (Syscall Tracing)                   │
└─────────────────────────────────────────────────────────────┘
```

## Quick Start

Add duende to your `Cargo.toml`:

```toml
[dependencies]
duende-core = "0.1"
duende-platform = "0.1"
```

See [Getting Started](./getting-started.md) for a complete example.
