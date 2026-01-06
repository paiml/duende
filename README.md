# duende

[![Crates.io](https://img.shields.io/crates/v/duende-core.svg)](https://crates.io/crates/duende-core)
[![Documentation](https://docs.rs/duende-core/badge.svg)](https://docs.rs/duende-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

<p align="center">
  <img src="assets/hero.svg" alt="Duende - Cross-Platform Daemon Orchestration" width="800">
</p>

Cross-platform daemon framework for the PAIML Sovereign AI Stack.

## Status

| Metric | Value | Falsification |
|--------|-------|---------------|
| Tests | 527 | `cargo test --workspace` |
| Coverage | See CI | `make coverage` |
| Platforms | 1 of 6 | NativeAdapter only |

## What Works (Falsifiable)

### duende-core (309 tests)

- **Daemon trait**: Async lifecycle with `init()`, `run()`, `shutdown()`, `health_check()`
- **DaemonManager**: Registration, status tracking, signal forwarding
- **RestartPolicy**: `Never`, `Always`, `OnFailure` with max retries
- **BackoffConfig**: Exponential backoff (1s initial, 2x multiplier, 60s max)
- **NativeAdapter**: Process spawning via `tokio::process`, signal delivery

```rust
use duende_core::{Daemon, DaemonConfig, DaemonContext, DaemonId, DaemonMetrics, ExitReason, HealthStatus, DaemonError};
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

### duende-mlock (44 tests)

Memory locking for swap-critical daemons. Prevents deadlock when daemon is swap backend.

```rust
use duende_mlock::{lock_all, MlockConfig, lock_with_config};

// Lock all current and future allocations
let status = lock_all()?;
assert!(status.is_locked());

// Or with config for non-critical contexts
let config = MlockConfig::builder()
    .current(true)
    .future(true)
    .required(false)  // Don't fail if mlock fails
    .build();
lock_with_config(config)?;
```

**Requires**: `CAP_IPC_LOCK` or `--cap-add=IPC_LOCK --ulimit memlock=-1:-1`

### duende-observe (55 tests)

- **Monitor**: Process metrics via `/proc/[pid]/stat` (Linux)
- **Tracer**: Syscall statistics, anti-pattern detection, anomaly detection

### duende-policy (45 tests)

- **CircuitBreaker**: Opens after N failures, resets on success
- **JidokaGate**: Stop-on-first-failure quality checks
- **ResourceLimiter**: cgroups v2 integration (Linux)

### duende-test (45 tests)

- **TestHarness**: Timeout-based async test runner
- **ChaosInjector**: Latency and error injection
- **MockDaemon**: Configurable test doubles

## What Does NOT Work (Stubs)

| Adapter | Status | Ticket |
|---------|--------|--------|
| SystemdAdapter | Returns `NotSupported` | DP-002 |
| LaunchdAdapter | Returns `NotSupported` | DP-004 |
| ContainerAdapter | Returns `NotSupported` | DP-005 |
| PepitaAdapter | Returns `NotSupported` | DP-006 |
| WosAdapter | Returns `NotSupported` | DP-007 |

## Crate Structure

```
duende/
├── crates/
│   ├── duende-core/       # 309 tests - Daemon trait, manager, native adapter
│   ├── duende-mlock/      # 44 tests  - mlockall() for swap safety
│   ├── duende-observe/    # 55 tests  - /proc monitoring, syscall tracing
│   ├── duende-platform/   # 29 tests  - Platform detection, memory helpers
│   ├── duende-policy/     # 45 tests  - Circuit breaker, jidoka, cgroups
│   └── duende-test/       # 45 tests  - Harness, chaos, mocks
```

## Development

```bash
cargo build                    # Build
cargo test --workspace         # Run 527 tests
make tier1                     # fmt + clippy + check (<3s)
make tier2                     # tests + deny (1-5min)
make coverage                  # Coverage report
```

## Roadmap

| ID | Title | Priority | Falsification Criteria |
|----|-------|----------|------------------------|
| DP-001 | mlock() Memory Locking | Critical | `duende_mlock::lock_all()` returns `Ok(Locked{..})` |
| DP-002 | Linux systemd Adapter | High | `systemctl status` shows managed unit |
| DP-003 | trueno-ublk Integration | High | ublk device serves I/O with memory locked |

## License

MIT
