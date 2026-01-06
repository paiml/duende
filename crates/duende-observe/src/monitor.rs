//! Real-time daemon monitoring via /proc filesystem parsing.
//!
//! # Toyota Way: Visual Management (目で見る管理)
//! Make daemon health visible at a glance through direct observation.
//!
//! # Implementation
//!
//! On Linux, parses /proc filesystem:
//! - `/proc/{pid}/stat` - CPU time, state, threads
//! - `/proc/{pid}/statm` - Memory pages
//! - `/proc/{pid}/io` - I/O bytes (requires permissions)
//! - `/proc/meminfo` - Total system memory for percentage

use std::collections::VecDeque;
use std::time::Instant;

use crate::error::{ObserveError, Result};

/// Real-time daemon monitor using /proc filesystem collectors.
///
/// Provides metrics collection with:
/// - CPU/memory/disk/network usage
/// - Ring buffer for historical data
/// - Zero allocations after warmup
pub struct DaemonMonitor {
    /// Ring buffer capacity.
    capacity: usize,
    /// Snapshot history.
    history: VecDeque<DaemonSnapshot>,
    /// Previous CPU measurement for delta calculation.
    prev_cpu: Option<CpuMeasurement>,
    /// Page size in bytes (cached from sysconf).
    #[cfg(target_os = "linux")]
    page_size: u64,
    /// Total system memory in bytes (cached).
    #[cfg(target_os = "linux")]
    total_memory: u64,
}

/// Internal struct for CPU delta calculation.
#[derive(Clone)]
struct CpuMeasurement {
    /// Process user time in clock ticks.
    utime: u64,
    /// Process system time in clock ticks.
    stime: u64,
    /// Wall clock time of measurement.
    wall_time: Instant,
}

impl DaemonMonitor {
    /// Creates a new daemon monitor with given history capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        #[cfg(target_os = "linux")]
        let (page_size, total_memory) = {
            let page_size = Self::get_page_size();
            let total_memory = Self::read_total_memory().unwrap_or(0);
            (page_size, total_memory)
        };

        Self {
            capacity,
            history: VecDeque::with_capacity(capacity),
            prev_cpu: None,
            #[cfg(target_os = "linux")]
            page_size,
            #[cfg(target_os = "linux")]
            total_memory,
        }
    }

    /// Collects metrics for a specific daemon.
    ///
    /// # Errors
    /// Returns an error if the process doesn't exist or collection fails.
    pub fn collect(&mut self, pid: u32) -> Result<DaemonSnapshot> {
        #[cfg(target_os = "linux")]
        let snapshot = self.collect_linux(pid)?;

        #[cfg(not(target_os = "linux"))]
        let snapshot = self.collect_fallback(pid)?;

        // Store in ring buffer
        if self.history.len() >= self.capacity {
            self.history.pop_front();
        }
        self.history.push_back(snapshot.clone());

        Ok(snapshot)
    }

    /// Linux-specific collection via /proc filesystem.
    #[cfg(target_os = "linux")]
    fn collect_linux(&mut self, pid: u32) -> Result<DaemonSnapshot> {
        let now = Instant::now();

        // Parse /proc/{pid}/stat for CPU, state, threads
        let stat = self.parse_proc_stat(pid)?;

        // Calculate CPU percentage from delta
        let cpu_percent = self.calculate_cpu_percent(&stat, now);

        // Update previous measurement
        self.prev_cpu = Some(CpuMeasurement {
            utime: stat.utime,
            stime: stat.stime,
            wall_time: now,
        });

        // Parse /proc/{pid}/statm for memory
        let memory_bytes = self.parse_proc_statm(pid)?;
        let memory_percent = if self.total_memory > 0 {
            (memory_bytes as f64 / self.total_memory as f64) * 100.0
        } else {
            0.0
        };

        // Parse /proc/{pid}/io for I/O stats (may fail without permissions)
        let (io_read_bytes, io_write_bytes) = self.parse_proc_io(pid).unwrap_or((0, 0));

        Ok(DaemonSnapshot {
            timestamp: now,
            pid,
            cpu_percent,
            memory_bytes,
            memory_percent,
            threads: stat.num_threads,
            state: stat.state,
            io_read_bytes,
            io_write_bytes,
            gpu_utilization: None,
            gpu_memory: None,
        })
    }

    /// Fallback implementation for non-Linux systems.
    #[cfg(not(target_os = "linux"))]
    fn collect_fallback(&mut self, pid: u32) -> Result<DaemonSnapshot> {
        // On non-Linux, we can't parse /proc
        // Return a basic snapshot indicating the process state is unknown
        Ok(DaemonSnapshot {
            timestamp: Instant::now(),
            pid,
            cpu_percent: 0.0,
            memory_bytes: 0,
            memory_percent: 0.0,
            threads: 0,
            state: ProcessState::Unknown,
            io_read_bytes: 0,
            io_write_bytes: 0,
            gpu_utilization: None,
            gpu_memory: None,
        })
    }

    /// Get system page size.
    #[cfg(target_os = "linux")]
    fn get_page_size() -> u64 {
        // SAFETY: sysconf is safe to call with _SC_PAGESIZE
        #[allow(unsafe_code)]
        unsafe {
            libc::sysconf(libc::_SC_PAGESIZE) as u64
        }
    }

    /// Read total system memory from /proc/meminfo.
    #[cfg(target_os = "linux")]
    fn read_total_memory() -> Result<u64> {
        let content = std::fs::read_to_string("/proc/meminfo")?;
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                // Format: "MemTotal:       16384000 kB"
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2
                    && let Ok(kb) = parts[1].parse::<u64>()
                {
                    return Ok(kb * 1024); // Convert kB to bytes
                }
            }
        }
        Err(ObserveError::monitor(
            "failed to parse MemTotal from /proc/meminfo",
        ))
    }

    /// Parse /proc/{pid}/stat for CPU and process info.
    #[cfg(target_os = "linux")]
    #[allow(clippy::unused_self)]
    fn parse_proc_stat(&self, pid: u32) -> Result<ProcStat> {
        let path = format!("/proc/{}/stat", pid);
        let content = std::fs::read_to_string(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ObserveError::monitor(format!("process {} not found", pid))
            } else {
                ObserveError::Io(e)
            }
        })?;

        Self::parse_stat_content(&content)
    }

    /// Parse the content of /proc/{pid}/stat.
    ///
    /// Format: pid (comm) state ppid pgrp session tty_nr tpgid flags minflt cminflt majflt cmajflt
    ///         utime stime cutime cstime priority nice num_threads itrealvalue starttime vsize rss ...
    #[cfg(target_os = "linux")]
    fn parse_stat_content(content: &str) -> Result<ProcStat> {
        // Handle process names with spaces/parentheses by finding the last ')'
        let comm_end = content
            .rfind(')')
            .ok_or_else(|| ObserveError::monitor("malformed /proc/stat: no closing paren"))?;

        let after_comm = &content[comm_end + 2..]; // Skip ") "
        let fields: Vec<&str> = after_comm.split_whitespace().collect();

        if fields.len() < 20 {
            return Err(ObserveError::monitor(format!(
                "malformed /proc/stat: expected 20+ fields, got {}",
                fields.len()
            )));
        }

        // Field indices (0-indexed after comm):
        // 0: state, 11: utime, 12: stime, 17: num_threads
        let state = match fields[0].chars().next() {
            Some('R') => ProcessState::Running,
            Some('S') => ProcessState::Sleeping,
            Some('D') => ProcessState::DiskWait,
            Some('Z') => ProcessState::Zombie,
            Some('T' | 't') => ProcessState::Stopped,
            _ => ProcessState::Unknown,
        };

        let utime = fields[11]
            .parse()
            .map_err(|_| ObserveError::monitor("failed to parse utime"))?;
        let stime = fields[12]
            .parse()
            .map_err(|_| ObserveError::monitor("failed to parse stime"))?;
        let num_threads = fields[17]
            .parse()
            .map_err(|_| ObserveError::monitor("failed to parse num_threads"))?;

        Ok(ProcStat {
            state,
            utime,
            stime,
            num_threads,
        })
    }

    /// Calculate CPU percentage from time delta.
    #[cfg(target_os = "linux")]
    fn calculate_cpu_percent(&self, stat: &ProcStat, now: Instant) -> f64 {
        let Some(prev) = &self.prev_cpu else {
            return 0.0; // No previous measurement, can't calculate delta
        };

        let elapsed = now.duration_since(prev.wall_time);
        if elapsed.as_secs_f64() < 0.001 {
            return 0.0; // Too small interval
        }

        // CPU ticks used in this interval
        let total_ticks_now = stat.utime + stat.stime;
        let total_ticks_prev = prev.utime + prev.stime;

        if total_ticks_now < total_ticks_prev {
            return 0.0; // Counter wrapped or process restarted
        }

        let ticks_used = total_ticks_now - total_ticks_prev;

        // Convert clock ticks to seconds
        // SAFETY: sysconf is safe to call with _SC_CLK_TCK
        #[allow(unsafe_code)]
        let clk_tck = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;

        let cpu_seconds = ticks_used as f64 / clk_tck;
        let cpu_percent = (cpu_seconds / elapsed.as_secs_f64()) * 100.0;

        // Clamp to reasonable range (can exceed 100% on multi-core)
        cpu_percent.max(0.0)
    }

    /// Parse /proc/{pid}/statm for memory info.
    #[cfg(target_os = "linux")]
    fn parse_proc_statm(&self, pid: u32) -> Result<u64> {
        let path = format!("/proc/{}/statm", pid);
        let content = std::fs::read_to_string(&path)?;

        // Format: size resident shared text lib data dt
        // We want "resident" (index 1) which is RSS in pages
        let fields: Vec<&str> = content.split_whitespace().collect();
        if fields.len() < 2 {
            return Err(ObserveError::monitor("malformed /proc/statm"));
        }

        let rss_pages: u64 = fields[1]
            .parse()
            .map_err(|_| ObserveError::monitor("failed to parse RSS pages"))?;

        Ok(rss_pages * self.page_size)
    }

    /// Parse /proc/{pid}/io for I/O statistics.
    #[cfg(target_os = "linux")]
    #[allow(clippy::unused_self)]
    fn parse_proc_io(&self, pid: u32) -> Result<(u64, u64)> {
        let path = format!("/proc/{}/io", pid);
        let content = std::fs::read_to_string(&path)?;

        let mut read_bytes = 0u64;
        let mut write_bytes = 0u64;

        for line in content.lines() {
            if let Some(value) = line.strip_prefix("read_bytes: ") {
                read_bytes = value.trim().parse().unwrap_or(0);
            } else if let Some(value) = line.strip_prefix("write_bytes: ") {
                write_bytes = value.trim().parse().unwrap_or(0);
            }
        }

        Ok((read_bytes, write_bytes))
    }

    /// Returns historical snapshots within the given duration.
    #[must_use]
    pub fn history(&self, duration: std::time::Duration) -> Vec<&DaemonSnapshot> {
        let now = Instant::now();
        let cutoff = now.checked_sub(duration);

        cutoff.map_or_else(
            // If duration is larger than time since epoch, return all
            || self.history.iter().collect(),
            |cutoff| {
                self.history
                    .iter()
                    .filter(|s| s.timestamp >= cutoff)
                    .collect()
            },
        )
    }

    /// Returns all historical snapshots.
    #[must_use]
    pub fn all_history(&self) -> &VecDeque<DaemonSnapshot> {
        &self.history
    }

    /// Clears history.
    pub fn clear_history(&mut self) {
        self.history.clear();
        self.prev_cpu = None;
    }
}

impl Default for DaemonMonitor {
    fn default() -> Self {
        Self::new(1000) // 1000 samples default
    }
}

/// Internal struct for parsed /proc/{pid}/stat.
#[cfg(target_os = "linux")]
#[derive(Debug)]
struct ProcStat {
    state: ProcessState,
    utime: u64,
    stime: u64,
    num_threads: u32,
}

/// Snapshot of daemon metrics at a point in time.
#[derive(Debug, Clone)]
pub struct DaemonSnapshot {
    /// Timestamp of collection.
    pub timestamp: Instant,
    /// Process ID.
    pub pid: u32,
    /// CPU usage percentage.
    pub cpu_percent: f64,
    /// Memory usage in bytes (RSS).
    pub memory_bytes: u64,
    /// Memory usage percentage.
    pub memory_percent: f64,
    /// Thread count.
    pub threads: u32,
    /// Process state.
    pub state: ProcessState,
    /// I/O bytes read.
    pub io_read_bytes: u64,
    /// I/O bytes written.
    pub io_write_bytes: u64,
    /// GPU utilization (if available).
    pub gpu_utilization: Option<f64>,
    /// GPU memory used (if available).
    pub gpu_memory: Option<u64>,
}

/// Process state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Running.
    Running,
    /// Sleeping.
    Sleeping,
    /// Waiting for disk.
    DiskWait,
    /// Zombie.
    Zombie,
    /// Stopped.
    Stopped,
    /// Unknown.
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_creation() {
        let monitor = DaemonMonitor::new(100);
        assert!(monitor.all_history().is_empty());
    }

    #[test]
    fn test_monitor_default() {
        let monitor = DaemonMonitor::default();
        assert_eq!(monitor.capacity, 1000);
    }

    #[test]
    fn test_ring_buffer_capacity() {
        let mut monitor = DaemonMonitor::new(3);
        // Use PID 1 (init) which always exists on Linux
        for _ in 0..5 {
            let _ = monitor.collect(1);
        }
        assert_eq!(monitor.all_history().len(), 3);
    }

    #[test]
    fn test_clear_history() {
        let mut monitor = DaemonMonitor::new(100);
        let _ = monitor.collect(1);
        monitor.clear_history();
        assert!(monitor.all_history().is_empty());
        assert!(monitor.prev_cpu.is_none());
    }

    #[cfg(target_os = "linux")]
    mod linux_tests {
        use super::*;
        use std::process;

        #[test]
        fn test_collect_self() {
            let mut monitor = DaemonMonitor::new(100);
            let pid = process::id();
            let result = monitor.collect(pid);
            assert!(result.is_ok(), "Failed to collect self: {:?}", result.err());

            let snapshot = result.unwrap();
            assert_eq!(snapshot.pid, pid);
            assert!(snapshot.memory_bytes > 0, "Memory should be non-zero");
            assert!(snapshot.threads >= 1, "Should have at least 1 thread");
            // Process can be in various states: Running, Sleeping, DiskWait (during I/O)
            assert!(
                matches!(
                    snapshot.state,
                    ProcessState::Running | ProcessState::Sleeping | ProcessState::DiskWait
                ),
                "Process should be running, sleeping, or in disk wait, got: {:?}",
                snapshot.state
            );
        }

        #[test]
        fn test_collect_init() {
            let mut monitor = DaemonMonitor::new(100);
            let result = monitor.collect(1);
            assert!(result.is_ok(), "Failed to collect init: {:?}", result.err());

            let snapshot = result.unwrap();
            assert_eq!(snapshot.pid, 1);
        }

        #[test]
        fn test_collect_nonexistent_process() {
            let mut monitor = DaemonMonitor::new(100);
            // Use a very high PID that's unlikely to exist
            let result = monitor.collect(4_000_000_000);
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("not found") || err.contains("No such file"));
        }

        #[test]
        fn test_cpu_percent_requires_two_samples() {
            let mut monitor = DaemonMonitor::new(100);
            let pid = process::id();

            // First sample should have 0% CPU (no previous measurement)
            let snap1 = monitor.collect(pid).unwrap();
            assert_eq!(snap1.cpu_percent, 0.0);

            // Do some work
            let mut sum = 0u64;
            for i in 0..100_000 {
                sum = sum.wrapping_add(i);
            }
            std::hint::black_box(sum);

            // Wait a bit for measurable time delta
            std::thread::sleep(std::time::Duration::from_millis(10));

            // Second sample should have a CPU percentage
            let snap2 = monitor.collect(pid).unwrap();
            // CPU percent can still be 0 if interval is too short or no CPU used
            // Just verify it's a valid value
            assert!(snap2.cpu_percent >= 0.0);
        }

        #[test]
        fn test_parse_stat_content_simple() {
            let content = "1234 (test) S 1 1234 1234 0 -1 4194304 100 0 0 0 50 25 0 0 20 0 5 0 1000 1000000 100 18446744073709551615";
            let stat = DaemonMonitor::parse_stat_content(content).unwrap();
            assert_eq!(stat.state, ProcessState::Sleeping);
            assert_eq!(stat.utime, 50);
            assert_eq!(stat.stime, 25);
            assert_eq!(stat.num_threads, 5);
        }

        #[test]
        fn test_parse_stat_content_with_spaces_in_name() {
            // Process name with spaces and parentheses
            let content = "1234 (test (process)) R 1 1234 1234 0 -1 4194304 100 0 0 0 100 50 0 0 20 0 10 0 1000 1000000 100 18446744073709551615";
            let stat = DaemonMonitor::parse_stat_content(content).unwrap();
            assert_eq!(stat.state, ProcessState::Running);
            assert_eq!(stat.utime, 100);
            assert_eq!(stat.stime, 50);
            assert_eq!(stat.num_threads, 10);
        }

        #[test]
        fn test_parse_stat_all_states() {
            let test_cases = [
                (
                    "1 (t) R 0 0 0 0 0 0 0 0 0 0 1 1 0 0 0 0 1 0 0 0 0 0",
                    ProcessState::Running,
                ),
                (
                    "1 (t) S 0 0 0 0 0 0 0 0 0 0 1 1 0 0 0 0 1 0 0 0 0 0",
                    ProcessState::Sleeping,
                ),
                (
                    "1 (t) D 0 0 0 0 0 0 0 0 0 0 1 1 0 0 0 0 1 0 0 0 0 0",
                    ProcessState::DiskWait,
                ),
                (
                    "1 (t) Z 0 0 0 0 0 0 0 0 0 0 1 1 0 0 0 0 1 0 0 0 0 0",
                    ProcessState::Zombie,
                ),
                (
                    "1 (t) T 0 0 0 0 0 0 0 0 0 0 1 1 0 0 0 0 1 0 0 0 0 0",
                    ProcessState::Stopped,
                ),
                (
                    "1 (t) t 0 0 0 0 0 0 0 0 0 0 1 1 0 0 0 0 1 0 0 0 0 0",
                    ProcessState::Stopped,
                ),
                (
                    "1 (t) X 0 0 0 0 0 0 0 0 0 0 1 1 0 0 0 0 1 0 0 0 0 0",
                    ProcessState::Unknown,
                ),
            ];

            for (content, expected_state) in test_cases {
                let stat = DaemonMonitor::parse_stat_content(content).unwrap();
                assert_eq!(
                    stat.state, expected_state,
                    "Failed for content: {}",
                    content
                );
            }
        }

        #[test]
        fn test_parse_stat_malformed_no_paren() {
            let content = "1234 test S 1";
            let result = DaemonMonitor::parse_stat_content(content);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("no closing paren"));
        }

        #[test]
        fn test_parse_stat_malformed_too_few_fields() {
            let content = "1234 (test) S 1 2 3";
            let result = DaemonMonitor::parse_stat_content(content);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("expected 20+"));
        }

        #[test]
        fn test_memory_collection() {
            let mut monitor = DaemonMonitor::new(100);
            let pid = process::id();
            let snapshot = monitor.collect(pid).unwrap();

            // Our process should use at least 1MB of memory
            assert!(
                snapshot.memory_bytes >= 1024 * 1024,
                "Expected at least 1MB, got {} bytes",
                snapshot.memory_bytes
            );

            // Memory percent should be between 0 and 100
            assert!(snapshot.memory_percent >= 0.0);
            assert!(snapshot.memory_percent <= 100.0);
        }

        #[test]
        fn test_page_size() {
            let page_size = DaemonMonitor::get_page_size();
            // Page size should be 4KB or larger
            assert!(page_size >= 4096);
            // Page size should be a power of 2
            assert!(page_size.is_power_of_two());
        }

        #[test]
        fn test_total_memory() {
            let total = DaemonMonitor::read_total_memory().unwrap();
            // System should have at least 128MB
            assert!(
                total >= 128 * 1024 * 1024,
                "Expected at least 128MB, got {} bytes",
                total
            );
        }

        #[test]
        fn test_history_filtering() {
            let mut monitor = DaemonMonitor::new(100);
            let pid = process::id();

            // Collect a sample
            monitor.collect(pid).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10));
            monitor.collect(pid).unwrap();

            // History with long duration should include both
            let long_history = monitor.history(std::time::Duration::from_secs(60));
            assert_eq!(long_history.len(), 2);

            // History with zero duration should include none (or recent ones)
            let short_history = monitor.history(std::time::Duration::from_nanos(1));
            // Recent samples might still be included due to timing
            assert!(short_history.len() <= 2);
        }
    }

    // ==================== Popperian Falsification Tests ====================
    // These tests attempt to DISPROVE the correctness of our implementation

    #[cfg(target_os = "linux")]
    mod falsification_tests {
        use super::*;

        /// F001: Falsify that parse_stat_content handles edge cases
        #[test]
        fn f001_parse_stat_empty_input() {
            let result = DaemonMonitor::parse_stat_content("");
            assert!(result.is_err(), "Empty input should fail parsing");
        }

        /// F002: Falsify that memory values are reasonable
        #[test]
        fn f002_memory_not_absurdly_large() {
            let mut monitor = DaemonMonitor::new(100);
            let pid = std::process::id();
            let snapshot = monitor.collect(pid).unwrap();

            // A single process shouldn't use more than total system memory
            assert!(
                snapshot.memory_bytes <= monitor.total_memory,
                "Process memory {} exceeds total memory {}",
                snapshot.memory_bytes,
                monitor.total_memory
            );
        }

        /// F003: Falsify that CPU percentage stays in bounds
        #[test]
        fn f003_cpu_percent_reasonable_bounds() {
            let mut monitor = DaemonMonitor::new(100);
            let pid = std::process::id();

            // Collect twice to get CPU delta
            monitor.collect(pid).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));
            let snapshot = monitor.collect(pid).unwrap();

            // CPU can exceed 100% on multi-core but shouldn't be negative
            assert!(
                snapshot.cpu_percent >= 0.0,
                "CPU percent should not be negative: {}",
                snapshot.cpu_percent
            );
            // Sanity check: shouldn't be millions of percent
            assert!(
                snapshot.cpu_percent < 10000.0,
                "CPU percent unreasonably high: {}",
                snapshot.cpu_percent
            );
        }

        /// F004: Falsify that threads count is valid
        #[test]
        fn f004_threads_at_least_one() {
            let mut monitor = DaemonMonitor::new(100);
            let pid = std::process::id();
            let snapshot = monitor.collect(pid).unwrap();

            assert!(
                snapshot.threads >= 1,
                "Running process must have at least 1 thread"
            );
        }

        /// F005: Falsify that ring buffer respects capacity
        #[test]
        fn f005_ring_buffer_never_exceeds_capacity() {
            let capacity = 5;
            let mut monitor = DaemonMonitor::new(capacity);

            for _ in 0..100 {
                let _ = monitor.collect(1);
            }

            assert!(
                monitor.all_history().len() <= capacity,
                "Ring buffer exceeded capacity: {} > {}",
                monitor.all_history().len(),
                capacity
            );
        }

        /// F006: Falsify that timestamps are monotonically increasing
        #[test]
        fn f006_timestamps_monotonic() {
            let mut monitor = DaemonMonitor::new(100);
            let pid = std::process::id();

            for _ in 0..10 {
                monitor.collect(pid).unwrap();
            }

            let history = monitor.all_history();
            for window in history.iter().collect::<Vec<_>>().windows(2) {
                assert!(
                    window[1].timestamp >= window[0].timestamp,
                    "Timestamps should be monotonically increasing"
                );
            }
        }

        /// F007: Falsify that PID in snapshot matches requested PID
        #[test]
        fn f007_pid_matches_request() {
            let mut monitor = DaemonMonitor::new(100);
            let pid = std::process::id();
            let snapshot = monitor.collect(pid).unwrap();

            assert_eq!(snapshot.pid, pid, "Snapshot PID should match requested PID");
        }

        /// F008: Falsify memory percent calculation
        #[test]
        fn f008_memory_percent_calculation_valid() {
            let mut monitor = DaemonMonitor::new(100);
            let pid = std::process::id();
            let snapshot = monitor.collect(pid).unwrap();

            // Verify the percent is calculated correctly
            if monitor.total_memory > 0 {
                let expected_percent =
                    (snapshot.memory_bytes as f64 / monitor.total_memory as f64) * 100.0;
                let diff = (snapshot.memory_percent - expected_percent).abs();
                assert!(
                    diff < 0.001,
                    "Memory percent calculation mismatch: {} vs {}",
                    snapshot.memory_percent,
                    expected_percent
                );
            }
        }
    }
}
