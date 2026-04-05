//! Criterion benchmarks for duende-core daemon framework.
//!
//! Benchmarks config validation, metrics collection, and TOML parsing
//! which are hot paths in daemon lifecycle management.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use duende_core::config::DaemonConfig;
use duende_core::metrics::DaemonMetrics;
use std::time::Duration;

fn bench_config_create_and_validate(c: &mut Criterion) {
    c.bench_function("daemon_config_create_validate", |b| {
        b.iter(|| {
            let config = DaemonConfig::new(
                black_box("my-daemon"),
                black_box("/usr/local/bin/my-daemon"),
            );
            let _ = config.validate();
        });
    });
}

fn bench_config_toml_roundtrip(c: &mut Criterion) {
    let config = DaemonConfig::new("bench-daemon", "/usr/local/bin/bench-daemon");
    let toml_str = toml::to_string_pretty(&config).unwrap();
    c.bench_function("config_toml_deserialize", |b| {
        b.iter(|| {
            let _: DaemonConfig = toml::from_str(black_box(&toml_str)).unwrap();
        });
    });
}

fn bench_metrics_record_request(c: &mut Criterion) {
    let metrics = DaemonMetrics::new();
    c.bench_function("metrics_record_request", |b| {
        b.iter(|| {
            metrics.record_request();
        });
    });
}

fn bench_metrics_record_duration(c: &mut Criterion) {
    let metrics = DaemonMetrics::new();
    c.bench_function("metrics_record_duration", |b| {
        b.iter(|| {
            metrics.record_duration(black_box(Duration::from_micros(150)));
        });
    });
}

fn bench_metrics_record_error(c: &mut Criterion) {
    let metrics = DaemonMetrics::new();
    c.bench_function("metrics_record_error", |b| {
        b.iter(|| {
            metrics.record_error();
        });
    });
}

fn bench_metrics_snapshot(c: &mut Criterion) {
    let metrics = DaemonMetrics::new();
    // Pre-populate with some data
    for i in 0..1000 {
        metrics.record_request();
        metrics.record_duration(Duration::from_micros(100 + i));
    }
    for _ in 0..10 {
        metrics.record_error();
    }
    c.bench_function("metrics_snapshot", |b| {
        b.iter(|| {
            black_box(metrics.snapshot());
        });
    });
}

criterion_group!(
    benches,
    bench_config_create_and_validate,
    bench_config_toml_roundtrip,
    bench_metrics_record_request,
    bench_metrics_record_duration,
    bench_metrics_record_error,
    bench_metrics_snapshot,
);
criterion_main!(benches);
