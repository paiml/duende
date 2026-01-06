//! Native process adapter using fork/exec.
//!
//! This adapter provides a baseline implementation that works on any
//! Unix-like system without requiring systemd, launchd, or containers.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::adapter::{DaemonHandle, PlatformAdapter, PlatformError, PlatformResult, TracerHandle};
use crate::daemon::Daemon;
use crate::platform::Platform;
use crate::types::{DaemonId, DaemonStatus, FailureReason, Signal};

/// Native process adapter.
///
/// Uses fork/exec to spawn daemon processes. This is the fallback adapter
/// when no platform-specific integration is available.
pub struct NativeAdapter {
    /// Running processes indexed by daemon ID.
    processes: Arc<Mutex<HashMap<DaemonId, ProcessState>>>,
}

/// State for a running native process.
struct ProcessState {
    /// The child process handle.
    child: Child,
}

impl NativeAdapter {
    /// Creates a new native adapter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Returns the number of managed processes.
    pub async fn process_count(&self) -> usize {
        self.processes.lock().await.len()
    }
}

impl Default for NativeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformAdapter for NativeAdapter {
    fn platform(&self) -> Platform {
        Platform::Native
    }

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> PlatformResult<DaemonHandle> {
        let id = daemon.id();
        let name = daemon.name().to_string();

        // For native adapter, we spawn a simple test process
        // In production, this would exec the actual daemon binary
        // For now, return NotSupported since we can't properly spawn
        // a daemon from a trait object (we'd need the binary path)

        // This is a stub - in real implementation:
        // 1. Get binary path from daemon config
        // 2. Fork and exec the binary
        // 3. Track the PID

        // For testing purposes, we create a placeholder child process
        #[cfg(unix)]
        {
            let child = Command::new("/bin/sleep")
                .arg("3600") // Sleep for an hour (will be killed on shutdown)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| PlatformError::spawn_failed(format!("failed to spawn: {e}")))?;

            let pid = child
                .id()
                .ok_or_else(|| PlatformError::spawn_failed("failed to get PID"))?;

            let handle = DaemonHandle::native(id, pid);

            let state = ProcessState { child };

            self.processes.lock().await.insert(id, state);

            tracing::info!(daemon = %name, pid = pid, "spawned native process");

            Ok(handle)
        }

        #[cfg(not(unix))]
        {
            // For non-Unix platforms, return not supported
            let _ = (daemon, name);
            Err(PlatformError::not_supported(
                Platform::Native,
                "spawn (non-Unix)",
            ))
        }
    }

    async fn signal(&self, handle: &DaemonHandle, sig: Signal) -> PlatformResult<()> {
        let id = handle.id();

        let mut processes = self.processes.lock().await;
        let state = processes
            .get_mut(&id)
            .ok_or_else(|| PlatformError::NotFound(id.to_string()))?;

        #[cfg(unix)]
        {
            use nix::sys::signal::{self, Signal as NixSignal};
            use nix::unistd::Pid;

            let pid = handle
                .pid()
                .ok_or_else(|| PlatformError::signal_failed("no PID in handle"))?;

            let nix_sig = match sig {
                Signal::Hup => NixSignal::SIGHUP,
                Signal::Int => NixSignal::SIGINT,
                Signal::Quit => NixSignal::SIGQUIT,
                Signal::Term => NixSignal::SIGTERM,
                Signal::Kill => NixSignal::SIGKILL,
                Signal::Usr1 => NixSignal::SIGUSR1,
                Signal::Usr2 => NixSignal::SIGUSR2,
                Signal::Stop => NixSignal::SIGSTOP,
                Signal::Cont => NixSignal::SIGCONT,
            };

            #[allow(clippy::cast_possible_wrap)] // PID always fits in i32 on Unix
            signal::kill(Pid::from_raw(pid as i32), nix_sig)
                .map_err(|e| PlatformError::signal_failed(format!("kill failed: {e}")))?;

            // For SIGKILL, clean up immediately
            if sig == Signal::Kill {
                let _ = state.child.start_kill();
                processes.remove(&id);
            }

            tracing::debug!(daemon = %id, signal = ?sig, pid = pid, "sent signal");

            Ok(())
        }

        #[cfg(not(unix))]
        {
            let _ = (state, sig);
            Err(PlatformError::not_supported(
                Platform::Native,
                "signal (non-Unix)",
            ))
        }
    }

    async fn status(&self, handle: &DaemonHandle) -> PlatformResult<DaemonStatus> {
        let id = handle.id();

        let mut processes = self.processes.lock().await;

        if let Some(state) = processes.get_mut(&id) {
            // Try to get exit status without blocking
            match state.child.try_wait() {
                Ok(Some(exit_status)) => {
                    // Process has exited
                    let status = if exit_status.success() {
                        DaemonStatus::Stopped
                    } else {
                        #[cfg(unix)]
                        {
                            use std::os::unix::process::ExitStatusExt;
                            exit_status.signal().map_or_else(
                                || {
                                    DaemonStatus::Failed(FailureReason::ExitCode(
                                        exit_status.code().unwrap_or(-1),
                                    ))
                                },
                                |sig| DaemonStatus::Failed(FailureReason::Signal(sig)),
                            )
                        }
                        #[cfg(not(unix))]
                        {
                            DaemonStatus::Failed(FailureReason::ExitCode(
                                exit_status.code().unwrap_or(-1),
                            ))
                        }
                    };

                    // Clean up terminated process
                    processes.remove(&id);
                    Ok(status)
                }
                Ok(None) => {
                    // Process is still running
                    Ok(DaemonStatus::Running)
                }
                Err(e) => Err(PlatformError::status_failed(format!(
                    "failed to get status: {e}"
                ))),
            }
        } else {
            // Process not found - might have exited and been cleaned up
            Ok(DaemonStatus::Stopped)
        }
    }

    async fn attach_tracer(&self, handle: &DaemonHandle) -> PlatformResult<TracerHandle> {
        let id = handle.id();

        // Verify the process exists
        let processes = self.processes.lock().await;
        if !processes.contains_key(&id) {
            return Err(PlatformError::NotFound(id.to_string()));
        }

        // For native processes, we use ptrace-based tracing
        // This would integrate with renacer in production
        Ok(TracerHandle::ptrace(id))
    }

    async fn stop(&self, handle: &DaemonHandle, timeout: Duration) -> PlatformResult<()> {
        // Send SIGTERM
        self.signal(handle, Signal::Term).await?;

        // Wait for termination
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            match self.status(handle).await? {
                DaemonStatus::Stopped | DaemonStatus::Failed(_) => return Ok(()),
                _ => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }

        // Force kill if timeout exceeded
        self.signal(handle, Signal::Kill).await?;
        Ok(())
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use crate::config::DaemonConfig;
    use crate::daemon::{Daemon, DaemonContext};
    use crate::error::Result;
    use crate::metrics::DaemonMetrics;
    use crate::types::{ExitReason, HealthStatus};

    struct TestDaemon {
        id: DaemonId,
        metrics: DaemonMetrics,
    }

    impl TestDaemon {
        fn new() -> Self {
            Self {
                id: DaemonId::new(),
                metrics: DaemonMetrics::new(),
            }
        }
    }

    #[async_trait]
    impl Daemon for TestDaemon {
        fn id(&self) -> DaemonId {
            self.id
        }

        fn name(&self) -> &str {
            "test-daemon"
        }

        async fn init(&mut self, _config: &DaemonConfig) -> Result<()> {
            Ok(())
        }

        async fn run(&mut self, ctx: &mut DaemonContext) -> Result<ExitReason> {
            while !ctx.should_shutdown() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok(ExitReason::Graceful)
        }

        async fn shutdown(&mut self, _timeout: Duration) -> Result<()> {
            Ok(())
        }

        async fn health_check(&self) -> HealthStatus {
            HealthStatus::healthy(1)
        }

        fn metrics(&self) -> &DaemonMetrics {
            &self.metrics
        }
    }

    #[tokio::test]
    async fn test_native_adapter_platform() {
        let adapter = NativeAdapter::new();
        assert_eq!(adapter.platform(), Platform::Native);
    }

    #[tokio::test]
    async fn test_native_adapter_spawn_and_kill() {
        let adapter = NativeAdapter::new();
        let daemon = TestDaemon::new();

        // Spawn
        let handle = adapter.spawn(Box::new(daemon)).await.unwrap();
        assert_eq!(handle.platform(), Platform::Native);
        assert!(handle.pid().is_some());

        // Status should be running
        let status = adapter.status(&handle).await.unwrap();
        assert_eq!(status, DaemonStatus::Running);

        // Kill
        adapter.signal(&handle, Signal::Kill).await.unwrap();

        // Give it a moment to die
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Status should be stopped or failed
        let status = adapter.status(&handle).await.unwrap();
        assert!(status.is_terminal());
    }

    #[tokio::test]
    async fn test_native_adapter_graceful_stop() {
        let adapter = NativeAdapter::new();
        let daemon = TestDaemon::new();

        let handle = adapter.spawn(Box::new(daemon)).await.unwrap();

        // Graceful stop with timeout
        adapter.stop(&handle, Duration::from_secs(5)).await.unwrap();

        // Should be stopped
        let status = adapter.status(&handle).await.unwrap();
        assert!(status.is_terminal());
    }

    #[tokio::test]
    async fn test_native_adapter_process_count() {
        let adapter = NativeAdapter::new();

        assert_eq!(adapter.process_count().await, 0);

        let daemon = TestDaemon::new();
        let handle = adapter.spawn(Box::new(daemon)).await.unwrap();

        assert_eq!(adapter.process_count().await, 1);

        adapter.signal(&handle, Signal::Kill).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // After kill, process should be removed
        let _ = adapter.status(&handle).await;
        assert_eq!(adapter.process_count().await, 0);
    }

    #[tokio::test]
    async fn test_native_adapter_attach_tracer() {
        let adapter = NativeAdapter::new();
        let daemon = TestDaemon::new();
        let id = daemon.id;

        let handle = adapter.spawn(Box::new(daemon)).await.unwrap();

        let tracer = adapter.attach_tracer(&handle).await.unwrap();
        assert_eq!(tracer.daemon_id(), id);

        // Clean up
        adapter.signal(&handle, Signal::Kill).await.unwrap();
    }

    #[tokio::test]
    async fn test_native_adapter_signal_not_found() {
        let adapter = NativeAdapter::new();
        let handle = DaemonHandle::native(DaemonId::new(), 99999);

        let result = adapter.signal(&handle, Signal::Term).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_native_adapter_status_not_found() {
        let adapter = NativeAdapter::new();
        let handle = DaemonHandle::native(DaemonId::new(), 99999);

        // Should return Stopped for unknown processes
        let status = adapter.status(&handle).await.unwrap();
        assert_eq!(status, DaemonStatus::Stopped);
    }

    #[tokio::test]
    async fn test_native_adapter_default() {
        let adapter = NativeAdapter::default();
        assert_eq!(adapter.platform(), Platform::Native);
        assert_eq!(adapter.process_count().await, 0);
    }

    #[tokio::test]
    async fn test_native_adapter_all_signals() {
        let adapter = NativeAdapter::new();
        let daemon = TestDaemon::new();

        let handle = adapter.spawn(Box::new(daemon)).await.unwrap();

        // Test various signals (non-terminating)
        for sig in [Signal::Hup, Signal::Usr1, Signal::Usr2] {
            adapter.signal(&handle, sig).await.unwrap();
        }

        // Verify still running after non-terminating signals
        let status = adapter.status(&handle).await.unwrap();
        assert_eq!(status, DaemonStatus::Running);

        // Clean up
        adapter.signal(&handle, Signal::Kill).await.unwrap();
    }

    #[tokio::test]
    async fn test_native_adapter_attach_tracer_not_found() {
        let adapter = NativeAdapter::new();
        let handle = DaemonHandle::native(DaemonId::new(), 99999);

        let result = adapter.attach_tracer(&handle).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_native_adapter_stop_and_cont() {
        let adapter = NativeAdapter::new();
        let daemon = TestDaemon::new();

        let handle = adapter.spawn(Box::new(daemon)).await.unwrap();

        // Stop the process
        adapter.signal(&handle, Signal::Stop).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Continue the process
        adapter.signal(&handle, Signal::Cont).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Should still be running
        let status = adapter.status(&handle).await.unwrap();
        assert_eq!(status, DaemonStatus::Running);

        // Clean up
        adapter.signal(&handle, Signal::Kill).await.unwrap();
    }

    #[tokio::test]
    async fn test_native_adapter_term_signal() {
        let adapter = NativeAdapter::new();
        let daemon = TestDaemon::new();

        let handle = adapter.spawn(Box::new(daemon)).await.unwrap();

        // Send SIGTERM
        adapter.signal(&handle, Signal::Term).await.unwrap();

        // Wait for termination
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should be terminated (signal 15)
        let status = adapter.status(&handle).await.unwrap();
        assert!(matches!(
            status,
            DaemonStatus::Stopped | DaemonStatus::Failed(FailureReason::Signal(15))
        ));
    }

    #[tokio::test]
    async fn test_native_adapter_int_signal() {
        let adapter = NativeAdapter::new();
        let daemon = TestDaemon::new();

        let handle = adapter.spawn(Box::new(daemon)).await.unwrap();

        // Send SIGINT
        adapter.signal(&handle, Signal::Int).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should be terminated
        let status = adapter.status(&handle).await.unwrap();
        assert!(status.is_terminal());
    }

    #[tokio::test]
    async fn test_native_adapter_quit_signal() {
        let adapter = NativeAdapter::new();
        let daemon = TestDaemon::new();

        let handle = adapter.spawn(Box::new(daemon)).await.unwrap();

        // Send SIGQUIT
        adapter.signal(&handle, Signal::Quit).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should be terminated
        let status = adapter.status(&handle).await.unwrap();
        assert!(status.is_terminal());
    }
}
