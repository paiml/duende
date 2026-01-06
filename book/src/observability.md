# Observability

Duende integrates with the PAIML observability stack.

## Metrics

RED method metrics are collected automatically:

- **Rate**: Requests per second
- **Errors**: Error rate
- **Duration**: Request latency

```rust
fn metrics(&self) -> &DaemonMetrics {
    &self.metrics
}
```

## Tracing

Integration with `renacer` for syscall tracing:

```rust
let tracer_handle = adapter.attach_tracer(&daemon_handle).await?;
```

## Logging

Structured logging with `tracing`:

```rust
use tracing::{info, warn, error};

info!(pid = %process.id(), "Daemon started");
warn!(queue_depth = %depth, "Queue backlog detected");
error!(error = %e, "Failed to process request");
```
