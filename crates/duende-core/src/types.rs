//! Core types for daemon lifecycle management.
//!
//! Per Iron Lotus Framework: UUIDs for stable IDs (Section 12.3),
//! explicit state machines, no implicit transitions.

use serde::{Deserialize, Serialize};

/// Unique identifier for a daemon instance.
///
/// Per Iron Lotus Framework case study, we use UUIDs instead of
/// indices to prevent invalidation when daemons restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DaemonId(uuid::Uuid);

impl DaemonId {
    /// Creates a new random daemon ID.
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// Creates a daemon ID from a UUID.
    #[must_use]
    pub const fn from_uuid(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the inner UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }
}

impl Default for DaemonId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DaemonId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Daemon lifecycle state.
///
/// State transitions follow a strict state machine:
/// ```text
/// Created → Starting → Running ↔ Paused → Stopping → Stopped
///                  ↓                   ↓
///               Failed ←───────────────┘
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DaemonStatus {
    /// Daemon has been created but not started.
    Created,
    /// Daemon is starting (init phase).
    Starting,
    /// Daemon is running normally.
    Running,
    /// Daemon is paused (SIGSTOP or equivalent).
    Paused,
    /// Daemon is shutting down.
    Stopping,
    /// Daemon has stopped normally.
    Stopped,
    /// Daemon has failed.
    Failed(FailureReason),
}

impl DaemonStatus {
    /// Returns true if the daemon is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Stopped | Self::Failed(_))
    }

    /// Returns true if the daemon is active (running or paused).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Paused)
    }

    /// Returns true if the daemon can receive signals.
    #[must_use]
    pub const fn can_signal(&self) -> bool {
        matches!(self, Self::Running | Self::Paused | Self::Stopping)
    }
}

/// Reason for daemon failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureReason {
    /// Crashed with signal.
    Signal(i32),
    /// Exited with non-zero code.
    ExitCode(i32),
    /// Resource exhaustion.
    ResourceExhausted,
    /// Policy violation.
    PolicyViolation,
    /// Health check timeout.
    HealthCheckTimeout,
    /// Internal error.
    Internal,
}

/// Reason for daemon exit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExitReason {
    /// Graceful shutdown requested.
    Graceful,
    /// Received signal.
    Signal(Signal),
    /// Error occurred.
    Error(String),
    /// Resource limit exceeded.
    ResourceExhausted(String),
    /// Policy violation.
    PolicyViolation(String),
}

/// Unix-style signals for daemon control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Signal {
    /// Hangup (reload configuration).
    Hup,
    /// Interrupt (graceful shutdown).
    Int,
    /// Quit (graceful shutdown with core dump).
    Quit,
    /// Terminate (graceful shutdown).
    Term,
    /// Kill (immediate termination).
    Kill,
    /// User signal 1.
    Usr1,
    /// User signal 2.
    Usr2,
    /// Stop (pause).
    Stop,
    /// Continue (resume).
    Cont,
}

impl Signal {
    /// Returns the Unix signal number.
    #[must_use]
    pub const fn as_i32(&self) -> i32 {
        match self {
            Self::Hup => 1,
            Self::Int => 2,
            Self::Quit => 3,
            Self::Term => 15,
            Self::Kill => 9,
            Self::Usr1 => 10,
            Self::Usr2 => 12,
            Self::Stop => 19,
            Self::Cont => 18,
        }
    }

    /// Creates a signal from a Unix signal number.
    #[must_use]
    pub const fn from_i32(sig: i32) -> Option<Self> {
        match sig {
            1 => Some(Self::Hup),
            2 => Some(Self::Int),
            3 => Some(Self::Quit),
            15 => Some(Self::Term),
            9 => Some(Self::Kill),
            10 => Some(Self::Usr1),
            12 => Some(Self::Usr2),
            19 => Some(Self::Stop),
            18 => Some(Self::Cont),
            _ => None,
        }
    }
}

/// Health check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Overall health status.
    pub healthy: bool,
    /// Individual health checks.
    pub checks: Vec<HealthCheck>,
    /// Time taken for health check.
    pub latency_ms: u64,
    /// Timestamp of last check (Unix epoch ms).
    pub last_check_epoch_ms: u64,
}

impl HealthStatus {
    /// Creates a healthy status.
    #[must_use]
    pub fn healthy(latency_ms: u64) -> Self {
        Self {
            healthy: true,
            checks: vec![],
            latency_ms,
            last_check_epoch_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        }
    }

    /// Creates an unhealthy status.
    #[must_use]
    pub fn unhealthy(reason: impl Into<String>, latency_ms: u64) -> Self {
        Self {
            healthy: false,
            checks: vec![HealthCheck {
                name: "main".to_string(),
                passed: false,
                message: Some(reason.into()),
            }],
            latency_ms,
            last_check_epoch_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        }
    }

    /// Returns true if all checks passed.
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.healthy
    }
}

/// Individual health check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Check name.
    pub name: String,
    /// Whether the check passed.
    pub passed: bool,
    /// Optional message.
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_id_unique() {
        let id1 = DaemonId::new();
        let id2 = DaemonId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_daemon_id_display() {
        let id = DaemonId::new();
        let display = format!("{}", id);
        // UUID format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
        assert!(display.contains('-'));
        assert_eq!(display.len(), 36);
    }

    #[test]
    fn test_daemon_id_default() {
        let id1 = DaemonId::default();
        let id2 = DaemonId::default();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_daemon_id_from_uuid() {
        let uuid = uuid::Uuid::nil();
        let id = DaemonId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), &uuid);
    }

    #[test]
    fn test_daemon_status_transitions() {
        assert!(!DaemonStatus::Created.is_terminal());
        assert!(DaemonStatus::Stopped.is_terminal());
        assert!(DaemonStatus::Failed(FailureReason::Internal).is_terminal());

        assert!(DaemonStatus::Running.is_active());
        assert!(!DaemonStatus::Starting.is_active());

        assert!(DaemonStatus::Running.can_signal());
        assert!(!DaemonStatus::Stopped.can_signal());
    }

    #[test]
    fn test_daemon_status_all_variants() {
        // Test all non-terminal states
        for status in [
            DaemonStatus::Created,
            DaemonStatus::Starting,
            DaemonStatus::Running,
            DaemonStatus::Paused,
            DaemonStatus::Stopping,
        ] {
            if matches!(status, DaemonStatus::Running | DaemonStatus::Paused) {
                assert!(status.is_active());
            } else {
                assert!(!status.is_active());
            }
            assert!(!status.is_terminal());
        }

        // Test all failure reasons in terminal state
        for reason in [
            FailureReason::Signal(9),
            FailureReason::ExitCode(1),
            FailureReason::ResourceExhausted,
            FailureReason::PolicyViolation,
            FailureReason::HealthCheckTimeout,
            FailureReason::Internal,
        ] {
            let status = DaemonStatus::Failed(reason);
            assert!(status.is_terminal());
            assert!(!status.is_active());
            assert!(!status.can_signal());
        }
    }

    #[test]
    fn test_daemon_status_can_signal() {
        // States that can receive signals
        assert!(DaemonStatus::Running.can_signal());
        assert!(DaemonStatus::Paused.can_signal());
        assert!(DaemonStatus::Stopping.can_signal());

        // States that cannot receive signals
        assert!(!DaemonStatus::Created.can_signal());
        assert!(!DaemonStatus::Starting.can_signal());
        assert!(!DaemonStatus::Stopped.can_signal());
        assert!(!DaemonStatus::Failed(FailureReason::Internal).can_signal());
    }

    #[test]
    fn test_signal_conversion() {
        assert_eq!(Signal::Term.as_i32(), 15);
        assert_eq!(Signal::from_i32(15), Some(Signal::Term));
        assert_eq!(Signal::from_i32(999), None);
    }

    #[test]
    fn test_signal_all_variants() {
        // Test all signal conversions
        let signals = [
            (Signal::Hup, 1),
            (Signal::Int, 2),
            (Signal::Quit, 3),
            (Signal::Term, 15),
            (Signal::Kill, 9),
            (Signal::Usr1, 10),
            (Signal::Usr2, 12),
            (Signal::Stop, 19),
            (Signal::Cont, 18),
        ];

        for (sig, num) in signals {
            assert_eq!(sig.as_i32(), num);
            assert_eq!(Signal::from_i32(num), Some(sig));
        }
    }

    #[test]
    fn test_health_status() {
        let healthy = HealthStatus::healthy(5);
        assert!(healthy.is_healthy());

        let unhealthy = HealthStatus::unhealthy("timeout", 100);
        assert!(!unhealthy.is_healthy());
    }

    #[test]
    fn test_health_status_timestamp() {
        let healthy = HealthStatus::healthy(5);
        // Timestamp should be non-zero (set to current time)
        assert!(healthy.last_check_epoch_ms > 0);
    }

    #[test]
    fn test_health_status_checks() {
        let healthy = HealthStatus::healthy(5);
        assert!(healthy.checks.is_empty());

        let unhealthy = HealthStatus::unhealthy("test error", 10);
        assert_eq!(unhealthy.checks.len(), 1);
        assert!(!unhealthy.checks[0].passed);
        assert_eq!(unhealthy.checks[0].name, "main");
        assert!(unhealthy.checks[0].message.is_some());
    }

    #[test]
    fn test_health_check_struct() {
        let check = HealthCheck {
            name: "database".to_string(),
            passed: true,
            message: Some("connected".to_string()),
        };
        assert!(check.passed);
        assert_eq!(check.name, "database");
    }

    #[test]
    fn test_exit_reason_variants() {
        // Test all exit reason variants exist and can be created
        let _ = ExitReason::Graceful;
        let _ = ExitReason::Signal(Signal::Term);
        let _ = ExitReason::Error("test error".to_string());
        let _ = ExitReason::ResourceExhausted("memory".to_string());
        let _ = ExitReason::PolicyViolation("cpu".to_string());
    }

    #[test]
    fn test_daemon_id_serialize_roundtrip() {
        let id = DaemonId::new();
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: DaemonId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn test_daemon_status_serialize_roundtrip() {
        for status in [
            DaemonStatus::Created,
            DaemonStatus::Starting,
            DaemonStatus::Running,
            DaemonStatus::Paused,
            DaemonStatus::Stopping,
            DaemonStatus::Stopped,
            DaemonStatus::Failed(FailureReason::ExitCode(1)),
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: DaemonStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }

    #[test]
    fn test_signal_serialize_roundtrip() {
        for sig in [
            Signal::Hup,
            Signal::Int,
            Signal::Quit,
            Signal::Term,
            Signal::Kill,
            Signal::Usr1,
            Signal::Usr2,
            Signal::Stop,
            Signal::Cont,
        ] {
            let json = serde_json::to_string(&sig).unwrap();
            let deserialized: Signal = serde_json::from_str(&json).unwrap();
            assert_eq!(sig, deserialized);
        }
    }
}
