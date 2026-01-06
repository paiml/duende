# duende-test

Testing infrastructure for the Duende daemon framework.

[![Crates.io](https://img.shields.io/crates/v/duende-test.svg)](https://crates.io/crates/duende-test)
[![Documentation](https://docs.rs/duende-test/badge.svg)](https://docs.rs/duende-test)
[![License](https://img.shields.io/crates/l/duende-test.svg)](LICENSE)

## Overview

This crate provides testing utilities:

- **Test harness**: Daemon lifecycle testing utilities
- **Chaos injection**: Latency, errors, packet loss simulation
- **Load testing**: Performance testing under load
- **Falsification tests**: 110 Popperian tests for spec compliance

## Usage

### Test Harness

```rust
use duende_test::{DaemonTestHarness, ChaosConfig};

let harness = DaemonTestHarness::builder()
    .with_platform(Platform::Native)
    .build();

let handle = harness.spawn(my_daemon).await?;
assert!(handle.health_check().await?.is_healthy());
```

### Chaos Testing

```rust
use duende_test::ChaosConfig;
use std::time::Duration;

let harness = DaemonTestHarness::builder()
    .with_chaos(ChaosConfig {
        latency_probability: 0.1,
        latency_duration: Duration::from_millis(500),
        error_probability: 0.05,
        ..Default::default()
    })
    .build();

// Daemon should remain healthy under chaos
let handle = harness.spawn(my_daemon).await?;
tokio::time::sleep(Duration::from_secs(30)).await;
assert!(handle.health_check().await?.is_healthy());
```

### Load Testing

```rust
use duende_test::{LoadTester, LoadTestConfig};

let tester = LoadTester::new();
let config = LoadTestConfig::stress();

let report = tester.run(config).await?;
assert!(report.passed());
println!("P99 latency: {:?}", report.latency_p99);
```

## Feature Flags

| Feature | Description |
|---------|-------------|
| `falsification` | Enable 110 Popperian falsification tests |

## Iron Lotus Framework

- **Built-in Quality** (品質の作り込み): Quality cannot be inspected in
- **Popperian Falsification**: Tests designed to refute claims
- **Extreme TDD**: Write failing tests first

## License

MIT OR Apache-2.0
