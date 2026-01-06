//! Core daemon trait and context.
//!
//! # Toyota Way: Standardized Work (標準作業)
//! Every daemon follows the same lifecycle contract, enabling
//! predictable behavior across platforms.

use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::config::DaemonConfig;
use crate::error::{DaemonError, Result};
use crate::metrics::DaemonMetrics;
use crate::types::{DaemonId, ExitReason, HealthStatus, Signal};

/// Core daemon abstraction for cross-platform lifecycle management.
///
/// # Implementation Guidelines
///
/// 1. **init**: Validate configuration, allocate resources, open connections.
///    Should be fast (< 100ms for most platforms).
///
/// 2. **run**: Main execution loop. Check `ctx.should_shutdown()` regularly.
///    Handle signals via `ctx.recv_signal()`.
///
/// 3. **shutdown**: Clean up resources, close connections, flush buffers.
///    Must complete within the configured timeout.
///
/// 4. **health_check**: Return current health status. Called periodically
///    by the platform adapter.
///
/// # Example
///
/// ```rust,ignore
/// use duende_core::{Daemon, DaemonConfig, DaemonContext, DaemonId, ExitReason, HealthStatus, DaemonMetrics, DaemonError};
/// use async_trait::async_trait;
/// use std::time::Duration;
///
/// struct MyDaemon {
///     id: DaemonId,
///     metrics: DaemonMetrics,
/// }
///
/// #[async_trait]
/// impl Daemon for MyDaemon {
///     fn id(&self) -> DaemonId { self.id }
///     fn name(&self) -> &str { "my-daemon" }
///
///     async fn init(&mut self, _config: &DaemonConfig) -> Result<(), DaemonError> {
///         Ok(())
///     }
///
///     async fn run(&mut self, ctx: &mut DaemonContext) -> Result<ExitReason, DaemonError> {
///         loop {
///             if ctx.should_shutdown() {
///                 return Ok(ExitReason::Graceful);
///             }
///
///             // Do work...
///             self.metrics.record_request();
///
///             tokio::time::sleep(Duration::from_millis(100)).await;
///         }
///     }
///
///     async fn shutdown(&mut self, _timeout: Duration) -> Result<(), DaemonError> {
///         Ok(())
///     }
///
///     async fn health_check(&self) -> HealthStatus {
///         HealthStatus::healthy(5)
///     }
///
///     fn metrics(&self) -> &DaemonMetrics {
///         &self.metrics
///     }
/// }
/// ```
#[async_trait]
pub trait Daemon: Send + Sync + 'static {
    /// Returns the unique identifier for this daemon instance.
    fn id(&self) -> DaemonId;

    /// Returns the human-readable name of this daemon.
    fn name(&self) -> &str;

    /// Initializes the daemon with the given configuration.
    ///
    /// This method is called once before `run()`. It should:
    /// - Validate configuration
    /// - Allocate resources
    /// - Open connections
    /// - Set up signal handlers
    ///
    /// # Poka-Yoke
    /// Fail fast on misconfiguration. Better to fail here than in `run()`.
    ///
    /// # Errors
    /// Returns an error if initialization fails.
    async fn init(&mut self, config: &DaemonConfig) -> Result<()>;

    /// Main execution loop.
    ///
    /// This method contains the daemon's main logic. It should:
    /// - Process work items
    /// - Handle signals via `ctx.recv_signal()`
    /// - Check `ctx.should_shutdown()` regularly
    /// - Update metrics
    ///
    /// # Heijunka
    /// Level workload processing for predictable behavior.
    ///
    /// # Returns
    /// Returns the reason for exit (graceful, signal, error, etc.)
    ///
    /// # Errors
    /// Returns an error if the daemon encounters a fatal error.
    async fn run(&mut self, ctx: &mut DaemonContext) -> Result<ExitReason>;

    /// Gracefully shuts down the daemon.
    ///
    /// This method is called when the daemon receives a shutdown signal.
    /// It should:
    /// - Stop accepting new work
    /// - Complete in-flight work (if possible within timeout)
    /// - Close connections
    /// - Flush buffers
    /// - Release resources
    ///
    /// # Jidoka
    /// Stop cleanly on signal. Don't corrupt state.
    ///
    /// # Errors
    /// Returns an error if shutdown fails (timeout, resource leak, etc.)
    async fn shutdown(&mut self, timeout: Duration) -> Result<()>;

    /// Performs a health check.
    ///
    /// This method is called periodically by the platform adapter.
    /// It should return quickly (< 1s) and not block.
    ///
    /// # Genchi Genbutsu
    /// Direct observation of daemon health.
    async fn health_check(&self) -> HealthStatus;

    /// Returns the daemon's metrics.
    ///
    /// # Kaizen
    /// Continuous improvement via metrics collection.
    fn metrics(&self) -> &DaemonMetrics;
}

/// Runtime context for a daemon.
///
/// Provides signal handling, shutdown coordination, and access to
/// platform-specific features.
pub struct DaemonContext {
    /// Signal receiver.
    signal_rx: mpsc::Receiver<Signal>,

    /// Shutdown flag.
    shutdown: bool,

    /// Configuration.
    config: DaemonConfig,
}

impl DaemonContext {
    /// Creates a new daemon context.
    #[must_use]
    pub fn new(config: DaemonConfig) -> (Self, DaemonContextHandle) {
        let (signal_tx, signal_rx) = mpsc::channel(16);

        let ctx = Self {
            signal_rx,
            shutdown: false,
            config,
        };

        let handle = DaemonContextHandle { signal_tx };

        (ctx, handle)
    }

    /// Returns true if the daemon should shut down.
    #[must_use]
    pub const fn should_shutdown(&self) -> bool {
        self.shutdown
    }

    /// Marks the daemon for shutdown.
    pub fn request_shutdown(&mut self) {
        self.shutdown = true;
    }

    /// Receives a signal, if available.
    ///
    /// Returns `None` if no signal is available.
    pub fn try_recv_signal(&mut self) -> Option<Signal> {
        match self.signal_rx.try_recv() {
            Ok(signal) => {
                // Auto-set shutdown flag for termination signals
                if matches!(signal, Signal::Term | Signal::Int | Signal::Quit) {
                    self.shutdown = true;
                }
                Some(signal)
            }
            Err(_) => None,
        }
    }

    /// Waits for a signal.
    ///
    /// This is async and will yield until a signal is received.
    pub async fn recv_signal(&mut self) -> Option<Signal> {
        let signal = self.signal_rx.recv().await?;

        // Auto-set shutdown flag for termination signals
        if matches!(signal, Signal::Term | Signal::Int | Signal::Quit) {
            self.shutdown = true;
        }

        Some(signal)
    }

    /// Returns the daemon configuration.
    #[must_use]
    pub const fn config(&self) -> &DaemonConfig {
        &self.config
    }
}

/// Handle for sending signals to a daemon context.
#[derive(Clone, Debug)]
pub struct DaemonContextHandle {
    signal_tx: mpsc::Sender<Signal>,
}

impl DaemonContextHandle {
    /// Sends a signal to the daemon.
    ///
    /// # Errors
    /// Returns an error if the signal cannot be sent (daemon exited).
    pub async fn send_signal(&self, signal: Signal) -> Result<()> {
        self.signal_tx
            .send(signal)
            .await
            .map_err(|_| DaemonError::Signal("daemon context closed".to_string()))
    }

    /// Requests graceful shutdown.
    ///
    /// # Errors
    /// Returns an error if the signal cannot be sent.
    pub async fn shutdown(&self) -> Result<()> {
        self.send_signal(Signal::Term).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DaemonConfig;

    #[tokio::test]
    async fn test_context_creation() {
        let config = DaemonConfig::new("test", "/bin/test");
        let (ctx, _handle) = DaemonContext::new(config);
        assert!(!ctx.should_shutdown());
    }

    #[tokio::test]
    async fn test_shutdown_request() {
        let config = DaemonConfig::new("test", "/bin/test");
        let (mut ctx, _handle) = DaemonContext::new(config);

        ctx.request_shutdown();
        assert!(ctx.should_shutdown());
    }

    #[tokio::test]
    async fn test_signal_sending() {
        let config = DaemonConfig::new("test", "/bin/test");
        let (mut ctx, handle) = DaemonContext::new(config);

        handle.send_signal(Signal::Hup).await.ok();
        let signal = ctx.try_recv_signal();
        assert_eq!(signal, Some(Signal::Hup));
        assert!(!ctx.should_shutdown()); // HUP doesn't trigger shutdown
    }

    #[tokio::test]
    async fn test_term_triggers_shutdown() {
        let config = DaemonConfig::new("test", "/bin/test");
        let (mut ctx, handle) = DaemonContext::new(config);

        handle.send_signal(Signal::Term).await.ok();
        let _ = ctx.try_recv_signal();
        assert!(ctx.should_shutdown()); // TERM triggers shutdown
    }

    #[tokio::test]
    async fn test_handle_shutdown() {
        let config = DaemonConfig::new("test", "/bin/test");
        let (mut ctx, handle) = DaemonContext::new(config);

        handle.shutdown().await.ok();
        let _ = ctx.try_recv_signal();
        assert!(ctx.should_shutdown());
    }

    #[tokio::test]
    async fn test_context_config() {
        let config = DaemonConfig::new("test-daemon", "/bin/test");
        let (ctx, _handle) = DaemonContext::new(config);
        assert_eq!(ctx.config().name, "test-daemon");
    }

    #[tokio::test]
    async fn test_recv_signal_async() {
        let config = DaemonConfig::new("test", "/bin/test");
        let (mut ctx, handle) = DaemonContext::new(config);

        // Spawn a task to send signal after a short delay
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            handle.send_signal(Signal::Usr1).await.ok();
        });

        let signal = ctx.recv_signal().await;
        assert_eq!(signal, Some(Signal::Usr1));
        assert!(!ctx.should_shutdown()); // USR1 doesn't trigger shutdown
    }

    #[tokio::test]
    async fn test_int_triggers_shutdown() {
        let config = DaemonConfig::new("test", "/bin/test");
        let (mut ctx, handle) = DaemonContext::new(config);

        handle.send_signal(Signal::Int).await.ok();
        let _ = ctx.try_recv_signal();
        assert!(ctx.should_shutdown()); // INT triggers shutdown
    }

    #[tokio::test]
    async fn test_quit_triggers_shutdown() {
        let config = DaemonConfig::new("test", "/bin/test");
        let (mut ctx, handle) = DaemonContext::new(config);

        handle.send_signal(Signal::Quit).await.ok();
        let _ = ctx.try_recv_signal();
        assert!(ctx.should_shutdown()); // QUIT triggers shutdown
    }

    #[tokio::test]
    async fn test_try_recv_signal_empty() {
        let config = DaemonConfig::new("test", "/bin/test");
        let (mut ctx, _handle) = DaemonContext::new(config);

        // No signal sent
        assert_eq!(ctx.try_recv_signal(), None);
    }

    #[tokio::test]
    async fn test_send_signal_closed_error() {
        let config = DaemonConfig::new("test", "/bin/test");
        let (ctx, handle) = DaemonContext::new(config);

        // Drop the context to close the receiver
        drop(ctx);

        // Now send should fail
        let result = handle.send_signal(Signal::Term).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_recv_signal_async_term_triggers_shutdown() {
        let config = DaemonConfig::new("test", "/bin/test");
        let (mut ctx, handle) = DaemonContext::new(config);

        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            handle.send_signal(Signal::Term).await.ok();
        });

        let signal = ctx.recv_signal().await;
        assert_eq!(signal, Some(Signal::Term));
        assert!(ctx.should_shutdown()); // TERM triggers shutdown via recv_signal too
    }
}
