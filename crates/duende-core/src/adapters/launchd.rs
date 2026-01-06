//! macOS launchd adapter implementation.
//!
//! Provides daemon management via launchd plist files and launchctl.

use crate::adapter::{DaemonHandle, PlatformAdapter, PlatformError, PlatformResult, TracerHandle};
use crate::daemon::Daemon;
use crate::platform::Platform;
use crate::types::{DaemonStatus, FailureReason, Signal};

use async_trait::async_trait;
use std::path::PathBuf;
use tokio::process::Command;

/// macOS launchd adapter.
///
/// Manages daemons via launchd using `launchctl` and plist files.
///
/// # Requirements
///
/// - macOS with launchd
/// - User permissions for user-level daemons, root for system-level
///
/// # Example
///
/// ```rust,ignore
/// use duende_core::adapters::LaunchdAdapter;
/// use duende_core::PlatformAdapter;
///
/// let adapter = LaunchdAdapter::user();
/// let handle = adapter.spawn(my_daemon).await?;
/// ```
pub struct LaunchdAdapter {
    /// Directory for plist files
    plist_dir: PathBuf,
    /// User or system domain
    domain: LaunchdDomain,
}

/// Launchd domain (user or system).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchdDomain {
    /// User session (~/Library/LaunchAgents)
    User,
    /// System-wide (/Library/LaunchDaemons)
    System,
}

impl LaunchdDomain {
    /// Returns the domain target for launchctl.
    fn target(&self) -> String {
        match self {
            Self::User => {
                // Get current user ID
                let uid = unsafe { libc::getuid() };
                format!("gui/{}", uid)
            }
            Self::System => "system".to_string(),
        }
    }
}

impl LaunchdAdapter {
    /// Creates a new launchd adapter with default settings (user mode).
    #[must_use]
    pub fn new() -> Self {
        Self::user()
    }

    /// Creates a new launchd adapter for user-level daemons.
    #[must_use]
    pub fn user() -> Self {
        let plist_dir = dirs_next::home_dir()
            .map(|p| p.join("Library/LaunchAgents"))
            .unwrap_or_else(|| PathBuf::from("~/Library/LaunchAgents"));

        Self {
            plist_dir,
            domain: LaunchdDomain::User,
        }
    }

    /// Creates a new launchd adapter for system-level daemons.
    ///
    /// Requires root permissions.
    #[must_use]
    pub fn system() -> Self {
        Self {
            plist_dir: PathBuf::from("/Library/LaunchDaemons"),
            domain: LaunchdDomain::System,
        }
    }

    /// Creates adapter with custom plist directory.
    #[must_use]
    pub fn with_plist_dir(plist_dir: PathBuf, domain: LaunchdDomain) -> Self {
        Self { plist_dir, domain }
    }

    /// Returns the plist directory path.
    #[must_use]
    pub fn plist_dir(&self) -> &PathBuf {
        &self.plist_dir
    }

    /// Returns the launchd domain.
    #[must_use]
    pub const fn domain(&self) -> LaunchdDomain {
        self.domain
    }

    /// Generates a service label from daemon name.
    fn service_label(daemon_name: &str) -> String {
        format!(
            "com.duende.{}",
            daemon_name.replace(' ', "-").replace('_', "-")
        )
    }

    /// Generates a plist file path.
    fn plist_path(&self, label: &str) -> PathBuf {
        self.plist_dir.join(format!("{}.plist", label))
    }

    /// Generates a plist XML for a daemon.
    fn generate_plist(label: &str, daemon_name: &str, program: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{program}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>ThrottleInterval</key>
    <integer>5</integer>
    <key>StandardOutPath</key>
    <string>/tmp/duende-{daemon_name}.out.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/duende-{daemon_name}.err.log</string>
</dict>
</plist>
"#,
            label = label,
            program = program,
            daemon_name = daemon_name
        )
    }

    /// Parses launchctl list output to determine status.
    fn parse_status(output: &str, label: &str) -> DaemonStatus {
        // launchctl list output format: PID\tStatus\tLabel
        for line in output.lines() {
            if line.contains(label) {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 2 {
                    // First column is PID (or "-" if not running)
                    let pid_str = parts[0].trim();
                    let status_code = parts[1].trim();

                    if pid_str != "-" {
                        // Running if we have a PID
                        return DaemonStatus::Running;
                    }

                    // Check exit code
                    if let Ok(code) = status_code.parse::<i32>() {
                        if code != 0 {
                            return DaemonStatus::Failed(FailureReason::ExitCode(code));
                        }
                    }

                    return DaemonStatus::Stopped;
                }
            }
        }

        // Service not found in list
        DaemonStatus::Stopped
    }

    /// Maps Signal to launchctl signal.
    fn signal_number(sig: Signal) -> i32 {
        match sig {
            Signal::Term => 15,
            Signal::Kill => 9,
            Signal::Int => 2,
            Signal::Quit => 3,
            Signal::Hup => 1,
            Signal::Usr1 => 30,
            Signal::Usr2 => 31,
            Signal::Stop => 17,
            Signal::Cont => 19,
        }
    }
}

impl Default for LaunchdAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformAdapter for LaunchdAdapter {
    fn platform(&self) -> Platform {
        Platform::MacOS
    }

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> PlatformResult<DaemonHandle> {
        let daemon_name = daemon.name().to_string();
        let daemon_id = daemon.id();
        let label = Self::service_label(&daemon_name);
        let plist_path = self.plist_path(&label);

        // Ensure plist directory exists
        tokio::fs::create_dir_all(&self.plist_dir)
            .await
            .map_err(|e| {
                PlatformError::spawn_failed(format!("Failed to create plist directory: {}", e))
            })?;

        // Generate and write plist file
        // For now, use /bin/true as placeholder - real implementation would use daemon config
        let plist_content = Self::generate_plist(&label, &daemon_name, "/usr/bin/true");
        tokio::fs::write(&plist_path, &plist_content)
            .await
            .map_err(|e| {
                PlatformError::spawn_failed(format!("Failed to write plist file: {}", e))
            })?;

        // Bootstrap the service
        let output = Command::new("launchctl")
            .arg("bootstrap")
            .arg(self.domain.target())
            .arg(&plist_path)
            .output()
            .await
            .map_err(|e| {
                PlatformError::spawn_failed(format!("Failed to execute launchctl: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Clean up plist file on failure
            let _ = tokio::fs::remove_file(&plist_path).await;
            return Err(PlatformError::spawn_failed(format!(
                "launchctl bootstrap failed: {}",
                stderr
            )));
        }

        Ok(DaemonHandle::launchd(daemon_id, label))
    }

    async fn signal(&self, handle: &DaemonHandle, sig: Signal) -> PlatformResult<()> {
        let label = handle.launchd_label().ok_or_else(|| {
            PlatformError::spawn_failed("Invalid handle type for launchd adapter")
        })?;

        // Use launchctl kill to send signal
        let output = Command::new("launchctl")
            .arg("kill")
            .arg(Self::signal_number(sig).to_string())
            .arg(format!("{}/{}", self.domain.target(), label))
            .output()
            .await
            .map_err(|e| {
                PlatformError::signal_failed(format!("Failed to execute launchctl: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PlatformError::signal_failed(format!(
                "launchctl kill failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    async fn status(&self, handle: &DaemonHandle) -> PlatformResult<DaemonStatus> {
        let label = handle.launchd_label().ok_or_else(|| {
            PlatformError::spawn_failed("Invalid handle type for launchd adapter")
        })?;

        let output = Command::new("launchctl")
            .arg("list")
            .output()
            .await
            .map_err(|e| {
                PlatformError::status_failed(format!("Failed to execute launchctl: {}", e))
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(Self::parse_status(&stdout, label))
    }

    async fn attach_tracer(&self, handle: &DaemonHandle) -> PlatformResult<TracerHandle> {
        let label = handle.launchd_label().ok_or_else(|| {
            PlatformError::spawn_failed("Invalid handle type for launchd adapter")
        })?;

        // Get PID from launchctl list
        let output = Command::new("launchctl")
            .arg("list")
            .arg(label)
            .output()
            .await
            .map_err(|e| PlatformError::tracer_failed(format!("Failed to get PID: {}", e)))?;

        if !output.status.success() {
            return Err(PlatformError::tracer_failed("Service not found"));
        }

        // Parse PID from output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let pid: u32 = stdout
            .lines()
            .find(|line| line.contains("PID"))
            .and_then(|line| line.split_whitespace().last())
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| PlatformError::tracer_failed("Could not determine PID"))?;

        if pid == 0 {
            return Err(PlatformError::tracer_failed("Service not running"));
        }

        Ok(TracerHandle::ptrace(handle.id()))
    }
}

impl LaunchdAdapter {
    /// Stops and removes a launchd service.
    pub async fn stop_and_unload(&self, label: &str) -> PlatformResult<()> {
        // Bootout (stop and unload) the service
        let output = Command::new("launchctl")
            .arg("bootout")
            .arg(format!("{}/{}", self.domain.target(), label))
            .output()
            .await
            .map_err(|e| {
                PlatformError::spawn_failed(format!("Failed to execute launchctl: {}", e))
            })?;

        if !output.status.success() {
            // Ignore errors - service might already be unloaded
        }

        // Remove plist file
        let plist_path = self.plist_path(label);
        let _ = tokio::fs::remove_file(&plist_path).await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_launchd_adapter_user() {
        let adapter = LaunchdAdapter::user();
        assert_eq!(adapter.domain(), LaunchdDomain::User);
        assert_eq!(adapter.platform(), Platform::MacOS);
    }

    #[test]
    fn test_launchd_adapter_system() {
        let adapter = LaunchdAdapter::system();
        assert_eq!(adapter.domain(), LaunchdDomain::System);
        assert_eq!(
            adapter.plist_dir(),
            &PathBuf::from("/Library/LaunchDaemons")
        );
    }

    #[test]
    fn test_launchd_adapter_default() {
        let adapter = LaunchdAdapter::default();
        assert_eq!(adapter.domain(), LaunchdDomain::User);
    }

    #[test]
    fn test_service_label_generation() {
        assert_eq!(
            LaunchdAdapter::service_label("my-daemon"),
            "com.duende.my-daemon"
        );
        assert_eq!(
            LaunchdAdapter::service_label("my daemon"),
            "com.duende.my-daemon"
        );
        assert_eq!(
            LaunchdAdapter::service_label("my_daemon"),
            "com.duende.my-daemon"
        );
    }

    #[test]
    fn test_signal_number() {
        assert_eq!(LaunchdAdapter::signal_number(Signal::Term), 15);
        assert_eq!(LaunchdAdapter::signal_number(Signal::Kill), 9);
        assert_eq!(LaunchdAdapter::signal_number(Signal::Hup), 1);
    }

    #[test]
    fn test_parse_status_running() {
        let output = "12345\t0\tcom.duende.test";
        assert_eq!(
            LaunchdAdapter::parse_status(output, "com.duende.test"),
            DaemonStatus::Running
        );
    }

    #[test]
    fn test_parse_status_stopped() {
        let output = "-\t0\tcom.duende.test";
        assert_eq!(
            LaunchdAdapter::parse_status(output, "com.duende.test"),
            DaemonStatus::Stopped
        );
    }

    #[test]
    fn test_parse_status_failed() {
        let output = "-\t1\tcom.duende.test";
        assert!(matches!(
            LaunchdAdapter::parse_status(output, "com.duende.test"),
            DaemonStatus::Failed(_)
        ));
    }

    #[test]
    fn test_parse_status_not_found() {
        let output = "-\t0\tcom.other.service";
        assert_eq!(
            LaunchdAdapter::parse_status(output, "com.duende.test"),
            DaemonStatus::Stopped
        );
    }

    #[test]
    fn test_plist_generation() {
        let plist = LaunchdAdapter::generate_plist("com.duende.test", "test", "/usr/bin/test");
        assert!(plist.contains("com.duende.test"));
        assert!(plist.contains("/usr/bin/test"));
        assert!(plist.contains("KeepAlive"));
    }

    #[test]
    fn test_domain_target_system() {
        assert_eq!(LaunchdDomain::System.target(), "system");
    }
}
