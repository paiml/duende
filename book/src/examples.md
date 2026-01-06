# Examples

Duende includes two runnable examples demonstrating daemon lifecycle and memory locking.

## Running Examples

```bash
# Daemon lifecycle example (runs until Ctrl+C)
cargo run --example daemon

# Daemon with memory locking
cargo run --example daemon -- --mlock

# Memory locking example (demonstrates DT-007)
cargo run --example mlock

# Memory locking with required flag (fails without CAP_IPC_LOCK)
cargo run --example mlock -- --required

# Check memory lock status
cargo run --example mlock -- --status
```

## Daemon Example

The daemon example demonstrates a complete daemon lifecycle with:
- Initialization with resource configuration
- Main loop with graceful shutdown via Ctrl+C
- Health checks and metrics
- Memory locking support (DT-007)

```bash
$ cargo run --example daemon
╔════════════════════════════════════════════════════════════╗
║              DUENDE DAEMON EXAMPLE                         ║
╠════════════════════════════════════════════════════════════╣
║  Framework: Duende (Cross-Platform Daemon Tooling)         ║
║  Iron Lotus: Toyota Production System for Software         ║
╚════════════════════════════════════════════════════════════╝

[INIT] Daemon 'counter-daemon' initializing...
[INIT] Binary: "/usr/bin/counter-daemon"
[INIT] Initialization complete
[HEALTH] Status: HEALTHY

[RUN] Daemon starting main loop...
[RUN] Press Ctrl+C to stop
[RUN] Count: 1 | Uptime: 0.0s | Rate: 27340.33/s | Memory locked: NO
[RUN] Count: 2 | Uptime: 1.0s | Rate: 2.00/s | Memory locked: NO
...
```

### Command Line Options

| Option | Description |
|--------|-------------|
| `--mlock` | Lock memory to prevent swap (requires CAP_IPC_LOCK) |
| `--foreground` | Run in foreground mode |
| `--help` | Show help |

## Memory Locking Example

Demonstrates **DT-007: Swap Deadlock Prevention** - critical for daemons serving as swap devices.

```bash
$ cargo run --example mlock
=== duende mlock Example ===
DT-007: Swap Deadlock Prevention

Method 1: Direct lock_daemon_memory() call
  required = false
  Result: SUCCESS - All memory locked
  VmLck: 6012 KB

Method 2: Using apply_memory_config()
  lock_memory = true
  lock_memory_required = false
  Result: SUCCESS
  VmLck: 6012 KB

=== Example Complete ===

For production use in containers:
  docker run --cap-add=IPC_LOCK --ulimit memlock=-1:-1 ...
```

### Why Memory Locking Matters

Without memory locking, a swap-device daemon can deadlock:

1. Kernel needs to swap pages OUT to the daemon's device
2. Daemon needs memory to process I/O request
3. Kernel tries to swap out daemon's pages to free memory
4. Swap goes to the same daemon → **DEADLOCK**

## Docker Testing

```bash
# Build and run mlock tests
./docker/test-mlock.sh --build

# Run individual tests
docker run --rm duende-mlock-test
docker run --rm --cap-add=IPC_LOCK duende-mlock-test
docker run --rm --cap-add=IPC_LOCK --ulimit memlock=-1:-1 duende-mlock-test

# Run daemon example with memory locking
docker run --rm -it --cap-add=IPC_LOCK \
    -v $(pwd):/app -w /app rust:1.83 \
    cargo run --example daemon -- --mlock
```

## Example Code

### Complete Daemon Implementation

```rust
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use duende_core::{
    Daemon, DaemonConfig, DaemonContext, DaemonId, DaemonMetrics,
    ExitReason, HealthStatus,
};

struct CounterDaemon {
    id: DaemonId,
    name: String,
    metrics: DaemonMetrics,
    counter: Arc<AtomicU64>,
    running: Arc<AtomicBool>,
}

#[async_trait]
impl Daemon for CounterDaemon {
    fn id(&self) -> DaemonId { self.id }
    fn name(&self) -> &str { &self.name }

    async fn init(&mut self, config: &DaemonConfig) -> Result<()> {
        self.running.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn run(&mut self, ctx: &mut DaemonContext) -> Result<ExitReason> {
        while !ctx.should_shutdown() {
            let count = self.counter.fetch_add(1, Ordering::Relaxed) + 1;
            self.metrics.record_request();
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        Ok(ExitReason::Graceful)
    }

    async fn shutdown(&mut self, timeout: Duration) -> Result<()> {
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn health_check(&self) -> HealthStatus {
        if self.running.load(Ordering::Relaxed) {
            HealthStatus::healthy(1)
        } else {
            HealthStatus::unhealthy("Not running", 0)
        }
    }

    fn metrics(&self) -> &DaemonMetrics { &self.metrics }
}
```

### Swap Device Daemon

For daemons that serve as swap devices (like trueno-ublk):

```rust
use duende_platform::{apply_memory_config, lock_daemon_memory, MlockResult};

async fn init(&mut self, config: &DaemonConfig) -> Result<()> {
    // CRITICAL: Lock memory before any allocations
    let mut resources = config.resources.clone();
    resources.lock_memory = true;
    resources.lock_memory_required = true;  // Fail if mlock unavailable
    apply_memory_config(&resources)?;

    // Rest of initialization...
    Ok(())
}
```

### Direct Memory Locking

```rust
use duende_platform::{lock_daemon_memory, is_memory_locked, MlockResult};

match lock_daemon_memory(false) {
    Ok(MlockResult::Success) => {
        println!("Memory locked: {}", is_memory_locked());
    }
    Ok(MlockResult::Failed(errno)) => {
        println!("Failed (errno={}), continuing", errno);
    }
    Ok(MlockResult::Disabled) => {
        println!("Platform doesn't support mlock");
    }
    Err(e) => {
        // Only happens when required=true
        return Err(e);
    }
}
```
