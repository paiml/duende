//! Native process adapter (fallback).
//!
//! Spawns daemons as native OS processes without systemd/launchd integration.

use async_trait::async_trait;
use std::process::Stdio;
use tokio::process::Command;

use duende_core::{Daemon, DaemonStatus, Signal};

use crate::adapter::{DaemonHandle, PlatformAdapter, TracerHandle};
use crate::detect::Platform;
use crate::error::{PlatformError, Result};

#[cfg(unix)]
use nix::sys::signal::{Signal as NixSignal, kill as nix_kill};
#[cfg(unix)]
use nix::unistd::Pid;

/// Native process adapter.
///
/// This is the fallback adapter when no platform-specific service manager
/// is available. Daemons are spawned as regular OS processes.
pub struct NativeAdapter {
    // Future: could hold process handles for management
}

impl NativeAdapter {
    /// Creates a new native adapter.
    #[must_use]
    pub const fn new() -> Self {
        Self {}
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

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> Result<DaemonHandle> {
        let config = duende_core::DaemonConfig::new(daemon.name(), "/bin/sh");

        // Build command
        let mut cmd = Command::new(&config.binary_path);
        cmd.args(&config.args)
            .envs(&config.env)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        if let Some(ref cwd) = config.working_dir {
            cmd.current_dir(cwd);
        }

        // Spawn process
        let child = cmd
            .spawn()
            .map_err(|e| PlatformError::spawn(format!("failed to spawn process: {e}")))?;

        let pid = child
            .id()
            .ok_or_else(|| PlatformError::spawn("process has no PID"))?;

        tracing::info!(pid = pid, name = daemon.name(), "spawned native daemon");

        Ok(DaemonHandle::native(pid))
    }

    async fn signal(&self, handle: &DaemonHandle, sig: Signal) -> Result<()> {
        let pid = handle
            .pid
            .ok_or_else(|| PlatformError::signal("no PID available"))?;

        #[cfg(unix)]
        {
            let nix_signal = match sig {
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

            #[allow(clippy::cast_possible_wrap)] // PID from u32 fits in i32 range
            nix_kill(Pid::from_raw(pid as i32), nix_signal)
                .map_err(|e| PlatformError::signal(format!("kill({pid}, {sig:?}) failed: {e}")))?;
        }

        #[cfg(not(unix))]
        {
            let _ = (pid, sig); // Suppress unused warnings
            return Err(PlatformError::not_supported(
                "signals not supported on this platform",
            ));
        }

        tracing::debug!(pid = pid, signal = ?sig, "sent signal to native daemon");
        Ok(())
    }

    async fn status(&self, handle: &DaemonHandle) -> Result<DaemonStatus> {
        let pid = handle
            .pid
            .ok_or_else(|| PlatformError::Status("no PID available".to_string()))?;

        #[cfg(unix)]
        {
            // Check if process exists by sending signal 0 (null signal)
            #[allow(clippy::cast_possible_wrap)] // PID from u32 fits in i32 range
            match nix_kill(Pid::from_raw(pid as i32), None) {
                Ok(()) => Ok(DaemonStatus::Running),
                Err(nix::errno::Errno::ESRCH) => Ok(DaemonStatus::Stopped),
                Err(e) => Err(PlatformError::Status(format!(
                    "failed to check process: {e}"
                ))),
            }
        }

        #[cfg(not(unix))]
        {
            let _ = pid; // Suppress unused warning
            Err(PlatformError::not_supported(
                "process status not available on this platform",
            ))
        }
    }

    async fn attach_tracer(&self, handle: &DaemonHandle) -> Result<TracerHandle> {
        let pid = handle
            .pid
            .ok_or_else(|| PlatformError::Tracer("no PID available".to_string()))?;

        // Verify process exists before attempting tracer attachment
        #[cfg(unix)]
        {
            #[allow(clippy::cast_possible_wrap)]
            match nix_kill(Pid::from_raw(pid as i32), None) {
                Ok(()) => {
                    // Process exists, tracer can be attached
                    tracing::info!(pid = pid, "tracer ready for attachment");
                }
                Err(nix::errno::Errno::ESRCH) => {
                    return Err(PlatformError::Tracer(format!(
                        "process {pid} does not exist"
                    )));
                }
                Err(e) => {
                    return Err(PlatformError::Tracer(format!(
                        "cannot verify process {pid}: {e}"
                    )));
                }
            }
        }

        #[cfg(not(unix))]
        tracing::warn!(pid = pid, "tracer attachment not supported on this platform");

        // Return tracer handle for renacer integration
        // Full ptrace attachment is deferred to when tracing is actually needed
        // since it requires CAP_SYS_PTRACE capability
        Ok(TracerHandle {
            platform: Platform::Native,
            id: format!("renacer:{pid}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_adapter_creation() {
        let adapter = NativeAdapter::new();
        assert_eq!(adapter.platform(), Platform::Native);
    }

    #[test]
    fn test_native_adapter_default() {
        let adapter = NativeAdapter::default();
        assert_eq!(adapter.platform(), Platform::Native);
    }

    #[test]
    fn test_daemon_handle_native() {
        let handle = DaemonHandle::native(1234);
        assert_eq!(handle.platform, Platform::Native);
        assert_eq!(handle.pid, Some(1234));
    }

    #[tokio::test]
    async fn test_signal_no_pid() {
        let adapter = NativeAdapter::new();
        let handle = DaemonHandle {
            platform: Platform::Native,
            pid: None,
            id: "test".to_string(),
        };

        let result = adapter.signal(&handle, Signal::Term).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_status_no_pid() {
        let adapter = NativeAdapter::new();
        let handle = DaemonHandle {
            platform: Platform::Native,
            pid: None,
            id: "test".to_string(),
        };

        let result = adapter.status(&handle).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_attach_tracer_no_pid() {
        let adapter = NativeAdapter::new();
        let handle = DaemonHandle {
            platform: Platform::Native,
            pid: None,
            id: "test".to_string(),
        };

        let result = adapter.attach_tracer(&handle).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_attach_tracer_with_valid_pid() {
        let adapter = NativeAdapter::new();
        // Use our own PID which always exists
        let pid = std::process::id();
        let handle = DaemonHandle::native(pid);

        let result = adapter.attach_tracer(&handle).await;
        assert!(result.is_ok(), "attach_tracer should succeed for existing process");
        let tracer = result.expect("tracer should succeed");
        assert_eq!(tracer.platform, Platform::Native);
        assert!(tracer.id.contains(&pid.to_string()));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_attach_tracer_nonexistent_pid() {
        let adapter = NativeAdapter::new();
        // Use a very high PID that should not exist
        let handle = DaemonHandle::native(4000000);

        let result = adapter.attach_tracer(&handle).await;
        assert!(result.is_err(), "attach_tracer should fail for non-existent process");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_status_nonexistent_process() {
        let adapter = NativeAdapter::new();
        // Use a very high PID that should not exist
        let handle = DaemonHandle::native(4000000);

        let result = adapter.status(&handle).await;
        assert!(result.is_ok());
        assert_eq!(result.expect("status"), DaemonStatus::Stopped);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_status_current_process() {
        let adapter = NativeAdapter::new();
        // Check status of our own process (should be running)
        let pid = std::process::id();
        let handle = DaemonHandle::native(pid);

        let result = adapter.status(&handle).await;
        assert!(result.is_ok());
        assert_eq!(result.expect("status"), DaemonStatus::Running);
    }
}
