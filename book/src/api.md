# API Reference

## duende-core

Core types and traits for daemon implementation.

### Daemon Trait

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

### DaemonManager

Orchestrates multiple daemons with restart policies.

See the [source documentation](https://docs.rs/duende-core) for full API details.

## duende-platform

Platform-specific adapters and memory management.

### Memory Locking

```rust
pub fn lock_daemon_memory(required: bool) -> Result<MlockResult>;
pub fn is_memory_locked() -> bool;
pub fn apply_memory_config(config: &ResourceConfig) -> Result<()>;
```

### Platform Detection

```rust
pub fn detect_platform() -> Platform;
```
