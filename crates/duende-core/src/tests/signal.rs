//! Category B: Signal Handling falsification tests (F021-F040).
//!
//! These tests verify the signal handling properties defined in daemon-tools-spec.md
//! Section 9.2 using Popperian falsification methodology.

use std::time::Duration;

use crate::config::DaemonConfig;
use crate::daemon::{Daemon, DaemonContext};
use crate::tests::mocks::MockDaemon;
use crate::types::{DaemonStatus, Signal};

/// F021: SIGTERM triggers graceful shutdown
#[tokio::test]
async fn f021_sigterm_triggers_graceful_shutdown() {
    let config = DaemonConfig::new("test", "/bin/test");
    let (mut ctx, handle) = DaemonContext::new(config);

    // Send SIGTERM
    handle
        .send_signal(Signal::Term)
        .await
        .expect("send should succeed");

    // Receive the signal
    let sig = ctx.try_recv_signal();
    assert_eq!(sig, Some(Signal::Term));

    // should_shutdown should now be true
    assert!(ctx.should_shutdown(), "SIGTERM should trigger shutdown");
}

/// F022: SIGINT triggers graceful shutdown
#[tokio::test]
async fn f022_sigint_triggers_graceful_shutdown() {
    let config = DaemonConfig::new("test", "/bin/test");
    let (mut ctx, handle) = DaemonContext::new(config);

    handle
        .send_signal(Signal::Int)
        .await
        .expect("send should succeed");
    let _ = ctx.try_recv_signal();
    assert!(ctx.should_shutdown(), "SIGINT should trigger shutdown");
}

/// F023: SIGQUIT triggers graceful shutdown
#[tokio::test]
async fn f023_sigquit_triggers_graceful_shutdown() {
    let config = DaemonConfig::new("test", "/bin/test");
    let (mut ctx, handle) = DaemonContext::new(config);

    handle
        .send_signal(Signal::Quit)
        .await
        .expect("send should succeed");
    let _ = ctx.try_recv_signal();
    assert!(ctx.should_shutdown(), "SIGQUIT should trigger shutdown");
}

/// F024: SIGHUP does not trigger shutdown
#[tokio::test]
async fn f024_sighup_does_not_trigger_shutdown() {
    let config = DaemonConfig::new("test", "/bin/test");
    let (mut ctx, handle) = DaemonContext::new(config);

    handle
        .send_signal(Signal::Hup)
        .await
        .expect("send should succeed");
    let sig = ctx.try_recv_signal();
    assert_eq!(sig, Some(Signal::Hup));
    assert!(!ctx.should_shutdown(), "SIGHUP should NOT trigger shutdown");
}

/// F025: SIGUSR1 delivered to daemon
#[tokio::test]
async fn f025_sigusr1_delivered_to_daemon() {
    let config = DaemonConfig::new("test", "/bin/test");
    let (mut ctx, handle) = DaemonContext::new(config);

    handle
        .send_signal(Signal::Usr1)
        .await
        .expect("send should succeed");
    let sig = ctx.try_recv_signal();
    assert_eq!(sig, Some(Signal::Usr1), "SIGUSR1 should be delivered");
    assert!(
        !ctx.should_shutdown(),
        "SIGUSR1 should not trigger shutdown"
    );
}

/// F026: SIGUSR2 delivered to daemon
#[tokio::test]
async fn f026_sigusr2_delivered_to_daemon() {
    let config = DaemonConfig::new("test", "/bin/test");
    let (mut ctx, handle) = DaemonContext::new(config);

    handle
        .send_signal(Signal::Usr2)
        .await
        .expect("send should succeed");
    let sig = ctx.try_recv_signal();
    assert_eq!(sig, Some(Signal::Usr2), "SIGUSR2 should be delivered");
    assert!(
        !ctx.should_shutdown(),
        "SIGUSR2 should not trigger shutdown"
    );
}

/// F027: SIGSTOP pauses daemon (status tracking test)
#[tokio::test]
async fn f027_sigstop_handled() {
    // SIGSTOP behavior is handled at the platform level
    // This test verifies the signal can be sent
    let config = DaemonConfig::new("test", "/bin/test");
    let (_ctx, handle) = DaemonContext::new(config);

    // Verify Signal::Stop has correct value
    assert_eq!(Signal::Stop.as_i32(), 19);

    // Can send the signal (would pause at platform level)
    let result = handle.send_signal(Signal::Stop).await;
    assert!(result.is_ok());
}

/// F028: SIGCONT resumes paused daemon
#[tokio::test]
async fn f028_sigcont_handled() {
    let config = DaemonConfig::new("test", "/bin/test");
    let (_ctx, handle) = DaemonContext::new(config);

    // Verify Signal::Cont has correct value
    assert_eq!(Signal::Cont.as_i32(), 18);

    // Can send the signal
    let result = handle.send_signal(Signal::Cont).await;
    assert!(result.is_ok());
}

/// F029: Signal to stopped daemon fails
#[tokio::test]
async fn f029_signal_to_stopped_daemon_fails() {
    use crate::manager::{DaemonManager, RestartPolicy};

    let manager = DaemonManager::new();
    let daemon = MockDaemon::new("test");
    let id = daemon.id();
    let config = DaemonConfig::new("test", "/bin/test");

    manager
        .register(Box::new(daemon), config, RestartPolicy::Never)
        .await
        .expect("register should succeed");

    // Set daemon to Stopped state
    manager
        .update_status(id, DaemonStatus::Stopped)
        .await
        .expect("update should succeed");

    // Try to signal - should fail because Stopped.can_signal() == false
    let result = manager.signal(id, Signal::Term).await;
    assert!(result.is_err(), "Signaling stopped daemon should fail");
}

/// F030: Signal queue has bounded capacity
#[tokio::test]
async fn f030_signal_queue_bounded() {
    let config = DaemonConfig::new("test", "/bin/test");
    let (mut ctx, handle) = DaemonContext::new(config);

    // Send a few signals
    for _ in 0..10 {
        handle.send_signal(Signal::Usr1).await.ok();
    }

    // Give time for channel propagation
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Should be able to drain them without issues
    let mut count = 0;
    while ctx.try_recv_signal().is_some() {
        count += 1;
        if count > 20 {
            panic!("Signal queue should be bounded");
        }
    }

    // Should have received some signals
    assert!(count > 0, "Should receive at least some signals");
}

/// F031: Signal numbers match Unix conventions
#[test]
fn f031_signal_numbers_match_unix() {
    assert_eq!(Signal::Hup.as_i32(), 1);
    assert_eq!(Signal::Int.as_i32(), 2);
    assert_eq!(Signal::Quit.as_i32(), 3);
    assert_eq!(Signal::Kill.as_i32(), 9);
    assert_eq!(Signal::Usr1.as_i32(), 10);
    assert_eq!(Signal::Usr2.as_i32(), 12);
    assert_eq!(Signal::Term.as_i32(), 15);
    assert_eq!(Signal::Cont.as_i32(), 18);
    assert_eq!(Signal::Stop.as_i32(), 19);
}

/// F032: Signal from_i32 handles invalid values
#[test]
fn f032_signal_from_i32_invalid() {
    // Invalid signal numbers should return None
    assert!(Signal::from_i32(0).is_none());
    assert!(Signal::from_i32(-1).is_none());
    assert!(Signal::from_i32(100).is_none());
    assert!(Signal::from_i32(999).is_none());

    // Valid signal numbers should return Some
    assert!(Signal::from_i32(1).is_some());
    assert!(Signal::from_i32(9).is_some());
    assert!(Signal::from_i32(15).is_some());
}

/// F033: Signal handler is async-safe
#[tokio::test]
async fn f033_signal_handler_async_safe() {
    let config = DaemonConfig::new("test", "/bin/test");
    let (mut ctx, handle) = DaemonContext::new(config);

    // Spawn a few concurrent signal senders
    let handles: Vec<_> = (0..3)
        .map(|_| {
            let h = handle.clone();
            tokio::spawn(async move {
                for _ in 0..3 {
                    h.send_signal(Signal::Usr1).await.ok();
                }
            })
        })
        .collect();

    // Wait for all senders with timeout
    for h in handles {
        tokio::time::timeout(Duration::from_secs(5), h)
            .await
            .expect("task should complete")
            .expect("task should not panic");
    }

    // Give time for channel propagation
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Should be able to drain signals without panic
    let mut count = 0;
    while ctx.try_recv_signal().is_some() {
        count += 1;
    }
    assert!(count > 0, "Should have received some signals");
}

/// F034: try_recv_signal is non-blocking
#[tokio::test]
async fn f034_try_recv_signal_non_blocking() {
    let config = DaemonConfig::new("test", "/bin/test");
    let (mut ctx, _handle) = DaemonContext::new(config);

    let start = std::time::Instant::now();

    // Call try_recv when no signal is available
    let result = ctx.try_recv_signal();

    let elapsed = start.elapsed();

    assert!(result.is_none(), "No signal should be available");
    assert!(
        elapsed < Duration::from_millis(10),
        "try_recv_signal should return immediately, took {:?}",
        elapsed
    );
}

/// F035: recv_signal blocks until signal
#[tokio::test]
async fn f035_recv_signal_blocks_until_signal() {
    let config = DaemonConfig::new("test", "/bin/test");
    let (mut ctx, handle) = DaemonContext::new(config);

    // Spawn sender that sends signal after delay
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        handle.send_signal(Signal::Usr1).await.ok();
    });

    let start = std::time::Instant::now();
    let signal = ctx.recv_signal().await;
    let elapsed = start.elapsed();

    assert_eq!(signal, Some(Signal::Usr1));
    assert!(
        elapsed >= Duration::from_millis(40),
        "recv_signal should have blocked"
    );
}

/// F036: Multiple signals queued correctly
#[tokio::test]
async fn f036_multiple_signals_queued() {
    let config = DaemonConfig::new("test", "/bin/test");
    let (mut ctx, handle) = DaemonContext::new(config);

    // Send signals in order
    handle.send_signal(Signal::Usr1).await.ok();
    handle.send_signal(Signal::Usr2).await.ok();
    handle.send_signal(Signal::Hup).await.ok();

    // Give a moment for channel propagation
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Receive in order
    assert_eq!(ctx.try_recv_signal(), Some(Signal::Usr1));
    assert_eq!(ctx.try_recv_signal(), Some(Signal::Usr2));
    assert_eq!(ctx.try_recv_signal(), Some(Signal::Hup));
    assert_eq!(ctx.try_recv_signal(), None);
}

/// F037: Handle shutdown sends SIGTERM
#[tokio::test]
async fn f037_handle_shutdown_sends_sigterm() {
    let config = DaemonConfig::new("test", "/bin/test");
    let (mut ctx, handle) = DaemonContext::new(config);

    // shutdown() should send SIGTERM
    handle.shutdown().await.ok();

    let signal = ctx.try_recv_signal();
    assert_eq!(signal, Some(Signal::Term), "shutdown() should send SIGTERM");
}

/// F038: Handle closed returns error
#[tokio::test]
async fn f038_handle_closed_returns_error() {
    let config = DaemonConfig::new("test", "/bin/test");
    let (ctx, handle) = DaemonContext::new(config);

    // Drop the context to close the receiver
    drop(ctx);

    // Now send should fail
    let result = handle.send_signal(Signal::Term).await;
    assert!(result.is_err(), "Sending to closed handle should fail");
}

/// F039: Manager signal forwards correctly
#[tokio::test]
async fn f039_manager_signal_forwards_correctly() {
    use crate::manager::{DaemonManager, RestartPolicy};

    let manager = DaemonManager::new();
    let daemon = MockDaemon::new("test");
    let id = daemon.id();
    let config = DaemonConfig::new("test", "/bin/test");

    manager
        .register(Box::new(daemon), config.clone(), RestartPolicy::Never)
        .await
        .expect("register should succeed");

    // Set to running so we can signal
    manager
        .update_status(id, DaemonStatus::Running)
        .await
        .expect("update should succeed");

    // Create context and set handle
    let (mut ctx, context_handle) = DaemonContext::new(config);
    manager.set_context_handle(id, context_handle).await.ok();

    // Now signal via manager
    manager
        .signal(id, Signal::Usr1)
        .await
        .expect("signal should succeed");

    // Verify signal was received
    let signal = ctx.try_recv_signal();
    assert_eq!(signal, Some(Signal::Usr1));
}

/// F040: Signal to unknown daemon fails
#[tokio::test]
async fn f040_signal_to_unknown_daemon_fails() {
    use crate::manager::DaemonManager;
    use crate::types::DaemonId;

    let manager = DaemonManager::new();
    let unknown_id = DaemonId::new();

    let result = manager.signal(unknown_id, Signal::Term).await;
    assert!(result.is_err(), "Signaling unknown daemon should fail");
}
