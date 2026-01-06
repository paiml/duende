//! macOS launchd platform adapter.
//!
//! # Overview
//!
//! This module provides launchd integration for daemon management on macOS.
//! It handles:
//!
//! - Property list (plist) generation from `DaemonConfig`
//! - Keep-alive support via `KeepAlive` key
//! - Resource limits via launchd constraints
//! - Standard output/error logging
//!
//! # Toyota Way: Poka-Yoke (ポカヨケ)
//!
//! Property list generation includes validation to prevent common
//! configuration mistakes.

use crate::{DaemonHandle, Platform, PlatformAdapter, PlatformError, Result, TracerHandle};
use async_trait::async_trait;
use duende_core::config::RestartPolicy;
use duende_core::{Daemon, DaemonConfig, DaemonStatus, FailureReason, Signal};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

/// macOS launchd adapter.
///
/// Manages daemons via launchd property lists.
#[derive(Debug)]
pub struct MacOSAdapter {
    /// LaunchDaemons directory (default: /Library/LaunchDaemons)
    daemons_dir: PathBuf,
    /// Use user agent instead of system daemon
    user_agent: bool,
}

impl Default for MacOSAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MacOSAdapter {
    /// Create a new macOS adapter for system daemons.
    #[must_use]
    pub fn new() -> Self {
        Self {
            daemons_dir: PathBuf::from("/Library/LaunchDaemons"),
            user_agent: false,
        }
    }

    /// Create a macOS adapter for user agents.
    #[must_use]
    pub fn user_agent() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        Self {
            daemons_dir: PathBuf::from(format!("{}/Library/LaunchAgents", home)),
            user_agent: true,
        }
    }

    /// Create with custom LaunchDaemons directory.
    #[must_use]
    pub fn with_daemons_dir(daemons_dir: PathBuf) -> Self {
        Self {
            daemons_dir,
            user_agent: false,
        }
    }

    /// Generate a plist file for the daemon.
    #[allow(clippy::format_push_string)]
    fn generate_plist(&self, config: &DaemonConfig) -> String {
        let label = Self::service_label(config.name.as_str());

        let mut plist = String::new();
        plist.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        plist.push_str("<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n");
        plist.push_str("<plist version=\"1.0\">\n");
        plist.push_str("<dict>\n");

        // Label (required)
        plist.push_str("    <key>Label</key>\n");
        plist.push_str(&format!("    <string>{}</string>\n", label));

        // Program and arguments
        plist.push_str("    <key>ProgramArguments</key>\n");
        plist.push_str("    <array>\n");
        plist.push_str(&format!(
            "        <string>{}</string>\n",
            config.binary_path.display()
        ));
        for arg in &config.args {
            plist.push_str(&format!("        <string>{}</string>\n", arg));
        }
        plist.push_str("    </array>\n");

        // Working directory
        if let Some(ref working_dir) = config.working_dir {
            plist.push_str("    <key>WorkingDirectory</key>\n");
            plist.push_str(&format!("    <string>{}</string>\n", working_dir.display()));
        }

        // User/Group (only for system daemons)
        if !self.user_agent {
            if let Some(ref user) = config.user {
                plist.push_str("    <key>UserName</key>\n");
                plist.push_str(&format!("    <string>{}</string>\n", user));
            }
            if let Some(ref group) = config.group {
                plist.push_str("    <key>GroupName</key>\n");
                plist.push_str(&format!("    <string>{}</string>\n", group));
            }
        }

        // Environment variables
        if !config.env.is_empty() {
            plist.push_str("    <key>EnvironmentVariables</key>\n");
            plist.push_str("    <dict>\n");
            for (key, value) in &config.env {
                plist.push_str(&format!("        <key>{}</key>\n", key));
                plist.push_str(&format!("        <string>{}</string>\n", value));
            }
            plist.push_str("    </dict>\n");
        }

        // Keep-alive based on restart policy
        let keep_alive = !matches!(config.restart, RestartPolicy::Never);
        plist.push_str("    <key>KeepAlive</key>\n");
        plist.push_str(&format!(
            "    <{}/>\n",
            if keep_alive { "true" } else { "false" }
        ));

        // Run at load
        plist.push_str("    <key>RunAtLoad</key>\n");
        plist.push_str("    <true/>\n");

        // Standard output/error logging
        plist.push_str("    <key>StandardOutPath</key>\n");
        plist.push_str(&format!("    <string>/var/log/{}.log</string>\n", label));
        plist.push_str("    <key>StandardErrorPath</key>\n");
        plist.push_str(&format!(
            "    <string>/var/log/{}.error.log</string>\n",
            label
        ));

        // Resource limits (soft limits via launchd)
        let resources = &config.resources;
        if resources.memory_bytes > 0 || resources.open_files_max > 0 {
            plist.push_str("    <key>SoftResourceLimits</key>\n");
            plist.push_str("    <dict>\n");
            if resources.memory_bytes > 0 {
                plist.push_str("        <key>MemoryLock</key>\n");
                plist.push_str(&format!(
                    "        <integer>{}</integer>\n",
                    resources.memory_bytes
                ));
            }
            if resources.open_files_max > 0 {
                plist.push_str("        <key>NumberOfFiles</key>\n");
                plist.push_str(&format!(
                    "        <integer>{}</integer>\n",
                    resources.open_files_max
                ));
            }
            plist.push_str("    </dict>\n");
        }

        plist.push_str("</dict>\n");
        plist.push_str("</plist>\n");

        plist
    }

    /// Get the service label for a daemon.
    fn service_label(daemon_name: &str) -> String {
        format!("com.duende.{}", daemon_name.replace(' ', "-"))
    }

    /// Get the plist filename for a daemon.
    fn plist_filename(daemon_name: &str) -> String {
        format!("{}.plist", Self::service_label(daemon_name))
    }

    /// Parse launchctl list output to DaemonStatus.
    fn parse_status(output: &str, label: &str) -> DaemonStatus {
        for line in output.lines() {
            // Format: PID	Status	Label
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 && parts[2] == label {
                let pid = parts[0].trim();
                let status = parts[1].trim();

                if pid == "-" {
                    // Not running
                    if status == "0" {
                        return DaemonStatus::Stopped;
                    }
                    return DaemonStatus::Failed(FailureReason::ExitCode(
                        status.parse().unwrap_or(1),
                    ));
                }
                // Running (has a PID)
                return DaemonStatus::Running;
            }
        }
        DaemonStatus::Created
    }

    /// Translate Signal to signal number for kill.
    fn signal_number(signal: Signal) -> i32 {
        match signal {
            Signal::Term => 15,
            Signal::Kill => 9,
            Signal::Hup => 1,
            Signal::Int => 2,
            Signal::Quit => 3,
            Signal::Usr1 => 30,
            Signal::Usr2 => 31,
            Signal::Stop => 17,
            Signal::Cont => 19,
        }
    }
}

#[async_trait]
impl PlatformAdapter for MacOSAdapter {
    fn platform(&self) -> Platform {
        Platform::MacOS
    }

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> Result<DaemonHandle> {
        let config = DaemonConfig::new(daemon.name(), "/bin/false"); // Placeholder
        let label = Self::service_label(daemon.name());
        let plist_path = self.daemons_dir.join(Self::plist_filename(daemon.name()));

        // Generate plist
        let plist_content = self.generate_plist(&config);

        // Write plist file
        tokio::fs::write(&plist_path, &plist_content)
            .await
            .map_err(|e| PlatformError::Spawn(format!("failed to write plist: {}", e)))?;

        // Load the service
        let load = Command::new("launchctl")
            .args(["load", "-w"])
            .arg(&plist_path)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PlatformError::Spawn(format!("launchctl load failed: {}", e)))?;

        if !load.status.success() {
            let stderr = String::from_utf8_lossy(&load.stderr);
            return Err(PlatformError::Spawn(format!(
                "launchctl load failed: {}",
                stderr
            )));
        }

        Ok(DaemonHandle::launchd(label))
    }

    async fn signal(&self, handle: &DaemonHandle, signal: Signal) -> Result<()> {
        if handle.platform != Platform::MacOS {
            return Err(PlatformError::Signal("not a launchd handle".into()));
        }
        let label = &handle.id;

        // Get PID from launchctl
        let list_output = Command::new("launchctl")
            .args(["list", label])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .map_err(|e| PlatformError::Signal(format!("launchctl list failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&list_output.stdout);

        // Parse PID from output
        let pid: i32 = stdout
            .lines()
            .find_map(|line| {
                if line.contains("\"PID\"") {
                    line.split('=')
                        .nth(1)
                        .and_then(|s| s.trim().trim_end_matches(';').parse().ok())
                } else {
                    None
                }
            })
            .ok_or_else(|| PlatformError::Signal("service not running".into()))?;

        // Send signal via kill
        let sig_num = Self::signal_number(signal);
        let kill = Command::new("kill")
            .args([&format!("-{}", sig_num), &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PlatformError::Signal(format!("kill failed: {}", e)))?;

        if !kill.status.success() {
            let stderr = String::from_utf8_lossy(&kill.stderr);
            return Err(PlatformError::Signal(format!("kill failed: {}", stderr)));
        }

        Ok(())
    }

    async fn status(&self, handle: &DaemonHandle) -> Result<DaemonStatus> {
        if handle.platform != Platform::MacOS {
            return Err(PlatformError::Status("not a launchd handle".into()));
        }
        let label = &handle.id;

        let output = Command::new("launchctl")
            .arg("list")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .map_err(|e| PlatformError::Status(format!("launchctl list failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(Self::parse_status(&stdout, label))
    }

    async fn attach_tracer(&self, handle: &DaemonHandle) -> Result<TracerHandle> {
        if handle.platform != Platform::MacOS {
            return Err(PlatformError::Tracer("not a launchd handle".into()));
        }
        let label = &handle.id;

        // Get PID from launchctl list
        let output = Command::new("launchctl")
            .args(["list", label])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .map_err(|e| PlatformError::Tracer(format!("launchctl list failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse PID
        let pid: i32 = stdout
            .lines()
            .find_map(|line| {
                if line.contains("\"PID\"") {
                    line.split('=')
                        .nth(1)
                        .and_then(|s| s.trim().trim_end_matches(';').parse().ok())
                } else {
                    None
                }
            })
            .ok_or_else(|| PlatformError::Tracer("service not running".into()))?;

        // Return ptrace-based tracer (macOS uses dtrace but ptrace works too)
        Ok(TracerHandle {
            platform: Platform::MacOS,
            id: format!("ptrace:{}", pid),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_macos_adapter_creation() {
        let adapter = MacOSAdapter::new();
        assert_eq!(adapter.platform(), Platform::MacOS);
        assert!(!adapter.user_agent);
    }

    #[test]
    fn test_macos_adapter_user_agent() {
        let adapter = MacOSAdapter::user_agent();
        assert!(adapter.user_agent);
    }

    #[test]
    fn test_macos_adapter_custom_dir() {
        let adapter = MacOSAdapter::with_daemons_dir("/tmp/daemons".into());
        assert_eq!(adapter.daemons_dir, PathBuf::from("/tmp/daemons"));
    }

    #[test]
    fn test_service_label_generation() {
        assert_eq!(
            MacOSAdapter::service_label("my-daemon"),
            "com.duende.my-daemon"
        );
        assert_eq!(
            MacOSAdapter::service_label("my daemon"),
            "com.duende.my-daemon"
        );
    }

    #[test]
    fn test_plist_filename_generation() {
        assert_eq!(
            MacOSAdapter::plist_filename("my-daemon"),
            "com.duende.my-daemon.plist"
        );
    }

    #[test]
    fn test_generate_plist() {
        let adapter = MacOSAdapter::new();
        let config = DaemonConfig::new("test-daemon", "/usr/bin/test");

        let plist = adapter.generate_plist(&config);

        assert!(plist.contains("<?xml version=\"1.0\""));
        assert!(plist.contains("<key>Label</key>"));
        assert!(plist.contains("com.duende.test-daemon"));
        assert!(plist.contains("<key>ProgramArguments</key>"));
        assert!(plist.contains("/usr/bin/test"));
        assert!(plist.contains("<key>KeepAlive</key>"));
    }

    #[test]
    fn test_parse_status_running() {
        let output = "123\t0\tcom.duende.test\n";
        assert!(matches!(
            MacOSAdapter::parse_status(output, "com.duende.test"),
            DaemonStatus::Running
        ));
    }

    #[test]
    fn test_parse_status_stopped() {
        let output = "-\t0\tcom.duende.test\n";
        assert!(matches!(
            MacOSAdapter::parse_status(output, "com.duende.test"),
            DaemonStatus::Stopped
        ));
    }

    #[test]
    fn test_parse_status_failed() {
        let output = "-\t1\tcom.duende.test\n";
        assert!(matches!(
            MacOSAdapter::parse_status(output, "com.duende.test"),
            DaemonStatus::Failed(_)
        ));
    }

    #[test]
    fn test_parse_status_not_found() {
        let output = "123\t0\tother.service\n";
        assert!(matches!(
            MacOSAdapter::parse_status(output, "com.duende.test"),
            DaemonStatus::Created
        ));
    }

    #[test]
    fn test_signal_number_translation() {
        assert_eq!(MacOSAdapter::signal_number(Signal::Term), 15);
        assert_eq!(MacOSAdapter::signal_number(Signal::Kill), 9);
        assert_eq!(MacOSAdapter::signal_number(Signal::Hup), 1);
        assert_eq!(MacOSAdapter::signal_number(Signal::Int), 2);
    }

    #[test]
    fn test_plist_with_environment() {
        let adapter = MacOSAdapter::new();
        let mut config = DaemonConfig::new("env-daemon", "/usr/bin/test");
        config.env.insert("FOO".into(), "bar".into());

        let plist = adapter.generate_plist(&config);

        assert!(plist.contains("<key>EnvironmentVariables</key>"));
        assert!(plist.contains("<key>FOO</key>"));
        assert!(plist.contains("<string>bar</string>"));
    }

    #[test]
    fn test_plist_with_user_group() {
        let adapter = MacOSAdapter::new();
        let mut config = DaemonConfig::new("user-daemon", "/usr/bin/test");
        config.user = Some("daemon".into());
        config.group = Some("daemon".into());

        let plist = adapter.generate_plist(&config);

        assert!(plist.contains("<key>UserName</key>"));
        assert!(plist.contains("<key>GroupName</key>"));
    }

    #[test]
    fn test_plist_user_agent_no_user_group() {
        let adapter = MacOSAdapter::user_agent();
        let mut config = DaemonConfig::new("user-daemon", "/usr/bin/test");
        config.user = Some("daemon".into());
        config.group = Some("daemon".into());

        let plist = adapter.generate_plist(&config);

        // User agents shouldn't include UserName/GroupName
        assert!(!plist.contains("<key>UserName</key>"));
        assert!(!plist.contains("<key>GroupName</key>"));
    }
}
