# Daemon Lifecycle

Duende daemons follow a well-defined lifecycle based on Toyota Production System principles.

## Lifecycle Phases

```
┌──────────────────────────────────────────────────────────┐
│                    DAEMON LIFECYCLE                       │
├──────────────────────────────────────────────────────────┤
│                                                          │
│    ┌─────────┐     ┌─────────┐     ┌──────────┐         │
│    │  INIT   │────▶│   RUN   │────▶│ SHUTDOWN │         │
│    └─────────┘     └─────────┘     └──────────┘         │
│         │               │                │               │
│         │               │                │               │
│    Poka-Yoke       Heijunka         Jidoka              │
│    (Fail Fast)   (Level Load)   (Stop Clean)            │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

## Init Phase

The `init` method is called once before `run`. It should:

- **Validate configuration** (Poka-Yoke: fail fast on misconfiguration)
- **Allocate resources** (memory, file handles)
- **Open connections** (databases, network)
- **Apply resource limits** (mlock, cgroups)

```rust
async fn init(&mut self, config: &DaemonConfig) -> Result<()> {
    // Apply memory locking if configured
    apply_memory_config(&config.resources)?;

    // Validate configuration
    config.validate()?;

    // Open database connection
    self.db = Database::connect(&config.db_url).await?;

    Ok(())
}
```

**Target duration:** < 100ms for most platforms.

## Run Phase

The `run` method contains the main execution loop. It should:

- **Check for shutdown** via `ctx.should_shutdown()`
- **Handle signals** via `ctx.recv_signal()`
- **Process work** with load leveling (Heijunka)
- **Update metrics** for observability

```rust
async fn run(&mut self, ctx: &mut DaemonContext) -> Result<ExitReason> {
    loop {
        if ctx.should_shutdown() {
            return Ok(ExitReason::Graceful);
        }

        // Handle signals
        if let Some(signal) = ctx.try_recv_signal() {
            match signal {
                Signal::Hup => self.reload_config().await?,
                Signal::Usr1 => self.dump_stats(),
                _ => {}
            }
        }

        // Process work
        self.process_next_item().await?;
        self.metrics.record_request();
    }
}
```

## Shutdown Phase

The `shutdown` method is called when the daemon receives a termination signal. It should:

- **Stop accepting new work**
- **Complete in-flight work** (within timeout)
- **Close connections**
- **Flush buffers**
- **Release resources**

```rust
async fn shutdown(&mut self, timeout: Duration) -> Result<()> {
    // Stop accepting new work
    self.accepting = false;

    // Wait for in-flight work (with timeout)
    tokio::time::timeout(timeout, self.drain_queue()).await?;

    // Close database connection
    self.db.close().await?;

    Ok(())
}
```

## Signal Handling

Duende handles the following signals:

| Signal | Action |
|--------|--------|
| `SIGTERM` | Graceful shutdown (sets `should_shutdown = true`) |
| `SIGINT` | Graceful shutdown |
| `SIGQUIT` | Graceful shutdown |
| `SIGHUP` | Reload configuration (custom handler) |
| `SIGUSR1` | Custom action |
| `SIGUSR2` | Custom action |
| `SIGSTOP` | Pause daemon |
| `SIGCONT` | Resume daemon |

## Health Checks

The `health_check` method is called periodically by the platform adapter:

```rust
async fn health_check(&self) -> HealthStatus {
    if self.db.is_connected() {
        HealthStatus::healthy(5)
    } else {
        HealthStatus::unhealthy("Database disconnected")
    }
}
```
