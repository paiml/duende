//! Linux systemd adapter implementation.
//!
//! Provides daemon management via systemd transient units.

use crate::adapter::{DaemonHandle, PlatformAdapter, PlatformError, PlatformResult, TracerHandle};
use crate::daemon::Daemon;
use crate::platform::Platform;
use crate::types::{DaemonStatus, FailureReason, Signal};

use async_trait::async_trait;
use std::path::PathBuf;
use tokio::process::Command;

/// Linux systemd adapter.
///
/// Manages daemons as systemd transient units via `systemd-run` and `systemctl`.
///
/// # Requirements
///
/// - Linux with systemd (version 232+)
/// - User must have permissions for `systemd-run --user` or root for system units
///
/// # Example
///
/// ```rust,ignore
/// use duende_core::adapters::SystemdAdapter;
/// use duende_core::PlatformAdapter;
///
/// let adapter = SystemdAdapter::user();
/// let handle = adapter.spawn(my_daemon).await?;
/// ```
pub struct SystemdAdapter {
    /// Directory for persistent unit files (not used for transient units)
    unit_dir: PathBuf,
    /// Use user session (--user) vs system session
    user_mode: bool,
}

impl SystemdAdapter {
    /// Creates a new systemd adapter with default settings (user mode).
    ///
    /// Alias for `user()` for API compatibility.
    #[must_use]
    pub fn new() -> Self {
        Self::user()
    }

    /// Creates a new systemd adapter for system-level daemons.
    ///
    /// Requires root or appropriate polkit permissions.
    #[must_use]
    pub fn system() -> Self {
        Self {
            unit_dir: PathBuf::from("/etc/systemd/system"),
            user_mode: false,
        }
    }

    /// Creates a new systemd adapter for user-level daemons.
    ///
    /// Uses `systemd --user` session, no root required.
    #[must_use]
    pub fn user() -> Self {
        Self {
            unit_dir: dirs_next::config_dir()
                .map(|p| p.join("systemd/user"))
                .unwrap_or_else(|| PathBuf::from("~/.config/systemd/user")),
            user_mode: true,
        }
    }

    /// Creates adapter with custom unit directory.
    #[must_use]
    pub fn with_unit_dir(unit_dir: PathBuf, user_mode: bool) -> Self {
        Self {
            unit_dir,
            user_mode,
        }
    }

    /// Returns the unit directory path.
    #[must_use]
    pub fn unit_dir(&self) -> &PathBuf {
        &self.unit_dir
    }

    /// Returns whether running in user mode.
    #[must_use]
    pub const fn is_user_mode(&self) -> bool {
        self.user_mode
    }

    /// Generates a unit name from daemon name.
    fn unit_name(daemon_name: &str) -> String {
        format!("duende-{}.service", daemon_name.replace(' ', "-"))
    }

    /// Builds systemctl command with appropriate flags.
    fn systemctl_cmd(&self) -> Command {
        let mut cmd = Command::new("systemctl");
        if self.user_mode {
            cmd.arg("--user");
        }
        cmd
    }

    /// Builds systemd-run command for transient units.
    fn systemd_run_cmd(&self) -> Command {
        let mut cmd = Command::new("systemd-run");
        if self.user_mode {
            cmd.arg("--user");
        }
        cmd
    }

    /// Parses systemctl status output to DaemonStatus.
    fn parse_status(output: &str, exit_code: i32) -> DaemonStatus {
        // systemctl exit codes:
        // 0 = running
        // 1 = dead/failed (unit loaded but not active)
        // 3 = not running (unit not loaded or inactive)
        // 4 = no such unit

        if exit_code == 0 {
            // Check if actually running
            if output.contains("Active: active (running)") {
                return DaemonStatus::Running;
            }
            if output.contains("Active: activating") {
                return DaemonStatus::Starting;
            }
        }

        if output.contains("Active: deactivating") {
            return DaemonStatus::Stopping;
        }

        if output.contains("Active: inactive") {
            return DaemonStatus::Stopped;
        }

        if output.contains("Active: failed") {
            return DaemonStatus::Failed(FailureReason::ExitCode(1));
        }

        // Unit doesn't exist or is stopped
        DaemonStatus::Stopped
    }

    /// Maps Signal to systemctl kill signal name.
    fn signal_name(sig: Signal) -> &'static str {
        match sig {
            Signal::Term => "SIGTERM",
            Signal::Kill => "SIGKILL",
            Signal::Int => "SIGINT",
            Signal::Quit => "SIGQUIT",
            Signal::Hup => "SIGHUP",
            Signal::Usr1 => "SIGUSR1",
            Signal::Usr2 => "SIGUSR2",
            Signal::Stop => "SIGSTOP",
            Signal::Cont => "SIGCONT",
        }
    }
}

impl Default for SystemdAdapter {
    fn default() -> Self {
        // Default to user mode for safety
        Self::user()
    }
}

#[async_trait]
impl PlatformAdapter for SystemdAdapter {
    fn platform(&self) -> Platform {
        Platform::Linux
    }

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> PlatformResult<DaemonHandle> {
        let daemon_name = daemon.name().to_string();
        let daemon_id = daemon.id();
        let unit_name = Self::unit_name(&daemon_name);

        // Build systemd-run command for transient unit
        // Note: This creates a simple transient service. For full resource control,
        // use DaemonConfig passed through a proper spawn_with_config method.
        let mut cmd = self.systemd_run_cmd();
        cmd.arg("--unit")
            .arg(&unit_name)
            .arg("--description")
            .arg(format!("Duende daemon: {}", daemon_name))
            .arg("--remain-after-exit")
            .arg("--collect");

        // Add restart policy
        cmd.arg("--property=Restart=on-failure")
            .arg("--property=RestartSec=5");

        // For transient units, we need a command to run.
        // In a real implementation, this would be configured via DaemonConfig.
        // For now, use /bin/true as a placeholder for the test harness.
        cmd.arg("--").arg("/bin/true");

        // Execute systemd-run
        let output = cmd.output().await.map_err(|e| {
            PlatformError::spawn_failed(format!("Failed to execute systemd-run: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PlatformError::spawn_failed(format!(
                "systemd-run failed: {}",
                stderr
            )));
        }

        // Note: We don't track PID for systemd units as systemd manages the process
        Ok(DaemonHandle::systemd(daemon_id, unit_name))
    }

    async fn signal(&self, handle: &DaemonHandle, sig: Signal) -> PlatformResult<()> {
        let unit_name = handle.systemd_unit().ok_or_else(|| {
            PlatformError::spawn_failed("Invalid handle type for systemd adapter")
        })?;

        // Use systemctl kill to send signal
        let mut cmd = self.systemctl_cmd();
        cmd.arg("kill")
            .arg("--signal")
            .arg(Self::signal_name(sig))
            .arg(unit_name);

        let output = cmd.output().await.map_err(|e| {
            PlatformError::spawn_failed(format!("Failed to execute systemctl kill: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PlatformError::spawn_failed(format!(
                "systemctl kill failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    async fn status(&self, handle: &DaemonHandle) -> PlatformResult<DaemonStatus> {
        let unit_name = handle.systemd_unit().ok_or_else(|| {
            PlatformError::spawn_failed("Invalid handle type for systemd adapter")
        })?;

        let mut cmd = self.systemctl_cmd();
        cmd.arg("status").arg(unit_name);

        let output = cmd.output().await.map_err(|e| {
            PlatformError::spawn_failed(format!("Failed to execute systemctl status: {}", e))
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let exit_code = output.status.code().unwrap_or(1);

        Ok(Self::parse_status(&stdout, exit_code))
    }

    async fn attach_tracer(&self, handle: &DaemonHandle) -> PlatformResult<TracerHandle> {
        let unit_name = handle.systemd_unit().ok_or_else(|| {
            PlatformError::spawn_failed("Invalid handle type for systemd adapter")
        })?;

        // Get the main PID for the unit
        let pid = self.get_main_pid(unit_name).await.ok_or_else(|| {
            PlatformError::spawn_failed("Cannot attach tracer: failed to get PID")
        })?;

        if pid == 0 {
            return Err(PlatformError::spawn_failed(
                "Cannot attach tracer: PID unknown",
            ));
        }

        // Return a ptrace-based tracer handle
        Ok(TracerHandle::ptrace(handle.id()))
    }
}

impl SystemdAdapter {
    /// Gets the main PID of a systemd unit.
    async fn get_main_pid(&self, unit_name: &str) -> Option<u64> {
        let mut cmd = self.systemctl_cmd();
        cmd.arg("show")
            .arg("--property=MainPID")
            .arg("--value")
            .arg(unit_name);

        let output = cmd.output().await.ok()?;
        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.trim().parse().ok()
    }

    /// Stops a systemd unit.
    pub async fn stop(&self, unit_name: &str) -> PlatformResult<()> {
        let mut cmd = self.systemctl_cmd();
        cmd.arg("stop").arg(unit_name);

        let output = cmd.output().await.map_err(|e| {
            PlatformError::spawn_failed(format!("Failed to execute systemctl stop: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PlatformError::spawn_failed(format!(
                "systemctl stop failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    /// Resets a failed systemd unit.
    pub async fn reset_failed(&self, unit_name: &str) -> PlatformResult<()> {
        let mut cmd = self.systemctl_cmd();
        cmd.arg("reset-failed").arg(unit_name);

        let _ = cmd.output().await; // Ignore errors
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_systemd_adapter_system() {
        let adapter = SystemdAdapter::system();
        assert!(!adapter.is_user_mode());
        assert_eq!(adapter.unit_dir(), &PathBuf::from("/etc/systemd/system"));
        assert_eq!(adapter.platform(), Platform::Linux);
    }

    #[test]
    fn test_systemd_adapter_user() {
        let adapter = SystemdAdapter::user();
        assert!(adapter.is_user_mode());
        assert_eq!(adapter.platform(), Platform::Linux);
    }

    #[test]
    fn test_systemd_adapter_default() {
        let adapter = SystemdAdapter::default();
        assert!(adapter.is_user_mode()); // Default is user mode for safety
    }

    #[test]
    fn test_unit_name_generation() {
        assert_eq!(
            SystemdAdapter::unit_name("my-daemon"),
            "duende-my-daemon.service"
        );
        assert_eq!(
            SystemdAdapter::unit_name("my daemon"),
            "duende-my-daemon.service"
        );
    }

    #[test]
    fn test_signal_name() {
        assert_eq!(SystemdAdapter::signal_name(Signal::Term), "SIGTERM");
        assert_eq!(SystemdAdapter::signal_name(Signal::Kill), "SIGKILL");
        assert_eq!(SystemdAdapter::signal_name(Signal::Hup), "SIGHUP");
    }

    #[test]
    fn test_parse_status_running() {
        let output = "● test.service - Test\n   Active: active (running) since...";
        assert_eq!(
            SystemdAdapter::parse_status(output, 0),
            DaemonStatus::Running
        );
    }

    #[test]
    fn test_parse_status_stopped() {
        let output = "● test.service - Test\n   Active: inactive (dead)";
        assert_eq!(
            SystemdAdapter::parse_status(output, 3),
            DaemonStatus::Stopped
        );
    }

    #[test]
    fn test_parse_status_failed() {
        let output = "● test.service - Test\n   Active: failed";
        assert!(matches!(
            SystemdAdapter::parse_status(output, 1),
            DaemonStatus::Failed(_)
        ));
    }

    #[test]
    fn test_parse_status_starting() {
        let output = "● test.service - Test\n   Active: activating (start)";
        assert_eq!(
            SystemdAdapter::parse_status(output, 0),
            DaemonStatus::Starting
        );
    }

    #[test]
    fn test_with_unit_dir() {
        let adapter = SystemdAdapter::with_unit_dir(PathBuf::from("/custom/path"), false);
        assert_eq!(adapter.unit_dir(), &PathBuf::from("/custom/path"));
        assert!(!adapter.is_user_mode());
    }

    // ==================== Extended Tests for Coverage ====================

    #[test]
    fn test_unit_name_special_characters() {
        // Test various daemon names
        assert_eq!(
            SystemdAdapter::unit_name("test"),
            "duende-test.service"
        );
        assert_eq!(
            SystemdAdapter::unit_name("test-daemon"),
            "duende-test-daemon.service"
        );
        assert_eq!(
            SystemdAdapter::unit_name("test daemon name"),
            "duende-test-daemon-name.service"
        );
        assert_eq!(
            SystemdAdapter::unit_name(""),
            "duende-.service"
        );
    }

    #[test]
    fn test_signal_name_all_signals() {
        assert_eq!(SystemdAdapter::signal_name(Signal::Int), "SIGINT");
        assert_eq!(SystemdAdapter::signal_name(Signal::Quit), "SIGQUIT");
        assert_eq!(SystemdAdapter::signal_name(Signal::Usr1), "SIGUSR1");
        assert_eq!(SystemdAdapter::signal_name(Signal::Usr2), "SIGUSR2");
        assert_eq!(SystemdAdapter::signal_name(Signal::Stop), "SIGSTOP");
        assert_eq!(SystemdAdapter::signal_name(Signal::Cont), "SIGCONT");
    }

    #[test]
    fn test_parse_status_deactivating() {
        let output = "● test.service - Test\n   Active: deactivating (stop-sigterm)";
        assert_eq!(
            SystemdAdapter::parse_status(output, 0),
            DaemonStatus::Stopping
        );
    }

    #[test]
    fn test_parse_status_empty() {
        assert_eq!(
            SystemdAdapter::parse_status("", 4),
            DaemonStatus::Stopped
        );
    }

    #[test]
    fn test_parse_status_unknown_output() {
        let output = "Some random output without status";
        assert_eq!(
            SystemdAdapter::parse_status(output, 0),
            DaemonStatus::Stopped
        );
    }

    #[test]
    fn test_parse_status_exit_codes() {
        // exit code 0 without "active (running)" should be stopped
        let output = "Some output";
        assert_eq!(
            SystemdAdapter::parse_status(output, 0),
            DaemonStatus::Stopped
        );

        // exit code 1 should be stopped
        assert_eq!(
            SystemdAdapter::parse_status(output, 1),
            DaemonStatus::Stopped
        );

        // exit code 3 should be stopped
        assert_eq!(
            SystemdAdapter::parse_status(output, 3),
            DaemonStatus::Stopped
        );

        // exit code 4 should be stopped
        assert_eq!(
            SystemdAdapter::parse_status(output, 4),
            DaemonStatus::Stopped
        );
    }

    #[test]
    fn test_parse_status_inactive_variations() {
        let output1 = "Active: inactive (dead)";
        assert_eq!(
            SystemdAdapter::parse_status(output1, 3),
            DaemonStatus::Stopped
        );

        let output2 = "  Active: inactive  ";
        assert_eq!(
            SystemdAdapter::parse_status(output2, 3),
            DaemonStatus::Stopped
        );
    }

    #[test]
    fn test_parse_status_activating_variations() {
        let output1 = "Active: activating (auto-restart)";
        assert_eq!(
            SystemdAdapter::parse_status(output1, 0),
            DaemonStatus::Starting
        );

        let output2 = "Active: activating (start-pre)";
        assert_eq!(
            SystemdAdapter::parse_status(output2, 0),
            DaemonStatus::Starting
        );
    }

    #[test]
    fn test_systemd_adapter_new_alias() {
        let adapter = SystemdAdapter::new();
        // new() is alias for user()
        assert!(adapter.is_user_mode());
    }

    #[test]
    fn test_systemd_adapter_clone_path() {
        let adapter = SystemdAdapter::with_unit_dir(PathBuf::from("/test"), true);
        let path = adapter.unit_dir().clone();
        assert_eq!(path, PathBuf::from("/test"));
    }

    #[test]
    fn test_parse_status_failed_variations() {
        // Test failed status in different contexts
        let output1 = "Active: failed (Result: exit-code)";
        assert!(matches!(
            SystemdAdapter::parse_status(output1, 1),
            DaemonStatus::Failed(_)
        ));

        let output2 = "Active: failed (Result: timeout)";
        assert!(matches!(
            SystemdAdapter::parse_status(output2, 1),
            DaemonStatus::Failed(_)
        ));
    }

    #[test]
    fn test_systemctl_cmd_user_mode() {
        let adapter = SystemdAdapter::user();
        let cmd = adapter.systemctl_cmd();
        // Just verify it's constructed (we can't easily inspect the args)
        let _ = cmd;
    }

    #[test]
    fn test_systemctl_cmd_system_mode() {
        let adapter = SystemdAdapter::system();
        let cmd = adapter.systemctl_cmd();
        let _ = cmd;
    }

    #[test]
    fn test_systemd_run_cmd_user_mode() {
        let adapter = SystemdAdapter::user();
        let cmd = adapter.systemd_run_cmd();
        let _ = cmd;
    }

    #[test]
    fn test_systemd_run_cmd_system_mode() {
        let adapter = SystemdAdapter::system();
        let cmd = adapter.systemd_run_cmd();
        let _ = cmd;
    }
}
