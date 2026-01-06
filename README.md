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
| Tests | 665 | `cargo test --workspace` |
| Coverage | See CI | `make coverage` |
| Platforms | 4 of 6 | Native, Linux, macOS, Container |

## What Works (Falsifiable)

### duende-core (334 tests)

- **Daemon trait**: Async lifecycle with `init()`, `run()`, `shutdown()`, `health_check()`
- **DaemonManager**: Registration, status tracking, signal forwarding
- **RestartPolicy**: `Never`, `Always`, `OnFailure` with max retries
- **BackoffConfig**: Exponential backoff (1s initial, 2x multiplier, 60s max)
- **NativeAdapter**: Process spawning via `tokio::process`, signal delivery
- **SystemdAdapter** (Linux): Transient units via `systemd-run`, `systemctl` commands
- **LaunchdAdapter** (macOS): Plist files via `launchctl bootstrap/bootout`
- **ContainerAdapter**: Docker/Podman/containerd via CLI commands

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

## Platform Support

| Adapter | Status | Platform | Falsification |
|---------|--------|----------|---------------|
| NativeAdapter | Implemented | All | `cargo run --example daemon` |
| SystemdAdapter | Implemented | Linux | `systemctl --user status duende-*` |
| LaunchdAdapter | Implemented | macOS | `launchctl list \| grep duende` |
| ContainerAdapter | Implemented | All | `docker ps \| grep duende` |
| PepitaAdapter | Stub | - | Returns `NotSupported` |
| WosAdapter | Stub | - | Returns `NotSupported` |

## Crate Structure

```
duende/
├── crates/
│   ├── duende-core/       # 334 tests - Daemon trait, manager, platform adapters
│   ├── duende-mlock/      # 44 tests  - mlockall() for swap safety
│   ├── duende-observe/    # 55 tests  - /proc monitoring, syscall tracing
│   ├── duende-platform/   # 29 tests  - Platform detection, memory helpers
│   ├── duende-policy/     # 45 tests  - Circuit breaker, jidoka, cgroups
│   └── duende-test/       # 45 tests  - Harness, chaos, mocks
```

## Development

```bash
cargo build                    # Build
cargo test --workspace         # Run 665 tests
make tier1                     # fmt + clippy + check (<3s)
make tier2                     # tests + deny (1-5min)
make coverage                  # Coverage report
```

## Roadmap

| ID | Title | Status | Falsification Criteria |
|----|-------|--------|------------------------|
| DP-001 | mlock() Memory Locking | Done | `duende_mlock::lock_all()` returns `Ok(Locked{..})` |
| DP-002 | Linux systemd Adapter | Done | `systemctl --user status` shows managed unit |
| DP-003 | trueno-ublk Integration | In Progress | ublk device serves I/O with memory locked |
| DP-004 | macOS launchd Adapter | Done | `launchctl list` shows managed service |
| DP-005 | Container Adapter | Done | `docker ps` shows managed container |
| DP-006 | pepita MicroVM Adapter | Planned | VM spawns with vsock communication |
| DP-007 | WOS Adapter | Planned | WebAssembly process managed via WOS API |

## License

MIT
