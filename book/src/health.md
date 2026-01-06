# Health Checks

Duende supports periodic health checks for monitoring daemon status.

## Configuration

```toml
[health_check]
enabled = true
interval = "30s"  # Check every 30 seconds
timeout = "10s"   # Timeout for each check
retries = 3       # Failures before unhealthy
```

## Implementation

```rust
async fn health_check(&self) -> HealthStatus {
    // Check dependencies
    if !self.db.is_connected() {
        return HealthStatus::unhealthy("Database disconnected");
    }

    // Check internal state
    if self.queue.len() > 10000 {
        return HealthStatus::degraded("Queue backlog");
    }

    // Return healthy with score
    HealthStatus::healthy(5)
}
```

## Health Status

| Status | Meaning |
|--------|---------|
| `Healthy(score)` | Operating normally, score 0-5 |
| `Degraded(reason)` | Working but impaired |
| `Unhealthy(reason)` | Not functioning properly |
