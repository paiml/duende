//! Container (Docker/OCI) platform adapter.
//!
//! # Overview
//!
//! This module provides container runtime integration for daemon management.
//! It handles:
//!
//! - Container creation from `DaemonConfig`
//! - Resource limits via cgroup v2 constraints
//! - Health check configuration
//! - Restart policies
//! - Log driver integration
//!
//! # Toyota Way: Heijunka (平準化)
//!
//! Container resource limits provide production leveling to ensure
//! consistent performance across deployments.

use crate::{DaemonHandle, Platform, PlatformAdapter, PlatformError, Result, TracerHandle};
use async_trait::async_trait;
use duende_core::config::RestartPolicy;
use duende_core::{Daemon, DaemonConfig, DaemonStatus, FailureReason, Signal};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

/// Container runtime adapter.
///
/// Manages daemons as OCI containers via Docker or compatible runtimes.
#[derive(Debug)]
pub struct ContainerAdapter {
    /// Docker socket path (reserved for socket-based API in future)
    #[allow(dead_code)]
    socket_path: PathBuf,
    /// Container runtime (docker, podman, containerd)
    runtime: ContainerRuntime,
    /// Image prefix for daemon containers
    image_prefix: String,
}

impl Default for ContainerAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Supported container runtimes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ContainerRuntime {
    /// Docker runtime
    #[default]
    Docker,
    /// Podman runtime
    Podman,
    /// containerd runtime (via ctr/nerdctl)
    Containerd,
}

impl ContainerRuntime {
    /// Get the CLI command for this runtime.
    fn cli_command(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
            Self::Containerd => "nerdctl",
        }
    }
}

impl ContainerAdapter {
    /// Create a new container adapter with Docker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            socket_path: PathBuf::from("/var/run/docker.sock"),
            runtime: ContainerRuntime::Docker,
            image_prefix: "duende".to_string(),
        }
    }

    /// Create with custom socket path.
    #[must_use]
    pub fn with_socket(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            runtime: ContainerRuntime::Docker,
            image_prefix: "duende".to_string(),
        }
    }

    /// Create with specific runtime.
    #[must_use]
    pub fn with_runtime(runtime: ContainerRuntime) -> Self {
        let socket_path = match runtime {
            ContainerRuntime::Docker => "/var/run/docker.sock",
            ContainerRuntime::Podman => "/var/run/podman/podman.sock",
            ContainerRuntime::Containerd => "/run/containerd/containerd.sock",
        };
        Self {
            socket_path: socket_path.into(),
            runtime,
            image_prefix: "duende".to_string(),
        }
    }

    /// Set image prefix for containers.
    #[must_use]
    pub fn with_image_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.image_prefix = prefix.into();
        self
    }

    /// Generate container name for a daemon.
    fn container_name(daemon_name: &str) -> String {
        format!("duende-{}", daemon_name.replace(' ', "-"))
    }

    /// Build container run arguments from config.
    fn build_run_args(&self, config: &DaemonConfig) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "-d".to_string(),
            "--name".to_string(),
            Self::container_name(&config.name),
        ];

        // Resource limits (cgroup v2)
        let resources = &config.resources;
        if resources.memory_bytes > 0 {
            args.push("--memory".to_string());
            args.push(format!("{}b", resources.memory_bytes));
        }
        if resources.cpu_quota_percent > 0.0 && resources.cpu_quota_percent < 100.0 {
            args.push("--cpus".to_string());
            args.push(format!("{:.2}", resources.cpu_quota_percent / 100.0));
        }
        if resources.pids_max > 0 {
            args.push("--pids-limit".to_string());
            args.push(resources.pids_max.to_string());
        }

        // Environment variables
        for (key, value) in &config.env {
            args.push("-e".to_string());
            args.push(format!("{}={}", key, value));
        }

        // Working directory
        if let Some(ref working_dir) = config.working_dir {
            args.push("-w".to_string());
            args.push(working_dir.display().to_string());
        }

        // User/Group
        if let Some(ref user) = config.user {
            args.push("--user".to_string());
            if let Some(ref group) = config.group {
                args.push(format!("{}:{}", user, group));
            } else {
                args.push(user.clone());
            }
        }

        // Restart policy
        let restart_policy = match &config.restart {
            RestartPolicy::Never => "no",
            RestartPolicy::Always => "always",
            RestartPolicy::OnFailure => "on-failure",
            RestartPolicy::UnlessStopped => "unless-stopped",
        };
        args.push("--restart".to_string());
        args.push(restart_policy.to_string());

        // Stop timeout
        args.push("--stop-timeout".to_string());
        args.push(config.shutdown_timeout.as_secs().to_string());

        // Image (use binary path base name or configured image)
        let image = format!(
            "{}/{}:latest",
            self.image_prefix,
            config
                .binary_path
                .file_name()
                .map_or_else(|| config.name.clone(), |n| n.to_string_lossy().to_string())
        );
        args.push(image);

        // Command arguments
        args.extend(config.args.iter().cloned());

        args
    }

    /// Parse docker inspect output to DaemonStatus.
    fn parse_status(output: &str) -> DaemonStatus {
        // Parse JSON output from docker inspect
        // Format: [{"State": {"Status": "running", "ExitCode": 0, ...}}]
        if output.contains("\"running\"") {
            DaemonStatus::Running
        } else if output.contains("\"exited\"") {
            // Extract exit code
            if let Some(code_start) = output.find("\"ExitCode\":") {
                let code_str = &output[code_start + 11..].trim_start();
                // Find end of number (non-digit and non-minus)
                let code_end = code_str
                    .find(|c: char| !c.is_ascii_digit() && c != '-')
                    .unwrap_or(code_str.len());
                if let Ok(code) = code_str[..code_end].parse::<i32>() {
                    if code == 0 {
                        return DaemonStatus::Stopped;
                    }
                    return DaemonStatus::Failed(FailureReason::ExitCode(code));
                }
            }
            DaemonStatus::Stopped
        } else if output.contains("\"created\"") {
            DaemonStatus::Created
        } else if output.contains("\"restarting\"") || output.contains("\"starting\"") {
            DaemonStatus::Starting
        } else if output.contains("\"paused\"") || output.contains("\"removing\"") {
            DaemonStatus::Stopping
        } else if output.contains("\"dead\"") {
            DaemonStatus::Failed(FailureReason::ExitCode(1))
        } else {
            DaemonStatus::Created
        }
    }

    /// Translate Signal to container signal name.
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
impl PlatformAdapter for ContainerAdapter {
    fn platform(&self) -> Platform {
        Platform::Container
    }

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> Result<DaemonHandle> {
        let config = DaemonConfig::new(daemon.name(), "/bin/daemon");
        let container_name = Self::container_name(daemon.name());
        let cli = self.runtime.cli_command();

        // Remove existing container if present
        let _ = Command::new(cli)
            .args(["rm", "-f", &container_name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        // Build and run container
        let args = self.build_run_args(&config);
        let output = Command::new(cli)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PlatformError::Spawn(format!("{} run failed: {}", cli, e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PlatformError::Spawn(format!(
                "{} run failed: {}",
                cli, stderr
            )));
        }

        // Get container ID from output
        let container_id = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();

        Ok(DaemonHandle::container(container_id))
    }

    async fn signal(&self, handle: &DaemonHandle, signal: Signal) -> Result<()> {
        if handle.platform != Platform::Container {
            return Err(PlatformError::Signal("not a container handle".into()));
        }

        let cli = self.runtime.cli_command();
        let sig_name = Self::signal_name(signal);

        let output = Command::new(cli)
            .args(["kill", "--signal", sig_name, &handle.id])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PlatformError::Signal(format!("{} kill failed: {}", cli, e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PlatformError::Signal(format!(
                "{} kill failed: {}",
                cli, stderr
            )));
        }

        Ok(())
    }

    async fn status(&self, handle: &DaemonHandle) -> Result<DaemonStatus> {
        if handle.platform != Platform::Container {
            return Err(PlatformError::Status("not a container handle".into()));
        }

        let cli = self.runtime.cli_command();
        let output = Command::new(cli)
            .args(["inspect", "--format", "{{json .State}}", &handle.id])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .map_err(|e| PlatformError::Status(format!("{} inspect failed: {}", cli, e)))?;

        if !output.status.success() {
            // Container not found = stopped
            return Ok(DaemonStatus::Stopped);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(Self::parse_status(&stdout))
    }

    async fn attach_tracer(&self, handle: &DaemonHandle) -> Result<TracerHandle> {
        if handle.platform != Platform::Container {
            return Err(PlatformError::Tracer("not a container handle".into()));
        }

        let cli = self.runtime.cli_command();

        // Get the container's main PID
        let output = Command::new(cli)
            .args(["inspect", "--format", "{{.State.Pid}}", &handle.id])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .map_err(|e| PlatformError::Tracer(format!("failed to get container PID: {}", e)))?;

        let pid_str = String::from_utf8_lossy(&output.stdout);
        let pid: u32 = pid_str
            .trim()
            .parse()
            .map_err(|_| PlatformError::Tracer("failed to parse container PID".into()))?;

        if pid == 0 {
            return Err(PlatformError::Tracer("container not running".into()));
        }

        // Return ptrace-based tracer handle for the container's init process
        Ok(TracerHandle {
            platform: Platform::Container,
            id: format!("ptrace:{}", pid),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_adapter_creation() {
        let adapter = ContainerAdapter::new();
        assert_eq!(adapter.platform(), Platform::Container);
        assert_eq!(adapter.runtime, ContainerRuntime::Docker);
    }

    #[test]
    fn test_container_runtime_variants() {
        let docker = ContainerAdapter::with_runtime(ContainerRuntime::Docker);
        assert!(docker.socket_path.to_string_lossy().contains("docker.sock"));

        let podman = ContainerAdapter::with_runtime(ContainerRuntime::Podman);
        assert!(podman.socket_path.to_string_lossy().contains("podman.sock"));

        let containerd = ContainerAdapter::with_runtime(ContainerRuntime::Containerd);
        assert!(containerd
            .socket_path
            .to_string_lossy()
            .contains("containerd.sock"));
    }

    #[test]
    fn test_container_runtime_cli_commands() {
        assert_eq!(ContainerRuntime::Docker.cli_command(), "docker");
        assert_eq!(ContainerRuntime::Podman.cli_command(), "podman");
        assert_eq!(ContainerRuntime::Containerd.cli_command(), "nerdctl");
    }

    #[test]
    fn test_container_name_generation() {
        assert_eq!(
            ContainerAdapter::container_name("my-daemon"),
            "duende-my-daemon"
        );
        assert_eq!(
            ContainerAdapter::container_name("my daemon"),
            "duende-my-daemon"
        );
    }

    #[test]
    fn test_build_run_args_basic() {
        let adapter = ContainerAdapter::new();
        let config = DaemonConfig::new("test-daemon", "/usr/bin/test");

        let args = adapter.build_run_args(&config);

        assert!(args.contains(&"run".to_string()));
        assert!(args.contains(&"-d".to_string()));
        assert!(args.contains(&"--name".to_string()));
        assert!(args.contains(&"duende-test-daemon".to_string()));
    }

    #[test]
    fn test_build_run_args_with_resources() {
        let adapter = ContainerAdapter::new();
        let mut config = DaemonConfig::new("resource-daemon", "/usr/bin/test");
        config.resources.memory_bytes = 1024 * 1024 * 512; // 512MB
        config.resources.pids_max = 50;

        let args = adapter.build_run_args(&config);

        assert!(args.contains(&"--memory".to_string()));
        assert!(args.contains(&"--pids-limit".to_string()));
        assert!(args.contains(&"50".to_string()));
    }

    #[test]
    fn test_build_run_args_with_env() {
        let adapter = ContainerAdapter::new();
        let mut config = DaemonConfig::new("env-daemon", "/usr/bin/test");
        config.env.insert("FOO".into(), "bar".into());

        let args = adapter.build_run_args(&config);

        assert!(args.contains(&"-e".to_string()));
        assert!(args.contains(&"FOO=bar".to_string()));
    }

    #[test]
    fn test_parse_status_running() {
        let output = r#"{"Status": "running", "ExitCode": 0}"#;
        assert!(matches!(
            ContainerAdapter::parse_status(output),
            DaemonStatus::Running
        ));
    }

    #[test]
    fn test_parse_status_exited_success() {
        let output = r#"{"Status": "exited", "ExitCode": 0}"#;
        assert!(matches!(
            ContainerAdapter::parse_status(output),
            DaemonStatus::Stopped
        ));
    }

    #[test]
    fn test_parse_status_exited_failure() {
        let output = r#"{"Status": "exited", "ExitCode": 1}"#;
        assert!(matches!(
            ContainerAdapter::parse_status(output),
            DaemonStatus::Failed(_)
        ));
    }

    #[test]
    fn test_parse_status_created() {
        let output = r#"{"Status": "created", "ExitCode": 0}"#;
        assert!(matches!(
            ContainerAdapter::parse_status(output),
            DaemonStatus::Created
        ));
    }

    #[test]
    fn test_parse_status_restarting() {
        let output = r#"{"Status": "restarting", "ExitCode": 0}"#;
        assert!(matches!(
            ContainerAdapter::parse_status(output),
            DaemonStatus::Starting
        ));
    }

    #[test]
    fn test_signal_name_translation() {
        assert_eq!(ContainerAdapter::signal_name(Signal::Term), "SIGTERM");
        assert_eq!(ContainerAdapter::signal_name(Signal::Kill), "SIGKILL");
        assert_eq!(ContainerAdapter::signal_name(Signal::Hup), "SIGHUP");
        assert_eq!(ContainerAdapter::signal_name(Signal::Stop), "SIGSTOP");
        assert_eq!(ContainerAdapter::signal_name(Signal::Cont), "SIGCONT");
    }

    #[test]
    fn test_with_image_prefix() {
        let adapter = ContainerAdapter::new().with_image_prefix("myorg");
        assert_eq!(adapter.image_prefix, "myorg");
    }

    #[test]
    fn test_default_implementation() {
        let adapter = ContainerAdapter::default();
        assert_eq!(adapter.runtime, ContainerRuntime::Docker);
        assert_eq!(adapter.image_prefix, "duende");
    }
}
