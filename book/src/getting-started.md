# Getting Started

## Installation

Add duende to your `Cargo.toml`:

```toml
[dependencies]
duende-core = "0.1"
duende-platform = "0.1"
async-trait = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "sync", "time", "signal"] }
```

## Your First Daemon

Here's a minimal daemon implementation:

```rust
use async_trait::async_trait;
use duende_core::{
    Daemon, DaemonConfig, DaemonContext, DaemonId, DaemonMetrics,
    ExitReason, HealthStatus, Result,
};
use std::time::Duration;

struct MyDaemon {
    id: DaemonId,
    metrics: DaemonMetrics,
}

impl MyDaemon {
    fn new() -> Self {
        Self {
            id: DaemonId::new(),
            metrics: DaemonMetrics::new(),
        }
    }
}

#[async_trait]
impl Daemon for MyDaemon {
    fn id(&self) -> DaemonId {
        self.id
    }

    fn name(&self) -> &str {
        "my-daemon"
    }

    async fn init(&mut self, config: &DaemonConfig) -> Result<()> {
        // Apply resource configuration (including mlock if enabled)
        duende_platform::apply_memory_config(&config.resources)?;

        // Your initialization code here
        println!("Daemon initialized");
        Ok(())
    }

    async fn run(&mut self, ctx: &mut DaemonContext) -> Result<ExitReason> {
        println!("Daemon running");

        loop {
            // Check for shutdown signal
            if ctx.should_shutdown() {
                return Ok(ExitReason::Graceful);
            }

            // Check for other signals
            if let Some(signal) = ctx.try_recv_signal() {
                println!("Received signal: {:?}", signal);
            }

            // Do your work here
            self.metrics.record_request();

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn shutdown(&mut self, _timeout: Duration) -> Result<()> {
        println!("Daemon shutting down");
        Ok(())
    }

    async fn health_check(&self) -> HealthStatus {
        HealthStatus::healthy(5)
    }

    fn metrics(&self) -> &DaemonMetrics {
        &self.metrics
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut daemon = MyDaemon::new();
    let config = DaemonConfig::new("my-daemon", "/usr/bin/my-daemon");

    daemon.init(&config).await?;

    let (mut ctx, _handle) = DaemonContext::new(config);
    let exit = daemon.run(&mut ctx).await?;

    println!("Daemon exited: {:?}", exit);
    Ok(())
}
```

## Platform-Specific Setup

### Linux (systemd)

Create a systemd unit file at `/etc/systemd/system/my-daemon.service`:

```ini
[Unit]
Description=My Daemon
After=network.target

[Service]
Type=simple
ExecStart=/usr/bin/my-daemon
Restart=on-failure
# For swap device daemons:
AmbientCapabilities=CAP_IPC_LOCK
LimitMEMLOCK=infinity

[Install]
WantedBy=multi-user.target
```

### Container

```dockerfile
FROM rust:1.83 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/my-daemon /usr/bin/
CMD ["/usr/bin/my-daemon"]
```

Run with:

```bash
docker run --cap-add=IPC_LOCK --ulimit memlock=-1:-1 my-daemon
```

## Next Steps

- [Daemon Lifecycle](./lifecycle.md) - Understanding the lifecycle phases
- [Configuration](./configuration.md) - Full configuration options
- [Memory Locking](./mlock.md) - Critical for swap device daemons
