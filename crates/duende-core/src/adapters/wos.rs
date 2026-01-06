//! WOS (WebAssembly Operating System) adapter implementation.
//!
//! Provides daemon management via WOS, PAIML's WebAssembly-based operating system.
//!
//! WOS runs WebAssembly modules as first-class processes with an 8-level
//! priority scheduler, process isolation, and IPC via message passing.

use crate::adapter::{DaemonHandle, PlatformAdapter, PlatformError, PlatformResult, TracerHandle};
use crate::daemon::Daemon;
use crate::platform::Platform;
use crate::types::{DaemonStatus, FailureReason, Signal};

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::RwLock;

/// Process ID allocator (starts at 2, as PID 1 is init).
static NEXT_PID: AtomicU32 = AtomicU32::new(2);

/// WOS (WebAssembly Operating System) adapter.
///
/// Manages daemons as WOS processes with priority-based scheduling.
///
/// # Architecture
///
/// ```text
/// WOS Kernel
/// ┌───────────────────────────────────────────┐
/// │  ┌─────────────┐    ┌─────────────────┐   │
/// │  │  Scheduler  │    │  Process Table  │   │
/// │  │  (8-level)  │    │                 │   │
/// │  └─────────────┘    └─────────────────┘   │
/// │         │                    │            │
/// │         ▼                    ▼            │
/// │  ┌─────────────────────────────────────┐  │
/// │  │         WASM Runtime                │  │
/// │  │  ┌───────┐ ┌───────┐ ┌───────┐     │  │
/// │  │  │ PID 1 │ │ PID 2 │ │ PID 3 │ ... │  │
/// │  │  │ init  │ │daemon1│ │daemon2│     │  │
/// │  │  └───────┘ └───────┘ └───────┘     │  │
/// │  └─────────────────────────────────────┘  │
/// └───────────────────────────────────────────┘
/// ```
///
/// # Priority Levels (0-7)
///
/// | Level | Name | Use Case |
/// |-------|------|----------|
/// | 0 | Critical | Kernel tasks, watchdogs |
/// | 1 | High | System services |
/// | 2 | Above Normal | Important daemons |
/// | 3 | Normal+ | User services with boost |
/// | 4 | Normal | Default for daemons |
/// | 5 | Below Normal | Background tasks |
/// | 6 | Low | Batch processing |
/// | 7 | Idle | Only when system idle |
///
/// # Example
///
/// ```rust,ignore
/// use duende_core::adapters::WosAdapter;
/// use duende_core::PlatformAdapter;
///
/// let adapter = WosAdapter::new();
/// let handle = adapter.spawn(my_daemon).await?;
/// ```
pub struct WosAdapter {
    /// Default priority for spawned processes (0-7)
    default_priority: u8,
    /// Process table for tracking spawned processes
    processes: Arc<RwLock<HashMap<u32, ProcessInfo>>>,
}

/// Information about a WOS process.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used for future process management operations
struct ProcessInfo {
    /// Process ID
    pid: u32,
    /// Parent process ID
    parent_pid: u32,
    /// Process name
    name: String,
    /// Priority level (0-7)
    priority: u8,
    /// Current process state
    state: ProcessState,
}

/// WOS process state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Variants used for future process state management
enum ProcessState {
    /// Process is ready to run
    Ready,
    /// Process is currently running
    Running,
    /// Process is waiting for I/O or event
    Waiting,
    /// Process is stopped (SIGSTOP)
    Stopped,
    /// Process has exited
    Exited(i32),
    /// Process was killed by signal
    Killed(i32),
}

impl WosAdapter {
    /// Creates a new WOS adapter with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            default_priority: 4, // Normal priority
            processes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Creates a WOS adapter with custom default priority.
    ///
    /// # Panics
    ///
    /// Panics if priority > 7.
    #[must_use]
    pub fn with_priority(priority: u8) -> Self {
        assert!(priority <= 7, "WOS priority must be 0-7");
        Self {
            default_priority: priority,
            processes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Returns the default priority.
    #[must_use]
    pub const fn default_priority(&self) -> u8 {
        self.default_priority
    }

    /// Allocates a new process ID.
    fn allocate_pid() -> u32 {
        NEXT_PID.fetch_add(1, Ordering::Relaxed)
    }

    /// Checks if running inside WOS.
    fn is_wos_environment() -> bool {
        // Check for WOS-specific markers
        cfg!(target_arch = "wasm32")
            || std::env::var("WOS_KERNEL").is_ok()
            || std::env::var("WOS_VERSION").is_ok()
    }

    /// Checks if wos-ctl CLI is available.
    async fn wos_ctl_available() -> bool {
        tokio::process::Command::new("wos-ctl")
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Maps Signal to WOS signal number.
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

    /// Maps priority name to level.
    #[must_use]
    pub fn priority_from_name(name: &str) -> Option<u8> {
        match name.to_lowercase().as_str() {
            "critical" => Some(0),
            "high" => Some(1),
            "above_normal" | "above-normal" => Some(2),
            "normal_plus" | "normal-plus" | "normal+" => Some(3),
            "normal" => Some(4),
            "below_normal" | "below-normal" => Some(5),
            "low" => Some(6),
            "idle" => Some(7),
            _ => None,
        }
    }

    /// Maps priority level to name.
    #[must_use]
    pub const fn priority_name(level: u8) -> &'static str {
        match level {
            0 => "critical",
            1 => "high",
            2 => "above_normal",
            3 => "normal_plus",
            4 => "normal",
            5 => "below_normal",
            6 => "low",
            7 => "idle",
            _ => "unknown",
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

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> PlatformResult<DaemonHandle> {
        // Check if we're in a WOS environment or have wos-ctl
        if !Self::is_wos_environment() && !Self::wos_ctl_available().await {
            return Err(PlatformError::spawn_failed(
                "WOS environment not detected and wos-ctl not found. \
                 Run inside WOS or install wos-ctl to manage WOS processes remotely.",
            ));
        }

        let daemon_name = daemon.name().to_string();
        let daemon_id = daemon.id();
        let pid = Self::allocate_pid();
        let priority = self.default_priority;

        // If we have wos-ctl, use it to spawn the process
        if Self::wos_ctl_available().await {
            // wos-ctl spawn --name <name> --priority <level> --wasm <path>
            let output = tokio::process::Command::new("wos-ctl")
                .arg("spawn")
                .arg("--name")
                .arg(&daemon_name)
                .arg("--priority")
                .arg(priority.to_string())
                .arg("--pid")
                .arg(pid.to_string())
                .output()
                .await
                .map_err(|e| {
                    PlatformError::spawn_failed(format!("Failed to execute wos-ctl: {}", e))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(PlatformError::spawn_failed(format!(
                    "wos-ctl spawn failed: {}",
                    stderr
                )));
            }
        }

        // Store process info
        let process_info = ProcessInfo {
            pid,
            parent_pid: 1, // Init is parent
            name: daemon_name,
            priority,
            state: ProcessState::Running,
        };

        self.processes.write().await.insert(pid, process_info);

        Ok(DaemonHandle::wos(daemon_id, pid))
    }

    async fn signal(&self, handle: &DaemonHandle, sig: Signal) -> PlatformResult<()> {
        let pid = handle
            .pid()
            .ok_or_else(|| PlatformError::spawn_failed("Invalid handle type for WOS adapter"))?;

        if Self::wos_ctl_available().await {
            // wos-ctl signal --pid <pid> --signal <sig>
            let output = tokio::process::Command::new("wos-ctl")
                .arg("signal")
                .arg("--pid")
                .arg(pid.to_string())
                .arg("--signal")
                .arg(Self::signal_number(sig).to_string())
                .output()
                .await
                .map_err(|e| {
                    PlatformError::signal_failed(format!("Failed to execute wos-ctl: {}", e))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(PlatformError::signal_failed(format!(
                    "wos-ctl signal failed: {}",
                    stderr
                )));
            }
        }

        // Update local state
        if let Some(process) = self.processes.write().await.get_mut(&pid) {
            match sig {
                Signal::Kill => process.state = ProcessState::Killed(9),
                Signal::Term => process.state = ProcessState::Exited(0),
                Signal::Stop => process.state = ProcessState::Stopped,
                Signal::Cont => {
                    if process.state == ProcessState::Stopped {
                        process.state = ProcessState::Running;
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn status(&self, handle: &DaemonHandle) -> PlatformResult<DaemonStatus> {
        let pid = handle
            .pid()
            .ok_or_else(|| PlatformError::spawn_failed("Invalid handle type for WOS adapter"))?;

        if Self::wos_ctl_available().await {
            // wos-ctl status --pid <pid> --json
            let output = tokio::process::Command::new("wos-ctl")
                .arg("status")
                .arg("--pid")
                .arg(pid.to_string())
                .arg("--json")
                .output()
                .await
                .map_err(|e| {
                    PlatformError::status_failed(format!("Failed to execute wos-ctl: {}", e))
                })?;

            if !output.status.success() {
                return Ok(DaemonStatus::Stopped);
            }

            let stdout = String::from_utf8_lossy(&output.stdout);

            if stdout.contains("\"state\": \"running\"") {
                return Ok(DaemonStatus::Running);
            } else if stdout.contains("\"state\": \"ready\"") {
                return Ok(DaemonStatus::Starting);
            } else if stdout.contains("\"state\": \"stopped\"") {
                return Ok(DaemonStatus::Paused);
            } else if stdout.contains("\"state\": \"exited\"") {
                // Parse exit code
                if let Some(code) = Self::extract_exit_code(&stdout) {
                    if code != 0 {
                        return Ok(DaemonStatus::Failed(FailureReason::ExitCode(code)));
                    }
                }
                return Ok(DaemonStatus::Stopped);
            }
        }

        // Check local state
        if let Some(process) = self.processes.read().await.get(&pid) {
            return Ok(match process.state {
                ProcessState::Ready | ProcessState::Running => DaemonStatus::Running,
                ProcessState::Waiting => DaemonStatus::Running,
                ProcessState::Stopped => DaemonStatus::Paused,
                ProcessState::Exited(code) if code != 0 => {
                    DaemonStatus::Failed(FailureReason::ExitCode(code))
                }
                ProcessState::Exited(_) => DaemonStatus::Stopped,
                ProcessState::Killed(sig) => DaemonStatus::Failed(FailureReason::Signal(sig)),
            });
        }

        Ok(DaemonStatus::Stopped)
    }

    async fn attach_tracer(&self, handle: &DaemonHandle) -> PlatformResult<TracerHandle> {
        let pid = handle
            .pid()
            .ok_or_else(|| PlatformError::spawn_failed("Invalid handle type for WOS adapter"))?;

        if pid == 0 {
            return Err(PlatformError::tracer_failed("Process not running"));
        }

        // WOS uses simulated tracing for WASM modules
        Ok(TracerHandle::simulated(handle.id()))
    }
}

impl WosAdapter {
    /// Extracts exit code from JSON status output.
    fn extract_exit_code(output: &str) -> Option<i32> {
        let patterns = ["\"exit_code\": ", "\"exit_code\":"];
        for pattern in patterns {
            if let Some(pos) = output.find(pattern) {
                let start = pos + pattern.len();
                let remaining = &output[start..];
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

    /// Lists all WOS processes.
    pub async fn list_processes(&self) -> PlatformResult<Vec<(u32, String)>> {
        if Self::wos_ctl_available().await {
            let output = tokio::process::Command::new("wos-ctl")
                .arg("ps")
                .arg("--format")
                .arg("pid,name")
                .output()
                .await
                .map_err(|e| {
                    PlatformError::spawn_failed(format!("Failed to execute wos-ctl: {}", e))
                })?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                return Ok(stdout
                    .lines()
                    .skip(1) // Skip header
                    .filter_map(|line| {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            Some((parts[0].parse().ok()?, parts[1].to_string()))
                        } else {
                            None
                        }
                    })
                    .collect());
            }
        }

        // Return local process table
        Ok(self
            .processes
            .read()
            .await
            .iter()
            .map(|(pid, info)| (*pid, info.name.clone()))
            .collect())
    }

    /// Sets process priority.
    pub async fn set_priority(&self, pid: u32, priority: u8) -> PlatformResult<()> {
        if priority > 7 {
            return Err(PlatformError::Config("Priority must be 0-7".into()));
        }

        if Self::wos_ctl_available().await {
            let output = tokio::process::Command::new("wos-ctl")
                .arg("renice")
                .arg("--pid")
                .arg(pid.to_string())
                .arg("--priority")
                .arg(priority.to_string())
                .output()
                .await
                .map_err(|e| {
                    PlatformError::spawn_failed(format!("Failed to execute wos-ctl: {}", e))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(PlatformError::spawn_failed(format!(
                    "wos-ctl renice failed: {}",
                    stderr
                )));
            }
        }

        // Update local state
        if let Some(process) = self.processes.write().await.get_mut(&pid) {
            process.priority = priority;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wos_adapter_new() {
        let adapter = WosAdapter::new();
        assert_eq!(adapter.default_priority(), 4);
        assert_eq!(adapter.platform(), Platform::Wos);
    }

    #[test]
    fn test_wos_adapter_with_priority() {
        let adapter = WosAdapter::with_priority(2);
        assert_eq!(adapter.default_priority(), 2);
    }

    #[test]
    fn test_wos_adapter_with_priority_boundary() {
        // Test all valid priority levels
        for priority in 0..=7 {
            let adapter = WosAdapter::with_priority(priority);
            assert_eq!(adapter.default_priority(), priority);
        }
    }

    #[test]
    #[should_panic(expected = "WOS priority must be 0-7")]
    fn test_wos_adapter_invalid_priority() {
        let _ = WosAdapter::with_priority(8);
    }

    #[test]
    fn test_wos_adapter_default() {
        let adapter = WosAdapter::default();
        assert_eq!(adapter.platform(), Platform::Wos);
    }

    #[test]
    fn test_allocate_pid() {
        let pid1 = WosAdapter::allocate_pid();
        let pid2 = WosAdapter::allocate_pid();
        assert!(pid2 > pid1);
    }

    #[test]
    fn test_allocate_pid_monotonic() {
        let pids: Vec<u32> = (0..10).map(|_| WosAdapter::allocate_pid()).collect();
        for i in 1..pids.len() {
            assert!(pids[i] > pids[i - 1], "PIDs must be monotonically increasing");
        }
    }

    #[test]
    fn test_signal_number() {
        assert_eq!(WosAdapter::signal_number(Signal::Term), 15);
        assert_eq!(WosAdapter::signal_number(Signal::Kill), 9);
    }

    #[test]
    fn test_signal_number_all_signals() {
        assert_eq!(WosAdapter::signal_number(Signal::Hup), 1);
        assert_eq!(WosAdapter::signal_number(Signal::Int), 2);
        assert_eq!(WosAdapter::signal_number(Signal::Quit), 3);
        assert_eq!(WosAdapter::signal_number(Signal::Kill), 9);
        assert_eq!(WosAdapter::signal_number(Signal::Usr1), 10);
        assert_eq!(WosAdapter::signal_number(Signal::Usr2), 12);
        assert_eq!(WosAdapter::signal_number(Signal::Term), 15);
        assert_eq!(WosAdapter::signal_number(Signal::Cont), 18);
        assert_eq!(WosAdapter::signal_number(Signal::Stop), 19);
    }

    #[test]
    fn test_priority_from_name() {
        assert_eq!(WosAdapter::priority_from_name("critical"), Some(0));
        assert_eq!(WosAdapter::priority_from_name("high"), Some(1));
        assert_eq!(WosAdapter::priority_from_name("normal"), Some(4));
        assert_eq!(WosAdapter::priority_from_name("idle"), Some(7));
        assert_eq!(WosAdapter::priority_from_name("invalid"), None);
    }

    #[test]
    fn test_priority_from_name_all_variants() {
        // Level 0
        assert_eq!(WosAdapter::priority_from_name("critical"), Some(0));
        assert_eq!(WosAdapter::priority_from_name("CRITICAL"), Some(0));

        // Level 1
        assert_eq!(WosAdapter::priority_from_name("high"), Some(1));
        assert_eq!(WosAdapter::priority_from_name("HIGH"), Some(1));

        // Level 2 - various separators
        assert_eq!(WosAdapter::priority_from_name("above_normal"), Some(2));
        assert_eq!(WosAdapter::priority_from_name("above-normal"), Some(2));
        assert_eq!(WosAdapter::priority_from_name("ABOVE_NORMAL"), Some(2));
        assert_eq!(WosAdapter::priority_from_name("Above-Normal"), Some(2));

        // Level 3 - normal plus variants
        assert_eq!(WosAdapter::priority_from_name("normal_plus"), Some(3));
        assert_eq!(WosAdapter::priority_from_name("normal-plus"), Some(3));
        assert_eq!(WosAdapter::priority_from_name("normal+"), Some(3));
        assert_eq!(WosAdapter::priority_from_name("NORMAL+"), Some(3));

        // Level 4
        assert_eq!(WosAdapter::priority_from_name("normal"), Some(4));
        assert_eq!(WosAdapter::priority_from_name("NORMAL"), Some(4));

        // Level 5
        assert_eq!(WosAdapter::priority_from_name("below_normal"), Some(5));
        assert_eq!(WosAdapter::priority_from_name("below-normal"), Some(5));
        assert_eq!(WosAdapter::priority_from_name("BELOW_NORMAL"), Some(5));

        // Level 6
        assert_eq!(WosAdapter::priority_from_name("low"), Some(6));
        assert_eq!(WosAdapter::priority_from_name("LOW"), Some(6));

        // Level 7
        assert_eq!(WosAdapter::priority_from_name("idle"), Some(7));
        assert_eq!(WosAdapter::priority_from_name("IDLE"), Some(7));

        // Invalid
        assert_eq!(WosAdapter::priority_from_name(""), None);
        assert_eq!(WosAdapter::priority_from_name("unknown"), None);
        assert_eq!(WosAdapter::priority_from_name("realtime"), None);
    }

    #[test]
    fn test_priority_name() {
        assert_eq!(WosAdapter::priority_name(0), "critical");
        assert_eq!(WosAdapter::priority_name(4), "normal");
        assert_eq!(WosAdapter::priority_name(7), "idle");
        assert_eq!(WosAdapter::priority_name(8), "unknown");
    }

    #[test]
    fn test_priority_name_all_levels() {
        assert_eq!(WosAdapter::priority_name(0), "critical");
        assert_eq!(WosAdapter::priority_name(1), "high");
        assert_eq!(WosAdapter::priority_name(2), "above_normal");
        assert_eq!(WosAdapter::priority_name(3), "normal_plus");
        assert_eq!(WosAdapter::priority_name(4), "normal");
        assert_eq!(WosAdapter::priority_name(5), "below_normal");
        assert_eq!(WosAdapter::priority_name(6), "low");
        assert_eq!(WosAdapter::priority_name(7), "idle");

        // Out of range
        assert_eq!(WosAdapter::priority_name(8), "unknown");
        assert_eq!(WosAdapter::priority_name(100), "unknown");
        assert_eq!(WosAdapter::priority_name(255), "unknown");
    }

    #[test]
    fn test_extract_exit_code() {
        assert_eq!(WosAdapter::extract_exit_code(r#""exit_code": 0"#), Some(0));
        assert_eq!(WosAdapter::extract_exit_code(r#""exit_code": 1"#), Some(1));
        assert_eq!(
            WosAdapter::extract_exit_code(r#""exit_code":137"#),
            Some(137)
        );
        assert_eq!(WosAdapter::extract_exit_code("no exit code"), None);
    }

    #[test]
    fn test_extract_exit_code_various_formats() {
        // With space after colon
        assert_eq!(WosAdapter::extract_exit_code(r#""exit_code": 0"#), Some(0));
        assert_eq!(WosAdapter::extract_exit_code(r#""exit_code": 42"#), Some(42));
        assert_eq!(WosAdapter::extract_exit_code(r#""exit_code": 255"#), Some(255));

        // Without space after colon
        assert_eq!(WosAdapter::extract_exit_code(r#""exit_code":0"#), Some(0));
        assert_eq!(WosAdapter::extract_exit_code(r#""exit_code":1"#), Some(1));

        // Negative exit codes
        assert_eq!(WosAdapter::extract_exit_code(r#""exit_code": -1"#), Some(-1));
        assert_eq!(WosAdapter::extract_exit_code(r#""exit_code":-15"#), Some(-15));

        // In larger JSON context
        assert_eq!(
            WosAdapter::extract_exit_code(r#"{"state": "exited", "exit_code": 128}"#),
            Some(128)
        );

        // Edge cases
        assert_eq!(WosAdapter::extract_exit_code(""), None);
        assert_eq!(WosAdapter::extract_exit_code("exit_code"), None);
        assert_eq!(WosAdapter::extract_exit_code(r#""exit_code": "#), None);
        assert_eq!(WosAdapter::extract_exit_code(r#""exit_code":"#), None);
    }

    #[test]
    fn test_process_state_equality() {
        assert_eq!(ProcessState::Ready, ProcessState::Ready);
        assert_eq!(ProcessState::Running, ProcessState::Running);
        assert_eq!(ProcessState::Waiting, ProcessState::Waiting);
        assert_eq!(ProcessState::Stopped, ProcessState::Stopped);
        assert_eq!(ProcessState::Exited(0), ProcessState::Exited(0));
        assert_eq!(ProcessState::Killed(9), ProcessState::Killed(9));

        // Different values should not be equal
        assert_ne!(ProcessState::Exited(0), ProcessState::Exited(1));
        assert_ne!(ProcessState::Killed(9), ProcessState::Killed(15));
        assert_ne!(ProcessState::Ready, ProcessState::Running);
    }

    #[test]
    fn test_process_state_clone() {
        let states = [
            ProcessState::Ready,
            ProcessState::Running,
            ProcessState::Waiting,
            ProcessState::Stopped,
            ProcessState::Exited(0),
            ProcessState::Exited(1),
            ProcessState::Killed(9),
            ProcessState::Killed(15),
        ];

        for state in states {
            let cloned = state;
            assert_eq!(state, cloned);
        }
    }

    #[test]
    fn test_process_state_debug() {
        assert!(format!("{:?}", ProcessState::Ready).contains("Ready"));
        assert!(format!("{:?}", ProcessState::Running).contains("Running"));
        assert!(format!("{:?}", ProcessState::Waiting).contains("Waiting"));
        assert!(format!("{:?}", ProcessState::Stopped).contains("Stopped"));
        assert!(format!("{:?}", ProcessState::Exited(42)).contains("42"));
        assert!(format!("{:?}", ProcessState::Killed(9)).contains("9"));
    }

    #[test]
    fn test_process_info_clone() {
        let info = ProcessInfo {
            pid: 123,
            parent_pid: 1,
            name: "test-daemon".to_string(),
            priority: 4,
            state: ProcessState::Running,
        };

        let cloned = info.clone();
        assert_eq!(cloned.pid, 123);
        assert_eq!(cloned.parent_pid, 1);
        assert_eq!(cloned.name, "test-daemon");
        assert_eq!(cloned.priority, 4);
        assert_eq!(cloned.state, ProcessState::Running);
    }

    #[test]
    fn test_process_info_debug() {
        let info = ProcessInfo {
            pid: 42,
            parent_pid: 1,
            name: "debug-test".to_string(),
            priority: 2,
            state: ProcessState::Waiting,
        };

        let debug_str = format!("{:?}", info);
        assert!(debug_str.contains("42"));
        assert!(debug_str.contains("debug-test"));
        assert!(debug_str.contains("Waiting"));
    }

    #[test]
    fn test_is_wos_environment_not_wos() {
        // We're not running in WOS, so this should be false
        // (unless WOS_KERNEL or WOS_VERSION env vars are set)
        let in_wos = WosAdapter::is_wos_environment();
        // Just verify it doesn't panic
        let _ = in_wos;
    }

    #[tokio::test]
    async fn test_wos_ctl_available_not_installed() {
        // wos-ctl is likely not installed in test environment
        let available = WosAdapter::wos_ctl_available().await;
        // Just verify it doesn't panic and returns a boolean
        let _ = available;
    }

    #[tokio::test]
    async fn test_wos_adapter_list_processes_empty() {
        let adapter = WosAdapter::new();
        // Without WOS environment, list should return local (empty) table
        if !WosAdapter::is_wos_environment() && !WosAdapter::wos_ctl_available().await {
            let processes = adapter.list_processes().await;
            // Either returns Ok with empty list or an error
            match processes {
                Ok(list) => assert!(list.is_empty()),
                Err(_) => {} // Also acceptable if wos-ctl not found
            }
        }
    }

    #[tokio::test]
    async fn test_wos_adapter_set_priority_invalid() {
        let adapter = WosAdapter::new();
        // Priority 8 is invalid
        let result = adapter.set_priority(123, 8).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("0-7"));
    }

    #[tokio::test]
    async fn test_wos_adapter_set_priority_valid_range() {
        let adapter = WosAdapter::new();
        // All priorities 0-7 should be valid (but may fail if pid doesn't exist)
        for priority in 0..=7 {
            let result = adapter.set_priority(99999, priority).await;
            // Should not fail with config error (may fail for other reasons)
            if let Err(e) = &result {
                assert!(
                    !e.to_string().contains("0-7"),
                    "Priority {} should be valid",
                    priority
                );
            }
        }
    }

    #[tokio::test]
    async fn test_wos_adapter_signal_invalid_handle() {
        let adapter = WosAdapter::new();
        // Create a handle without a PID (container type doesn't have pid())
        let handle = DaemonHandle::container(crate::types::DaemonId::new(), "docker", "test-container");
        let result = adapter.signal(&handle, Signal::Term).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_wos_adapter_status_invalid_handle() {
        let adapter = WosAdapter::new();
        let handle = DaemonHandle::container(crate::types::DaemonId::new(), "docker", "test-container");
        let result = adapter.status(&handle).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_wos_adapter_attach_tracer_invalid_handle() {
        let adapter = WosAdapter::new();
        let handle = DaemonHandle::container(crate::types::DaemonId::new(), "docker", "test-container");
        let result = adapter.attach_tracer(&handle).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_wos_adapter_attach_tracer_pid_zero() {
        let adapter = WosAdapter::new();
        // WOS handle with pid 0 should fail
        let handle = DaemonHandle::wos(crate::types::DaemonId::new(), 0);
        let result = adapter.attach_tracer(&handle).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not running"));
    }

    #[tokio::test]
    async fn test_wos_adapter_attach_tracer_valid_pid() {
        let adapter = WosAdapter::new();
        let id = crate::types::DaemonId::new();
        let handle = DaemonHandle::wos(id, 123);
        let result = adapter.attach_tracer(&handle).await;
        assert!(result.is_ok());
        let tracer = result.unwrap();
        assert_eq!(tracer.daemon_id(), id);
    }

    #[tokio::test]
    async fn test_wos_adapter_status_unknown_pid() {
        let adapter = WosAdapter::new();
        let handle = DaemonHandle::wos(crate::types::DaemonId::new(), 99999);
        // Without WOS environment, should return Stopped
        if !WosAdapter::is_wos_environment() && !WosAdapter::wos_ctl_available().await {
            let status = adapter.status(&handle).await.unwrap();
            assert_eq!(status, DaemonStatus::Stopped);
        }
    }

    #[tokio::test]
    async fn test_wos_adapter_spawn_fails_without_wos() {
        // Skip if WOS environment is detected
        if WosAdapter::is_wos_environment() {
            return;
        }

        let adapter = WosAdapter::new();

        struct TestDaemon {
            id: crate::types::DaemonId,
            metrics: crate::metrics::DaemonMetrics,
        }

        #[async_trait::async_trait]
        impl crate::daemon::Daemon for TestDaemon {
            fn id(&self) -> crate::types::DaemonId {
                self.id
            }
            fn name(&self) -> &str {
                "test"
            }
            async fn init(&mut self, _: &crate::config::DaemonConfig) -> crate::error::Result<()> {
                Ok(())
            }
            async fn run(
                &mut self,
                _: &mut crate::daemon::DaemonContext,
            ) -> crate::error::Result<crate::types::ExitReason> {
                Ok(crate::types::ExitReason::Graceful)
            }
            async fn shutdown(&mut self, _: std::time::Duration) -> crate::error::Result<()> {
                Ok(())
            }
            async fn health_check(&self) -> crate::types::HealthStatus {
                crate::types::HealthStatus::healthy(1)
            }
            fn metrics(&self) -> &crate::metrics::DaemonMetrics {
                &self.metrics
            }
        }

        let daemon = TestDaemon {
            id: crate::types::DaemonId::new(),
            metrics: crate::metrics::DaemonMetrics::new(),
        };

        let result = adapter.spawn(Box::new(daemon)).await;
        assert!(result.is_err());
        // Should fail because WOS is not available
        let err = result.unwrap_err();
        assert!(err.to_string().contains("WOS") || err.to_string().contains("wos-ctl"));
    }

    #[test]
    fn test_priority_roundtrip() {
        // Verify that priority_name and priority_from_name are consistent
        for level in 0..=7 {
            let name = WosAdapter::priority_name(level);
            let parsed = WosAdapter::priority_from_name(name);
            assert_eq!(
                parsed,
                Some(level),
                "Roundtrip failed for level {}",
                level
            );
        }
    }
}
