//! pepita MicroVM platform adapter.
//!
//! # Overview
//!
//! This module provides pepita MicroVM integration for daemon management.
//! It handles:
//!
//! - VM creation with configured vCPUs and memory
//! - Virtio-vsock communication (CID-based addressing)
//! - Virtio-blk file passing for root filesystem
//! - Guest kernel boot with custom parameters
//!
//! # Reference
//!
//! pepita is part of the PAIML Sovereign AI Stack, providing lightweight
//! microVMs for isolated workload execution with sub-100ms boot times.
//!
//! # Toyota Way: Muda (無駄)
//!
//! MicroVMs eliminate waste by providing minimal-footprint isolation
//! compared to full VMs. Boot time <100ms, memory overhead <10MB.

use crate::{DaemonHandle, Platform, PlatformAdapter, PlatformError, Result, TracerHandle};
use async_trait::async_trait;
use duende_core::{Daemon, DaemonConfig, DaemonStatus, FailureReason, Signal};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::process::Command;

/// Global CID counter for unique VM addressing.
static NEXT_CID: AtomicU32 = AtomicU32::new(3); // Start at 3 (0-2 reserved)

/// pepita MicroVM adapter.
///
/// Manages daemons inside pepita microVMs with virtio communication.
#[derive(Debug)]
pub struct PepitaAdapter {
    /// pepita binary path
    pepita_path: PathBuf,
    /// Default vCPU count
    default_vcpus: u32,
    /// Default memory (MB)
    default_memory_mb: u64,
    /// Kernel image path
    kernel_path: Option<PathBuf>,
    /// Root filesystem path
    rootfs_path: Option<PathBuf>,
    /// vsock port for daemon communication
    vsock_port: u32,
}

impl Default for PepitaAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PepitaAdapter {
    /// Create a new pepita adapter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pepita_path: PathBuf::from("pepita"),
            default_vcpus: 1,
            default_memory_mb: 512,
            kernel_path: None,
            rootfs_path: None,
            vsock_port: 5000,
        }
    }

    /// Create with custom pepita binary path.
    #[must_use]
    pub fn with_binary(pepita_path: PathBuf) -> Self {
        Self {
            pepita_path,
            default_vcpus: 1,
            default_memory_mb: 512,
            kernel_path: None,
            rootfs_path: None,
            vsock_port: 5000,
        }
    }

    /// Set default vCPU count.
    #[must_use]
    pub fn with_vcpus(mut self, vcpus: u32) -> Self {
        self.default_vcpus = vcpus;
        self
    }

    /// Set default memory.
    #[must_use]
    pub fn with_memory_mb(mut self, memory_mb: u64) -> Self {
        self.default_memory_mb = memory_mb;
        self
    }

    /// Set kernel image path.
    #[must_use]
    pub fn with_kernel(mut self, kernel_path: PathBuf) -> Self {
        self.kernel_path = Some(kernel_path);
        self
    }

    /// Set root filesystem path.
    #[must_use]
    pub fn with_rootfs(mut self, rootfs_path: PathBuf) -> Self {
        self.rootfs_path = Some(rootfs_path);
        self
    }

    /// Set vsock port.
    #[must_use]
    pub fn with_vsock_port(mut self, port: u32) -> Self {
        self.vsock_port = port;
        self
    }

    /// Allocate a unique CID for a new VM.
    fn allocate_cid() -> u32 {
        NEXT_CID.fetch_add(1, Ordering::SeqCst)
    }

    /// Generate VM name for a daemon.
    fn vm_name(daemon_name: &str) -> String {
        format!("duende-vm-{}", daemon_name.replace(' ', "-"))
    }

    /// Build pepita run arguments from config.
    fn build_run_args(&self, config: &DaemonConfig, cid: u32) -> Vec<String> {
        let mut args = vec!["run".to_string()];

        // VM name
        args.push("--name".to_string());
        args.push(Self::vm_name(&config.name));

        // vCPUs
        let vcpus = if config.resources.cpu_quota_percent > 0.0 {
            // Map CPU quota to vCPU count (rough approximation)
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let v = (config.resources.cpu_quota_percent / 100.0).ceil() as u32;
            v.max(1)
        } else {
            self.default_vcpus
        };
        args.push("--vcpus".to_string());
        args.push(vcpus.to_string());

        // Memory
        let memory_mb = if config.resources.memory_bytes > 0 {
            config.resources.memory_bytes / (1024 * 1024)
        } else {
            self.default_memory_mb
        };
        args.push("--memory".to_string());
        args.push(format!("{}M", memory_mb));

        // Kernel
        if let Some(ref kernel) = self.kernel_path {
            args.push("--kernel".to_string());
            args.push(kernel.display().to_string());
        }

        // Root filesystem
        if let Some(ref rootfs) = self.rootfs_path {
            args.push("--rootfs".to_string());
            args.push(rootfs.display().to_string());
        }

        // vsock for communication
        args.push("--vsock".to_string());
        args.push(format!("cid={},port={}", cid, self.vsock_port));

        // Binary to run inside VM
        args.push("--exec".to_string());
        args.push(config.binary_path.display().to_string());

        // Arguments
        for arg in &config.args {
            args.push(arg.clone());
        }

        args
    }

    /// Parse pepita status output.
    fn parse_status(output: &str) -> DaemonStatus {
        // Expected format: "state: running" or "state: stopped" etc.
        let output_lower = output.to_lowercase();
        if output_lower.contains("running") {
            DaemonStatus::Running
        } else if output_lower.contains("stopped") || output_lower.contains("shutdown") {
            DaemonStatus::Stopped
        } else if output_lower.contains("crashed") || output_lower.contains("failed") {
            DaemonStatus::Failed(FailureReason::ExitCode(1))
        } else if output_lower.contains("booting") || output_lower.contains("starting") {
            DaemonStatus::Starting
        } else if output_lower.contains("pausing") || output_lower.contains("stopping") {
            DaemonStatus::Stopping
        } else {
            // Default to Created for unknown states (including "created", "pending", etc.)
            DaemonStatus::Created
        }
    }

    /// Translate Signal to pepita signal name.
    fn signal_name(signal: Signal) -> &'static str {
        match signal {
            Signal::Term => "TERM",
            Signal::Kill => "KILL",
            Signal::Hup => "HUP",
            Signal::Int => "INT",
            Signal::Quit => "QUIT",
            Signal::Usr1 => "USR1",
            Signal::Usr2 => "USR2",
            Signal::Stop => "STOP",
            Signal::Cont => "CONT",
        }
    }
}

#[async_trait]
impl PlatformAdapter for PepitaAdapter {
    fn platform(&self) -> Platform {
        Platform::PepitaMicroVM
    }

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> Result<DaemonHandle> {
        let config = DaemonConfig::new(daemon.name(), "/bin/daemon");
        let cid = Self::allocate_cid();
        let vm_name = Self::vm_name(daemon.name());

        // Build and run VM
        let args = self.build_run_args(&config, cid);
        let output = Command::new(&self.pepita_path)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PlatformError::Spawn(format!("pepita run failed: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PlatformError::Spawn(format!(
                "pepita run failed: {}",
                stderr
            )));
        }

        // VM ID is the name or from stdout
        let vm_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let id = if vm_id.is_empty() { vm_name } else { vm_id };

        Ok(DaemonHandle::pepita(id))
    }

    async fn signal(&self, handle: &DaemonHandle, signal: Signal) -> Result<()> {
        if handle.platform != Platform::PepitaMicroVM {
            return Err(PlatformError::Signal("not a pepita handle".into()));
        }

        let sig_name = Self::signal_name(signal);

        // Send signal via pepita signal command
        let output = Command::new(&self.pepita_path)
            .args(["signal", "--vm", &handle.id, "--signal", sig_name])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| PlatformError::Signal(format!("pepita signal failed: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PlatformError::Signal(format!(
                "pepita signal failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    async fn status(&self, handle: &DaemonHandle) -> Result<DaemonStatus> {
        if handle.platform != Platform::PepitaMicroVM {
            return Err(PlatformError::Status("not a pepita handle".into()));
        }

        let output = Command::new(&self.pepita_path)
            .args(["status", "--vm", &handle.id])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .map_err(|e| PlatformError::Status(format!("pepita status failed: {}", e)))?;

        if !output.status.success() {
            // VM not found = stopped
            return Ok(DaemonStatus::Stopped);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(Self::parse_status(&stdout))
    }

    async fn attach_tracer(&self, handle: &DaemonHandle) -> Result<TracerHandle> {
        if handle.platform != Platform::PepitaMicroVM {
            return Err(PlatformError::Tracer("not a pepita handle".into()));
        }

        // For MicroVMs, we use vsock-based tracing
        // Get the VM's CID from pepita info
        let output = Command::new(&self.pepita_path)
            .args(["info", "--vm", &handle.id, "--format", "json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .map_err(|e| PlatformError::Tracer(format!("failed to get VM info: {}", e)))?;

        if !output.status.success() {
            return Err(PlatformError::Tracer("VM not running".into()));
        }

        // Parse CID from JSON output (simplified)
        let stdout = String::from_utf8_lossy(&output.stdout);
        let cid = stdout
            .lines()
            .find_map(|line| {
                if line.contains("\"cid\":") {
                    line.split(':')
                        .nth(1)
                        .and_then(|s| s.trim().trim_matches(['"', ','].as_ref()).parse().ok())
                } else {
                    None
                }
            })
            .unwrap_or(3u32); // Default CID if not found

        // Return vsock-based tracer handle
        Ok(TracerHandle {
            platform: Platform::PepitaMicroVM,
            id: format!("vsock:{}:{}", cid, self.vsock_port),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pepita_adapter_creation() {
        let adapter = PepitaAdapter::new();
        assert_eq!(adapter.platform(), Platform::PepitaMicroVM);
        assert_eq!(adapter.default_vcpus, 1);
        assert_eq!(adapter.default_memory_mb, 512);
    }

    #[test]
    fn test_pepita_builder_pattern() {
        let adapter = PepitaAdapter::new().with_vcpus(4).with_memory_mb(2048);

        assert_eq!(adapter.default_vcpus, 4);
        assert_eq!(adapter.default_memory_mb, 2048);
    }

    #[test]
    fn test_pepita_with_kernel_and_rootfs() {
        let adapter = PepitaAdapter::new()
            .with_kernel("/boot/vmlinux".into())
            .with_rootfs("/images/rootfs.ext4".into());

        assert_eq!(adapter.kernel_path, Some(PathBuf::from("/boot/vmlinux")));
        assert_eq!(
            adapter.rootfs_path,
            Some(PathBuf::from("/images/rootfs.ext4"))
        );
    }

    #[test]
    fn test_pepita_with_vsock_port() {
        let adapter = PepitaAdapter::new().with_vsock_port(9000);
        assert_eq!(adapter.vsock_port, 9000);
    }

    #[test]
    fn test_vm_name_generation() {
        assert_eq!(PepitaAdapter::vm_name("my-daemon"), "duende-vm-my-daemon");
        assert_eq!(PepitaAdapter::vm_name("my daemon"), "duende-vm-my-daemon");
    }

    #[test]
    fn test_cid_allocation() {
        let cid1 = PepitaAdapter::allocate_cid();
        let cid2 = PepitaAdapter::allocate_cid();
        assert!(cid2 > cid1, "CIDs should increment");
        assert!(cid1 >= 3, "CIDs should start at 3 (0-2 reserved)");
    }

    #[test]
    fn test_build_run_args_basic() {
        let adapter = PepitaAdapter::new();
        let config = DaemonConfig::new("test-daemon", "/usr/bin/test");

        let args = adapter.build_run_args(&config, 10);

        assert!(args.contains(&"run".to_string()));
        assert!(args.contains(&"--name".to_string()));
        assert!(args.contains(&"--vcpus".to_string()));
        assert!(args.contains(&"--memory".to_string()));
        assert!(args.contains(&"--vsock".to_string()));
    }

    #[test]
    fn test_build_run_args_with_kernel() {
        let adapter = PepitaAdapter::new()
            .with_kernel("/boot/vmlinux".into())
            .with_rootfs("/images/rootfs.ext4".into());
        let config = DaemonConfig::new("test-daemon", "/usr/bin/test");

        let args = adapter.build_run_args(&config, 10);

        assert!(args.contains(&"--kernel".to_string()));
        assert!(args.contains(&"/boot/vmlinux".to_string()));
        assert!(args.contains(&"--rootfs".to_string()));
        assert!(args.contains(&"/images/rootfs.ext4".to_string()));
    }

    #[test]
    fn test_parse_status_running() {
        let output = "state: running\npid: 1234\n";
        assert!(matches!(
            PepitaAdapter::parse_status(output),
            DaemonStatus::Running
        ));
    }

    #[test]
    fn test_parse_status_stopped() {
        let output = "state: stopped\n";
        assert!(matches!(
            PepitaAdapter::parse_status(output),
            DaemonStatus::Stopped
        ));
    }

    #[test]
    fn test_parse_status_crashed() {
        let output = "state: crashed\nexit_code: 1\n";
        assert!(matches!(
            PepitaAdapter::parse_status(output),
            DaemonStatus::Failed(_)
        ));
    }

    #[test]
    fn test_parse_status_booting() {
        let output = "state: booting\n";
        assert!(matches!(
            PepitaAdapter::parse_status(output),
            DaemonStatus::Starting
        ));
    }

    #[test]
    fn test_signal_name_translation() {
        assert_eq!(PepitaAdapter::signal_name(Signal::Term), "TERM");
        assert_eq!(PepitaAdapter::signal_name(Signal::Kill), "KILL");
        assert_eq!(PepitaAdapter::signal_name(Signal::Hup), "HUP");
        assert_eq!(PepitaAdapter::signal_name(Signal::Stop), "STOP");
        assert_eq!(PepitaAdapter::signal_name(Signal::Cont), "CONT");
    }

    #[test]
    fn test_default_implementation() {
        let adapter = PepitaAdapter::default();
        assert_eq!(adapter.default_vcpus, 1);
        assert_eq!(adapter.default_memory_mb, 512);
        assert_eq!(adapter.vsock_port, 5000);
    }

    #[test]
    fn test_with_binary_path() {
        let adapter = PepitaAdapter::with_binary("/usr/local/bin/pepita".into());
        assert_eq!(adapter.pepita_path, PathBuf::from("/usr/local/bin/pepita"));
    }
}
