//! Platform adapter abstraction for daemon lifecycle management.
//!
//! # Toyota Way: Standardized Work (標準作業)
//! Every platform adapter follows the same contract, enabling
//! predictable daemon behavior across Linux, macOS, containers,
//! pepita microVMs, and WOS.

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::daemon::Daemon;
use crate::platform::Platform;
use crate::types::{DaemonId, DaemonStatus, Signal};

// =============================================================================
// PlatformError
// =============================================================================

/// Error type for platform adapter operations.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    /// Platform operation not supported.
    #[error("operation not supported on {platform}: {operation}")]
    NotSupported {
        /// The platform.
        platform: Platform,
        /// The operation that was attempted.
        operation: String,
    },

    /// Failed to spawn daemon.
    #[error("spawn failed: {0}")]
    SpawnFailed(String),

    /// Failed to signal daemon.
    #[error("signal failed: {0}")]
    SignalFailed(String),

    /// Failed to query status.
    #[error("status query failed: {0}")]
    StatusFailed(String),

    /// Failed to attach tracer.
    #[error("tracer attachment failed: {0}")]
    TracerFailed(String),

    /// Daemon not found.
    #[error("daemon not found: {0}")]
    NotFound(String),

    /// Invalid state for operation.
    #[error("invalid state: {0}")]
    InvalidState(String),

    /// Timeout during operation.
    #[error("operation timed out after {0:?}")]
    Timeout(Duration),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Configuration error.
    #[error("configuration error: {0}")]
    Config(String),

    /// Resource limit exceeded.
    #[error("resource limit exceeded: {0}")]
    ResourceLimit(String),

    /// Permission denied.
    #[error("permission denied: {0}")]
    PermissionDenied(String),
}

impl PlatformError {
    /// Creates a "not supported" error.
    #[must_use]
    pub fn not_supported(platform: Platform, operation: impl Into<String>) -> Self {
        Self::NotSupported {
            platform,
            operation: operation.into(),
        }
    }

    /// Creates a spawn failed error.
    #[must_use]
    pub fn spawn_failed(msg: impl Into<String>) -> Self {
        Self::SpawnFailed(msg.into())
    }

    /// Creates a signal failed error.
    #[must_use]
    pub fn signal_failed(msg: impl Into<String>) -> Self {
        Self::SignalFailed(msg.into())
    }

    /// Creates a status query failed error.
    #[must_use]
    pub fn status_failed(msg: impl Into<String>) -> Self {
        Self::StatusFailed(msg.into())
    }

    /// Creates a tracer attachment failed error.
    #[must_use]
    pub fn tracer_failed(msg: impl Into<String>) -> Self {
        Self::TracerFailed(msg.into())
    }

    /// Returns true if this error indicates the operation is not supported.
    #[must_use]
    pub const fn is_not_supported(&self) -> bool {
        matches!(self, Self::NotSupported { .. })
    }

    /// Returns true if this error is recoverable.
    #[must_use]
    pub const fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::Timeout(_) | Self::ResourceLimit(_) | Self::InvalidState(_)
        )
    }
}

/// Result type for platform operations.
pub type PlatformResult<T> = std::result::Result<T, PlatformError>;

// =============================================================================
// DaemonHandle
// =============================================================================

/// Handle to a running daemon instance.
///
/// The handle type varies by platform:
/// - Linux/systemd: Unit name
/// - macOS/launchd: Service label
/// - Container: Container ID
/// - pepita: VM ID + vsock transport
/// - WOS: Process ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonHandle {
    /// Daemon ID.
    id: DaemonId,
    /// Platform this daemon is running on.
    platform: Platform,
    /// Platform-specific handle data.
    handle_data: HandleData,
}

/// Platform-specific handle data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HandleData {
    /// Linux systemd unit name.
    Systemd {
        /// Unit name (e.g., "my-daemon.service").
        unit_name: String,
    },
    /// macOS launchd service label.
    Launchd {
        /// Service label (e.g., "com.example.my-daemon").
        label: String,
    },
    /// Container ID.
    Container {
        /// Container runtime (docker, podman, containerd).
        runtime: String,
        /// Container ID.
        container_id: String,
    },
    /// pepita MicroVM.
    Pepita {
        /// VM ID.
        vm_id: String,
        /// vsock CID.
        vsock_cid: u32,
    },
    /// WOS process.
    Wos {
        /// Process ID.
        pid: u32,
    },
    /// Native process.
    Native {
        /// Process ID.
        pid: u32,
    },
}

impl DaemonHandle {
    /// Creates a systemd handle.
    #[must_use]
    pub fn systemd(id: DaemonId, unit_name: impl Into<String>) -> Self {
        Self {
            id,
            platform: Platform::Linux,
            handle_data: HandleData::Systemd {
                unit_name: unit_name.into(),
            },
        }
    }

    /// Creates a launchd handle.
    #[must_use]
    pub fn launchd(id: DaemonId, label: impl Into<String>) -> Self {
        Self {
            id,
            platform: Platform::MacOS,
            handle_data: HandleData::Launchd {
                label: label.into(),
            },
        }
    }

    /// Creates a container handle.
    #[must_use]
    pub fn container(
        id: DaemonId,
        runtime: impl Into<String>,
        container_id: impl Into<String>,
    ) -> Self {
        Self {
            id,
            platform: Platform::Container,
            handle_data: HandleData::Container {
                runtime: runtime.into(),
                container_id: container_id.into(),
            },
        }
    }

    /// Creates a pepita handle.
    #[must_use]
    pub fn pepita(id: DaemonId, vm_id: impl Into<String>, vsock_cid: u32) -> Self {
        Self {
            id,
            platform: Platform::PepitaMicroVM,
            handle_data: HandleData::Pepita {
                vm_id: vm_id.into(),
                vsock_cid,
            },
        }
    }

    /// Creates a WOS handle.
    #[must_use]
    pub fn wos(id: DaemonId, pid: u32) -> Self {
        Self {
            id,
            platform: Platform::Wos,
            handle_data: HandleData::Wos { pid },
        }
    }

    /// Creates a native process handle.
    #[must_use]
    pub fn native(id: DaemonId, pid: u32) -> Self {
        Self {
            id,
            platform: Platform::Native,
            handle_data: HandleData::Native { pid },
        }
    }

    /// Returns the daemon ID.
    #[must_use]
    pub const fn id(&self) -> DaemonId {
        self.id
    }

    /// Returns the platform.
    #[must_use]
    pub const fn platform(&self) -> Platform {
        self.platform
    }

    /// Returns the handle data.
    #[must_use]
    pub const fn handle_data(&self) -> &HandleData {
        &self.handle_data
    }

    /// Returns the systemd unit name, if applicable.
    #[must_use]
    pub fn systemd_unit(&self) -> Option<&str> {
        match &self.handle_data {
            HandleData::Systemd { unit_name } => Some(unit_name),
            _ => None,
        }
    }

    /// Returns the launchd label, if applicable.
    #[must_use]
    pub fn launchd_label(&self) -> Option<&str> {
        match &self.handle_data {
            HandleData::Launchd { label } => Some(label),
            _ => None,
        }
    }

    /// Returns the container ID, if applicable.
    #[must_use]
    pub fn container_id(&self) -> Option<&str> {
        match &self.handle_data {
            HandleData::Container { container_id, .. } => Some(container_id),
            _ => None,
        }
    }

    /// Returns the process ID, if applicable.
    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        match &self.handle_data {
            HandleData::Wos { pid } | HandleData::Native { pid } => Some(*pid),
            _ => None,
        }
    }

    /// Returns the vsock CID, if applicable (pepita).
    #[must_use]
    pub fn vsock_cid(&self) -> Option<u32> {
        match &self.handle_data {
            HandleData::Pepita { vsock_cid, .. } => Some(*vsock_cid),
            _ => None,
        }
    }
}

impl fmt::Display for DaemonHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.handle_data {
            HandleData::Systemd { unit_name } => write!(f, "systemd:{}", unit_name),
            HandleData::Launchd { label } => write!(f, "launchd:{}", label),
            HandleData::Container {
                runtime,
                container_id,
            } => {
                write!(f, "{}:{}", runtime, container_id)
            }
            HandleData::Pepita { vm_id, vsock_cid } => {
                write!(f, "pepita:{}@cid{}", vm_id, vsock_cid)
            }
            HandleData::Wos { pid } => write!(f, "wos:pid{}", pid),
            HandleData::Native { pid } => write!(f, "native:pid{}", pid),
        }
    }
}

// =============================================================================
// TracerHandle
// =============================================================================

/// Handle to an attached tracer (renacer integration).
///
/// # Toyota Way: Genchi Genbutsu (現地現物)
/// Direct observation of daemon behavior via syscall tracing.
#[derive(Debug, Clone)]
pub struct TracerHandle {
    /// Daemon being traced.
    daemon_id: DaemonId,
    /// Tracer type.
    tracer_type: TracerType,
}

/// Type of tracer attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TracerType {
    /// Local ptrace-based tracer.
    Ptrace,
    /// eBPF-based tracer.
    Ebpf,
    /// Remote tracer via vsock (pepita).
    RemoteVsock,
    /// Simulated tracer (WOS).
    Simulated,
}

impl TracerHandle {
    /// Creates a ptrace-based tracer handle.
    #[must_use]
    pub fn ptrace(daemon_id: DaemonId) -> Self {
        Self {
            daemon_id,
            tracer_type: TracerType::Ptrace,
        }
    }

    /// Creates an eBPF-based tracer handle.
    #[must_use]
    pub fn ebpf(daemon_id: DaemonId) -> Self {
        Self {
            daemon_id,
            tracer_type: TracerType::Ebpf,
        }
    }

    /// Creates a remote vsock-based tracer handle.
    #[must_use]
    pub fn remote_vsock(daemon_id: DaemonId) -> Self {
        Self {
            daemon_id,
            tracer_type: TracerType::RemoteVsock,
        }
    }

    /// Creates a simulated tracer handle.
    #[must_use]
    pub fn simulated(daemon_id: DaemonId) -> Self {
        Self {
            daemon_id,
            tracer_type: TracerType::Simulated,
        }
    }

    /// Returns the daemon ID being traced.
    #[must_use]
    pub const fn daemon_id(&self) -> DaemonId {
        self.daemon_id
    }

    /// Returns the tracer type.
    #[must_use]
    pub const fn tracer_type(&self) -> TracerType {
        self.tracer_type
    }
}

// =============================================================================
// PlatformAdapter trait
// =============================================================================

/// Platform-specific daemon adapter.
///
/// # Toyota Way: Standardized Work
/// Every platform implements the same lifecycle contract:
/// - spawn: Create and start daemon
/// - signal: Send signal to daemon
/// - status: Query daemon status
/// - attach_tracer: Attach renacer tracer
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    /// Returns the platform identifier.
    fn platform(&self) -> Platform;

    /// Spawns a daemon on this platform.
    ///
    /// # Errors
    /// Returns an error if the daemon cannot be spawned.
    async fn spawn(&self, daemon: Box<dyn Daemon>) -> PlatformResult<DaemonHandle>;

    /// Sends a signal to a daemon.
    ///
    /// # Errors
    /// Returns an error if the signal cannot be delivered.
    async fn signal(&self, handle: &DaemonHandle, sig: Signal) -> PlatformResult<()>;

    /// Queries daemon status.
    ///
    /// # Errors
    /// Returns an error if the status cannot be determined.
    async fn status(&self, handle: &DaemonHandle) -> PlatformResult<DaemonStatus>;

    /// Attaches a tracer to a running daemon.
    ///
    /// # Errors
    /// Returns an error if the tracer cannot be attached.
    async fn attach_tracer(&self, handle: &DaemonHandle) -> PlatformResult<TracerHandle>;

    /// Gracefully stops a daemon.
    ///
    /// Sends SIGTERM and waits for termination up to the timeout.
    ///
    /// # Errors
    /// Returns an error if the daemon cannot be stopped.
    async fn stop(&self, handle: &DaemonHandle, timeout: Duration) -> PlatformResult<()> {
        self.signal(handle, Signal::Term).await?;

        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if let Ok(status) = self.status(handle).await
                && status.is_terminal() {
                    return Ok(());
                }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Err(PlatformError::Timeout(timeout))
    }

    /// Forcefully kills a daemon.
    ///
    /// Sends SIGKILL immediately.
    ///
    /// # Errors
    /// Returns an error if the daemon cannot be killed.
    async fn kill(&self, handle: &DaemonHandle) -> PlatformResult<()> {
        self.signal(handle, Signal::Kill).await
    }

    /// Pauses a daemon.
    ///
    /// Sends SIGSTOP.
    ///
    /// # Errors
    /// Returns an error if the daemon cannot be paused.
    async fn pause(&self, handle: &DaemonHandle) -> PlatformResult<()> {
        self.signal(handle, Signal::Stop).await
    }

    /// Resumes a paused daemon.
    ///
    /// Sends SIGCONT.
    ///
    /// # Errors
    /// Returns an error if the daemon cannot be resumed.
    async fn resume(&self, handle: &DaemonHandle) -> PlatformResult<()> {
        self.signal(handle, Signal::Cont).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // PlatformError tests
    // =========================================================================

    #[test]
    fn test_platform_error_not_supported() {
        let err = PlatformError::not_supported(Platform::MacOS, "cgroups");
        assert!(err.is_not_supported());
        assert!(!err.is_recoverable());
        assert!(err.to_string().contains("macos"));
        assert!(err.to_string().contains("cgroups"));
    }

    #[test]
    fn test_platform_error_spawn_failed() {
        let err = PlatformError::spawn_failed("binary not found");
        assert!(!err.is_not_supported());
        assert!(err.to_string().contains("spawn"));
        assert!(err.to_string().contains("binary not found"));
    }

    #[test]
    fn test_platform_error_timeout_recoverable() {
        let err = PlatformError::Timeout(Duration::from_secs(30));
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_platform_error_resource_limit_recoverable() {
        let err = PlatformError::ResourceLimit("memory".into());
        assert!(err.is_recoverable());
    }

    // =========================================================================
    // DaemonHandle tests
    // =========================================================================

    #[test]
    fn test_daemon_handle_systemd() {
        let id = DaemonId::new();
        let handle = DaemonHandle::systemd(id, "test.service");

        assert_eq!(handle.id(), id);
        assert_eq!(handle.platform(), Platform::Linux);
        assert_eq!(handle.systemd_unit(), Some("test.service"));
        assert_eq!(handle.launchd_label(), None);
        assert!(handle.to_string().contains("systemd:test.service"));
    }

    #[test]
    fn test_daemon_handle_launchd() {
        let id = DaemonId::new();
        let handle = DaemonHandle::launchd(id, "com.example.daemon");

        assert_eq!(handle.platform(), Platform::MacOS);
        assert_eq!(handle.launchd_label(), Some("com.example.daemon"));
        assert!(handle.to_string().contains("launchd:com.example.daemon"));
    }

    #[test]
    fn test_daemon_handle_container() {
        let id = DaemonId::new();
        let handle = DaemonHandle::container(id, "docker", "abc123");

        assert_eq!(handle.platform(), Platform::Container);
        assert_eq!(handle.container_id(), Some("abc123"));
        assert!(handle.to_string().contains("docker:abc123"));
    }

    #[test]
    fn test_daemon_handle_pepita() {
        let id = DaemonId::new();
        let handle = DaemonHandle::pepita(id, "vm-1234", 3);

        assert_eq!(handle.platform(), Platform::PepitaMicroVM);
        assert_eq!(handle.vsock_cid(), Some(3));
        assert!(handle.to_string().contains("pepita"));
        assert!(handle.to_string().contains("cid3"));
    }

    #[test]
    fn test_daemon_handle_wos() {
        let id = DaemonId::new();
        let handle = DaemonHandle::wos(id, 42);

        assert_eq!(handle.platform(), Platform::Wos);
        assert_eq!(handle.pid(), Some(42));
        assert!(handle.to_string().contains("wos:pid42"));
    }

    #[test]
    fn test_daemon_handle_native() {
        let id = DaemonId::new();
        let handle = DaemonHandle::native(id, 12345);

        assert_eq!(handle.platform(), Platform::Native);
        assert_eq!(handle.pid(), Some(12345));
        assert!(handle.to_string().contains("native:pid12345"));
    }

    #[test]
    fn test_daemon_handle_serialize_roundtrip() {
        let id = DaemonId::new();
        let handle = DaemonHandle::systemd(id, "test.service");

        let json = serde_json::to_string(&handle).unwrap();
        let deserialized: DaemonHandle = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id(), id);
        assert_eq!(deserialized.platform(), Platform::Linux);
        assert_eq!(deserialized.systemd_unit(), Some("test.service"));
    }

    // =========================================================================
    // TracerHandle tests
    // =========================================================================

    #[test]
    fn test_tracer_handle_ptrace() {
        let id = DaemonId::new();
        let tracer = TracerHandle::ptrace(id);

        assert_eq!(tracer.daemon_id(), id);
        assert_eq!(tracer.tracer_type(), TracerType::Ptrace);
    }

    #[test]
    fn test_tracer_handle_ebpf() {
        let id = DaemonId::new();
        let tracer = TracerHandle::ebpf(id);

        assert_eq!(tracer.tracer_type(), TracerType::Ebpf);
    }

    #[test]
    fn test_tracer_handle_remote_vsock() {
        let id = DaemonId::new();
        let tracer = TracerHandle::remote_vsock(id);

        assert_eq!(tracer.tracer_type(), TracerType::RemoteVsock);
    }

    #[test]
    fn test_tracer_handle_simulated() {
        let id = DaemonId::new();
        let tracer = TracerHandle::simulated(id);

        assert_eq!(tracer.tracer_type(), TracerType::Simulated);
    }

    #[test]
    fn test_tracer_type_equality() {
        assert_eq!(TracerType::Ptrace, TracerType::Ptrace);
        assert_ne!(TracerType::Ptrace, TracerType::Ebpf);
    }

    // =========================================================================
    // HandleData tests
    // =========================================================================

    #[test]
    fn test_handle_data_all_variants() {
        // Test that all variants can be created and matched
        let variants = vec![
            HandleData::Systemd {
                unit_name: "test".into(),
            },
            HandleData::Launchd {
                label: "test".into(),
            },
            HandleData::Container {
                runtime: "docker".into(),
                container_id: "abc".into(),
            },
            HandleData::Pepita {
                vm_id: "vm".into(),
                vsock_cid: 1,
            },
            HandleData::Wos { pid: 1 },
            HandleData::Native { pid: 1 },
        ];

        for variant in variants {
            // Verify each variant can be debug-printed
            let _ = format!("{:?}", variant);
        }
    }
}
