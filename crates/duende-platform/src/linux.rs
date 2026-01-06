//! Linux systemd platform adapter.
//!
//! # Overview
//!
//! This module provides systemd integration for daemon management on Linux systems.
//! It handles:
//!
//! - Unit file generation from `DaemonConfig`
//! - cgroup resource limits (memory, CPU)
//! - Journal log integration
//! - Restart policies via `Restart=` directives
//!
//! # Toyota Way: Standardized Work (標準作業)
//!
//! systemd units follow a standardized template to ensure consistency
//! across all managed daemons.

use crate::{DaemonHandle, Platform, PlatformAdapter, PlatformError, Result, TracerHandle};
use async_trait::async_trait;
use duende_core::config::RestartPolicy;
use duende_core::{Daemon, DaemonConfig, DaemonStatus, FailureReason, Signal};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

/// Linux systemd adapter.
///
/// Manages daemons via systemd service units with full cgroup support.
#[derive(Debug)]
pub struct LinuxAdapter {
    /// Unit file directory (default: /etc/systemd/system)
    unit_dir: PathBuf,
    /// Runtime directory for transient units (reserved for future use)
    #[allow(dead_code)]
    runtime_dir: PathBuf,
}

impl Default for LinuxAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxAdapter {
    /// Create a new Linux adapter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            unit_dir: PathBuf::from("/etc/systemd/system"),
            runtime_dir: PathBuf::from("/run/systemd/transient"),
        }
    }

    /// Create with custom unit directory.
    #[must_use]
    pub fn with_unit_dir(unit_dir: PathBuf) -> Self {
        Self {
            unit_dir,
            runtime_dir: PathBuf::from("/run/systemd/transient"),
        }
    }

    /// Generate a systemd unit file for the daemon.
    #[allow(clippy::unused_self, clippy::format_push_string)]
    fn generate_unit_file(&self, config: &DaemonConfig) -> String {
        let mut unit = String::new();

        // [Unit] section
        unit.push_str("[Unit]\n");
        unit.push_str(&format!("Description={}\n", config.description));
        unit.push_str("After=network.target\n");
        unit.push('\n');

        // [Service] section
        unit.push_str("[Service]\n");
        unit.push_str("Type=simple\n");
        unit.push_str(&format!("ExecStart={}", config.binary_path.display()));

        // Add arguments
        for arg in &config.args {
            unit.push_str(&format!(" {}", arg));
        }
        unit.push('\n');

        // Working directory
        if let Some(ref working_dir) = config.working_dir {
            unit.push_str(&format!("WorkingDirectory={}\n", working_dir.display()));
        }

        // User/Group
        if let Some(ref user) = config.user {
            unit.push_str(&format!("User={}\n", user));
        }
        if let Some(ref group) = config.group {
            unit.push_str(&format!("Group={}\n", group));
        }

        // Environment variables
        for (key, value) in &config.env {
            unit.push_str(&format!("Environment=\"{}={}\"\n", key, value));
        }

        // Resource limits (cgroups)
        let resources = &config.resources;
        if resources.memory_bytes > 0 {
            unit.push_str(&format!("MemoryMax={}\n", resources.memory_bytes));
        }
        if resources.cpu_quota_percent > 0.0 && resources.cpu_quota_percent < 100.0 {
            #[allow(clippy::cast_possible_truncation)]
            let quota = (resources.cpu_quota_percent * 10000.0) as u64;
            unit.push_str(&format!("CPUQuota={}%\n", quota / 10000));
        }
        if resources.pids_max > 0 {
            unit.push_str(&format!("TasksMax={}\n", resources.pids_max));
        }

        // Restart policy
        let restart_directive = match &config.restart {
            RestartPolicy::Never => "no",
            RestartPolicy::Always => "always",
            RestartPolicy::OnFailure => "on-failure",
            RestartPolicy::UnlessStopped => "unless-stopped",
        };
        unit.push_str(&format!("Restart={}\n", restart_directive));

        // Shutdown timeout
        unit.push_str(&format!(
            "TimeoutStopSec={}\n",
            config.shutdown_timeout.as_secs()
        ));

        unit.push('\n');

        // [Install] section
        unit.push_str("[Install]\n");
        unit.push_str("WantedBy=multi-user.target\n");

        unit
    }

    /// Get the unit name for a daemon.
    fn unit_name(daemon_name: &str) -> String {
        format!("duende-{}.service", daemon_name.replace(' ', "-"))
    }

    /// Parse systemctl status output to DaemonStatus.
    fn parse_status(output: &str) -> DaemonStatus {
        // Parse ActiveState from systemctl show output
        for line in output.lines() {
            if let Some(state) = line.strip_prefix("ActiveState=") {
                return match state.trim() {
                    "active" | "running" => DaemonStatus::Running,
                    "inactive" | "dead" => DaemonStatus::Stopped,
                    "failed" => DaemonStatus::Failed(FailureReason::ExitCode(1)),
                    "activating" | "reloading" => DaemonStatus::Starting,
                    "deactivating" => DaemonStatus::Stopping,
                    _ => DaemonStatus::Created,
                };
            }
        }
        DaemonStatus::Created
    }

    /// Translate Signal to systemd signal name.
    fn signal_name(signal: Signal) -> &'static str {
        match signal {
            Signal::Term => "SIGTERM",
            Signal::Kill => "SIGKILL",
            Signal::Hup => "SIGHUP",
            Signal::Int => "SIGINT",
            Signal::Quit => "SIGQUIT",
            Signal::Usr1 => "SIGUSR1",
            Signal::Usr2 => "SIGUSR2",
            Signal::Stop => "SIGSTOP",
            Signal::Cont => "SIGCONT",
        }
    }
}

#[async_trait]
impl PlatformAdapter for LinuxAdapter {
    fn platform(&self) -> Platform {
        Platform::Linux
    }

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> Result<DaemonHandle> {
        let config = DaemonConfig::new(daemon.name(), "/bin/false"); // Placeholder
        let unit_name = Self::unit_name(daemon.name());
        let unit_path = self.unit_dir.join(&unit_name);

        // Generate unit file
        let unit_content = self.generate_unit_file(&config);

        // Write unit file
        tokio::fs::write(&unit_path, &unit_content)
            .await
            .map_err(|e| PlatformError::Spawn(format!("failed to write unit file: {}", e)))?;

        // Reload systemd
        let reload = Command::new("systemctl")
            .arg("daemon-reload")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map_err(|e| PlatformError::Spawn(format!("daemon-reload failed: {}", e)))?;

        if !reload.success() {
            return Err(PlatformError::Spawn("daemon-reload failed".into()));
        }

        // Start the service
        let start = Command::new("systemctl")
            .args(["start", &unit_name])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PlatformError::Spawn(format!("systemctl start failed: {}", e)))?;

        if !start.status.success() {
            let stderr = String::from_utf8_lossy(&start.stderr);
            return Err(PlatformError::Spawn(format!(
                "systemctl start failed: {}",
                stderr
            )));
        }

        Ok(DaemonHandle::systemd(unit_name))
    }

    async fn signal(&self, handle: &DaemonHandle, signal: Signal) -> Result<()> {
        if handle.platform != Platform::Linux {
            return Err(PlatformError::Signal("not a systemd handle".into()));
        }
        let unit_name = &handle.id;

        let sig_name = Self::signal_name(signal);

        let output = Command::new("systemctl")
            .args(["kill", "--signal", sig_name, unit_name])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PlatformError::Signal(format!("systemctl kill failed: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PlatformError::Signal(format!(
                "systemctl kill failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    async fn status(&self, handle: &DaemonHandle) -> Result<DaemonStatus> {
        if handle.platform != Platform::Linux {
            return Err(PlatformError::Status("not a systemd handle".into()));
        }
        let unit_name = &handle.id;

        let output = Command::new("systemctl")
            .args(["show", "--property=ActiveState", unit_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .map_err(|e| PlatformError::Status(format!("systemctl show failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(Self::parse_status(&stdout))
    }

    async fn attach_tracer(&self, handle: &DaemonHandle) -> Result<TracerHandle> {
        if handle.platform != Platform::Linux {
            return Err(PlatformError::Tracer("not a systemd handle".into()));
        }
        let unit_name = &handle.id;

        // Get the main PID of the service
        let output = Command::new("systemctl")
            .args(["show", "--property=MainPID", unit_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .map_err(|e| PlatformError::Tracer(format!("failed to get MainPID: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let pid: u32 = stdout
            .lines()
            .find_map(|line| line.strip_prefix("MainPID="))
            .and_then(|s| s.trim().parse().ok())
            .ok_or_else(|| PlatformError::Tracer("failed to parse MainPID".into()))?;

        if pid == 0 {
            return Err(PlatformError::Tracer("service not running".into()));
        }

        // Return a ptrace-based tracer handle
        Ok(TracerHandle {
            platform: Platform::Linux,
            id: format!("ptrace:{}", pid),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linux_adapter_creation() {
        let adapter = LinuxAdapter::new();
        assert_eq!(adapter.platform(), Platform::Linux);
    }

    #[test]
    fn test_linux_adapter_custom_unit_dir() {
        let adapter = LinuxAdapter::with_unit_dir("/tmp/units".into());
        assert_eq!(adapter.unit_dir, PathBuf::from("/tmp/units"));
    }

    #[test]
    fn test_unit_name_generation() {
        assert_eq!(
            LinuxAdapter::unit_name("my-daemon"),
            "duende-my-daemon.service"
        );
        assert_eq!(
            LinuxAdapter::unit_name("my daemon"),
            "duende-my-daemon.service"
        );
    }

    #[test]
    fn test_generate_unit_file() {
        let adapter = LinuxAdapter::new();
        let config = DaemonConfig::new("test-daemon", "/usr/bin/test");

        let unit = adapter.generate_unit_file(&config);

        assert!(unit.contains("[Unit]"));
        assert!(unit.contains("[Service]"));
        assert!(unit.contains("[Install]"));
        assert!(unit.contains("ExecStart=/usr/bin/test"));
        assert!(unit.contains("Type=simple"));
    }

    #[test]
    fn test_parse_status_active() {
        let output = "ActiveState=active\n";
        assert!(matches!(
            LinuxAdapter::parse_status(output),
            DaemonStatus::Running
        ));
    }

    #[test]
    fn test_parse_status_inactive() {
        let output = "ActiveState=inactive\n";
        assert!(matches!(
            LinuxAdapter::parse_status(output),
            DaemonStatus::Stopped
        ));
    }

    #[test]
    fn test_parse_status_failed() {
        let output = "ActiveState=failed\n";
        assert!(matches!(
            LinuxAdapter::parse_status(output),
            DaemonStatus::Failed(_)
        ));
    }

    #[test]
    fn test_parse_status_activating() {
        let output = "ActiveState=activating\n";
        assert!(matches!(
            LinuxAdapter::parse_status(output),
            DaemonStatus::Starting
        ));
    }

    #[test]
    fn test_signal_name_translation() {
        assert_eq!(LinuxAdapter::signal_name(Signal::Term), "SIGTERM");
        assert_eq!(LinuxAdapter::signal_name(Signal::Kill), "SIGKILL");
        assert_eq!(LinuxAdapter::signal_name(Signal::Hup), "SIGHUP");
        assert_eq!(LinuxAdapter::signal_name(Signal::Stop), "SIGSTOP");
        assert_eq!(LinuxAdapter::signal_name(Signal::Cont), "SIGCONT");
    }

    #[test]
    fn test_unit_file_with_resources() {
        let adapter = LinuxAdapter::new();
        let mut config = DaemonConfig::new("resource-daemon", "/usr/bin/test");
        config.resources.memory_bytes = 1024 * 1024 * 512; // 512MB
        config.resources.pids_max = 50;

        let unit = adapter.generate_unit_file(&config);

        assert!(unit.contains("MemoryMax="));
        assert!(unit.contains("TasksMax=50"));
    }

    #[test]
    fn test_unit_file_with_user_group() {
        let adapter = LinuxAdapter::new();
        let mut config = DaemonConfig::new("user-daemon", "/usr/bin/test");
        config.user = Some("daemon".into());
        config.group = Some("daemon".into());

        let unit = adapter.generate_unit_file(&config);

        assert!(unit.contains("User=daemon"));
        assert!(unit.contains("Group=daemon"));
    }

    #[test]
    fn test_unit_file_with_environment() {
        let adapter = LinuxAdapter::new();
        let mut config = DaemonConfig::new("env-daemon", "/usr/bin/test");
        config.env.insert("FOO".into(), "bar".into());
        config.env.insert("BAZ".into(), "qux".into());

        let unit = adapter.generate_unit_file(&config);

        assert!(unit.contains("Environment="));
    }
}
