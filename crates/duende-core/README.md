# duende-core

Core daemon lifecycle primitives for the Duende framework.

[![Crates.io](https://img.shields.io/crates/v/duende-core.svg)](https://crates.io/crates/duende-core)
[![Documentation](https://docs.rs/duende-core/badge.svg)](https://docs.rs/duende-core)
[![License](https://img.shields.io/crates/l/duende-core.svg)](LICENSE)

## Overview

This crate provides foundational types and traits for cross-platform daemon management:

- **`Daemon` trait**: Define daemon lifecycle (init, run, shutdown)
- **`DaemonConfig`**: Configuration with restart policies, health checks
- **`DaemonMetrics`**: RED method metrics (Rate, Errors, Duration)
- **`DaemonContext`**: Runtime context and signal handling
- **`DaemonManager`**: Manages multiple daemons with supervision

## Quick Start

```rust
use duende_core::{Daemon, DaemonConfig, DaemonContext, DaemonId, ExitReason};
use async_trait::async_trait;

struct MyDaemon {
    id: DaemonId,
}

#[async_trait]
impl Daemon for MyDaemon {
    fn id(&self) -> DaemonId { self.id }
    fn name(&self) -> &str { "my-daemon" }

    async fn init(&mut self, _config: &DaemonConfig) -> duende_core::Result<()> {
        Ok(())
    }

    async fn run(&mut self, ctx: &mut DaemonContext) -> duende_core::Result<ExitReason> {
        while !ctx.should_shutdown() {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        Ok(ExitReason::Graceful)
    }

    async fn shutdown(&mut self, _timeout: std::time::Duration) -> duende_core::Result<()> {
        Ok(())
    }
}
```

## Iron Lotus Framework

This crate follows Toyota Production System principles:

- **Jidoka** (自働化): Stop-on-error with automatic failover
- **Poka-Yoke** (ポカヨケ): Type-safe APIs preventing misuse
- **Kaizen** (改善): Continuous metrics collection for optimization
- **Genchi Genbutsu** (現地現物): Direct observation via tracing

## License

MIT OR Apache-2.0
