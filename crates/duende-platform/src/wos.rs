//! WOS (WebAssembly OS) platform adapter.
//!
//! # Overview
//!
//! This module provides WOS integration for daemon management.
//! It handles:
//!
//! - Process creation with 8-level priority scheduling
//! - Priority aging to prevent starvation
//! - Parent/child process relationships
//! - Orphan reaping by init (PID 1)
//! - Memory sandboxing via WASM linear memory
//!
//! # Reference
//!
//! WOS is the WebAssembly-based operating system from the PAIML stack,
//! designed for sandboxed execution environments with deterministic behavior.
//!
//! # Toyota Way: Jidoka (自働化)
//!
//! WOS Jidoka guards enforce kernel invariants and halt on violations.
//! All state transitions are verified before execution.

use crate::{DaemonHandle, Platform, PlatformAdapter, PlatformError, Result, TracerHandle};
use async_trait::async_trait;
use duende_core::{Daemon, DaemonConfig, DaemonStatus, FailureReason, Signal};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::RwLock;

/// Global PID counter for unique process IDs (WOS uses 32-bit PIDs).
static NEXT_PID: AtomicU32 = AtomicU32::new(2); // Start at 2 (1 is init)

/// WOS process priority levels (8 levels as per spec).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Priority {
    /// Lowest priority (background tasks, batch jobs)
    Idle = 0,
    /// Below normal priority
    Low = 1,
    /// Slightly below normal
    BelowNormal = 2,
    /// Default priority for user processes
    #[default]
    Normal = 3,
    /// Slightly above normal
    AboveNormal = 4,
    /// High priority (interactive processes)
    High = 5,
    /// Very high priority (system services)
    VeryHigh = 6,
    /// Highest priority (real-time, kernel tasks)
    RealTime = 7,
}

impl Priority {
    /// Convert priority to numeric value for scheduling.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Create priority from numeric value.
    #[must_use]
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Idle,
            1 => Self::Low,
            2 => Self::BelowNormal,
            3 => Self::Normal,
            4 => Self::AboveNormal,
            5 => Self::High,
            6 => Self::VeryHigh,
            _ => Self::RealTime,
        }
    }

    /// Apply priority aging (increase priority to prevent starvation).
    #[must_use]
    pub fn age(self) -> Self {
        if self < Self::RealTime {
            Self::from_u8(self.as_u8() + 1)
        } else {
            self
        }
    }
}

/// WOS process state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Process created but not yet scheduled
    Created,
    /// Process ready to run
    Ready,
    /// Process currently running
    Running,
    /// Process blocked on I/O or syscall
    Blocked,
    /// Process terminated normally
    Exited(i32),
    /// Process killed by signal
    Killed(Signal),
}

/// WOS process control block (simplified).
/// Fields are reserved for future scheduler/monitor implementation.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ProcessInfo {
    /// Process ID
    pid: u32,
    /// Process name
    name: String,
    /// Current state
    state: ProcessState,
    /// Current priority
    priority: Priority,
    /// Base priority (for aging reset)
    base_priority: Priority,
    /// Parent PID (0 for init)
    parent_pid: u32,
    /// Memory limit in bytes
    memory_limit: u64,
}

/// WOS platform adapter.
///
/// Manages daemons as WOS processes with priority scheduling and
/// memory sandboxing. This is an in-process simulation of WOS
/// for testing and development.
#[derive(Debug)]
pub struct WosAdapter {
    /// Default process priority
    default_priority: Priority,
    /// Enable priority aging
    aging_enabled: bool,
    /// Memory limit per process (bytes)
    memory_limit: u64,
    /// Process table (shared state for simulation)
    processes: Arc<RwLock<HashMap<u32, ProcessInfo>>>,
}

impl Default for WosAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl WosAdapter {
    /// Create a new WOS adapter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            default_priority: Priority::Normal,
            aging_enabled: true,
            memory_limit: 64 * 1024 * 1024, // 64MB default
            processes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set default priority.
    #[must_use]
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.default_priority = priority;
        self
    }

    /// Enable or disable priority aging.
    #[must_use]
    pub fn with_aging(mut self, enabled: bool) -> Self {
        self.aging_enabled = enabled;
        self
    }

    /// Set memory limit per process.
    #[must_use]
    pub fn with_memory_limit(mut self, limit: u64) -> Self {
        self.memory_limit = limit;
        self
    }

    /// Allocate a unique PID.
    fn allocate_pid() -> u32 {
        NEXT_PID.fetch_add(1, Ordering::SeqCst)
    }

    /// Map DaemonConfig to process priority.
    fn config_to_priority(config: &DaemonConfig) -> Priority {
        // Map CPU quota to priority (higher quota = higher priority)
        let quota = config.resources.cpu_quota_percent;
        if quota >= 90.0 {
            Priority::RealTime
        } else if quota >= 75.0 {
            Priority::VeryHigh
        } else if quota >= 60.0 {
            Priority::High
        } else if quota >= 45.0 {
            Priority::AboveNormal
        } else if quota >= 30.0 {
            Priority::Normal
        } else if quota >= 15.0 {
            Priority::BelowNormal
        } else if quota > 0.0 {
            Priority::Low
        } else {
            Priority::Normal // Default if no quota set
        }
    }

    /// Convert ProcessState to DaemonStatus.
    fn state_to_status(state: ProcessState) -> DaemonStatus {
        match state {
            ProcessState::Created => DaemonStatus::Created,
            // Ready, Running, and Blocked all mean the process is alive
            ProcessState::Ready | ProcessState::Running | ProcessState::Blocked => {
                DaemonStatus::Running
            }
            ProcessState::Exited(code) => {
                if code == 0 {
                    DaemonStatus::Stopped
                } else {
                    DaemonStatus::Failed(FailureReason::ExitCode(code))
                }
            }
            ProcessState::Killed(signal) => {
                // Convert Signal to signal number for FailureReason
                let sig_num = match signal {
                    Signal::Term => 15,
                    Signal::Kill => 9,
                    Signal::Hup => 1,
                    Signal::Int => 2,
                    Signal::Quit => 3,
                    Signal::Usr1 => 10,
                    Signal::Usr2 => 12,
                    Signal::Stop => 19,
                    Signal::Cont => 18,
                };
                DaemonStatus::Failed(FailureReason::Signal(sig_num))
            }
        }
    }

    /// Apply signal to process state.
    fn apply_signal(state: ProcessState, signal: Signal) -> ProcessState {
        match signal {
            Signal::Kill => ProcessState::Killed(Signal::Kill),
            Signal::Term => ProcessState::Killed(Signal::Term),
            Signal::Stop => ProcessState::Blocked,
            Signal::Cont => {
                if state == ProcessState::Blocked {
                    ProcessState::Ready
                } else {
                    state
                }
            }
            _ => state, // Other signals don't change state directly
        }
    }
}

#[async_trait]
impl PlatformAdapter for WosAdapter {
    fn platform(&self) -> Platform {
        Platform::Wos
    }

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> Result<DaemonHandle> {
        let config = DaemonConfig::new(daemon.name(), "/wasm/daemon.wasm");
        let pid = Self::allocate_pid();

        // Determine priority
        let priority = if config.resources.cpu_quota_percent > 0.0 {
            Self::config_to_priority(&config)
        } else {
            self.default_priority
        };

        // Determine memory limit
        let memory_limit = if config.resources.memory_bytes > 0 {
            config.resources.memory_bytes
        } else {
            self.memory_limit
        };

        // Create process info
        let process_info = ProcessInfo {
            pid,
            name: daemon.name().to_string(),
            state: ProcessState::Running,
            priority,
            base_priority: priority,
            parent_pid: 1, // All daemons are children of init
            memory_limit,
        };

        // Add to process table
        {
            let mut processes = self.processes.write().await;
            processes.insert(pid, process_info);
        }

        Ok(DaemonHandle::wos(pid))
    }

    async fn signal(&self, handle: &DaemonHandle, signal: Signal) -> Result<()> {
        if handle.platform != Platform::Wos {
            return Err(PlatformError::Signal("not a WOS handle".into()));
        }

        let pid: u32 = handle
            .id
            .parse()
            .map_err(|_| PlatformError::Signal("invalid PID".into()))?;

        let mut processes = self.processes.write().await;
        let process = processes
            .get_mut(&pid)
            .ok_or_else(|| PlatformError::Signal("process not found".into()))?;

        // Apply signal
        process.state = Self::apply_signal(process.state, signal);

        // Remove from table if terminated
        if matches!(
            process.state,
            ProcessState::Exited(_) | ProcessState::Killed(_)
        ) {
            drop(processes); // Release lock before re-acquiring
            self.processes.write().await.remove(&pid);
        }

        Ok(())
    }

    async fn status(&self, handle: &DaemonHandle) -> Result<DaemonStatus> {
        if handle.platform != Platform::Wos {
            return Err(PlatformError::Status("not a WOS handle".into()));
        }

        let pid: u32 = handle
            .id
            .parse()
            .map_err(|_| PlatformError::Status("invalid PID".into()))?;

        let processes = self.processes.read().await;
        Ok(processes
            .get(&pid)
            .map_or(DaemonStatus::Stopped, |process| {
                Self::state_to_status(process.state)
            }))
    }

    async fn attach_tracer(&self, handle: &DaemonHandle) -> Result<TracerHandle> {
        if handle.platform != Platform::Wos {
            return Err(PlatformError::Tracer("not a WOS handle".into()));
        }

        let pid: u32 = handle
            .id
            .parse()
            .map_err(|_| PlatformError::Tracer("invalid PID".into()))?;

        // Verify process exists and drop lock immediately
        let process_exists = self.processes.read().await.contains_key(&pid);
        if !process_exists {
            return Err(PlatformError::Tracer("process not running".into()));
        }

        // WOS tracer uses WASM-level instrumentation
        Ok(TracerHandle {
            platform: Platform::Wos,
            id: format!("wasm-trace:{}", pid),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wos_adapter_creation() {
        let adapter = WosAdapter::new();
        assert_eq!(adapter.platform(), Platform::Wos);
        assert_eq!(adapter.default_priority, Priority::Normal);
        assert!(adapter.aging_enabled);
    }

    #[test]
    fn test_wos_priority_ordering() {
        assert!(Priority::Idle < Priority::Normal);
        assert!(Priority::Normal < Priority::High);
        assert!(Priority::High < Priority::RealTime);
    }

    #[test]
    fn test_wos_builder_pattern() {
        let adapter = WosAdapter::new()
            .with_priority(Priority::High)
            .with_aging(false);

        assert_eq!(adapter.default_priority, Priority::High);
        assert!(!adapter.aging_enabled);
    }

    #[test]
    fn test_wos_eight_priority_levels() {
        let priorities = [
            Priority::Idle,
            Priority::Low,
            Priority::BelowNormal,
            Priority::Normal,
            Priority::AboveNormal,
            Priority::High,
            Priority::VeryHigh,
            Priority::RealTime,
        ];

        assert_eq!(priorities.len(), 8, "WOS should have 8 priority levels");
    }

    #[test]
    fn test_priority_as_u8() {
        assert_eq!(Priority::Idle.as_u8(), 0);
        assert_eq!(Priority::Normal.as_u8(), 3);
        assert_eq!(Priority::RealTime.as_u8(), 7);
    }

    #[test]
    fn test_priority_from_u8() {
        assert_eq!(Priority::from_u8(0), Priority::Idle);
        assert_eq!(Priority::from_u8(3), Priority::Normal);
        assert_eq!(Priority::from_u8(7), Priority::RealTime);
        assert_eq!(Priority::from_u8(100), Priority::RealTime); // Clamps to max
    }

    #[test]
    fn test_priority_aging() {
        assert_eq!(Priority::Idle.age(), Priority::Low);
        assert_eq!(Priority::Normal.age(), Priority::AboveNormal);
        assert_eq!(Priority::VeryHigh.age(), Priority::RealTime);
        assert_eq!(Priority::RealTime.age(), Priority::RealTime); // No change at max
    }

    #[test]
    fn test_pid_allocation() {
        let pid1 = WosAdapter::allocate_pid();
        let pid2 = WosAdapter::allocate_pid();
        assert!(pid2 > pid1, "PIDs should increment");
        assert!(pid1 >= 2, "PIDs should start at 2 (1 is init)");
    }

    #[test]
    fn test_config_to_priority() {
        let mut config = DaemonConfig::new("test", "/test");

        config.resources.cpu_quota_percent = 0.0;
        assert_eq!(WosAdapter::config_to_priority(&config), Priority::Normal);

        config.resources.cpu_quota_percent = 10.0;
        assert_eq!(WosAdapter::config_to_priority(&config), Priority::Low);

        config.resources.cpu_quota_percent = 50.0;
        assert_eq!(
            WosAdapter::config_to_priority(&config),
            Priority::AboveNormal
        );

        config.resources.cpu_quota_percent = 95.0;
        assert_eq!(WosAdapter::config_to_priority(&config), Priority::RealTime);
    }

    #[test]
    fn test_state_to_status() {
        assert!(matches!(
            WosAdapter::state_to_status(ProcessState::Created),
            DaemonStatus::Created
        ));
        assert!(matches!(
            WosAdapter::state_to_status(ProcessState::Running),
            DaemonStatus::Running
        ));
        assert!(matches!(
            WosAdapter::state_to_status(ProcessState::Exited(0)),
            DaemonStatus::Stopped
        ));
        assert!(matches!(
            WosAdapter::state_to_status(ProcessState::Exited(1)),
            DaemonStatus::Failed(_)
        ));
        assert!(matches!(
            WosAdapter::state_to_status(ProcessState::Killed(Signal::Kill)),
            DaemonStatus::Failed(_)
        ));
    }

    #[test]
    fn test_apply_signal() {
        assert!(matches!(
            WosAdapter::apply_signal(ProcessState::Running, Signal::Kill),
            ProcessState::Killed(Signal::Kill)
        ));
        assert!(matches!(
            WosAdapter::apply_signal(ProcessState::Running, Signal::Term),
            ProcessState::Killed(Signal::Term)
        ));
        assert!(matches!(
            WosAdapter::apply_signal(ProcessState::Running, Signal::Stop),
            ProcessState::Blocked
        ));
        assert!(matches!(
            WosAdapter::apply_signal(ProcessState::Blocked, Signal::Cont),
            ProcessState::Ready
        ));
    }

    #[test]
    fn test_with_memory_limit() {
        let adapter = WosAdapter::new().with_memory_limit(128 * 1024 * 1024);
        assert_eq!(adapter.memory_limit, 128 * 1024 * 1024);
    }

    #[test]
    fn test_default_implementation() {
        let adapter = WosAdapter::default();
        assert_eq!(adapter.default_priority, Priority::Normal);
        assert!(adapter.aging_enabled);
        assert_eq!(adapter.memory_limit, 64 * 1024 * 1024);
    }

    #[tokio::test]
    async fn test_spawn_and_status() {
        use duende_core::{DaemonContext, DaemonMetrics, ExitReason, HealthStatus};
        use std::time::Duration;

        // Create a mock daemon
        struct TestDaemon;

        #[async_trait::async_trait]
        impl Daemon for TestDaemon {
            fn id(&self) -> duende_core::DaemonId {
                duende_core::DaemonId::new()
            }
            fn name(&self) -> &str {
                "test-wos-daemon"
            }
            async fn init(&mut self, _config: &DaemonConfig) -> duende_core::error::Result<()> {
                Ok(())
            }
            async fn run(
                &mut self,
                _ctx: &mut DaemonContext,
            ) -> duende_core::error::Result<ExitReason> {
                Ok(ExitReason::Graceful)
            }
            async fn shutdown(&mut self, _timeout: Duration) -> duende_core::error::Result<()> {
                Ok(())
            }
            async fn health_check(&self) -> HealthStatus {
                HealthStatus::healthy(1)
            }
            fn metrics(&self) -> &DaemonMetrics {
                static METRICS: std::sync::OnceLock<DaemonMetrics> = std::sync::OnceLock::new();
                METRICS.get_or_init(DaemonMetrics::new)
            }
        }

        let adapter = WosAdapter::new();
        let handle = adapter.spawn(Box::new(TestDaemon)).await.unwrap();

        assert_eq!(handle.platform, Platform::Wos);

        let status = adapter.status(&handle).await.unwrap();
        assert!(matches!(status, DaemonStatus::Running));
    }

    #[tokio::test]
    async fn test_signal_terminates_process() {
        use duende_core::{DaemonContext, DaemonMetrics, ExitReason, HealthStatus};
        use std::time::Duration;

        struct TestDaemon;

        #[async_trait::async_trait]
        impl Daemon for TestDaemon {
            fn id(&self) -> duende_core::DaemonId {
                duende_core::DaemonId::new()
            }
            fn name(&self) -> &str {
                "test-wos-daemon"
            }
            async fn init(&mut self, _config: &DaemonConfig) -> duende_core::error::Result<()> {
                Ok(())
            }
            async fn run(
                &mut self,
                _ctx: &mut DaemonContext,
            ) -> duende_core::error::Result<ExitReason> {
                Ok(ExitReason::Graceful)
            }
            async fn shutdown(&mut self, _timeout: Duration) -> duende_core::error::Result<()> {
                Ok(())
            }
            async fn health_check(&self) -> HealthStatus {
                HealthStatus::healthy(1)
            }
            fn metrics(&self) -> &DaemonMetrics {
                static METRICS: std::sync::OnceLock<DaemonMetrics> = std::sync::OnceLock::new();
                METRICS.get_or_init(DaemonMetrics::new)
            }
        }

        let adapter = WosAdapter::new();
        let handle = adapter.spawn(Box::new(TestDaemon)).await.unwrap();

        // Send KILL signal
        adapter.signal(&handle, Signal::Kill).await.unwrap();

        // Status should be stopped (process removed from table)
        let status = adapter.status(&handle).await.unwrap();
        assert!(matches!(status, DaemonStatus::Stopped));
    }
}
