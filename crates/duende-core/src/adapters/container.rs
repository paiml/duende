//! Container (Docker/OCI) adapter implementation.
//!
//! Provides daemon management via container runtimes (Docker, Podman, containerd).

use crate::adapter::{DaemonHandle, PlatformAdapter, PlatformError, PlatformResult, TracerHandle};
use crate::daemon::Daemon;
use crate::platform::Platform;
use crate::types::{DaemonStatus, FailureReason, Signal};

use async_trait::async_trait;
use tokio::process::Command;

/// Container runtime type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerRuntime {
    /// Docker runtime.
    Docker,
    /// Podman runtime.
    Podman,
    /// containerd runtime (via ctr).
    Containerd,
}

impl ContainerRuntime {
    /// Returns the runtime CLI command name.
    #[must_use]
    pub const fn command(&self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
            Self::Containerd => "ctr",
        }
    }

    /// Returns the runtime name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
            Self::Containerd => "containerd",
        }
    }
}

/// Container adapter for Docker/OCI runtimes.
///
/// Manages daemons as containers using docker, podman, or containerd.
///
/// # Example
///
/// ```rust,ignore
/// use duende_core::adapters::ContainerAdapter;
/// use duende_core::PlatformAdapter;
///
/// let adapter = ContainerAdapter::docker();
/// let handle = adapter.spawn(my_daemon).await?;
/// ```
pub struct ContainerAdapter {
    /// Container runtime
    runtime: ContainerRuntime,
    /// Default image for daemons
    default_image: String,
}

impl ContainerAdapter {
    /// Creates a new container adapter with Docker runtime.
    #[must_use]
    pub fn docker() -> Self {
        Self {
            runtime: ContainerRuntime::Docker,
            default_image: "alpine:latest".to_string(),
        }
    }

    /// Creates a new container adapter with Podman runtime.
    #[must_use]
    pub fn podman() -> Self {
        Self {
            runtime: ContainerRuntime::Podman,
            default_image: "alpine:latest".to_string(),
        }
    }

    /// Creates a new container adapter with containerd runtime.
    #[must_use]
    pub fn containerd() -> Self {
        Self {
            runtime: ContainerRuntime::Containerd,
            default_image: "docker.io/library/alpine:latest".to_string(),
        }
    }

    /// Creates adapter with custom runtime and image.
    #[must_use]
    pub fn with_config(runtime: ContainerRuntime, default_image: impl Into<String>) -> Self {
        Self {
            runtime,
            default_image: default_image.into(),
        }
    }

    /// Returns the container runtime.
    #[must_use]
    pub const fn runtime(&self) -> ContainerRuntime {
        self.runtime
    }

    /// Returns the default image.
    #[must_use]
    pub fn default_image(&self) -> &str {
        &self.default_image
    }

    /// Generates a container name from daemon name.
    fn container_name(daemon_name: &str) -> String {
        format!("duende-{}", daemon_name.replace(' ', "-").replace('_', "-"))
    }

    /// Maps Signal to container kill signal name.
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

    /// Parses container inspect output to DaemonStatus.
    fn parse_status(output: &str) -> DaemonStatus {
        // Parse JSON output from docker/podman inspect
        if output.contains("\"Running\": true") || output.contains("\"running\": true") {
            return DaemonStatus::Running;
        }

        if output.contains("\"Restarting\": true") || output.contains("\"restarting\": true") {
            return DaemonStatus::Starting;
        }

        if output.contains("\"Paused\": true") || output.contains("\"paused\": true") {
            return DaemonStatus::Paused;
        }

        // Check exit code
        if let Some(code) = Self::extract_exit_code(output) {
            if code != 0 {
                return DaemonStatus::Failed(FailureReason::ExitCode(code));
            }
        }

        DaemonStatus::Stopped
    }

    /// Extracts exit code from inspect output.
    fn extract_exit_code(output: &str) -> Option<i32> {
        // Look for "ExitCode": <number> or "ExitCode":<number>
        let patterns = ["\"ExitCode\": ", "\"ExitCode\":"];
        for pattern in patterns {
            if let Some(pos) = output.find(pattern) {
                let start = pos + pattern.len();
                let remaining = &output[start..];
                // Find the end of the number (including negative sign)
                let num_str: String = remaining
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '-')
                    .collect();
                if !num_str.is_empty() {
                    return num_str.parse().ok();
                }
            }
        }
        None
    }

    /// Gets container ID by name.
    async fn get_container_id(&self, name: &str) -> Option<String> {
        let output = Command::new(self.runtime.command())
            .args(["ps", "-aq", "--filter", &format!("name=^{}$", name)])
            .output()
            .await
            .ok()?;

        if output.status.success() {
            let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !id.is_empty() {
                return Some(id);
            }
        }

        None
    }
}

impl Default for ContainerAdapter {
    fn default() -> Self {
        Self::docker()
    }
}

#[async_trait]
impl PlatformAdapter for ContainerAdapter {
    fn platform(&self) -> Platform {
        Platform::Container
    }

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> PlatformResult<DaemonHandle> {
        let daemon_name = daemon.name().to_string();
        let daemon_id = daemon.id();
        let container_name = Self::container_name(&daemon_name);

        // Build container run command
        let mut cmd = Command::new(self.runtime.command());

        match self.runtime {
            ContainerRuntime::Docker | ContainerRuntime::Podman => {
                cmd.arg("run")
                    .arg("-d") // detached
                    .arg("--name")
                    .arg(&container_name)
                    .arg("--restart")
                    .arg("on-failure:5")
                    .arg(&self.default_image)
                    .arg("/bin/sh")
                    .arg("-c")
                    .arg("while true; do sleep 1; done"); // placeholder command
            }
            ContainerRuntime::Containerd => {
                // containerd uses ctr with different syntax
                cmd.arg("run")
                    .arg("-d")
                    .arg(&self.default_image)
                    .arg(&container_name)
                    .arg("/bin/sh")
                    .arg("-c")
                    .arg("while true; do sleep 1; done");
            }
        }

        let output = cmd.output().await.map_err(|e| {
            PlatformError::spawn_failed(format!(
                "Failed to execute {}: {}",
                self.runtime.command(),
                e
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PlatformError::spawn_failed(format!(
                "{} run failed: {}",
                self.runtime.command(),
                stderr
            )));
        }

        // Get the container ID from output
        let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let container_id = if container_id.is_empty() {
            // Try to get it by name
            self.get_container_id(&container_name)
                .await
                .unwrap_or_else(|| container_name.clone())
        } else {
            container_id
        };

        Ok(DaemonHandle::container(
            daemon_id,
            self.runtime.name(),
            container_id,
        ))
    }

    async fn signal(&self, handle: &DaemonHandle, sig: Signal) -> PlatformResult<()> {
        let container_id = handle.container_id().ok_or_else(|| {
            PlatformError::spawn_failed("Invalid handle type for container adapter")
        })?;

        let mut cmd = Command::new(self.runtime.command());

        match self.runtime {
            ContainerRuntime::Docker | ContainerRuntime::Podman => {
                cmd.arg("kill")
                    .arg("--signal")
                    .arg(Self::signal_name(sig))
                    .arg(container_id);
            }
            ContainerRuntime::Containerd => {
                // ctr uses task kill
                cmd.arg("task")
                    .arg("kill")
                    .arg("--signal")
                    .arg(Self::signal_name(sig))
                    .arg(container_id);
            }
        }

        let output = cmd.output().await.map_err(|e| {
            PlatformError::signal_failed(format!(
                "Failed to execute {}: {}",
                self.runtime.command(),
                e
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PlatformError::signal_failed(format!(
                "{} kill failed: {}",
                self.runtime.command(),
                stderr
            )));
        }

        Ok(())
    }

    async fn status(&self, handle: &DaemonHandle) -> PlatformResult<DaemonStatus> {
        let container_id = handle.container_id().ok_or_else(|| {
            PlatformError::spawn_failed("Invalid handle type for container adapter")
        })?;

        let mut cmd = Command::new(self.runtime.command());

        match self.runtime {
            ContainerRuntime::Docker | ContainerRuntime::Podman => {
                cmd.arg("inspect")
                    .arg("--format")
                    .arg("{{json .State}}")
                    .arg(container_id);
            }
            ContainerRuntime::Containerd => {
                cmd.arg("task").arg("list").arg(container_id);
            }
        }

        let output = cmd.output().await.map_err(|e| {
            PlatformError::status_failed(format!(
                "Failed to execute {}: {}",
                self.runtime.command(),
                e
            ))
        })?;

        if !output.status.success() {
            // Container doesn't exist
            return Ok(DaemonStatus::Stopped);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(Self::parse_status(&stdout))
    }

    async fn attach_tracer(&self, handle: &DaemonHandle) -> PlatformResult<TracerHandle> {
        let container_id = handle.container_id().ok_or_else(|| {
            PlatformError::spawn_failed("Invalid handle type for container adapter")
        })?;

        // Get the container's main process PID
        let mut cmd = Command::new(self.runtime.command());

        match self.runtime {
            ContainerRuntime::Docker | ContainerRuntime::Podman => {
                cmd.arg("inspect")
                    .arg("--format")
                    .arg("{{.State.Pid}}")
                    .arg(container_id);
            }
            ContainerRuntime::Containerd => {
                // ctr doesn't have a direct way to get PID
                return Err(PlatformError::tracer_failed(
                    "containerd tracer attachment not supported",
                ));
            }
        }

        let output = cmd.output().await.map_err(|e| {
            PlatformError::tracer_failed(format!("Failed to get container PID: {}", e))
        })?;

        if !output.status.success() {
            return Err(PlatformError::tracer_failed("Container not found"));
        }

        let pid: u32 = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .map_err(|_| PlatformError::tracer_failed("Invalid PID"))?;

        if pid == 0 {
            return Err(PlatformError::tracer_failed("Container not running"));
        }

        Ok(TracerHandle::ptrace(handle.id()))
    }
}

impl ContainerAdapter {
    /// Removes a container.
    pub async fn remove(&self, container_id: &str, force: bool) -> PlatformResult<()> {
        let mut cmd = Command::new(self.runtime.command());

        match self.runtime {
            ContainerRuntime::Docker | ContainerRuntime::Podman => {
                cmd.arg("rm");
                if force {
                    cmd.arg("-f");
                }
                cmd.arg(container_id);
            }
            ContainerRuntime::Containerd => {
                cmd.arg("container").arg("rm");
                if force {
                    cmd.arg("-f");
                }
                cmd.arg(container_id);
            }
        }

        let output = cmd.output().await.map_err(|e| {
            PlatformError::spawn_failed(format!("Failed to remove container: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PlatformError::spawn_failed(format!(
                "Failed to remove container: {}",
                stderr
            )));
        }

        Ok(())
    }

    /// Checks if the runtime is available.
    pub async fn is_available(&self) -> bool {
        Command::new(self.runtime.command())
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_runtime_command() {
        assert_eq!(ContainerRuntime::Docker.command(), "docker");
        assert_eq!(ContainerRuntime::Podman.command(), "podman");
        assert_eq!(ContainerRuntime::Containerd.command(), "ctr");
    }

    #[test]
    fn test_container_runtime_name() {
        assert_eq!(ContainerRuntime::Docker.name(), "docker");
        assert_eq!(ContainerRuntime::Podman.name(), "podman");
        assert_eq!(ContainerRuntime::Containerd.name(), "containerd");
    }

    #[test]
    fn test_container_adapter_docker() {
        let adapter = ContainerAdapter::docker();
        assert_eq!(adapter.runtime(), ContainerRuntime::Docker);
        assert_eq!(adapter.platform(), Platform::Container);
    }

    #[test]
    fn test_container_adapter_podman() {
        let adapter = ContainerAdapter::podman();
        assert_eq!(adapter.runtime(), ContainerRuntime::Podman);
    }

    #[test]
    fn test_container_adapter_containerd() {
        let adapter = ContainerAdapter::containerd();
        assert_eq!(adapter.runtime(), ContainerRuntime::Containerd);
    }

    #[test]
    fn test_container_adapter_default() {
        let adapter = ContainerAdapter::default();
        assert_eq!(adapter.runtime(), ContainerRuntime::Docker);
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
        assert_eq!(
            ContainerAdapter::container_name("my_daemon"),
            "duende-my-daemon"
        );
    }

    #[test]
    fn test_signal_name() {
        assert_eq!(ContainerAdapter::signal_name(Signal::Term), "SIGTERM");
        assert_eq!(ContainerAdapter::signal_name(Signal::Kill), "SIGKILL");
    }

    #[test]
    fn test_parse_status_running() {
        let output = r#"{"Running": true, "Paused": false}"#;
        assert_eq!(
            ContainerAdapter::parse_status(output),
            DaemonStatus::Running
        );
    }

    #[test]
    fn test_parse_status_paused() {
        let output = r#"{"Running": false, "Paused": true}"#;
        assert_eq!(ContainerAdapter::parse_status(output), DaemonStatus::Paused);
    }

    #[test]
    fn test_parse_status_stopped() {
        let output = r#"{"Running": false, "Paused": false, "ExitCode": 0}"#;
        assert_eq!(
            ContainerAdapter::parse_status(output),
            DaemonStatus::Stopped
        );
    }

    #[test]
    fn test_parse_status_failed() {
        let output = r#"{"Running": false, "ExitCode": 1}"#;
        assert!(matches!(
            ContainerAdapter::parse_status(output),
            DaemonStatus::Failed(_)
        ));
    }

    #[test]
    fn test_extract_exit_code() {
        assert_eq!(
            ContainerAdapter::extract_exit_code(r#""ExitCode": 0"#),
            Some(0)
        );
        assert_eq!(
            ContainerAdapter::extract_exit_code(r#""ExitCode": 1"#),
            Some(1)
        );
        assert_eq!(
            ContainerAdapter::extract_exit_code(r#""ExitCode":137"#),
            Some(137)
        );
        assert_eq!(ContainerAdapter::extract_exit_code("no exit code"), None);
    }

    #[test]
    fn test_with_config() {
        let adapter = ContainerAdapter::with_config(ContainerRuntime::Podman, "ubuntu:22.04");
        assert_eq!(adapter.runtime(), ContainerRuntime::Podman);
        assert_eq!(adapter.default_image(), "ubuntu:22.04");
    }
}
