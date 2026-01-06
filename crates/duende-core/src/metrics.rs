//! Daemon metrics following the RED method (Rate, Errors, Duration).
//!
//! # Reference
//! Wilkins, T. (2018). "The RED Method: How to instrument your services."
//! Weaveworks Blog.
//!
//! # Toyota Way: Visual Management (目で見る管理)
//! Make daemon health visible at a glance.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Daemon metrics collection following RED method.
///
/// Thread-safe metrics that can be shared across async tasks.
#[derive(Debug, Clone)]
pub struct DaemonMetrics {
    inner: Arc<MetricsInner>,
}

#[derive(Debug)]
struct MetricsInner {
    // Rate metrics
    requests_total: AtomicU64,

    // Error metrics
    errors_total: AtomicU64,

    // Duration metrics (stored as microseconds for atomic operations)
    duration_sum_us: AtomicU64,
    duration_count: AtomicU64,
    duration_max_us: AtomicU64,

    // Resource metrics
    cpu_usage_permille: AtomicU64, // CPU usage * 1000 (for precision)
    memory_bytes: AtomicU64,
    open_fds: AtomicU64,
    thread_count: AtomicU64,

    // Circuit breaker
    circuit_breaker_trips: AtomicU64,
    successful_recoveries: AtomicU64,

    // Start time for uptime calculation
    start_time: Instant,
}

impl DaemonMetrics {
    /// Creates a new metrics collector.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MetricsInner {
                requests_total: AtomicU64::new(0),
                errors_total: AtomicU64::new(0),
                duration_sum_us: AtomicU64::new(0),
                duration_count: AtomicU64::new(0),
                duration_max_us: AtomicU64::new(0),
                cpu_usage_permille: AtomicU64::new(0),
                memory_bytes: AtomicU64::new(0),
                open_fds: AtomicU64::new(0),
                thread_count: AtomicU64::new(0),
                circuit_breaker_trips: AtomicU64::new(0),
                successful_recoveries: AtomicU64::new(0),
                start_time: Instant::now(),
            }),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Rate metrics
    // ═══════════════════════════════════════════════════════════════════════════

    /// Increments the request counter.
    pub fn record_request(&self) {
        self.inner.requests_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns total requests processed.
    #[must_use]
    pub fn requests_total(&self) -> u64 {
        self.inner.requests_total.load(Ordering::Relaxed)
    }

    /// Returns requests per second since start.
    #[must_use]
    pub fn requests_per_second(&self) -> f64 {
        let elapsed = self.inner.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.requests_total() as f64 / elapsed
        } else {
            0.0
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Error metrics
    // ═══════════════════════════════════════════════════════════════════════════

    /// Increments the error counter.
    pub fn record_error(&self) {
        self.inner.errors_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns total errors.
    #[must_use]
    pub fn errors_total(&self) -> u64 {
        self.inner.errors_total.load(Ordering::Relaxed)
    }

    /// Returns error rate (errors / requests).
    #[must_use]
    pub fn error_rate(&self) -> f64 {
        let requests = self.requests_total();
        if requests > 0 {
            self.errors_total() as f64 / requests as f64
        } else {
            0.0
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Duration metrics
    // ═══════════════════════════════════════════════════════════════════════════

    /// Records a request duration.
    pub fn record_duration(&self, duration: Duration) {
        let us = duration.as_micros() as u64;
        self.inner.duration_sum_us.fetch_add(us, Ordering::Relaxed);
        self.inner.duration_count.fetch_add(1, Ordering::Relaxed);

        // Update max (not perfectly atomic but close enough for metrics)
        let mut current_max = self.inner.duration_max_us.load(Ordering::Relaxed);
        while us > current_max {
            match self.inner.duration_max_us.compare_exchange_weak(
                current_max,
                us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_max = actual,
            }
        }
    }

    /// Returns average duration.
    #[must_use]
    pub fn duration_avg(&self) -> Duration {
        let count = self.inner.duration_count.load(Ordering::Relaxed);
        if count > 0 {
            let sum_us = self.inner.duration_sum_us.load(Ordering::Relaxed);
            Duration::from_micros(sum_us / count)
        } else {
            Duration::ZERO
        }
    }

    /// Returns maximum duration.
    #[must_use]
    pub fn duration_max(&self) -> Duration {
        Duration::from_micros(self.inner.duration_max_us.load(Ordering::Relaxed))
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Resource metrics
    // ═══════════════════════════════════════════════════════════════════════════

    /// Sets CPU usage (0.0 to 100.0).
    pub fn set_cpu_usage(&self, percent: f64) {
        let permille = (percent * 10.0) as u64;
        self.inner
            .cpu_usage_permille
            .store(permille, Ordering::Relaxed);
    }

    /// Returns CPU usage percentage.
    #[must_use]
    pub fn cpu_usage(&self) -> f64 {
        self.inner.cpu_usage_permille.load(Ordering::Relaxed) as f64 / 10.0
    }

    /// Sets memory usage in bytes.
    pub fn set_memory_bytes(&self, bytes: u64) {
        self.inner.memory_bytes.store(bytes, Ordering::Relaxed);
    }

    /// Returns memory usage in bytes.
    #[must_use]
    pub fn memory_bytes(&self) -> u64 {
        self.inner.memory_bytes.load(Ordering::Relaxed)
    }

    /// Sets open file descriptor count.
    pub fn set_open_fds(&self, count: u64) {
        self.inner.open_fds.store(count, Ordering::Relaxed);
    }

    /// Returns open file descriptor count.
    #[must_use]
    pub fn open_fds(&self) -> u64 {
        self.inner.open_fds.load(Ordering::Relaxed)
    }

    /// Sets thread count.
    pub fn set_thread_count(&self, count: u64) {
        self.inner.thread_count.store(count, Ordering::Relaxed);
    }

    /// Returns thread count.
    #[must_use]
    pub fn thread_count(&self) -> u64 {
        self.inner.thread_count.load(Ordering::Relaxed)
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Circuit breaker metrics
    // ═══════════════════════════════════════════════════════════════════════════

    /// Records a circuit breaker trip.
    pub fn record_circuit_breaker_trip(&self) {
        self.inner
            .circuit_breaker_trips
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Returns circuit breaker trip count.
    #[must_use]
    pub fn circuit_breaker_trips(&self) -> u64 {
        self.inner.circuit_breaker_trips.load(Ordering::Relaxed)
    }

    /// Records a successful recovery.
    pub fn record_recovery(&self) {
        self.inner
            .successful_recoveries
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Returns successful recovery count.
    #[must_use]
    pub fn successful_recoveries(&self) -> u64 {
        self.inner.successful_recoveries.load(Ordering::Relaxed)
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Uptime
    // ═══════════════════════════════════════════════════════════════════════════

    /// Returns daemon uptime.
    #[must_use]
    pub fn uptime(&self) -> Duration {
        self.inner.start_time.elapsed()
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Snapshot
    // ═══════════════════════════════════════════════════════════════════════════

    /// Creates a snapshot of current metrics.
    #[must_use]
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            requests_total: self.requests_total(),
            requests_per_second: self.requests_per_second(),
            errors_total: self.errors_total(),
            error_rate: self.error_rate(),
            duration_avg_us: self.duration_avg().as_micros() as u64,
            duration_max_us: self.duration_max().as_micros() as u64,
            cpu_usage_percent: self.cpu_usage(),
            memory_bytes: self.memory_bytes(),
            open_fds: self.open_fds(),
            thread_count: self.thread_count(),
            circuit_breaker_trips: self.circuit_breaker_trips(),
            successful_recoveries: self.successful_recoveries(),
            uptime_secs: self.uptime().as_secs(),
        }
    }
}

impl Default for DaemonMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of daemon metrics at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// Total requests processed.
    pub requests_total: u64,
    /// Requests per second.
    pub requests_per_second: f64,
    /// Total errors.
    pub errors_total: u64,
    /// Error rate (0.0 to 1.0).
    pub error_rate: f64,
    /// Average duration in microseconds.
    pub duration_avg_us: u64,
    /// Maximum duration in microseconds.
    pub duration_max_us: u64,
    /// CPU usage percentage.
    pub cpu_usage_percent: f64,
    /// Memory usage in bytes.
    pub memory_bytes: u64,
    /// Open file descriptors.
    pub open_fds: u64,
    /// Thread count.
    pub thread_count: u64,
    /// Circuit breaker trips.
    pub circuit_breaker_trips: u64,
    /// Successful recoveries.
    pub successful_recoveries: u64,
    /// Uptime in seconds.
    pub uptime_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_new() {
        let metrics = DaemonMetrics::new();
        assert_eq!(metrics.requests_total(), 0);
        assert_eq!(metrics.errors_total(), 0);
    }

    #[test]
    fn test_request_counting() {
        let metrics = DaemonMetrics::new();
        metrics.record_request();
        metrics.record_request();
        metrics.record_request();
        assert_eq!(metrics.requests_total(), 3);
    }

    #[test]
    fn test_error_rate() {
        let metrics = DaemonMetrics::new();
        for _ in 0..10 {
            metrics.record_request();
        }
        metrics.record_error();
        metrics.record_error();
        assert!((metrics.error_rate() - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_duration_tracking() {
        let metrics = DaemonMetrics::new();
        metrics.record_duration(Duration::from_millis(10));
        metrics.record_duration(Duration::from_millis(20));
        metrics.record_duration(Duration::from_millis(30));

        assert_eq!(metrics.duration_avg(), Duration::from_millis(20));
        assert_eq!(metrics.duration_max(), Duration::from_millis(30));
    }

    #[test]
    fn test_resource_metrics() {
        let metrics = DaemonMetrics::new();
        metrics.set_cpu_usage(45.5);
        metrics.set_memory_bytes(1024 * 1024);

        assert!((metrics.cpu_usage() - 45.5).abs() < 0.1);
        assert_eq!(metrics.memory_bytes(), 1024 * 1024);
    }

    #[test]
    fn test_snapshot() {
        let metrics = DaemonMetrics::new();
        metrics.record_request();
        metrics.record_error();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.requests_total, 1);
        assert_eq!(snapshot.errors_total, 1);
    }

    #[test]
    fn test_metrics_clone() {
        let metrics1 = DaemonMetrics::new();
        metrics1.record_request();

        let metrics2 = metrics1.clone();
        metrics1.record_request();

        // Both should see 2 requests (shared inner)
        assert_eq!(metrics1.requests_total(), 2);
        assert_eq!(metrics2.requests_total(), 2);
    }

    #[test]
    fn test_metrics_default() {
        let metrics = DaemonMetrics::default();
        assert_eq!(metrics.requests_total(), 0);
        assert_eq!(metrics.errors_total(), 0);
    }

    #[test]
    fn test_requests_per_second() {
        let metrics = DaemonMetrics::new();
        // Just verify it doesn't panic and returns reasonable value
        let rps = metrics.requests_per_second();
        assert!(rps >= 0.0);

        metrics.record_request();
        metrics.record_request();
        // RPS should be positive after some requests
        // (might be very high since almost no time has passed)
        let rps = metrics.requests_per_second();
        assert!(rps >= 0.0);
    }

    #[test]
    fn test_error_rate_zero_requests() {
        let metrics = DaemonMetrics::new();
        // No requests means 0 error rate
        assert_eq!(metrics.error_rate(), 0.0);
    }

    #[test]
    fn test_thread_count() {
        let metrics = DaemonMetrics::new();
        assert_eq!(metrics.thread_count(), 0);

        metrics.set_thread_count(4);
        assert_eq!(metrics.thread_count(), 4);
    }

    #[test]
    fn test_open_fds() {
        let metrics = DaemonMetrics::new();
        assert_eq!(metrics.open_fds(), 0);

        metrics.set_open_fds(128);
        assert_eq!(metrics.open_fds(), 128);
    }

    #[test]
    fn test_circuit_breaker() {
        let metrics = DaemonMetrics::new();
        assert_eq!(metrics.circuit_breaker_trips(), 0);

        metrics.record_circuit_breaker_trip();
        metrics.record_circuit_breaker_trip();
        assert_eq!(metrics.circuit_breaker_trips(), 2);
    }

    #[test]
    fn test_recovery() {
        let metrics = DaemonMetrics::new();
        assert_eq!(metrics.successful_recoveries(), 0);

        metrics.record_recovery();
        metrics.record_recovery();
        metrics.record_recovery();
        assert_eq!(metrics.successful_recoveries(), 3);
    }

    #[test]
    fn test_uptime() {
        let metrics = DaemonMetrics::new();
        std::thread::sleep(Duration::from_millis(10));
        let uptime = metrics.uptime();
        assert!(uptime >= Duration::from_millis(10));
    }

    #[test]
    fn test_duration_zero_count() {
        let metrics = DaemonMetrics::new();
        // No durations recorded - should return ZERO
        assert_eq!(metrics.duration_avg(), Duration::ZERO);
        assert_eq!(metrics.duration_max(), Duration::ZERO);
    }

    #[test]
    fn test_duration_max_update() {
        let metrics = DaemonMetrics::new();
        metrics.record_duration(Duration::from_millis(100));
        assert_eq!(metrics.duration_max(), Duration::from_millis(100));

        // Smaller duration shouldn't update max
        metrics.record_duration(Duration::from_millis(50));
        assert_eq!(metrics.duration_max(), Duration::from_millis(100));

        // Larger duration should update max
        metrics.record_duration(Duration::from_millis(200));
        assert_eq!(metrics.duration_max(), Duration::from_millis(200));
    }

    #[test]
    fn test_snapshot_all_fields() {
        let metrics = DaemonMetrics::new();
        metrics.record_request();
        metrics.record_request();
        metrics.record_error();
        metrics.record_duration(Duration::from_millis(10));
        metrics.set_cpu_usage(25.0);
        metrics.set_memory_bytes(1024);
        metrics.set_open_fds(64);
        metrics.set_thread_count(8);
        metrics.record_circuit_breaker_trip();
        metrics.record_recovery();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.requests_total, 2);
        assert_eq!(snapshot.errors_total, 1);
        assert!((snapshot.error_rate - 0.5).abs() < 0.001);
        assert_eq!(snapshot.duration_avg_us, 10000);
        assert_eq!(snapshot.duration_max_us, 10000);
        assert!((snapshot.cpu_usage_percent - 25.0).abs() < 0.1);
        assert_eq!(snapshot.memory_bytes, 1024);
        assert_eq!(snapshot.open_fds, 64);
        assert_eq!(snapshot.thread_count, 8);
        assert_eq!(snapshot.circuit_breaker_trips, 1);
        assert_eq!(snapshot.successful_recoveries, 1);
    }

    #[test]
    fn test_metrics_snapshot_serialize() {
        let metrics = DaemonMetrics::new();
        metrics.record_request();
        let snapshot = metrics.snapshot();

        let json = serde_json::to_string(&snapshot).unwrap();
        let deserialized: MetricsSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.requests_total, 1);
    }
}
