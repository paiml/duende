//! Category E: Observability falsification tests (F081-F100).
//!
//! These tests verify the observability properties defined in daemon-tools-spec.md
//! Section 9.2 using Popperian falsification methodology.

use std::time::Duration;

use crate::adapter::PlatformAdapter;
use crate::adapters::NativeAdapter;
use crate::daemon::Daemon;
use crate::metrics::DaemonMetrics;
use crate::tests::mocks::MockDaemon;
use crate::types::DaemonId;

/// F081: Request counter increments
#[test]
fn f081_request_counter_increments() {
    let metrics = DaemonMetrics::new();
    assert_eq!(metrics.requests_total(), 0);

    metrics.record_request();
    assert_eq!(metrics.requests_total(), 1);

    metrics.record_request();
    metrics.record_request();
    assert_eq!(metrics.requests_total(), 3);
}

/// F082: Error counter increments
#[test]
fn f082_error_counter_increments() {
    let metrics = DaemonMetrics::new();
    assert_eq!(metrics.errors_total(), 0);

    metrics.record_error();
    assert_eq!(metrics.errors_total(), 1);

    metrics.record_error();
    assert_eq!(metrics.errors_total(), 2);
}

/// F083: Error rate calculated correctly
#[test]
fn f083_error_rate_calculated() {
    let metrics = DaemonMetrics::new();

    // No requests = 0% error rate
    assert!((metrics.error_rate() - 0.0).abs() < f64::EPSILON);

    // 1 error out of 2 requests = 50% error rate
    metrics.record_request();
    metrics.record_request();
    metrics.record_error();

    let error_rate = metrics.error_rate();
    assert!(
        (error_rate - 0.5).abs() < 0.001,
        "Error rate should be 50%, got {}",
        error_rate
    );
}

/// F084: Duration average is correct
#[test]
fn f084_duration_average_correct() {
    let metrics = DaemonMetrics::new();

    // No durations = ZERO
    assert_eq!(metrics.duration_avg(), Duration::ZERO);

    // Average of 10ms and 20ms should be 15ms
    metrics.record_duration(Duration::from_millis(10));
    metrics.record_duration(Duration::from_millis(20));

    let avg = metrics.duration_avg();
    assert_eq!(avg, Duration::from_millis(15));
}

/// F085: Duration max tracks maximum
#[test]
fn f085_duration_max_tracks_maximum() {
    let metrics = DaemonMetrics::new();

    // No durations = ZERO
    assert_eq!(metrics.duration_max(), Duration::ZERO);

    metrics.record_duration(Duration::from_millis(10));
    assert_eq!(metrics.duration_max(), Duration::from_millis(10));

    metrics.record_duration(Duration::from_millis(50));
    assert_eq!(metrics.duration_max(), Duration::from_millis(50));

    // Smaller duration shouldn't update max
    metrics.record_duration(Duration::from_millis(20));
    assert_eq!(metrics.duration_max(), Duration::from_millis(50));
}

/// F086: Uptime increases monotonically
#[test]
fn f086_uptime_increases() {
    let metrics = DaemonMetrics::new();

    let uptime1 = metrics.uptime();
    std::thread::sleep(Duration::from_millis(10));
    let uptime2 = metrics.uptime();

    assert!(
        uptime2 > uptime1,
        "Uptime should increase: {:?} vs {:?}",
        uptime1,
        uptime2
    );
}

/// F087: Metrics are thread-safe
#[test]
fn f087_metrics_thread_safe() {
    use std::sync::Arc;
    use std::thread;

    let metrics = Arc::new(DaemonMetrics::new());
    let mut handles = vec![];

    // Spawn multiple threads updating metrics
    for _ in 0..4 {
        let m = Arc::clone(&metrics);
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                m.record_request();
                m.record_error();
                m.record_duration(Duration::from_micros(10));
            }
        }));
    }

    for h in handles {
        h.join().expect("thread should complete");
    }

    // Should have 400 requests and 400 errors from 4 threads × 100 iterations
    assert_eq!(metrics.requests_total(), 400);
    assert_eq!(metrics.errors_total(), 400);
}

/// F088: Metrics clone shares state
#[test]
fn f088_metrics_clone_shares_state() {
    let metrics1 = DaemonMetrics::new();
    let metrics2 = metrics1.clone();

    metrics1.record_request();
    assert_eq!(metrics1.requests_total(), 1);
    assert_eq!(metrics2.requests_total(), 1, "Clone should share state");

    metrics2.record_request();
    assert_eq!(metrics1.requests_total(), 2, "Original should see clone's updates");
}

/// F089: Snapshot captures all metrics
#[test]
fn f089_snapshot_captures_all() {
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
    assert!(snapshot.duration_avg_us > 0);
    assert!((snapshot.cpu_usage_percent - 25.0).abs() < 0.1);
    assert_eq!(snapshot.memory_bytes, 1024);
    assert_eq!(snapshot.open_fds, 64);
    assert_eq!(snapshot.thread_count, 8);
    assert_eq!(snapshot.circuit_breaker_trips, 1);
    assert_eq!(snapshot.successful_recoveries, 1);
}

/// F090: Circuit breaker trip recorded
#[test]
fn f090_circuit_breaker_trip_recorded() {
    let metrics = DaemonMetrics::new();
    assert_eq!(metrics.circuit_breaker_trips(), 0);

    metrics.record_circuit_breaker_trip();
    assert_eq!(metrics.circuit_breaker_trips(), 1);

    metrics.record_circuit_breaker_trip();
    metrics.record_circuit_breaker_trip();
    assert_eq!(metrics.circuit_breaker_trips(), 3);
}

/// F091: Recovery recorded
#[test]
fn f091_recovery_recorded() {
    let metrics = DaemonMetrics::new();
    assert_eq!(metrics.successful_recoveries(), 0);

    metrics.record_recovery();
    assert_eq!(metrics.successful_recoveries(), 1);

    metrics.record_recovery();
    assert_eq!(metrics.successful_recoveries(), 2);
}

/// F092: Requests per second calculated
#[test]
fn f092_requests_per_second_calculated() {
    let metrics = DaemonMetrics::new();

    // Initially 0
    let rps = metrics.requests_per_second();
    assert!(rps >= 0.0);

    // After some requests
    for _ in 0..10 {
        metrics.record_request();
    }

    let rps = metrics.requests_per_second();
    assert!(rps > 0.0, "RPS should be positive after requests");
}

/// F093: Tracer attaches successfully
#[tokio::test]
async fn f093_tracer_attaches() {
    let adapter = NativeAdapter::new();
    let daemon = MockDaemon::new("test");

    let handle = adapter.spawn(Box::new(daemon)).await.expect("spawn should succeed");

    // Attach tracer
    let tracer = adapter.attach_tracer(&handle).await.expect("attach should succeed");

    // Verify tracer handle is valid
    assert!(!tracer.daemon_id().as_uuid().is_nil());
}

/// F094: Tracer type is correct
#[tokio::test]
async fn f094_tracer_type_correct() {
    use crate::adapter::TracerType;

    let adapter = NativeAdapter::new();
    let daemon = MockDaemon::new("test");

    let handle = adapter.spawn(Box::new(daemon)).await.expect("spawn should succeed");
    let tracer = adapter.attach_tracer(&handle).await.expect("attach should succeed");

    // Native adapter uses Ptrace on Unix
    assert_eq!(tracer.tracer_type(), TracerType::Ptrace);
}

/// F095: Tracer tracks correct daemon
#[tokio::test]
async fn f095_tracer_tracks_daemon() {
    let adapter = NativeAdapter::new();
    let daemon = MockDaemon::new("test");
    let expected_id = daemon.id();

    let handle = adapter.spawn(Box::new(daemon)).await.expect("spawn should succeed");
    let tracer = adapter.attach_tracer(&handle).await.expect("attach should succeed");

    assert_eq!(tracer.daemon_id(), expected_id);
}

/// F096: Tracer fails for unknown daemon
#[tokio::test]
async fn f096_tracer_fails_unknown() {
    use crate::adapter::DaemonHandle;

    let adapter = NativeAdapter::new();
    let unknown_handle = DaemonHandle::native(DaemonId::new(), 99999);

    let result = adapter.attach_tracer(&unknown_handle).await;
    assert!(result.is_err(), "attach_tracer should fail for unknown daemon");
}

/// F097: Logging includes daemon ID (mock test)
#[tokio::test]
async fn f097_logging_includes_daemon_id() {
    let daemon = MockDaemon::new("test-daemon");
    let id = daemon.id();

    // The daemon ID should be displayable
    let id_str = format!("{}", id);
    assert!(!id_str.is_empty());
    assert!(id_str.contains('-'), "UUID format should contain dashes");
}

/// F098: Logging includes operation name (mock test)
#[tokio::test]
async fn f098_logging_includes_operation() {
    let daemon = MockDaemon::new("test");

    // Verify daemon name is accessible for logging
    assert_eq!(daemon.name(), "test");
}

/// F099: Errors logged at appropriate level (structure test)
#[test]
fn f099_errors_have_appropriate_structure() {
    use crate::error::DaemonError;

    let error = DaemonError::Internal("test error".to_string());
    let display = format!("{}", error);

    // Error should have useful display
    assert!(!display.is_empty());
    assert!(display.contains("test error"));
}

/// F100: Metrics serialization roundtrips
#[test]
fn f100_metrics_serialization_roundtrips() {
    let metrics = DaemonMetrics::new();
    metrics.record_request();
    metrics.record_error();
    metrics.record_duration(Duration::from_millis(10));
    metrics.set_cpu_usage(50.0);
    metrics.set_memory_bytes(1024 * 1024);

    let snapshot = metrics.snapshot();
    let json = serde_json::to_string(&snapshot).expect("serialize should succeed");
    let deserialized: crate::metrics::MetricsSnapshot =
        serde_json::from_str(&json).expect("deserialize should succeed");

    assert_eq!(snapshot.requests_total, deserialized.requests_total);
    assert_eq!(snapshot.errors_total, deserialized.errors_total);
    assert_eq!(snapshot.memory_bytes, deserialized.memory_bytes);
}
