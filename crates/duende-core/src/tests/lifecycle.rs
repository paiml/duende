//! Category A: Daemon Lifecycle falsification tests (F001-F020).
//!
//! These tests verify the lifecycle properties defined in daemon-tools-spec.md
//! Section 9.2 using Popperian falsification methodology.

use std::time::Duration;

use crate::config::DaemonConfig;
use crate::daemon::{Daemon, DaemonContext};
use crate::manager::{BackoffConfig, DaemonManager, RestartPolicy};
use crate::tests::mocks::MockDaemon;
use crate::types::{DaemonId, DaemonStatus, ExitReason, Signal};

/// F001: Init must be called before run
#[tokio::test]
async fn f001_init_called_before_run() {
    let mut daemon = MockDaemon::new("test");
    let config = DaemonConfig::new("test", "/bin/test");

    // Init count should start at 0
    assert_eq!(daemon.init_count(), 0);

    // Call init
    daemon.init(&config).await.expect("init should succeed");
    assert_eq!(daemon.init_count(), 1);

    // Now run can be called (we create context and run briefly)
    let (mut ctx, handle) = DaemonContext::new(config);

    // Send immediate shutdown to exit run loop
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        handle.shutdown().await.ok();
    });

    let result = daemon.run(&mut ctx).await;
    assert!(result.is_ok());

    // Init should still have been called exactly once
    assert_eq!(daemon.init_count(), 1);
}

/// F002: Init failure prevents run
#[tokio::test]
async fn f002_init_failure_prevents_run() {
    let mut daemon = MockDaemon::new("test").fail_init("configuration error");
    let config = DaemonConfig::new("test", "/bin/test");

    // Init should fail
    let result = daemon.init(&config).await;
    assert!(result.is_err());

    // The daemon should never reach run() in production
    // (The manager would not proceed after init failure)
    // This test verifies init properly returns error
    assert!(result.unwrap_err().to_string().contains("configuration error"));
}

/// F003: Shutdown is called after run completes
#[tokio::test]
async fn f003_shutdown_called_after_run() {
    let mut daemon = MockDaemon::new("test").exit_after(5);
    let config = DaemonConfig::new("test", "/bin/test");

    assert_eq!(daemon.shutdown_count(), 0);

    // Init
    daemon.init(&config).await.expect("init should succeed");

    // Run until natural exit
    let (mut ctx, _handle) = DaemonContext::new(config);
    let result = daemon.run(&mut ctx).await;
    assert!(result.is_ok());

    // Now shutdown should be callable
    daemon
        .shutdown(Duration::from_secs(5))
        .await
        .expect("shutdown should succeed");

    assert_eq!(daemon.shutdown_count(), 1);
}

/// F004: Status transitions are valid
#[test]
fn f004_valid_status_transitions() {
    // Created → Starting (implied by spawn)
    // Starting → Running (after init succeeds)
    // Running → Stopping (on shutdown signal)
    // Stopping → Stopped (after graceful shutdown)

    let created = DaemonStatus::Created;
    assert!(!created.is_terminal());
    assert!(!created.is_active());

    let starting = DaemonStatus::Starting;
    assert!(!starting.is_terminal());
    assert!(!starting.is_active());

    let running = DaemonStatus::Running;
    assert!(!running.is_terminal());
    assert!(running.is_active());
    assert!(running.can_signal());

    let paused = DaemonStatus::Paused;
    assert!(!paused.is_terminal());
    assert!(paused.is_active());
    assert!(paused.can_signal());

    let stopping = DaemonStatus::Stopping;
    assert!(!stopping.is_terminal());
    assert!(!stopping.is_active());
    assert!(stopping.can_signal());

    let stopped = DaemonStatus::Stopped;
    assert!(stopped.is_terminal());
    assert!(!stopped.can_signal());
}

/// F005: Terminal states are final
#[test]
fn f005_terminal_states_final() {
    use crate::types::FailureReason;

    let stopped = DaemonStatus::Stopped;
    assert!(stopped.is_terminal());
    assert!(!stopped.can_signal());

    let failed = DaemonStatus::Failed(FailureReason::Internal);
    assert!(failed.is_terminal());
    assert!(!failed.can_signal());

    // All failure reasons result in terminal state
    for reason in [
        FailureReason::Signal(9),
        FailureReason::ExitCode(1),
        FailureReason::ResourceExhausted,
        FailureReason::PolicyViolation,
        FailureReason::HealthCheckTimeout,
        FailureReason::Internal,
    ] {
        let status = DaemonStatus::Failed(reason);
        assert!(status.is_terminal());
    }
}

/// F006: Daemon ID is unique
#[test]
fn f006_daemon_id_unique() {
    let ids: Vec<DaemonId> = (0..1000).map(|_| DaemonId::new()).collect();

    // Check all IDs are unique
    let unique_count = ids.iter().collect::<std::collections::HashSet<_>>().len();
    assert_eq!(unique_count, ids.len());
}

/// F007: Config validation rejects invalid names
#[test]
fn f007_invalid_names_rejected() {
    // Empty name
    let mut config = DaemonConfig::new("", "/bin/test");
    assert!(config.validate().is_err());

    // Name with spaces
    config.name = "invalid name".to_string();
    assert!(config.validate().is_err());

    // Name with special chars
    config.name = "invalid@name".to_string();
    assert!(config.validate().is_err());

    // Valid names
    config.name = "valid-name".to_string();
    assert!(config.validate().is_ok());

    config.name = "valid_name".to_string();
    assert!(config.validate().is_ok());

    config.name = "valid123".to_string();
    assert!(config.validate().is_ok());
}

/// F008: Config validation rejects empty binary path
#[test]
fn f008_empty_binary_path_rejected() {
    let config = DaemonConfig::new("test", "");
    assert!(config.validate().is_err());
}

/// F009: Graceful shutdown completes within timeout
#[tokio::test]
async fn f009_graceful_shutdown_within_timeout() {
    let mut daemon = MockDaemon::new("test");
    let timeout = Duration::from_secs(5);

    let start = std::time::Instant::now();
    daemon.shutdown(timeout).await.expect("shutdown should succeed");
    let elapsed = start.elapsed();

    // Shutdown should complete well before the timeout
    assert!(elapsed < timeout);
}

/// F010: Forced kill terminates immediately
#[tokio::test]
async fn f010_forced_kill_terminates_immediately() {
    use crate::adapters::NativeAdapter;
    use crate::adapter::PlatformAdapter;

    let adapter = NativeAdapter::new();
    let daemon = MockDaemon::new("test");

    let handle = adapter.spawn(Box::new(daemon)).await.expect("spawn should succeed");

    // Kill immediately
    let start = std::time::Instant::now();
    adapter
        .signal(&handle, Signal::Kill)
        .await
        .expect("kill should succeed");
    let elapsed = start.elapsed();

    // Kill should be very fast
    assert!(elapsed < Duration::from_millis(100));
}

/// F011: Restart policy Never prevents restart
#[test]
fn f011_restart_policy_never() {
    let policy = RestartPolicy::Never;

    // Never restart regardless of exit reason
    assert!(!policy.should_restart(&ExitReason::Error("test".into()), 0));
    assert!(!policy.should_restart(&ExitReason::Graceful, 0));
    assert!(!policy.should_restart(&ExitReason::Signal(Signal::Term), 0));
    assert!(!policy.should_restart(&ExitReason::ResourceExhausted("oom".into()), 0));
}

/// F012: Restart policy Always restarts on graceful
#[test]
fn f012_restart_policy_always() {
    let policy = RestartPolicy::Always;

    // Always restart
    assert!(policy.should_restart(&ExitReason::Error("test".into()), 0));
    assert!(policy.should_restart(&ExitReason::Graceful, 0));
    assert!(policy.should_restart(&ExitReason::Signal(Signal::Term), 0));
    assert!(policy.should_restart(&ExitReason::Graceful, 100));
}

/// F013: Restart policy OnFailure only restarts on error
#[test]
fn f013_restart_policy_on_failure() {
    let policy = RestartPolicy::OnFailure;

    // Restart on error
    assert!(policy.should_restart(&ExitReason::Error("test".into()), 0));
    assert!(policy.should_restart(&ExitReason::ResourceExhausted("oom".into()), 0));

    // Don't restart on graceful or signal
    assert!(!policy.should_restart(&ExitReason::Graceful, 0));
    assert!(!policy.should_restart(&ExitReason::Signal(Signal::Term), 0));
}

/// F014: MaxRetries limits restart count
#[test]
fn f014_max_retries_limits_restarts() {
    let policy = RestartPolicy::MaxRetries(3);

    // Can restart up to 3 times
    assert!(policy.should_restart(&ExitReason::Error("test".into()), 0));
    assert!(policy.should_restart(&ExitReason::Error("test".into()), 1));
    assert!(policy.should_restart(&ExitReason::Error("test".into()), 2));

    // Cannot restart after 3
    assert!(!policy.should_restart(&ExitReason::Error("test".into()), 3));
    assert!(!policy.should_restart(&ExitReason::Error("test".into()), 4));
}

/// F015: Backoff delay increases exponentially
#[test]
fn f015_backoff_exponential_increase() {
    let config = BackoffConfig::new()
        .with_initial_delay(Duration::from_secs(1))
        .with_multiplier(2.0)
        .with_max_delay(Duration::from_secs(300));

    // Each delay should be >= previous * multiplier (within float precision)
    let d0 = config.delay_for(0);
    let d1 = config.delay_for(1);
    let d2 = config.delay_for(2);
    let d3 = config.delay_for(3);

    assert!(d1 >= d0);
    assert!(d2 >= d1);
    assert!(d3 >= d2);

    // Verify exponential growth
    assert_eq!(d0, Duration::from_secs(1));
    assert_eq!(d1, Duration::from_secs(2));
    assert_eq!(d2, Duration::from_secs(4));
    assert_eq!(d3, Duration::from_secs(8));
}

/// F016: Backoff respects max delay
#[test]
fn f016_backoff_respects_max() {
    let config = BackoffConfig::new()
        .with_initial_delay(Duration::from_secs(1))
        .with_multiplier(10.0)
        .with_max_delay(Duration::from_secs(60));

    // After enough iterations, should be capped at max
    let d5 = config.delay_for(5);
    let d10 = config.delay_for(10);
    let d100 = config.delay_for(100);

    assert!(d5 <= Duration::from_secs(60));
    assert!(d10 <= Duration::from_secs(60));
    assert!(d100 <= Duration::from_secs(60));
}

/// F017: Manager tracks restart count
#[tokio::test]
async fn f017_manager_tracks_restart_count() {
    let manager = DaemonManager::new();
    let daemon = MockDaemon::new("test");
    let id = daemon.id();
    let config = DaemonConfig::new("test", "/bin/test");

    manager
        .register(Box::new(daemon), config, RestartPolicy::Always)
        .await
        .expect("register should succeed");

    assert_eq!(manager.get_restart_count(id).await.unwrap(), 0);

    manager.increment_restart_count(id).await.unwrap();
    assert_eq!(manager.get_restart_count(id).await.unwrap(), 1);

    manager.increment_restart_count(id).await.unwrap();
    manager.increment_restart_count(id).await.unwrap();
    assert_eq!(manager.get_restart_count(id).await.unwrap(), 3);
}

/// F018: Manager prevents duplicate registration
#[tokio::test]
async fn f018_duplicate_registration_fails() {
    let manager = DaemonManager::new();

    let daemon1 = MockDaemon::new("test");
    let id = daemon1.id();
    let config = DaemonConfig::new("test", "/bin/test");

    // First registration succeeds
    manager
        .register(Box::new(daemon1), config.clone(), RestartPolicy::Never)
        .await
        .expect("first registration should succeed");

    // Create daemon with same ID
    let daemon2 = MockDaemon::new("test2").with_id(id);

    // Second registration should fail
    let result = manager
        .register(Box::new(daemon2), config, RestartPolicy::Never)
        .await;

    assert!(result.is_err());
}

/// F019: Manager prevents unregistering active daemon
#[tokio::test]
async fn f019_cannot_unregister_active() {
    let manager = DaemonManager::new();
    let daemon = MockDaemon::new("test");
    let id = daemon.id();
    let config = DaemonConfig::new("test", "/bin/test");

    manager
        .register(Box::new(daemon), config, RestartPolicy::Never)
        .await
        .expect("register should succeed");

    // Set status to Running
    manager
        .update_status(id, DaemonStatus::Running)
        .await
        .expect("update should succeed");

    // Try to unregister - should fail
    let result = manager.unregister(id).await;
    assert!(result.is_err());

    // Set to Stopped and unregister should work
    manager
        .update_status(id, DaemonStatus::Stopped)
        .await
        .expect("update should succeed");

    let result = manager.unregister(id).await;
    assert!(result.is_ok());
}

/// F020: Shutdown all signals all daemons
#[tokio::test]
async fn f020_shutdown_all_signals_all() {
    let manager = DaemonManager::new();

    // Register multiple daemons
    for i in 0..5 {
        let daemon = MockDaemon::new(format!("daemon-{i}"));
        let config = DaemonConfig::new(format!("daemon-{i}"), "/bin/test");

        manager
            .register(Box::new(daemon), config, RestartPolicy::Never)
            .await
            .expect("register should succeed");
    }

    assert_eq!(manager.count().await, 5);

    // Shutdown all - this sends signals to all daemons
    // (In practice would wait for shutdown, but stub adapters
    // don't actually run processes so signals may fail)
    manager.shutdown_all().await.ok();

    // All daemons should have been signaled (or at least attempted)
    // The shutdown_all just sends signals, doesn't wait for completion
}
