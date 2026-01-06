//! Platform adapter implementations.
//!
//! Each adapter implements the [`PlatformAdapter`] trait for its respective platform.
//!
//! # Available Adapters
//!
//! - [`NativeAdapter`]: Fork/exec-based native process management (fully implemented)
//! - [`SystemdAdapter`]: Linux systemd integration (stub - returns `NotSupported`)
//! - [`LaunchdAdapter`]: macOS launchd integration (stub - returns `NotSupported`)
//! - [`ContainerAdapter`]: Docker/OCI container management (stub - returns `NotSupported`)
//! - [`PepitaAdapter`]: pepita MicroVM integration (stub - returns `NotSupported`)
//! - [`WosAdapter`]: WOS (WebAssembly OS) integration (stub - returns `NotSupported`)

mod native;

pub use native::NativeAdapter;

// Platform-specific adapters (stubs for now)

use crate::adapter::{DaemonHandle, PlatformAdapter, PlatformError, PlatformResult, TracerHandle};
use crate::daemon::Daemon;
use crate::platform::Platform;
use crate::types::{DaemonStatus, Signal};

use async_trait::async_trait;

// =============================================================================
// SystemdAdapter (Linux)
// =============================================================================

/// Linux systemd adapter stub.
///
/// This adapter returns `NotSupported` for all operations.
/// Full systemd integration is tracked in roadmap.yaml (DP-002).
pub struct SystemdAdapter {
    _unit_dir: std::path::PathBuf,
}

impl SystemdAdapter {
    /// Creates a new systemd adapter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            _unit_dir: std::path::PathBuf::from("/etc/systemd/system"),
        }
    }
}

impl Default for SystemdAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformAdapter for SystemdAdapter {
    fn platform(&self) -> Platform {
        Platform::Linux
    }

    async fn spawn(&self, _daemon: Box<dyn Daemon>) -> PlatformResult<DaemonHandle> {
        Err(PlatformError::not_supported(Platform::Linux, "spawn"))
    }

    async fn signal(&self, _handle: &DaemonHandle, _sig: Signal) -> PlatformResult<()> {
        Err(PlatformError::not_supported(Platform::Linux, "signal"))
    }

    async fn status(&self, _handle: &DaemonHandle) -> PlatformResult<DaemonStatus> {
        Err(PlatformError::not_supported(Platform::Linux, "status"))
    }

    async fn attach_tracer(&self, _handle: &DaemonHandle) -> PlatformResult<TracerHandle> {
        Err(PlatformError::not_supported(Platform::Linux, "attach_tracer"))
    }
}

// =============================================================================
// LaunchdAdapter (macOS)
// =============================================================================

/// macOS launchd adapter stub.
///
/// This adapter returns `NotSupported` for all operations.
/// Full launchd integration is tracked in roadmap.yaml (DP-004).
pub struct LaunchdAdapter {
    _plist_dir: std::path::PathBuf,
}

impl LaunchdAdapter {
    /// Creates a new launchd adapter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            _plist_dir: std::path::PathBuf::from("/Library/LaunchDaemons"),
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

    async fn spawn(&self, _daemon: Box<dyn Daemon>) -> PlatformResult<DaemonHandle> {
        Err(PlatformError::not_supported(Platform::MacOS, "spawn"))
    }

    async fn signal(&self, _handle: &DaemonHandle, _sig: Signal) -> PlatformResult<()> {
        Err(PlatformError::not_supported(Platform::MacOS, "signal"))
    }

    async fn status(&self, _handle: &DaemonHandle) -> PlatformResult<DaemonStatus> {
        Err(PlatformError::not_supported(Platform::MacOS, "status"))
    }

    async fn attach_tracer(&self, _handle: &DaemonHandle) -> PlatformResult<TracerHandle> {
        Err(PlatformError::not_supported(Platform::MacOS, "attach_tracer"))
    }
}

// =============================================================================
// ContainerAdapter (Docker/OCI)
// =============================================================================

/// Container runtime type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerRuntime {
    /// Docker runtime.
    Docker,
    /// Podman runtime.
    Podman,
    /// containerd runtime.
    Containerd,
}

impl ContainerRuntime {
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

/// Docker/OCI container adapter stub.
///
/// This adapter returns `NotSupported` for all operations.
/// Full container integration is tracked in roadmap.yaml (DP-005).
pub struct ContainerAdapter {
    runtime: ContainerRuntime,
}

impl ContainerAdapter {
    /// Creates a new container adapter with Docker runtime.
    #[must_use]
    pub fn docker() -> Self {
        Self {
            runtime: ContainerRuntime::Docker,
        }
    }

    /// Creates a new container adapter with Podman runtime.
    #[must_use]
    pub fn podman() -> Self {
        Self {
            runtime: ContainerRuntime::Podman,
        }
    }

    /// Creates a new container adapter with containerd runtime.
    #[must_use]
    pub fn containerd() -> Self {
        Self {
            runtime: ContainerRuntime::Containerd,
        }
    }

    /// Returns the container runtime.
    #[must_use]
    pub const fn runtime(&self) -> ContainerRuntime {
        self.runtime
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

    async fn spawn(&self, _daemon: Box<dyn Daemon>) -> PlatformResult<DaemonHandle> {
        Err(PlatformError::not_supported(Platform::Container, "spawn"))
    }

    async fn signal(&self, _handle: &DaemonHandle, _sig: Signal) -> PlatformResult<()> {
        Err(PlatformError::not_supported(Platform::Container, "signal"))
    }

    async fn status(&self, _handle: &DaemonHandle) -> PlatformResult<DaemonStatus> {
        Err(PlatformError::not_supported(Platform::Container, "status"))
    }

    async fn attach_tracer(&self, _handle: &DaemonHandle) -> PlatformResult<TracerHandle> {
        Err(PlatformError::not_supported(
            Platform::Container,
            "attach_tracer",
        ))
    }
}

// =============================================================================
// PepitaAdapter (MicroVM)
// =============================================================================

/// pepita MicroVM adapter stub.
///
/// This adapter returns `NotSupported` for all operations.
/// Full pepita integration is tracked in roadmap.yaml (DP-006).
pub struct PepitaAdapter {
    _vsock_base_port: u32,
}

impl PepitaAdapter {
    /// Creates a new pepita adapter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            _vsock_base_port: 5000,
        }
    }

    /// Creates a pepita adapter with custom vsock base port.
    #[must_use]
    pub const fn with_vsock_port(vsock_base_port: u32) -> Self {
        Self {
            _vsock_base_port: vsock_base_port,
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

    async fn spawn(&self, _daemon: Box<dyn Daemon>) -> PlatformResult<DaemonHandle> {
        Err(PlatformError::not_supported(
            Platform::PepitaMicroVM,
            "spawn",
        ))
    }

    async fn signal(&self, _handle: &DaemonHandle, _sig: Signal) -> PlatformResult<()> {
        Err(PlatformError::not_supported(
            Platform::PepitaMicroVM,
            "signal",
        ))
    }

    async fn status(&self, _handle: &DaemonHandle) -> PlatformResult<DaemonStatus> {
        Err(PlatformError::not_supported(
            Platform::PepitaMicroVM,
            "status",
        ))
    }

    async fn attach_tracer(&self, _handle: &DaemonHandle) -> PlatformResult<TracerHandle> {
        Err(PlatformError::not_supported(
            Platform::PepitaMicroVM,
            "attach_tracer",
        ))
    }
}

// =============================================================================
// WosAdapter (WebAssembly OS)
// =============================================================================

/// WOS (WebAssembly Operating System) adapter stub.
///
/// This adapter returns `NotSupported` for all operations.
/// Full WOS integration is tracked in roadmap.yaml (DP-007).
pub struct WosAdapter {
    _priority_default: u8,
}

impl WosAdapter {
    /// Creates a new WOS adapter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            _priority_default: 4, // Normal priority (0-7 scale)
        }
    }
}

impl Default for WosAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformAdapter for WosAdapter {
    fn platform(&self) -> Platform {
        Platform::Wos
    }

    async fn spawn(&self, _daemon: Box<dyn Daemon>) -> PlatformResult<DaemonHandle> {
        Err(PlatformError::not_supported(Platform::Wos, "spawn"))
    }

    async fn signal(&self, _handle: &DaemonHandle, _sig: Signal) -> PlatformResult<()> {
        Err(PlatformError::not_supported(Platform::Wos, "signal"))
    }

    async fn status(&self, _handle: &DaemonHandle) -> PlatformResult<DaemonStatus> {
        Err(PlatformError::not_supported(Platform::Wos, "status"))
    }

    async fn attach_tracer(&self, _handle: &DaemonHandle) -> PlatformResult<TracerHandle> {
        Err(PlatformError::not_supported(Platform::Wos, "attach_tracer"))
    }
}

// =============================================================================
// select_adapter - Factory function
// =============================================================================

/// Selects the appropriate platform adapter for the current platform.
#[must_use]
pub fn select_adapter(platform: Platform) -> Box<dyn PlatformAdapter> {
    match platform {
        Platform::Linux => Box::new(SystemdAdapter::new()),
        Platform::MacOS => Box::new(LaunchdAdapter::new()),
        Platform::Container => Box::new(ContainerAdapter::docker()),
        Platform::PepitaMicroVM => Box::new(PepitaAdapter::new()),
        Platform::Wos => Box::new(WosAdapter::new()),
        Platform::Native => Box::new(NativeAdapter::new()),
    }
}

/// Selects the appropriate platform adapter for the detected platform.
#[must_use]
pub fn select_adapter_auto() -> Box<dyn PlatformAdapter> {
    select_adapter(crate::platform::detect_platform())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // =========================================================================
    // SystemdAdapter tests
    // =========================================================================

    #[tokio::test]
    async fn test_systemd_adapter_platform() {
        let adapter = SystemdAdapter::new();
        assert_eq!(adapter.platform(), Platform::Linux);
    }

    #[tokio::test]
    async fn test_systemd_adapter_not_supported() {
        let adapter = SystemdAdapter::new();
        let result = adapter.spawn(Box::new(TestDaemon::new())).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_supported());
    }

    // =========================================================================
    // LaunchdAdapter tests
    // =========================================================================

    #[tokio::test]
    async fn test_launchd_adapter_platform() {
        let adapter = LaunchdAdapter::new();
        assert_eq!(adapter.platform(), Platform::MacOS);
    }

    #[tokio::test]
    async fn test_launchd_adapter_not_supported() {
        let adapter = LaunchdAdapter::new();
        let result = adapter.spawn(Box::new(TestDaemon::new())).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_supported());
    }

    // =========================================================================
    // ContainerAdapter tests
    // =========================================================================

    #[tokio::test]
    async fn test_container_adapter_platform() {
        let adapter = ContainerAdapter::docker();
        assert_eq!(adapter.platform(), Platform::Container);
    }

    #[test]
    fn test_container_runtime_variants() {
        let docker = ContainerAdapter::docker();
        assert_eq!(docker.runtime(), ContainerRuntime::Docker);
        assert_eq!(docker.runtime().name(), "docker");

        let podman = ContainerAdapter::podman();
        assert_eq!(podman.runtime(), ContainerRuntime::Podman);
        assert_eq!(podman.runtime().name(), "podman");

        let containerd = ContainerAdapter::containerd();
        assert_eq!(containerd.runtime(), ContainerRuntime::Containerd);
        assert_eq!(containerd.runtime().name(), "containerd");
    }

    // =========================================================================
    // PepitaAdapter tests
    // =========================================================================

    #[tokio::test]
    async fn test_pepita_adapter_platform() {
        let adapter = PepitaAdapter::new();
        assert_eq!(adapter.platform(), Platform::PepitaMicroVM);
    }

    #[test]
    fn test_pepita_adapter_custom_port() {
        let adapter = PepitaAdapter::with_vsock_port(9000);
        assert_eq!(adapter.platform(), Platform::PepitaMicroVM);
    }

    // =========================================================================
    // WosAdapter tests
    // =========================================================================

    #[tokio::test]
    async fn test_wos_adapter_platform() {
        let adapter = WosAdapter::new();
        assert_eq!(adapter.platform(), Platform::Wos);
    }

    #[tokio::test]
    async fn test_wos_adapter_not_supported() {
        let adapter = WosAdapter::new();
        let handle = DaemonHandle::wos(DaemonId::new(), 1);

        // All operations should return NotSupported
        let result = adapter.signal(&handle, Signal::Term).await;
        assert!(result.unwrap_err().is_not_supported());

        let result = adapter.status(&handle).await;
        assert!(result.unwrap_err().is_not_supported());

        let result = adapter.attach_tracer(&handle).await;
        assert!(result.unwrap_err().is_not_supported());
    }

    // =========================================================================
    // PepitaAdapter additional tests
    // =========================================================================

    #[tokio::test]
    async fn test_pepita_adapter_not_supported() {
        let adapter = PepitaAdapter::new();
        let handle = DaemonHandle::pepita(DaemonId::new(), "vm-test", 1);

        let result = adapter.signal(&handle, Signal::Term).await;
        assert!(result.unwrap_err().is_not_supported());

        let result = adapter.status(&handle).await;
        assert!(result.unwrap_err().is_not_supported());

        let result = adapter.attach_tracer(&handle).await;
        assert!(result.unwrap_err().is_not_supported());
    }

    // =========================================================================
    // ContainerAdapter additional tests
    // =========================================================================

    #[tokio::test]
    async fn test_container_adapter_not_supported() {
        let adapter = ContainerAdapter::docker();
        let handle = DaemonHandle::container(DaemonId::new(), "docker", "abc123");

        let result = adapter.signal(&handle, Signal::Term).await;
        assert!(result.unwrap_err().is_not_supported());

        let result = adapter.status(&handle).await;
        assert!(result.unwrap_err().is_not_supported());

        let result = adapter.attach_tracer(&handle).await;
        assert!(result.unwrap_err().is_not_supported());
    }

    // =========================================================================
    // SystemdAdapter additional tests
    // =========================================================================

    #[tokio::test]
    async fn test_systemd_adapter_all_not_supported() {
        let adapter = SystemdAdapter::new();
        let handle = DaemonHandle::systemd(DaemonId::new(), "test.service");

        let result = adapter.signal(&handle, Signal::Term).await;
        assert!(result.unwrap_err().is_not_supported());

        let result = adapter.status(&handle).await;
        assert!(result.unwrap_err().is_not_supported());

        let result = adapter.attach_tracer(&handle).await;
        assert!(result.unwrap_err().is_not_supported());
    }

    // =========================================================================
    // LaunchdAdapter additional tests
    // =========================================================================

    #[tokio::test]
    async fn test_launchd_adapter_all_not_supported() {
        let adapter = LaunchdAdapter::new();
        let handle = DaemonHandle::launchd(DaemonId::new(), "com.test.daemon");

        let result = adapter.signal(&handle, Signal::Term).await;
        assert!(result.unwrap_err().is_not_supported());

        let result = adapter.status(&handle).await;
        assert!(result.unwrap_err().is_not_supported());

        let result = adapter.attach_tracer(&handle).await;
        assert!(result.unwrap_err().is_not_supported());
    }

    // =========================================================================
    // select_adapter tests
    // =========================================================================

    #[test]
    fn test_select_adapter_all_platforms() {
        for platform in [
            Platform::Linux,
            Platform::MacOS,
            Platform::Container,
            Platform::PepitaMicroVM,
            Platform::Wos,
            Platform::Native,
        ] {
            let adapter = select_adapter(platform);
            assert_eq!(adapter.platform(), platform);
        }
    }

    #[test]
    fn test_select_adapter_auto() {
        let adapter = select_adapter_auto();
        // Should return some valid adapter
        let platform = adapter.platform();
        assert!(matches!(
            platform,
            Platform::Linux
                | Platform::MacOS
                | Platform::Container
                | Platform::PepitaMicroVM
                | Platform::Wos
                | Platform::Native
        ));
    }

    #[test]
    fn test_systemd_adapter_default() {
        let adapter = SystemdAdapter::default();
        assert_eq!(adapter.platform(), Platform::Linux);
    }

    #[test]
    fn test_launchd_adapter_default() {
        let adapter = LaunchdAdapter::default();
        assert_eq!(adapter.platform(), Platform::MacOS);
    }

    #[test]
    fn test_container_adapter_default() {
        let adapter = ContainerAdapter::default();
        assert_eq!(adapter.platform(), Platform::Container);
        assert_eq!(adapter.runtime(), ContainerRuntime::Docker);
    }

    #[test]
    fn test_pepita_adapter_default() {
        let adapter = PepitaAdapter::default();
        assert_eq!(adapter.platform(), Platform::PepitaMicroVM);
    }

    #[test]
    fn test_wos_adapter_default() {
        let adapter = WosAdapter::default();
        assert_eq!(adapter.platform(), Platform::Wos);
    }

    // =========================================================================
    // Test daemon for adapter tests
    // =========================================================================

    use crate::config::DaemonConfig;
    use crate::daemon::{Daemon, DaemonContext};
    use crate::error::Result;
    use crate::metrics::DaemonMetrics;
    use crate::types::{DaemonId, ExitReason, HealthStatus};

    struct TestDaemon {
        id: DaemonId,
        metrics: DaemonMetrics,
    }

    impl TestDaemon {
        fn new() -> Self {
            Self {
                id: DaemonId::new(),
                metrics: DaemonMetrics::new(),
            }
        }
    }

    #[async_trait]
    impl Daemon for TestDaemon {
        fn id(&self) -> DaemonId {
            self.id
        }

        fn name(&self) -> &str {
            "test-daemon"
        }

        async fn init(&mut self, _config: &DaemonConfig) -> Result<()> {
            Ok(())
        }

        async fn run(&mut self, ctx: &mut DaemonContext) -> Result<ExitReason> {
            while !ctx.should_shutdown() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok(ExitReason::Graceful)
        }

        async fn shutdown(&mut self, _timeout: Duration) -> Result<()> {
            Ok(())
        }

        async fn health_check(&self) -> HealthStatus {
            HealthStatus::healthy(1)
        }

        fn metrics(&self) -> &DaemonMetrics {
            &self.metrics
        }
    }
}
