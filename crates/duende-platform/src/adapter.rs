//! Platform adapter trait.
//!
//! # Toyota Way: Standardized Work (標準作業)
//! Every platform follows the same adapter contract.

use async_trait::async_trait;

use duende_core::{Daemon, DaemonStatus, Signal};

use crate::detect::Platform;
use crate::error::Result;

/// Handle to a running daemon.
#[derive(Debug, Clone)]
pub struct DaemonHandle {
    /// Platform that spawned this daemon.
    pub platform: Platform,
    /// Platform-specific identifier.
    pub id: String,
    /// Process ID (if applicable).
    pub pid: Option<u32>,
}

impl DaemonHandle {
    /// Creates a native daemon handle.
    #[must_use]
    pub fn native(pid: u32) -> Self {
        Self {
            platform: Platform::Native,
            id: pid.to_string(),
            pid: Some(pid),
        }
    }

    /// Creates a systemd daemon handle.
    #[must_use]
    pub fn systemd(unit_name: impl Into<String>) -> Self {
        Self {
            platform: Platform::Linux,
            id: unit_name.into(),
            pid: None,
        }
    }

    /// Creates a launchd daemon handle.
    #[must_use]
    pub fn launchd(label: impl Into<String>) -> Self {
        Self {
            platform: Platform::MacOS,
            id: label.into(),
            pid: None,
        }
    }

    /// Creates a container daemon handle.
    #[must_use]
    pub fn container(container_id: impl Into<String>) -> Self {
        Self {
            platform: Platform::Container,
            id: container_id.into(),
            pid: None,
        }
    }

    /// Creates a pepita daemon handle.
    #[must_use]
    pub fn pepita(vm_id: impl Into<String>) -> Self {
        Self {
            platform: Platform::PepitaMicroVM,
            id: vm_id.into(),
            pid: None,
        }
    }

    /// Creates a WOS daemon handle.
    #[must_use]
    pub fn wos(process_id: u32) -> Self {
        Self {
            platform: Platform::Wos,
            id: process_id.to_string(),
            pid: Some(process_id),
        }
    }
}

/// Handle to an attached tracer.
#[derive(Debug)]
pub struct TracerHandle {
    /// Platform that owns this tracer.
    pub platform: Platform,
    /// Tracer identifier.
    pub id: String,
}

/// Platform-specific daemon adapter.
///
/// Each platform implements this trait to provide daemon lifecycle
/// management in a platform-appropriate way.
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    /// Returns the platform this adapter supports.
    fn platform(&self) -> Platform;

    /// Spawns a daemon on this platform.
    ///
    /// # Errors
    /// Returns an error if spawning fails.
    async fn spawn(&self, daemon: Box<dyn Daemon>) -> Result<DaemonHandle>;

    /// Sends a signal to a daemon.
    ///
    /// # Errors
    /// Returns an error if signaling fails.
    async fn signal(&self, handle: &DaemonHandle, sig: Signal) -> Result<()>;

    /// Queries the status of a daemon.
    ///
    /// # Errors
    /// Returns an error if status query fails.
    async fn status(&self, handle: &DaemonHandle) -> Result<DaemonStatus>;

    /// Attaches a tracer to a daemon (renacer integration).
    ///
    /// # Errors
    /// Returns an error if tracer attachment fails.
    async fn attach_tracer(&self, handle: &DaemonHandle) -> Result<TracerHandle>;
}
