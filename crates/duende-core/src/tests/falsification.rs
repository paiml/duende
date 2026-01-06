//! Falsification test framework.
//!
//! # Certeza Methodology - Popperian Falsification
//!
//! Per daemon-tools-spec.md Section 9.2, each test is designed to
//! DISPROVE that the system has a certain property. If we CANNOT
//! disprove it, we gain confidence that the property holds.
//!
//! ## Test Categories
//!
//! | Category | ID Range | Description |
//! |----------|----------|-------------|
//! | A | F001-F020 | Daemon Lifecycle |
//! | B | F021-F040 | Signal Handling |
//! | C | F041-F060 | Resource Limits |
//! | D | F061-F080 | Health Checks |
//! | E | F081-F100 | Observability |
//! | F | F101-F110 | Platform Adapters |

use std::fmt;
use std::time::Duration;

/// Falsification test definition.
#[derive(Debug, Clone)]
pub struct FalsificationTest {
    /// Test ID (e.g., "F001").
    id: String,
    /// Test category.
    category: Category,
    /// Test description.
    description: String,
    /// Property being tested.
    property: String,
    /// Expected timeout.
    timeout: Duration,
}

/// Test category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// A: Daemon Lifecycle (F001-F020)
    Lifecycle,
    /// B: Signal Handling (F021-F040)
    SignalHandling,
    /// C: Resource Limits (F041-F060)
    ResourceLimits,
    /// D: Health Checks (F061-F080)
    HealthChecks,
    /// E: Observability (F081-F100)
    Observability,
    /// F: Platform Adapters (F101-F110)
    PlatformAdapters,
}

impl Category {
    /// Returns the category name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Lifecycle => "Lifecycle",
            Self::SignalHandling => "Signal Handling",
            Self::ResourceLimits => "Resource Limits",
            Self::HealthChecks => "Health Checks",
            Self::Observability => "Observability",
            Self::PlatformAdapters => "Platform Adapters",
        }
    }

    /// Returns the ID prefix for this category.
    #[must_use]
    pub const fn prefix(&self) -> char {
        match self {
            Self::Lifecycle => 'A',
            Self::SignalHandling => 'B',
            Self::ResourceLimits => 'C',
            Self::HealthChecks => 'D',
            Self::Observability => 'E',
            Self::PlatformAdapters => 'F',
        }
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl FalsificationTest {
    /// Creates a new falsification test.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        category: Category,
        description: impl Into<String>,
        property: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            category,
            description: description.into(),
            property: property.into(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Sets the test timeout.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Returns the test ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the test category.
    #[must_use]
    pub const fn category(&self) -> Category {
        self.category
    }

    /// Returns the test description.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Returns the property being tested.
    #[must_use]
    pub fn property(&self) -> &str {
        &self.property
    }

    /// Returns the test timeout.
    #[must_use]
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }
}

impl fmt::Display for FalsificationTest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {} - {}", self.id, self.category, self.description)
    }
}

/// All falsification tests per daemon-tools-spec.md Section 9.2.
pub fn all_tests() -> Vec<FalsificationTest> {
    let mut tests = Vec::new();

    // Category A: Daemon Lifecycle (F001-F020)
    tests.extend(lifecycle_tests());

    // Category B: Signal Handling (F021-F040)
    tests.extend(signal_tests());

    // Category C: Resource Limits (F041-F060)
    tests.extend(resource_tests());

    // Category D: Health Checks (F061-F080)
    tests.extend(health_tests());

    // Category E: Observability (F081-F100)
    tests.extend(observability_tests());

    // Category F: Platform Adapters (F101-F110)
    tests.extend(platform_tests());

    tests
}

/// Category A: Daemon Lifecycle tests (F001-F020).
fn lifecycle_tests() -> Vec<FalsificationTest> {
    vec![
        FalsificationTest::new(
            "F001",
            Category::Lifecycle,
            "Init must be called before run",
            "Daemon.init() is called exactly once before Daemon.run()",
        ),
        FalsificationTest::new(
            "F002",
            Category::Lifecycle,
            "Init failure prevents run",
            "If init() returns Err, run() is never called",
        ),
        FalsificationTest::new(
            "F003",
            Category::Lifecycle,
            "Shutdown is called after run completes",
            "Daemon.shutdown() is called exactly once after run() returns",
        ),
        FalsificationTest::new(
            "F004",
            Category::Lifecycle,
            "Status transitions are valid",
            "Status only transitions via valid state machine paths",
        ),
        FalsificationTest::new(
            "F005",
            Category::Lifecycle,
            "Terminal states are final",
            "Once in Stopped or Failed, no further transitions occur",
        ),
        FalsificationTest::new(
            "F006",
            Category::Lifecycle,
            "Daemon ID is unique",
            "No two daemons share the same DaemonId",
        ),
        FalsificationTest::new(
            "F007",
            Category::Lifecycle,
            "Config validation rejects invalid names",
            "DaemonConfig.validate() fails for invalid names",
        ),
        FalsificationTest::new(
            "F008",
            Category::Lifecycle,
            "Config validation rejects empty binary path",
            "DaemonConfig.validate() fails for empty binary_path",
        ),
        FalsificationTest::new(
            "F009",
            Category::Lifecycle,
            "Graceful shutdown completes within timeout",
            "shutdown() completes within configured timeout",
        ),
        FalsificationTest::new(
            "F010",
            Category::Lifecycle,
            "Forced kill terminates immediately",
            "SIGKILL terminates daemon within 100ms",
        )
        .with_timeout(Duration::from_secs(5)),
        FalsificationTest::new(
            "F011",
            Category::Lifecycle,
            "Restart policy Never prevents restart",
            "RestartPolicy::Never never restarts regardless of exit reason",
        ),
        FalsificationTest::new(
            "F012",
            Category::Lifecycle,
            "Restart policy Always restarts on graceful",
            "RestartPolicy::Always restarts even on graceful exit",
        ),
        FalsificationTest::new(
            "F013",
            Category::Lifecycle,
            "Restart policy OnFailure only restarts on error",
            "RestartPolicy::OnFailure only restarts on Error/ResourceExhausted",
        ),
        FalsificationTest::new(
            "F014",
            Category::Lifecycle,
            "MaxRetries limits restart count",
            "RestartPolicy::MaxRetries(N) stops after N restarts",
        ),
        FalsificationTest::new(
            "F015",
            Category::Lifecycle,
            "Backoff delay increases exponentially",
            "BackoffConfig.delay_for(n) >= delay_for(n-1) * multiplier",
        ),
        FalsificationTest::new(
            "F016",
            Category::Lifecycle,
            "Backoff respects max delay",
            "BackoffConfig.delay_for(n) never exceeds max_delay",
        ),
        FalsificationTest::new(
            "F017",
            Category::Lifecycle,
            "Manager tracks restart count",
            "DaemonManager.get_restart_count() increments correctly",
        ),
        FalsificationTest::new(
            "F018",
            Category::Lifecycle,
            "Manager prevents duplicate registration",
            "Registering daemon with same ID returns error",
        ),
        FalsificationTest::new(
            "F019",
            Category::Lifecycle,
            "Manager prevents unregistering active daemon",
            "Unregistering Running daemon returns error",
        ),
        FalsificationTest::new(
            "F020",
            Category::Lifecycle,
            "Shutdown all signals all daemons",
            "shutdown_all() sends SIGTERM to all registered daemons",
        ),
    ]
}

/// Category B: Signal Handling tests (F021-F040).
fn signal_tests() -> Vec<FalsificationTest> {
    vec![
        FalsificationTest::new(
            "F021",
            Category::SignalHandling,
            "SIGTERM triggers graceful shutdown",
            "Receiving SIGTERM sets should_shutdown() to true",
        ),
        FalsificationTest::new(
            "F022",
            Category::SignalHandling,
            "SIGINT triggers graceful shutdown",
            "Receiving SIGINT sets should_shutdown() to true",
        ),
        FalsificationTest::new(
            "F023",
            Category::SignalHandling,
            "SIGQUIT triggers graceful shutdown",
            "Receiving SIGQUIT sets should_shutdown() to true",
        ),
        FalsificationTest::new(
            "F024",
            Category::SignalHandling,
            "SIGHUP does not trigger shutdown",
            "Receiving SIGHUP does NOT set should_shutdown() to true",
        ),
        FalsificationTest::new(
            "F025",
            Category::SignalHandling,
            "SIGUSR1 delivered to daemon",
            "SIGUSR1 is delivered via recv_signal()",
        ),
        FalsificationTest::new(
            "F026",
            Category::SignalHandling,
            "SIGUSR2 delivered to daemon",
            "SIGUSR2 is delivered via recv_signal()",
        ),
        FalsificationTest::new(
            "F027",
            Category::SignalHandling,
            "SIGSTOP pauses daemon",
            "SIGSTOP changes status to Paused",
        ),
        FalsificationTest::new(
            "F028",
            Category::SignalHandling,
            "SIGCONT resumes paused daemon",
            "SIGCONT changes Paused status to Running",
        ),
        FalsificationTest::new(
            "F029",
            Category::SignalHandling,
            "Signal to stopped daemon fails",
            "Sending signal to Stopped daemon returns error",
        ),
        FalsificationTest::new(
            "F030",
            Category::SignalHandling,
            "Signal queue has bounded capacity",
            "Signal channel does not grow unboundedly",
        ),
        FalsificationTest::new(
            "F031",
            Category::SignalHandling,
            "Signal numbers match Unix conventions",
            "Signal::as_i32() returns correct Unix signal numbers",
        ),
        FalsificationTest::new(
            "F032",
            Category::SignalHandling,
            "Signal from_i32 handles invalid values",
            "Signal::from_i32(invalid) returns None",
        ),
        FalsificationTest::new(
            "F033",
            Category::SignalHandling,
            "Signal handler is async-safe",
            "Signal handling does not block async runtime",
        ),
        FalsificationTest::new(
            "F034",
            Category::SignalHandling,
            "try_recv_signal is non-blocking",
            "try_recv_signal() returns immediately with None if no signal",
        ),
        FalsificationTest::new(
            "F035",
            Category::SignalHandling,
            "recv_signal blocks until signal",
            "recv_signal() yields until signal available",
        ),
        FalsificationTest::new(
            "F036",
            Category::SignalHandling,
            "Multiple signals queued correctly",
            "Multiple signals sent are received in order",
        ),
        FalsificationTest::new(
            "F037",
            Category::SignalHandling,
            "Handle shutdown sends SIGTERM",
            "DaemonContextHandle.shutdown() sends SIGTERM",
        ),
        FalsificationTest::new(
            "F038",
            Category::SignalHandling,
            "Handle closed returns error",
            "send_signal() on closed handle returns error",
        ),
        FalsificationTest::new(
            "F039",
            Category::SignalHandling,
            "Manager signal forwards correctly",
            "DaemonManager.signal() reaches the correct daemon",
        ),
        FalsificationTest::new(
            "F040",
            Category::SignalHandling,
            "Signal to unknown daemon fails",
            "Manager.signal() with invalid ID returns NotFound error",
        ),
    ]
}

/// Category C: Resource Limits tests (F041-F060).
fn resource_tests() -> Vec<FalsificationTest> {
    vec![
        FalsificationTest::new(
            "F041",
            Category::ResourceLimits,
            "Memory limit enforced",
            "Daemon exceeding memory_bytes triggers ResourceLimit error",
        ),
        FalsificationTest::new(
            "F042",
            Category::ResourceLimits,
            "CPU quota enforced",
            "Daemon exceeding cpu_quota_percent is throttled",
        ),
        FalsificationTest::new(
            "F043",
            Category::ResourceLimits,
            "Open files limit enforced",
            "Daemon exceeding open_files_max cannot open new files",
        ),
        FalsificationTest::new(
            "F044",
            Category::ResourceLimits,
            "Process limit enforced",
            "Daemon exceeding pids_max cannot spawn children",
        ),
        FalsificationTest::new(
            "F045",
            Category::ResourceLimits,
            "Default memory limit is 512MB",
            "ResourceConfig::default().memory_bytes == 512*1024*1024",
        ),
        FalsificationTest::new(
            "F046",
            Category::ResourceLimits,
            "Default CPU quota is 100%",
            "ResourceConfig::default().cpu_quota_percent == 100.0",
        ),
        FalsificationTest::new(
            "F047",
            Category::ResourceLimits,
            "Zero memory limit rejected",
            "ResourceConfig.validate() fails with memory_bytes=0",
        ),
        FalsificationTest::new(
            "F048",
            Category::ResourceLimits,
            "Negative CPU quota rejected",
            "ResourceConfig.validate() fails with cpu_quota_percent<=0",
        ),
        FalsificationTest::new(
            "F049",
            Category::ResourceLimits,
            "Zero pids_max rejected",
            "ResourceConfig.validate() fails with pids_max=0",
        ),
        FalsificationTest::new(
            "F050",
            Category::ResourceLimits,
            "lock_memory default is false",
            "ResourceConfig::default().lock_memory == false",
        ),
        FalsificationTest::new(
            "F051",
            Category::ResourceLimits,
            "lock_memory_required default is false",
            "ResourceConfig::default().lock_memory_required == false",
        ),
        FalsificationTest::new(
            "F052",
            Category::ResourceLimits,
            "I/O limits enforced",
            "io_read_bps and io_write_bps are respected",
        ),
        FalsificationTest::new(
            "F053",
            Category::ResourceLimits,
            "CPU shares affect scheduling",
            "cpu_shares influences relative CPU time",
        ),
        FalsificationTest::new(
            "F054",
            Category::ResourceLimits,
            "Memory+swap limit includes swap",
            "memory_swap_bytes limits total memory+swap",
        ),
        FalsificationTest::new(
            "F055",
            Category::ResourceLimits,
            "Resource error is recoverable",
            "DaemonError::ResourceLimit.is_recoverable() == true",
        ),
        FalsificationTest::new(
            "F056",
            Category::ResourceLimits,
            "Metrics track memory usage",
            "DaemonMetrics.memory_bytes() reflects actual usage",
        ),
        FalsificationTest::new(
            "F057",
            Category::ResourceLimits,
            "Metrics track CPU usage",
            "DaemonMetrics.cpu_usage() reflects actual usage",
        ),
        FalsificationTest::new(
            "F058",
            Category::ResourceLimits,
            "Metrics track open FDs",
            "DaemonMetrics.open_fds() reflects actual count",
        ),
        FalsificationTest::new(
            "F059",
            Category::ResourceLimits,
            "Metrics track thread count",
            "DaemonMetrics.thread_count() reflects actual count",
        ),
        FalsificationTest::new(
            "F060",
            Category::ResourceLimits,
            "Resource snapshot is consistent",
            "MetricsSnapshot captures all resource metrics atomically",
        ),
    ]
}

/// Category D: Health Check tests (F061-F080).
fn health_tests() -> Vec<FalsificationTest> {
    vec![
        FalsificationTest::new(
            "F061",
            Category::HealthChecks,
            "Health check default interval is 30s",
            "HealthCheckConfig::default().interval == 30s",
        ),
        FalsificationTest::new(
            "F062",
            Category::HealthChecks,
            "Health check default timeout is 10s",
            "HealthCheckConfig::default().timeout == 10s",
        ),
        FalsificationTest::new(
            "F063",
            Category::HealthChecks,
            "Health check default retries is 3",
            "HealthCheckConfig::default().retries == 3",
        ),
        FalsificationTest::new(
            "F064",
            Category::HealthChecks,
            "Health check can be disabled",
            "HealthCheckConfig.enabled = false skips checks",
        ),
        FalsificationTest::new(
            "F065",
            Category::HealthChecks,
            "Healthy status returns true",
            "HealthStatus::healthy().is_healthy() == true",
        ),
        FalsificationTest::new(
            "F066",
            Category::HealthChecks,
            "Unhealthy status returns false",
            "HealthStatus::unhealthy().is_healthy() == false",
        ),
        FalsificationTest::new(
            "F067",
            Category::HealthChecks,
            "Health latency is tracked",
            "HealthStatus.latency_ms reflects actual check time",
        ),
        FalsificationTest::new(
            "F068",
            Category::HealthChecks,
            "Health check timeout triggers failure",
            "health_check() exceeding timeout returns unhealthy",
        ),
        FalsificationTest::new(
            "F069",
            Category::HealthChecks,
            "Retry count affects failure threshold",
            "Daemon marked Failed only after retries attempts",
        ),
        FalsificationTest::new(
            "F070",
            Category::HealthChecks,
            "Manager updates health status",
            "DaemonManager.update_health() stores result",
        ),
        FalsificationTest::new(
            "F071",
            Category::HealthChecks,
            "Manager retrieves health status",
            "DaemonManager.get_health() returns last result",
        ),
        FalsificationTest::new(
            "F072",
            Category::HealthChecks,
            "Health check runs in background",
            "Health checks do not block main run() loop",
        ),
        FalsificationTest::new(
            "F073",
            Category::HealthChecks,
            "Health check respects interval",
            "Checks occur at configured interval",
        ),
        FalsificationTest::new(
            "F074",
            Category::HealthChecks,
            "Individual checks tracked separately",
            "HealthStatus.checks contains individual check results",
        ),
        FalsificationTest::new(
            "F075",
            Category::HealthChecks,
            "Health timestamp is accurate",
            "last_check_epoch_ms matches actual check time",
        ),
        FalsificationTest::new(
            "F076",
            Category::HealthChecks,
            "Health error is recoverable",
            "DaemonError::HealthCheck.is_recoverable() == true",
        ),
        FalsificationTest::new(
            "F077",
            Category::HealthChecks,
            "Failed health triggers restart",
            "Consecutive failures trigger restart if configured",
        ),
        FalsificationTest::new(
            "F078",
            Category::HealthChecks,
            "Health recovery resets counter",
            "Passing health check resets failure counter",
        ),
        FalsificationTest::new(
            "F079",
            Category::HealthChecks,
            "Health check serializes correctly",
            "HealthStatus roundtrips through serde",
        ),
        FalsificationTest::new(
            "F080",
            Category::HealthChecks,
            "Circuit breaker trips on failures",
            "Repeated health failures trip circuit breaker",
        ),
    ]
}

/// Category E: Observability tests (F081-F100).
fn observability_tests() -> Vec<FalsificationTest> {
    vec![
        FalsificationTest::new(
            "F081",
            Category::Observability,
            "Request counter increments",
            "record_request() increments requests_total",
        ),
        FalsificationTest::new(
            "F082",
            Category::Observability,
            "Error counter increments",
            "record_error() increments errors_total",
        ),
        FalsificationTest::new(
            "F083",
            Category::Observability,
            "Error rate calculated correctly",
            "error_rate() == errors_total / requests_total",
        ),
        FalsificationTest::new(
            "F084",
            Category::Observability,
            "Duration average is correct",
            "duration_avg() == sum(durations) / count",
        ),
        FalsificationTest::new(
            "F085",
            Category::Observability,
            "Duration max tracks maximum",
            "duration_max() >= all recorded durations",
        ),
        FalsificationTest::new(
            "F086",
            Category::Observability,
            "Uptime increases monotonically",
            "uptime() always increases",
        ),
        FalsificationTest::new(
            "F087",
            Category::Observability,
            "Metrics are thread-safe",
            "Concurrent updates don't corrupt data",
        ),
        FalsificationTest::new(
            "F088",
            Category::Observability,
            "Metrics clone shares state",
            "Cloned DaemonMetrics see same values",
        ),
        FalsificationTest::new(
            "F089",
            Category::Observability,
            "Snapshot captures all metrics",
            "MetricsSnapshot contains all metric values",
        ),
        FalsificationTest::new(
            "F090",
            Category::Observability,
            "Circuit breaker trip recorded",
            "record_circuit_breaker_trip() increments counter",
        ),
        FalsificationTest::new(
            "F091",
            Category::Observability,
            "Recovery recorded",
            "record_recovery() increments counter",
        ),
        FalsificationTest::new(
            "F092",
            Category::Observability,
            "Requests per second calculated",
            "requests_per_second() reflects actual rate",
        ),
        FalsificationTest::new(
            "F093",
            Category::Observability,
            "Tracer attaches successfully",
            "attach_tracer() returns TracerHandle on success",
        ),
        FalsificationTest::new(
            "F094",
            Category::Observability,
            "Tracer type is correct",
            "TracerHandle.tracer_type() matches platform",
        ),
        FalsificationTest::new(
            "F095",
            Category::Observability,
            "Tracer tracks correct daemon",
            "TracerHandle.daemon_id() matches target",
        ),
        FalsificationTest::new(
            "F096",
            Category::Observability,
            "Tracer fails for unknown daemon",
            "attach_tracer() fails for non-existent daemon",
        ),
        FalsificationTest::new(
            "F097",
            Category::Observability,
            "Logging includes daemon ID",
            "Log entries include daemon ID for correlation",
        ),
        FalsificationTest::new(
            "F098",
            Category::Observability,
            "Logging includes operation name",
            "Log entries include operation (init/run/shutdown)",
        ),
        FalsificationTest::new(
            "F099",
            Category::Observability,
            "Errors logged at appropriate level",
            "Errors logged at error level, not warn",
        ),
        FalsificationTest::new(
            "F100",
            Category::Observability,
            "Metrics serialization roundtrips",
            "MetricsSnapshot roundtrips through serde",
        ),
    ]
}

/// Category F: Platform Adapter tests (F101-F110).
fn platform_tests() -> Vec<FalsificationTest> {
    vec![
        FalsificationTest::new(
            "F101",
            Category::PlatformAdapters,
            "Platform detection returns valid platform",
            "detect_platform() returns a valid Platform variant",
        ),
        FalsificationTest::new(
            "F102",
            Category::PlatformAdapters,
            "select_adapter returns correct type",
            "select_adapter(platform).platform() == platform",
        ),
        FalsificationTest::new(
            "F103",
            Category::PlatformAdapters,
            "Native adapter spawns process",
            "NativeAdapter.spawn() returns valid handle",
        ),
        FalsificationTest::new(
            "F104",
            Category::PlatformAdapters,
            "Native adapter tracks PID",
            "DaemonHandle.pid() returns actual process ID",
        ),
        FalsificationTest::new(
            "F105",
            Category::PlatformAdapters,
            "Native adapter signals process",
            "signal() delivers signal to process",
        ),
        FalsificationTest::new(
            "F106",
            Category::PlatformAdapters,
            "Native adapter reports status",
            "status() reflects actual process state",
        ),
        FalsificationTest::new(
            "F107",
            Category::PlatformAdapters,
            "Stub adapters return NotSupported",
            "Unimplemented adapters return PlatformError::NotSupported",
        ),
        FalsificationTest::new(
            "F108",
            Category::PlatformAdapters,
            "Handle serialization roundtrips",
            "DaemonHandle roundtrips through serde",
        ),
        FalsificationTest::new(
            "F109",
            Category::PlatformAdapters,
            "Handle display is informative",
            "DaemonHandle Display includes platform and ID info",
        ),
        FalsificationTest::new(
            "F110",
            Category::PlatformAdapters,
            "Platform isolation flags correct",
            "Platform.supports_isolation() correct for each variant",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_tests_count() {
        let tests = all_tests();
        assert_eq!(tests.len(), 110);
    }

    #[test]
    fn test_lifecycle_tests_count() {
        let tests = lifecycle_tests();
        assert_eq!(tests.len(), 20);
    }

    #[test]
    fn test_signal_tests_count() {
        let tests = signal_tests();
        assert_eq!(tests.len(), 20);
    }

    #[test]
    fn test_resource_tests_count() {
        let tests = resource_tests();
        assert_eq!(tests.len(), 20);
    }

    #[test]
    fn test_health_tests_count() {
        let tests = health_tests();
        assert_eq!(tests.len(), 20);
    }

    #[test]
    fn test_observability_tests_count() {
        let tests = observability_tests();
        assert_eq!(tests.len(), 20);
    }

    #[test]
    fn test_platform_tests_count() {
        let tests = platform_tests();
        assert_eq!(tests.len(), 10);
    }

    #[test]
    fn test_category_names() {
        assert_eq!(Category::Lifecycle.name(), "Lifecycle");
        assert_eq!(Category::SignalHandling.name(), "Signal Handling");
        assert_eq!(Category::ResourceLimits.name(), "Resource Limits");
        assert_eq!(Category::HealthChecks.name(), "Health Checks");
        assert_eq!(Category::Observability.name(), "Observability");
        assert_eq!(Category::PlatformAdapters.name(), "Platform Adapters");
    }

    #[test]
    fn test_category_prefixes() {
        assert_eq!(Category::Lifecycle.prefix(), 'A');
        assert_eq!(Category::SignalHandling.prefix(), 'B');
        assert_eq!(Category::ResourceLimits.prefix(), 'C');
        assert_eq!(Category::HealthChecks.prefix(), 'D');
        assert_eq!(Category::Observability.prefix(), 'E');
        assert_eq!(Category::PlatformAdapters.prefix(), 'F');
    }

    #[test]
    fn test_falsification_test_display() {
        let test = FalsificationTest::new(
            "F001",
            Category::Lifecycle,
            "Test description",
            "Test property",
        );
        let display = format!("{}", test);
        assert!(display.contains("F001"));
        assert!(display.contains("Lifecycle"));
        assert!(display.contains("Test description"));
    }

    #[test]
    fn test_test_ids_are_unique() {
        let tests = all_tests();
        let ids: std::collections::HashSet<_> = tests.iter().map(|t| t.id()).collect();
        assert_eq!(ids.len(), tests.len(), "All test IDs should be unique");
    }
}
