# duende-observe

Observability integration for the Duende daemon framework.

[![Crates.io](https://img.shields.io/crates/v/duende-observe.svg)](https://crates.io/crates/duende-observe)
[![Documentation](https://docs.rs/duende-observe/badge.svg)](https://docs.rs/duende-observe)
[![License](https://img.shields.io/crates/l/duende-observe.svg)](LICENSE)

## Overview

This crate provides observability features:

- **Renacer integration**: Syscall tracing with source correlation
- **ttop integration**: Real-time resource monitoring via trueno-viz collectors
- **Metrics export**: Prometheus and OTLP format support

## Usage

```rust
use duende_observe::{DaemonTracer, DaemonMonitor};

// Attach tracer to daemon
let mut tracer = DaemonTracer::new();
tracer.attach(daemon_pid).await?;

// Collect syscall trace
let report = tracer.collect().await?;
println!("Critical path: {:?}", report.critical_path);

// Monitor daemon resources
let mut monitor = DaemonMonitor::new();
let snapshot = monitor.collect(daemon_pid)?;
println!("CPU: {}%, Memory: {} bytes", snapshot.cpu_percent, snapshot.memory_bytes);
```

## Iron Lotus Framework

- **Genchi Genbutsu** (現地現物): Direct observation via syscall tracing
- **Visual Management** (目で見る管理): Real-time metrics dashboards
- **Kaizen** (改善): Continuous improvement via metrics collection

## License

MIT OR Apache-2.0
