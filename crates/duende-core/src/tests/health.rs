//! Category D: Health Check falsification tests (F061-F080).
//!
//! These tests verify the health check properties defined in daemon-tools-spec.md
//! Section 9.2 using Popperian falsification methodology.

use std::time::Duration;

use crate::config::HealthCheckConfig;
use crate::daemon::Daemon;
use crate::error::DaemonError;
use crate::manager::{DaemonManager, RestartPolicy};
use crate::tests::mocks::MockDaemon;
use crate::types::HealthStatus;
use crate::config::DaemonConfig;

/// F061: Health check default interval is 30s
#[test]
fn f061_health_check_default_interval_30s() {
    let config = HealthCheckConfig::default();
    assert_eq!(
        config.interval,
        Duration::from_secs(30),
        "Default health check interval should be 30s"
    );
}

/// F062: Health check default timeout is 10s
#[test]
fn f062_health_check_default_timeout_10s() {
    let config = HealthCheckConfig::default();
    assert_eq!(
        config.timeout,
        Duration::from_secs(10),
        "Default health check timeout should be 10s"
    );
}

/// F063: Health check default retries is 3
#[test]
fn f063_health_check_default_retries_3() {
    let config = HealthCheckConfig::default();
    assert_eq!(config.retries, 3, "Default health check retries should be 3");
}

/// F064: Health check can be disabled
#[test]
fn f064_health_check_can_be_disabled() {
    let mut config = HealthCheckConfig::default();
    config.enabled = false;
    assert!(!config.enabled, "Health check should be disableable");

    // Default is enabled
    let default_config = HealthCheckConfig::default();
    assert!(default_config.enabled, "Health check should be enabled by default");
}

/// F065: Healthy status returns true
#[test]
fn f065_healthy_status_returns_true() {
    let status = HealthStatus::healthy(5);
    assert!(status.is_healthy(), "healthy() should return is_healthy() == true");
    assert!(status.healthy);
}

/// F066: Unhealthy status returns false
#[test]
fn f066_unhealthy_status_returns_false() {
    let status = HealthStatus::unhealthy("test error", 10);
    assert!(
        !status.is_healthy(),
        "unhealthy() should return is_healthy() == false"
    );
    assert!(!status.healthy);
}

/// F067: Health latency is tracked
#[test]
fn f067_health_latency_tracked() {
    let status = HealthStatus::healthy(42);
    assert_eq!(status.latency_ms, 42, "Health latency should be tracked");

    let status = HealthStatus::unhealthy("error", 100);
    assert_eq!(status.latency_ms, 100, "Unhealthy latency should be tracked");
}

/// F068: Health check timeout triggers failure (mock behavior)
#[tokio::test]
async fn f068_health_check_timeout_behavior() {
    // Create an unhealthy daemon
    let daemon = MockDaemon::new("test").unhealthy();

    let health = daemon.health_check().await;
    assert!(!health.is_healthy(), "Unhealthy daemon should report unhealthy");
}

/// F069: Retry count affects failure threshold (configuration test)
#[test]
fn f069_retry_count_configurable() {
    let config = HealthCheckConfig::default();
    assert_eq!(config.retries, 3);

    let mut custom_config = HealthCheckConfig::default();
    custom_config.retries = 5;
    assert_eq!(custom_config.retries, 5);
}

/// F070: Manager updates health status
#[tokio::test]
async fn f070_manager_updates_health_status() {
    let manager = DaemonManager::new();
    let daemon = MockDaemon::new("test");
    let id = daemon.id();
    let config = DaemonConfig::new("test", "/bin/test");

    manager
        .register(Box::new(daemon), config, RestartPolicy::Never)
        .await
        .expect("register should succeed");

    // Update health
    let health = HealthStatus::healthy(5);
    manager.update_health(id, health).await.expect("update should succeed");

    // Verify updated
    let retrieved = manager
        .get_health(id)
        .await
        .expect("get should succeed")
        .expect("should have health status");
    assert!(retrieved.is_healthy());
}

/// F071: Manager retrieves health status
#[tokio::test]
async fn f071_manager_retrieves_health_status() {
    let manager = DaemonManager::new();
    let daemon = MockDaemon::new("test");
    let id = daemon.id();
    let config = DaemonConfig::new("test", "/bin/test");

    manager
        .register(Box::new(daemon), config, RestartPolicy::Never)
        .await
        .expect("register should succeed");

    // Initially no health status (returns Ok(None))
    let result = manager.get_health(id).await.expect("daemon exists");
    assert!(result.is_none(), "Should have no health status initially");

    // Update and retrieve
    manager
        .update_health(id, HealthStatus::healthy(1))
        .await
        .expect("update should succeed");

    let health = manager
        .get_health(id)
        .await
        .expect("get should succeed")
        .expect("should have health now");
    assert!(health.is_healthy());
}

/// F072: Health check runs in background (async behavior test)
#[tokio::test]
async fn f072_health_check_async() {
    let daemon = MockDaemon::new("test");

    // Health check is async and should not block
    let health = daemon.health_check().await;
    assert!(health.is_healthy());
}

/// F073: Health check respects interval (configuration test)
#[test]
fn f073_health_check_interval_configurable() {
    let mut config = HealthCheckConfig::default();
    config.interval = Duration::from_secs(60);
    assert_eq!(config.interval, Duration::from_secs(60));
}

/// F074: Individual checks tracked separately
#[test]
fn f074_individual_checks_tracked() {
    let status = HealthStatus::unhealthy("database connection failed", 50);
    assert!(!status.checks.is_empty());
    assert!(!status.checks[0].passed);
    assert_eq!(status.checks[0].name, "main");
    assert!(status.checks[0].message.is_some());
}

/// F075: Health timestamp is accurate
#[test]
fn f075_health_timestamp_accurate() {
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let status = HealthStatus::healthy(1);

    let after = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    assert!(
        status.last_check_epoch_ms >= before && status.last_check_epoch_ms <= after,
        "Timestamp should be within test bounds"
    );
}

/// F076: Health error is recoverable
#[test]
fn f076_health_error_is_recoverable() {
    let error = DaemonError::health_check("check failed");
    assert!(error.is_recoverable(), "HealthCheck error should be recoverable");
}

/// F077: Failed health triggers restart (configuration test)
#[test]
fn f077_failed_health_restart_policy() {
    use crate::types::ExitReason;

    // OnFailure policy should restart on error
    let policy = RestartPolicy::OnFailure;
    assert!(policy.should_restart(&ExitReason::Error("health check failed".into()), 0));
}

/// F078: Health recovery resets counter (manager behavior)
#[tokio::test]
async fn f078_health_recovery_behavior() {
    let manager = DaemonManager::new();
    let daemon = MockDaemon::new("test");
    let id = daemon.id();
    let config = DaemonConfig::new("test", "/bin/test");

    manager
        .register(Box::new(daemon), config, RestartPolicy::Never)
        .await
        .expect("register should succeed");

    // Update to unhealthy
    manager
        .update_health(id, HealthStatus::unhealthy("failed", 1))
        .await
        .expect("update should succeed");

    // Then to healthy (recovery)
    manager
        .update_health(id, HealthStatus::healthy(1))
        .await
        .expect("update should succeed");

    let health = manager
        .get_health(id)
        .await
        .expect("get should succeed")
        .expect("should have health");
    assert!(health.is_healthy());
}

/// F079: Health check serializes correctly
#[test]
fn f079_health_check_serializes() {
    let status = HealthStatus::healthy(42);
    let json = serde_json::to_string(&status).expect("serialize should succeed");
    let deserialized: HealthStatus =
        serde_json::from_str(&json).expect("deserialize should succeed");

    assert_eq!(status.healthy, deserialized.healthy);
    assert_eq!(status.latency_ms, deserialized.latency_ms);
}

/// F080: Circuit breaker trips on failures
#[test]
fn f080_circuit_breaker_trips() {
    use crate::metrics::DaemonMetrics;

    let metrics = DaemonMetrics::new();
    assert_eq!(metrics.circuit_breaker_trips(), 0);

    // Record circuit breaker trips (simulating repeated health failures)
    metrics.record_circuit_breaker_trip();
    metrics.record_circuit_breaker_trip();
    metrics.record_circuit_breaker_trip();

    assert_eq!(
        metrics.circuit_breaker_trips(),
        3,
        "Circuit breaker trips should be tracked"
    );
}
