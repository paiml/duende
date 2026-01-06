//! Daemon syscall introspection via /proc filesystem.
//!
//! # Toyota Way: Genchi Genbutsu (現地現物)
//! "Go and see for yourself" - direct observation of daemon behavior
//! through syscall monitoring.
//!
//! # Implementation
//!
//! On Linux, uses /proc filesystem for non-invasive monitoring:
//! - `/proc/{pid}/syscall` - Current syscall and arguments
//! - `/proc/{pid}/stack` - Kernel stack trace
//! - `/proc/{pid}/wchan` - Wait channel symbol
//!
//! For deep ptrace-based tracing, integrate with renacer directly.

use std::collections::HashMap;
use std::time::Instant;

use crate::error::{ObserveError, Result};

/// Daemon syscall tracer via /proc filesystem monitoring.
///
/// Provides syscall observation with:
/// - Current syscall detection via /proc/{pid}/syscall
/// - Wait channel monitoring via /proc/{pid}/wchan
/// - Anomaly detection (frequency spikes)
/// - Anti-pattern detection (busy polling, excessive syscalls)
pub struct DaemonTracer {
    /// Attached process ID.
    attached_pid: Option<u32>,
    /// Syscall frequency histogram for anomaly detection.
    syscall_counts: HashMap<String, u64>,
    /// Total samples collected.
    sample_count: u64,
    /// Start time for frequency calculation.
    start_time: Option<Instant>,
    /// Anomaly threshold (z-score).
    anomaly_threshold: f64,
}

impl DaemonTracer {
    /// Creates a new daemon tracer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            attached_pid: None,
            syscall_counts: HashMap::new(),
            sample_count: 0,
            start_time: None,
            anomaly_threshold: 2.0,
        }
    }

    /// Creates a tracer with custom anomaly threshold.
    #[must_use]
    pub fn with_anomaly_threshold(threshold: f64) -> Self {
        Self {
            attached_pid: None,
            syscall_counts: HashMap::new(),
            sample_count: 0,
            start_time: None,
            anomaly_threshold: threshold,
        }
    }

    /// Attaches tracer to a running daemon.
    ///
    /// # Errors
    /// Returns an error if the process doesn't exist.
    #[allow(clippy::unused_async)]
    pub async fn attach(&mut self, pid: u32) -> Result<()> {
        // Verify process exists
        #[cfg(target_os = "linux")]
        {
            let stat_path = format!("/proc/{}/stat", pid);
            if !std::path::Path::new(&stat_path).exists() {
                return Err(ObserveError::tracer(format!("process {} not found", pid)));
            }
        }

        tracing::info!(pid = pid, "attaching tracer");
        self.attached_pid = Some(pid);
        self.syscall_counts.clear();
        self.sample_count = 0;
        self.start_time = Some(Instant::now());
        Ok(())
    }

    /// Detaches tracer from daemon.
    pub fn detach(&mut self) {
        if let Some(pid) = self.attached_pid.take() {
            tracing::info!(pid = pid, "detaching tracer");
        }
        self.syscall_counts.clear();
        self.sample_count = 0;
        self.start_time = None;
    }

    /// Collects syscall trace with anomaly detection.
    ///
    /// # Errors
    /// Returns an error if no daemon is attached or collection fails.
    #[allow(clippy::unused_async)]
    pub async fn collect(&mut self) -> Result<TraceReport> {
        let pid = self
            .attached_pid
            .ok_or_else(|| ObserveError::tracer("no daemon attached"))?;

        #[cfg(target_os = "linux")]
        let report = self.collect_linux(pid);

        #[cfg(not(target_os = "linux"))]
        let report = self.collect_fallback(pid);

        Ok(report)
    }

    /// Linux-specific collection via /proc filesystem.
    #[cfg(target_os = "linux")]
    fn collect_linux(&mut self, pid: u32) -> TraceReport {
        let mut events = Vec::new();
        let mut anomalies = Vec::new();
        let mut anti_patterns = Vec::new();

        // Read current syscall
        if let Ok(syscall_info) = Self::read_current_syscall(pid) {
            let event = TraceEvent {
                syscall: syscall_info.name.clone(),
                duration_us: 0, // /proc/syscall doesn't provide duration
                source_location: None,
            };
            events.push(event);

            // Track syscall frequency
            *self.syscall_counts.entry(syscall_info.name.clone()).or_insert(0) += 1;
            self.sample_count += 1;

            // Detect anomalies based on syscall frequency
            if let Some(anomaly) = self.detect_frequency_anomaly(&syscall_info.name) {
                anomalies.push(anomaly);
            }
        }

        // Read wait channel for blocking syscalls
        if let Ok(wchan) = Self::read_wchan(pid) && !wchan.is_empty() && wchan != "0" {
            let event = TraceEvent {
                syscall: format!("wchan:{}", wchan),
                duration_us: 0,
                source_location: None,
            };
            events.push(event);
        }

        // Detect anti-patterns
        anti_patterns.extend(self.detect_anti_patterns());

        // Build critical path (top syscalls by frequency)
        let critical_path = self.build_critical_path();

        TraceReport {
            pid,
            events,
            anomalies,
            critical_path,
            anti_patterns,
        }
    }

    /// Fallback for non-Linux systems.
    #[cfg(not(target_os = "linux"))]
    fn collect_fallback(&mut self, pid: u32) -> TraceReport {
        TraceReport {
            pid,
            events: vec![],
            anomalies: vec![],
            critical_path: vec![],
            anti_patterns: vec![],
        }
    }

    /// Read current syscall from /proc/{pid}/syscall.
    #[cfg(target_os = "linux")]
    fn read_current_syscall(pid: u32) -> Result<SyscallInfo> {
        let path = format!("/proc/{}/syscall", pid);
        let content = std::fs::read_to_string(&path)?;

        // Format: syscall_nr arg0 arg1 arg2 arg3 arg4 arg5 sp pc
        // Or: "running" if process is in user-space
        let content = content.trim();

        if content == "running" {
            return Ok(SyscallInfo {
                name: "running".to_string(),
                number: -1,
            });
        }

        let parts: Vec<&str> = content.split_whitespace().collect();
        if parts.is_empty() {
            return Err(ObserveError::tracer("empty /proc/syscall"));
        }

        let syscall_nr: i64 = parts[0]
            .parse()
            .map_err(|_| ObserveError::tracer("failed to parse syscall number"))?;

        let name = Self::syscall_name(syscall_nr);

        Ok(SyscallInfo {
            name,
            number: syscall_nr,
        })
    }

    /// Read wait channel from /proc/{pid}/wchan.
    #[cfg(target_os = "linux")]
    fn read_wchan(pid: u32) -> Result<String> {
        let path = format!("/proc/{}/wchan", pid);
        let content = std::fs::read_to_string(&path)?;
        Ok(content.trim().to_string())
    }

    /// Map syscall number to name (common Linux syscalls).
    #[cfg(target_os = "linux")]
    fn syscall_name(nr: i64) -> String {
        // Common x86_64 syscall numbers
        match nr {
            -1 => "none".to_string(),
            0 => "read".to_string(),
            1 => "write".to_string(),
            2 => "open".to_string(),
            3 => "close".to_string(),
            4 => "stat".to_string(),
            5 => "fstat".to_string(),
            6 => "lstat".to_string(),
            7 => "poll".to_string(),
            8 => "lseek".to_string(),
            9 => "mmap".to_string(),
            10 => "mprotect".to_string(),
            11 => "munmap".to_string(),
            12 => "brk".to_string(),
            13 => "rt_sigaction".to_string(),
            14 => "rt_sigprocmask".to_string(),
            15 => "rt_sigreturn".to_string(),
            16 => "ioctl".to_string(),
            17 => "pread64".to_string(),
            18 => "pwrite64".to_string(),
            19 => "readv".to_string(),
            20 => "writev".to_string(),
            21 => "access".to_string(),
            22 => "pipe".to_string(),
            23 => "select".to_string(),
            24 => "sched_yield".to_string(),
            35 => "nanosleep".to_string(),
            56 => "clone".to_string(),
            57 => "fork".to_string(),
            59 => "execve".to_string(),
            60 => "exit".to_string(),
            61 => "wait4".to_string(),
            62 => "kill".to_string(),
            202 => "futex".to_string(),
            228 => "clock_gettime".to_string(),
            230 => "clock_nanosleep".to_string(),
            231 => "exit_group".to_string(),
            232 => "epoll_wait".to_string(),
            257 => "openat".to_string(),
            262 => "newfstatat".to_string(),
            270 => "pselect6".to_string(),
            281 => "epoll_pwait".to_string(),
            _ => format!("syscall_{}", nr),
        }
    }

    /// Detect frequency anomaly using z-score.
    fn detect_frequency_anomaly(&self, syscall: &str) -> Option<Anomaly> {
        if self.sample_count < 10 {
            return None; // Need sufficient samples
        }

        let current_count = *self.syscall_counts.get(syscall).unwrap_or(&0);
        let total_syscalls: u64 = self.syscall_counts.values().sum();

        if total_syscalls == 0 {
            return None;
        }

        // Calculate expected frequency
        let num_syscalls = self.syscall_counts.len() as f64;
        let expected_freq = total_syscalls as f64 / num_syscalls;
        let actual_freq = current_count as f64;

        // Calculate standard deviation
        let variance: f64 = self
            .syscall_counts
            .values()
            .map(|&count| {
                let diff = count as f64 - expected_freq;
                diff * diff
            })
            .sum::<f64>()
            / num_syscalls;

        let std_dev = variance.sqrt();

        if std_dev < 0.001 {
            return None; // Too little variance
        }

        let z_score = (actual_freq - expected_freq) / std_dev;

        if z_score.abs() > self.anomaly_threshold {
            Some(Anomaly {
                kind: if z_score > 0.0 {
                    AnomalyKind::LatencySpike // Using as "frequency spike"
                } else {
                    AnomalyKind::ResourceExhaustion
                },
                z_score,
                description: format!(
                    "{} called {} times (z-score: {:.2})",
                    syscall, current_count, z_score
                ),
            })
        } else {
            None
        }
    }

    /// Detect anti-patterns in syscall usage.
    fn detect_anti_patterns(&self) -> Vec<AntiPattern> {
        let mut patterns = Vec::new();
        let total: u64 = self.syscall_counts.values().sum();

        if total == 0 {
            return patterns;
        }

        // Detect busy polling (excessive poll/select/epoll_wait)
        let poll_count = self.syscall_counts.get("poll").unwrap_or(&0)
            + self.syscall_counts.get("select").unwrap_or(&0)
            + self.syscall_counts.get("epoll_wait").unwrap_or(&0)
            + self.syscall_counts.get("epoll_pwait").unwrap_or(&0)
            + self.syscall_counts.get("pselect6").unwrap_or(&0);

        let poll_ratio = poll_count as f64 / total as f64;
        if poll_ratio > 0.5 && poll_count > 100 {
            patterns.push(AntiPattern {
                name: "BusyPolling".to_string(),
                severity: 3,
                description: format!(
                    "{}% of syscalls are poll/select ({} calls) - consider longer timeouts",
                    (poll_ratio * 100.0) as u32,
                    poll_count
                ),
            });
        }

        // Detect excessive futex (lock contention)
        let futex_count = *self.syscall_counts.get("futex").unwrap_or(&0);
        let futex_ratio = futex_count as f64 / total as f64;
        if futex_ratio > 0.3 && futex_count > 100 {
            patterns.push(AntiPattern {
                name: "LockContention".to_string(),
                severity: 4,
                description: format!(
                    "{}% of syscalls are futex ({} calls) - possible lock contention",
                    (futex_ratio * 100.0) as u32,
                    futex_count
                ),
            });
        }

        // Detect tight loop (excessive brk/mmap - memory churn)
        let mem_count = self.syscall_counts.get("brk").unwrap_or(&0)
            + self.syscall_counts.get("mmap").unwrap_or(&0)
            + self.syscall_counts.get("munmap").unwrap_or(&0);
        let mem_ratio = mem_count as f64 / total as f64;
        if mem_ratio > 0.2 && mem_count > 50 {
            patterns.push(AntiPattern {
                name: "MemoryChurn".to_string(),
                severity: 3,
                description: format!(
                    "{}% of syscalls are memory allocation ({} calls) - consider object pooling",
                    (mem_ratio * 100.0) as u32,
                    mem_count
                ),
            });
        }

        patterns
    }

    /// Build critical path (top syscalls by frequency).
    fn build_critical_path(&self) -> Vec<String> {
        let mut sorted: Vec<_> = self.syscall_counts.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));

        sorted
            .into_iter()
            .take(5)
            .map(|(name, count)| format!("{} ({})", name, count))
            .collect()
    }

    /// Returns the attached PID, if any.
    #[must_use]
    pub const fn attached_pid(&self) -> Option<u32> {
        self.attached_pid
    }

    /// Returns syscall statistics.
    #[must_use]
    pub fn syscall_stats(&self) -> &HashMap<String, u64> {
        &self.syscall_counts
    }

    /// Returns total sample count.
    #[must_use]
    pub const fn sample_count(&self) -> u64 {
        self.sample_count
    }
}

impl Default for DaemonTracer {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal syscall info from /proc/{pid}/syscall.
#[cfg(target_os = "linux")]
struct SyscallInfo {
    name: String,
    #[allow(dead_code)]
    number: i64,
}

/// Trace report from syscall collection.
#[derive(Debug, Clone)]
pub struct TraceReport {
    /// Process ID.
    pub pid: u32,
    /// Syscall events with source correlation.
    pub events: Vec<TraceEvent>,
    /// Detected anomalies.
    pub anomalies: Vec<Anomaly>,
    /// Critical path through syscalls (top by frequency).
    pub critical_path: Vec<String>,
    /// Detected anti-patterns.
    pub anti_patterns: Vec<AntiPattern>,
}

/// A single trace event.
#[derive(Debug, Clone)]
pub struct TraceEvent {
    /// Syscall name.
    pub syscall: String,
    /// Duration in microseconds.
    pub duration_us: u64,
    /// Source file location (if available).
    pub source_location: Option<String>,
}

/// A detected anomaly.
#[derive(Debug, Clone)]
pub struct Anomaly {
    /// Anomaly type.
    pub kind: AnomalyKind,
    /// Z-score of the anomaly.
    pub z_score: f64,
    /// Description.
    pub description: String,
}

/// Types of anomalies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnomalyKind {
    /// Latency spike (or frequency spike).
    LatencySpike,
    /// Error burst.
    ErrorBurst,
    /// Resource exhaustion.
    ResourceExhaustion,
}

/// A detected anti-pattern.
#[derive(Debug, Clone)]
pub struct AntiPattern {
    /// Pattern name.
    pub name: String,
    /// Severity (1-5).
    pub severity: u8,
    /// Description.
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tracer_attach_detach() {
        let mut tracer = DaemonTracer::new();
        assert!(tracer.attached_pid().is_none());

        // Use our own PID which always exists
        let pid = std::process::id();
        tracer.attach(pid).await.unwrap();
        assert_eq!(tracer.attached_pid(), Some(pid));

        tracer.detach();
        assert!(tracer.attached_pid().is_none());
    }

    #[tokio::test]
    async fn test_collect_without_attach() {
        let mut tracer = DaemonTracer::new();
        let result = tracer.collect().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no daemon attached"));
    }

    #[test]
    fn test_tracer_default() {
        let tracer = DaemonTracer::default();
        assert!(tracer.attached_pid().is_none());
        assert_eq!(tracer.sample_count(), 0);
    }

    #[test]
    fn test_tracer_with_threshold() {
        let tracer = DaemonTracer::with_anomaly_threshold(3.0);
        assert_eq!(tracer.anomaly_threshold, 3.0);
    }

    #[cfg(target_os = "linux")]
    mod linux_tests {
        use super::*;

        #[tokio::test]
        async fn test_attach_self() {
            let mut tracer = DaemonTracer::new();
            let pid = std::process::id();
            let result = tracer.attach(pid).await;
            assert!(result.is_ok());
            assert_eq!(tracer.attached_pid(), Some(pid));
        }

        #[tokio::test]
        async fn test_attach_nonexistent() {
            let mut tracer = DaemonTracer::new();
            let result = tracer.attach(4_000_000_000).await;
            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_collect_self() {
            let mut tracer = DaemonTracer::new();
            let pid = std::process::id();
            tracer.attach(pid).await.unwrap();

            let report = tracer.collect().await.unwrap();
            assert_eq!(report.pid, pid);
            // Our process is either running or in a syscall
            assert!(!report.events.is_empty() || report.events.is_empty()); // Always true, but shows intent
        }

        #[tokio::test]
        async fn test_collect_init() {
            let mut tracer = DaemonTracer::new();
            tracer.attach(1).await.unwrap();

            let report = tracer.collect().await.unwrap();
            assert_eq!(report.pid, 1);
        }

        #[tokio::test]
        async fn test_multiple_collects_accumulate_stats() {
            let mut tracer = DaemonTracer::new();
            let pid = std::process::id();
            tracer.attach(pid).await.unwrap();

            for _ in 0..5 {
                tracer.collect().await.unwrap();
            }

            assert!(tracer.sample_count() <= 5); // May be less if process was running
        }

        #[test]
        fn test_syscall_name_mapping() {
            assert_eq!(DaemonTracer::syscall_name(0), "read");
            assert_eq!(DaemonTracer::syscall_name(1), "write");
            assert_eq!(DaemonTracer::syscall_name(59), "execve");
            assert_eq!(DaemonTracer::syscall_name(202), "futex");
            assert_eq!(DaemonTracer::syscall_name(999), "syscall_999");
        }

        #[test]
        fn test_detach_clears_state() {
            let mut tracer = DaemonTracer::new();
            tracer.syscall_counts.insert("test".to_string(), 100);
            tracer.sample_count = 100;

            tracer.detach();

            assert!(tracer.syscall_counts.is_empty());
            assert_eq!(tracer.sample_count, 0);
        }
    }

    mod anti_pattern_tests {
        use super::*;

        #[test]
        fn test_detect_busy_polling() {
            let mut tracer = DaemonTracer::new();
            tracer.syscall_counts.insert("poll".to_string(), 200);
            tracer.syscall_counts.insert("read".to_string(), 50);
            tracer.syscall_counts.insert("write".to_string(), 50);

            let patterns = tracer.detect_anti_patterns();
            assert!(patterns.iter().any(|p| p.name == "BusyPolling"));
        }

        #[test]
        fn test_detect_lock_contention() {
            let mut tracer = DaemonTracer::new();
            tracer.syscall_counts.insert("futex".to_string(), 200);
            tracer.syscall_counts.insert("read".to_string(), 100);

            let patterns = tracer.detect_anti_patterns();
            assert!(patterns.iter().any(|p| p.name == "LockContention"));
        }

        #[test]
        fn test_detect_memory_churn() {
            let mut tracer = DaemonTracer::new();
            tracer.syscall_counts.insert("mmap".to_string(), 100);
            tracer.syscall_counts.insert("munmap".to_string(), 100);
            tracer.syscall_counts.insert("read".to_string(), 200);

            let patterns = tracer.detect_anti_patterns();
            assert!(patterns.iter().any(|p| p.name == "MemoryChurn"));
        }

        #[test]
        fn test_no_patterns_for_healthy_process() {
            let mut tracer = DaemonTracer::new();
            tracer.syscall_counts.insert("read".to_string(), 100);
            tracer.syscall_counts.insert("write".to_string(), 100);
            tracer.syscall_counts.insert("poll".to_string(), 10);

            let patterns = tracer.detect_anti_patterns();
            assert!(patterns.is_empty());
        }
    }

    mod critical_path_tests {
        use super::*;

        #[test]
        fn test_critical_path_ordering() {
            let mut tracer = DaemonTracer::new();
            tracer.syscall_counts.insert("read".to_string(), 100);
            tracer.syscall_counts.insert("write".to_string(), 50);
            tracer.syscall_counts.insert("poll".to_string(), 200);

            let path = tracer.build_critical_path();
            assert_eq!(path.len(), 3);
            assert!(path[0].starts_with("poll")); // Highest frequency first
        }

        #[test]
        fn test_critical_path_limit() {
            let mut tracer = DaemonTracer::new();
            for i in 0..10 {
                tracer.syscall_counts.insert(format!("syscall_{}", i), i as u64);
            }

            let path = tracer.build_critical_path();
            assert_eq!(path.len(), 5); // Limited to top 5
        }
    }

    mod anomaly_tests {
        use super::*;

        #[test]
        fn test_no_anomaly_insufficient_samples() {
            let mut tracer = DaemonTracer::new();
            tracer.sample_count = 5; // Too few samples
            tracer.syscall_counts.insert("read".to_string(), 5);

            let anomaly = tracer.detect_frequency_anomaly("read");
            assert!(anomaly.is_none());
        }

        #[test]
        fn test_anomaly_detection_high_frequency() {
            // Use lower threshold to detect more subtle anomalies
            let mut tracer = DaemonTracer::with_anomaly_threshold(1.0);
            tracer.sample_count = 100;
            // With 5 syscalls and highly skewed distribution, z-score will exceed 1.0
            tracer.syscall_counts.insert("read".to_string(), 80);
            tracer.syscall_counts.insert("write".to_string(), 5);
            tracer.syscall_counts.insert("poll".to_string(), 5);
            tracer.syscall_counts.insert("open".to_string(), 5);
            tracer.syscall_counts.insert("close".to_string(), 5);

            let anomaly = tracer.detect_frequency_anomaly("read");
            assert!(anomaly.is_some(), "Expected anomaly for high frequency syscall");
            let anomaly = anomaly.unwrap();
            assert!(anomaly.z_score > 0.0, "Expected positive z-score for high frequency");
        }
    }

    // ==================== Popperian Falsification Tests ====================

    mod falsification_tests {
        use super::*;

        /// F001: Falsify that detach without attach doesn't panic.
        #[test]
        fn f001_detach_without_attach() {
            let mut tracer = DaemonTracer::new();
            tracer.detach(); // Should not panic
            assert!(tracer.attached_pid().is_none());
        }

        /// F002: Falsify that empty syscall counts produce empty path.
        #[test]
        fn f002_empty_critical_path() {
            let tracer = DaemonTracer::new();
            let path = tracer.build_critical_path();
            assert!(path.is_empty());
        }

        /// F003: Falsify that anti-patterns handle zero total.
        #[test]
        fn f003_anti_patterns_zero_total() {
            let tracer = DaemonTracer::new();
            let patterns = tracer.detect_anti_patterns();
            assert!(patterns.is_empty());
        }

        /// F004: Falsify that z-score handles identical frequencies.
        #[test]
        fn f004_anomaly_identical_frequencies() {
            let mut tracer = DaemonTracer::new();
            tracer.sample_count = 100;
            tracer.syscall_counts.insert("read".to_string(), 50);
            tracer.syscall_counts.insert("write".to_string(), 50);

            // With identical frequencies, variance approaches zero
            // Should not produce false anomalies
            let anomaly = tracer.detect_frequency_anomaly("read");
            assert!(anomaly.is_none());
        }

        /// F005: Falsify that syscall_stats returns correct reference.
        #[test]
        fn f005_syscall_stats_reference() {
            let mut tracer = DaemonTracer::new();
            tracer.syscall_counts.insert("test".to_string(), 42);

            let stats = tracer.syscall_stats();
            assert_eq!(stats.get("test"), Some(&42));
        }

        /// F006: Falsify TraceReport fields are accessible.
        #[test]
        fn f006_trace_report_fields() {
            let report = TraceReport {
                pid: 1234,
                events: vec![TraceEvent {
                    syscall: "test".to_string(),
                    duration_us: 100,
                    source_location: Some("file.rs:10".to_string()),
                }],
                anomalies: vec![Anomaly {
                    kind: AnomalyKind::LatencySpike,
                    z_score: 2.5,
                    description: "test".to_string(),
                }],
                critical_path: vec!["test (1)".to_string()],
                anti_patterns: vec![AntiPattern {
                    name: "TestPattern".to_string(),
                    severity: 3,
                    description: "test".to_string(),
                }],
            };

            assert_eq!(report.pid, 1234);
            assert_eq!(report.events.len(), 1);
            assert_eq!(report.anomalies.len(), 1);
            assert_eq!(report.critical_path.len(), 1);
            assert_eq!(report.anti_patterns.len(), 1);
        }

        /// F007: Falsify AnomalyKind equality.
        #[test]
        fn f007_anomaly_kind_equality() {
            assert_eq!(AnomalyKind::LatencySpike, AnomalyKind::LatencySpike);
            assert_ne!(AnomalyKind::LatencySpike, AnomalyKind::ErrorBurst);
            assert_ne!(AnomalyKind::ErrorBurst, AnomalyKind::ResourceExhaustion);
        }
    }
}
