# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Duende is a cross-platform daemon orchestration framework for the **PAIML Sovereign AI Stack**. It provides unified lifecycle management, observability, and policy enforcement for long-running processes across Linux (systemd), macOS (launchd), Docker containers, pepita microVMs, and WOS (WebAssembly Operating System).

## Build and Development Commands

```bash
# Build
cargo build                    # Debug build
cargo build --release --locked # Release build

# Testing (Iron Lotus Tiers)
make tier1                     # On-save (<3s): fmt, clippy, check
make tier2                     # Pre-commit (1-5min): tests, deny
make tier3                     # Pre-merge: coverage, mutants, falsification
make tier4                     # CI/CD: release tests + security

# Individual test commands
cargo test --lib --workspace                    # Unit tests only
cargo test --workspace                          # All tests
cargo test -p duende-core                       # Single crate tests
cargo test -p duende-core test_context          # Single test by name
cargo test -p duende-mlock -- --nocapture       # With output

# Coverage and mutation testing
make coverage                  # Coverage report (target/coverage/html/)
make mutants-fast              # Quick mutation testing (4 jobs)

# Linting and formatting
make fmt                       # Format code
make clippy                    # Run clippy
make deny                      # Supply chain security check

# Examples
cargo run --example daemon     # Basic daemon example
cargo run --example mlock      # Memory locking example
make example-mlock-status      # Check mlock status
```

## Architecture

### Workspace Crates

| Crate | Purpose |
|-------|---------|
| `duende-core` | Daemon trait, config, manager, metrics, platform adapters |
| `duende-mlock` | Memory locking (`mlockall`) for swap-critical daemons |
| `duende-platform` | Platform detection and memory helpers |
| `duende-observe` | Observability (tracer, monitor, health) |
| `duende-policy` | Policy enforcement (gates, circuit breaker, limiter, jidoka) |
| `duende-test` | Testing infrastructure (harness, chaos, load, falsification) |

### Key Types (duende-core)

**Daemon Trait** - Core lifecycle contract:
```rust
#[async_trait]
pub trait Daemon: Send + Sync + 'static {
    fn id(&self) -> DaemonId;
    fn name(&self) -> &str;
    async fn init(&mut self, config: &DaemonConfig) -> Result<()>;
    async fn run(&mut self, ctx: &mut DaemonContext) -> Result<ExitReason>;
    async fn shutdown(&mut self, timeout: Duration) -> Result<()>;
    async fn health_check(&self) -> HealthStatus;
    fn metrics(&self) -> &DaemonMetrics;
}
```

**PlatformAdapter Trait** - Platform-specific operations:
```rust
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    fn platform(&self) -> Platform;
    async fn spawn(&self, daemon: Box<dyn Daemon>) -> PlatformResult<DaemonHandle>;
    async fn signal(&self, handle: &DaemonHandle, sig: Signal) -> PlatformResult<()>;
    async fn status(&self, handle: &DaemonHandle) -> PlatformResult<DaemonStatus>;
    async fn attach_tracer(&self, handle: &DaemonHandle) -> PlatformResult<TracerHandle>;
}
```

**Platform Adapters** (in `duende-core/src/adapters/`):
- `NativeAdapter` - Direct process spawning (implemented)
- `SystemdAdapter`, `LaunchdAdapter`, `ContainerAdapter`, `PepitaAdapter`, `WosAdapter` - Return `NotSupported` (stubs)

### Memory Locking (duende-mlock)

Critical for swap device daemons (e.g., trueno-ublk) to prevent deadlocks:

```rust
use duende_mlock::{lock_all, MlockConfig, lock_with_config};

// Simple: lock all current and future allocations
let status = lock_all()?;

// Configurable: allow failures in non-critical contexts
let config = MlockConfig::builder()
    .current(true)
    .future(true)
    .required(false)  // Don't fail if mlock fails
    .build();
lock_with_config(config)?;
```

Container requirements for mlock:
```bash
docker run --cap-add=IPC_LOCK --ulimit memlock=-1:-1 ...
```

## Iron Lotus Framework

### Forbidden Patterns (enforced via workspace lints)
```rust
// DENY in Cargo.toml [workspace.lints.clippy]
foo.unwrap()           // Use ? or handle Result
foo.expect("msg")      // Use ? or handle Result
panic!("msg")          // Return Result instead
todo!()                // Implement or remove
unimplemented!()       // Implement or remove
```

### Quality Targets
- Coverage: ≥95% line coverage
- Mutation: ≥80% mutation score
- SATD: 0 (zero technical debt comments)

### Dependency Policy

| Priority | Source | Action |
|----------|--------|--------|
| P0 | PAIML Stack | Prefer (trueno, repartir, renacer) |
| P1 | Rust std/core | Accept |
| P2 | Pure Rust crates | Evaluate (MIT/Apache-2.0 only) |
| P3 | Crates with C deps | Avoid |
| P4 | Foreign FFI | Prohibit |

Banned crates (in `deny.toml`): `openssl`, `openssl-sys`, `rayon` (use repartir instead)

## Testing

### Falsification Tests
110 falsification tests organized by category (F001-F110):
- Lifecycle (F001-F020)
- Resources (F021-F040)
- Observability (F041-F060)
- Policy (F061-F080)
- Platform (F081-F110)

Run with: `make falsification` or `cargo test --workspace --features falsification -- falsification`

### Docker Testing (mlock)
```bash
make docker-mlock-build        # Build test container
make docker-mlock-test         # Run mlock tests in container
make docker-mlock-test-build   # Build and run
```

## Roadmap Priority

| ID | Title | Status |
|----|-------|--------|
| DP-001 | mlock() Memory Locking | In Progress |
| DP-002 | Linux systemd Adapter | Planned |
| DP-003 | trueno-ublk Integration | Planned |

Critical path: DP-001 → DP-002 → DP-003 (trueno-ublk implements `Daemon` trait)

## Rust Edition

This project uses **Rust 2024 edition** (`edition = "2024"` in Cargo.toml), requiring Rust 1.83+.
