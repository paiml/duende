//! pepita MicroVM adapter implementation.
//!
//! Provides daemon management via pepita microVMs with vsock communication.
//!
//! pepita is PAIML's lightweight microVM implementation similar to Firecracker,
//! optimized for running single-purpose daemons with minimal overhead.

use crate::adapter::{DaemonHandle, PlatformAdapter, PlatformError, PlatformResult, TracerHandle};
use crate::daemon::Daemon;
use crate::platform::Platform;
use crate::types::{DaemonStatus, FailureReason, Signal};

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Vsock CID allocator.
static NEXT_CID: AtomicU32 = AtomicU32::new(3); // CID 0-2 are reserved

/// pepita MicroVM adapter.
///
/// Manages daemons inside lightweight microVMs via vsock communication.
///
/// # Architecture
///
/// ```text
/// Host                          MicroVM
/// ┌─────────────────┐          ┌─────────────────┐
/// │  PepitaAdapter  │          │  pepita guest   │
/// │  ┌───────────┐  │  vsock   │  ┌───────────┐  │
/// │  │ VmManager ├──┼──────────┼──┤ DaemonCtl │  │
/// │  └───────────┘  │          │  └───────────┘  │
/// └─────────────────┘          └─────────────────┘
/// ```
///
/// # Requirements
///
/// - Linux with KVM support (`/dev/kvm`)
/// - pepita VMM installed
/// - Kernel and rootfs images for guests
///
/// # Example
///
/// ```rust,ignore
/// use duende_core::adapters::PepitaAdapter;
/// use duende_core::PlatformAdapter;
///
/// let adapter = PepitaAdapter::new();
/// let handle = adapter.spawn(my_daemon).await?;
/// ```
pub struct PepitaAdapter {
    /// Vsock base port for daemon communication
    vsock_base_port: u32,
    /// Running VMs indexed by daemon ID
    vms: Arc<RwLock<HashMap<uuid::Uuid, VmInfo>>>,
    /// Default kernel path
    default_kernel: Option<String>,
    /// Default rootfs path
    default_rootfs: Option<String>,
}

/// Information about a running VM.
#[derive(Debug, Clone)]
struct VmInfo {
    /// VM identifier
    vm_id: String,
    /// Vsock CID
    vsock_cid: u32,
    /// Process ID of VMM process (if spawned locally)
    vmm_pid: Option<u32>,
    /// Current VM state
    state: VmState,
}

/// VM state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VmState {
    /// VM is starting
    Starting,
    /// VM is running
    Running,
    /// VM is paused
    Paused,
    /// VM has stopped
    Stopped,
    /// VM failed
    Failed,
}

impl PepitaAdapter {
    /// Creates a new pepita adapter with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            vsock_base_port: 5000,
            vms: Arc::new(RwLock::new(HashMap::new())),
            default_kernel: None,
            default_rootfs: None,
        }
    }

    /// Creates a pepita adapter with custom vsock base port.
    #[must_use]
    pub fn with_vsock_port(vsock_base_port: u32) -> Self {
        Self {
            vsock_base_port,
            vms: Arc::new(RwLock::new(HashMap::new())),
            default_kernel: None,
            default_rootfs: None,
        }
    }

    /// Creates a pepita adapter with kernel and rootfs paths.
    #[must_use]
    pub fn with_images(kernel: impl Into<String>, rootfs: impl Into<String>) -> Self {
        Self {
            vsock_base_port: 5000,
            vms: Arc::new(RwLock::new(HashMap::new())),
            default_kernel: Some(kernel.into()),
            default_rootfs: Some(rootfs.into()),
        }
    }

    /// Returns the vsock base port.
    #[must_use]
    pub const fn vsock_base_port(&self) -> u32 {
        self.vsock_base_port
    }

    /// Allocates a new vsock CID.
    fn allocate_cid() -> u32 {
        NEXT_CID.fetch_add(1, Ordering::Relaxed)
    }

    /// Generates a VM ID from daemon name.
    fn vm_id(daemon_name: &str) -> String {
        format!("duende-vm-{}", daemon_name.replace(' ', "-").replace('_', "-"))
    }

    /// Checks if KVM is available.
    fn kvm_available() -> bool {
        std::path::Path::new("/dev/kvm").exists()
    }

    /// Checks if pepita VMM is installed.
    async fn pepita_available() -> bool {
        tokio::process::Command::new("pepita")
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Maps Signal to signal number for vsock command.
    fn signal_number(sig: Signal) -> i32 {
        match sig {
            Signal::Term => 15,
            Signal::Kill => 9,
            Signal::Int => 2,
            Signal::Quit => 3,
            Signal::Hup => 1,
            Signal::Usr1 => 10,
            Signal::Usr2 => 12,
            Signal::Stop => 19,
            Signal::Cont => 18,
        }
    }
}

impl Default for PepitaAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformAdapter for PepitaAdapter {
    fn platform(&self) -> Platform {
        Platform::PepitaMicroVM
    }

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> PlatformResult<DaemonHandle> {
        // Check prerequisites
        if !Self::kvm_available() {
            return Err(PlatformError::spawn_failed(
                "KVM not available: /dev/kvm not found. Ensure KVM is enabled and you have permissions.",
            ));
        }

        if !Self::pepita_available().await {
            return Err(PlatformError::spawn_failed(
                "pepita VMM not found. Install pepita or add it to PATH.",
            ));
        }

        let kernel = self.default_kernel.as_ref().ok_or_else(|| {
            PlatformError::Config("No kernel image configured. Use with_images() to set kernel path.".into())
        })?;

        let rootfs = self.default_rootfs.as_ref().ok_or_else(|| {
            PlatformError::Config("No rootfs image configured. Use with_images() to set rootfs path.".into())
        })?;

        let daemon_name = daemon.name().to_string();
        let daemon_id = daemon.id();
        let vm_id = Self::vm_id(&daemon_name);
        let vsock_cid = Self::allocate_cid();

        // Build pepita command
        // pepita run --kernel <path> --rootfs <path> --vsock-cid <cid> --memory <mb> --cpus <n>
        let output = tokio::process::Command::new("pepita")
            .arg("run")
            .arg("--kernel").arg(kernel)
            .arg("--rootfs").arg(rootfs)
            .arg("--vsock-cid").arg(vsock_cid.to_string())
            .arg("--memory").arg("256") // Default 256MB
            .arg("--cpus").arg("1")
            .arg("--name").arg(&vm_id)
            .arg("--daemon") // Run in background
            .output()
            .await
            .map_err(|e| PlatformError::spawn_failed(format!("Failed to execute pepita: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PlatformError::spawn_failed(format!(
                "pepita run failed: {}",
                stderr
            )));
        }

        // Store VM info
        let vm_info = VmInfo {
            vm_id: vm_id.clone(),
            vsock_cid,
            vmm_pid: None, // pepita manages this internally
            state: VmState::Running,
        };

        self.vms.write().await.insert(*daemon_id.as_uuid(), vm_info);

        Ok(DaemonHandle::pepita(daemon_id, vm_id, vsock_cid))
    }

    async fn signal(&self, handle: &DaemonHandle, sig: Signal) -> PlatformResult<()> {
        let (vm_id, vsock_cid) = match (handle.pepita_vm_id(), handle.vsock_cid()) {
            (Some(id), Some(cid)) => (id, cid),
            _ => {
                return Err(PlatformError::spawn_failed(
                    "Invalid handle type for pepita adapter",
                ))
            }
        };

        // Send signal via vsock or pepita CLI
        // pepita signal --name <vm_id> --signal <sig>
        let output = tokio::process::Command::new("pepita")
            .arg("signal")
            .arg("--name").arg(vm_id)
            .arg("--signal").arg(Self::signal_number(sig).to_string())
            .output()
            .await
            .map_err(|e| PlatformError::signal_failed(format!("Failed to execute pepita: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PlatformError::signal_failed(format!(
                "pepita signal failed: {}",
                stderr
            )));
        }

        // Update state if stopping
        if matches!(sig, Signal::Term | Signal::Kill) {
            if let Some(vm_info) = self.vms.write().await.get_mut(handle.id().as_uuid()) {
                vm_info.state = VmState::Stopped;
            }
        }

        Ok(())
    }

    async fn status(&self, handle: &DaemonHandle) -> PlatformResult<DaemonStatus> {
        let vm_id = handle.pepita_vm_id().ok_or_else(|| {
            PlatformError::spawn_failed("Invalid handle type for pepita adapter")
        })?;

        // Query pepita for VM status
        // pepita status --name <vm_id> --json
        let output = tokio::process::Command::new("pepita")
            .arg("status")
            .arg("--name").arg(vm_id)
            .arg("--json")
            .output()
            .await
            .map_err(|e| PlatformError::status_failed(format!("Failed to execute pepita: {}", e)))?;

        if !output.status.success() {
            // VM not found = stopped
            return Ok(DaemonStatus::Stopped);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse JSON status
        if stdout.contains("\"state\": \"running\"") || stdout.contains("\"state\":\"running\"") {
            Ok(DaemonStatus::Running)
        } else if stdout.contains("\"state\": \"paused\"") {
            Ok(DaemonStatus::Paused)
        } else if stdout.contains("\"state\": \"failed\"") {
            Ok(DaemonStatus::Failed(FailureReason::ExitCode(1)))
        } else {
            Ok(DaemonStatus::Stopped)
        }
    }

    async fn attach_tracer(&self, handle: &DaemonHandle) -> PlatformResult<TracerHandle> {
        let vsock_cid = handle.vsock_cid().ok_or_else(|| {
            PlatformError::spawn_failed("Invalid handle type for pepita adapter")
        })?;

        if vsock_cid == 0 {
            return Err(PlatformError::tracer_failed("VM not running"));
        }

        // Return a remote vsock-based tracer handle
        Ok(TracerHandle::remote_vsock(handle.id()))
    }
}

impl PepitaAdapter {
    /// Stops and destroys a VM.
    pub async fn destroy(&self, vm_id: &str) -> PlatformResult<()> {
        let output = tokio::process::Command::new("pepita")
            .arg("destroy")
            .arg("--name").arg(vm_id)
            .arg("--force")
            .output()
            .await
            .map_err(|e| PlatformError::spawn_failed(format!("Failed to execute pepita: {}", e)))?;

        if !output.status.success() {
            // Ignore errors - VM might already be destroyed
        }

        Ok(())
    }

    /// Lists all running VMs.
    pub async fn list_vms(&self) -> PlatformResult<Vec<String>> {
        let output = tokio::process::Command::new("pepita")
            .arg("list")
            .arg("--format").arg("name")
            .output()
            .await
            .map_err(|e| PlatformError::spawn_failed(format!("Failed to execute pepita: {}", e)))?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().map(|s| s.to_string()).collect())
    }
}

// Extend DaemonHandle for pepita-specific accessors
impl crate::adapter::DaemonHandle {
    /// Returns the pepita VM ID, if applicable.
    #[must_use]
    pub fn pepita_vm_id(&self) -> Option<&str> {
        match self.handle_data() {
            crate::adapter::HandleData::Pepita { vm_id, .. } => Some(vm_id),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pepita_adapter_new() {
        let adapter = PepitaAdapter::new();
        assert_eq!(adapter.vsock_base_port(), 5000);
        assert_eq!(adapter.platform(), Platform::PepitaMicroVM);
    }

    #[test]
    fn test_pepita_adapter_with_vsock_port() {
        let adapter = PepitaAdapter::with_vsock_port(9000);
        assert_eq!(adapter.vsock_base_port(), 9000);
    }

    #[test]
    fn test_pepita_adapter_with_images() {
        let adapter = PepitaAdapter::with_images("/boot/vmlinuz", "/var/lib/rootfs.img");
        assert!(adapter.default_kernel.is_some());
        assert!(adapter.default_rootfs.is_some());
    }

    #[test]
    fn test_pepita_adapter_default() {
        let adapter = PepitaAdapter::default();
        assert_eq!(adapter.platform(), Platform::PepitaMicroVM);
    }

    #[test]
    fn test_vm_id_generation() {
        assert_eq!(PepitaAdapter::vm_id("my-daemon"), "duende-vm-my-daemon");
        assert_eq!(PepitaAdapter::vm_id("my daemon"), "duende-vm-my-daemon");
    }

    #[test]
    fn test_allocate_cid() {
        let cid1 = PepitaAdapter::allocate_cid();
        let cid2 = PepitaAdapter::allocate_cid();
        assert!(cid2 > cid1);
    }

    #[test]
    fn test_signal_number() {
        assert_eq!(PepitaAdapter::signal_number(Signal::Term), 15);
        assert_eq!(PepitaAdapter::signal_number(Signal::Kill), 9);
    }

    #[tokio::test]
    async fn test_pepita_adapter_spawn_fails_without_kvm() {
        // Skip if KVM is available (would need pepita)
        if PepitaAdapter::kvm_available() {
            return;
        }

        let adapter = PepitaAdapter::with_images("/boot/vmlinuz", "/rootfs.img");

        struct TestDaemon {
            id: crate::types::DaemonId,
            metrics: crate::metrics::DaemonMetrics,
        }

        #[async_trait::async_trait]
        impl crate::daemon::Daemon for TestDaemon {
            fn id(&self) -> crate::types::DaemonId { self.id }
            fn name(&self) -> &str { "test" }
            async fn init(&mut self, _: &crate::config::DaemonConfig) -> crate::error::Result<()> { Ok(()) }
            async fn run(&mut self, _: &mut crate::daemon::DaemonContext) -> crate::error::Result<crate::types::ExitReason> {
                Ok(crate::types::ExitReason::Graceful)
            }
            async fn shutdown(&mut self, _: std::time::Duration) -> crate::error::Result<()> { Ok(()) }
            async fn health_check(&self) -> crate::types::HealthStatus { crate::types::HealthStatus::healthy(1) }
            fn metrics(&self) -> &crate::metrics::DaemonMetrics { &self.metrics }
        }

        let daemon = TestDaemon {
            id: crate::types::DaemonId::new(),
            metrics: crate::metrics::DaemonMetrics::new(),
        };

        let result = adapter.spawn(Box::new(daemon)).await;
        assert!(result.is_err());
        // Should fail because KVM is not available
        let err = result.unwrap_err();
        assert!(err.to_string().contains("KVM") || err.to_string().contains("pepita"));
    }
}
