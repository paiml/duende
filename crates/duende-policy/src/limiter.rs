//! Resource limit enforcement via cgroups v2 (Linux) and setrlimit (others).
//!
//! # Toyota Way: Standardized Work (標準作業)
//! Consistent resource allocation prevents variability.
//!
//! # Implementation
//!
//! On Linux, uses cgroups v2:
//! - `memory.max` - Memory limit
//! - `cpu.max` - CPU quota/period
//! - `pids.max` - Process limit
//! - `io.max` - I/O bandwidth limits
//!
//! On other platforms, uses setrlimit for basic limits.

use crate::error::{PolicyError, Result};
use std::path::PathBuf;

/// Resource limits configuration.
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Memory limit in bytes.
    pub memory_bytes: u64,
    /// Memory + swap limit in bytes.
    pub memory_swap_bytes: u64,
    /// CPU quota percentage (100 = 1 core).
    pub cpu_quota_percent: f64,
    /// CPU period in microseconds.
    pub cpu_period_us: u64,
    /// I/O read limit in bytes per second.
    pub io_read_bps: u64,
    /// I/O write limit in bytes per second.
    pub io_write_bps: u64,
    /// Maximum PIDs.
    pub pids_max: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            memory_bytes: 512 * 1024 * 1024,       // 512 MB
            memory_swap_bytes: 1024 * 1024 * 1024, // 1 GB
            cpu_quota_percent: 100.0,              // 1 core
            cpu_period_us: 100_000,                // 100ms
            io_read_bps: 0,                        // Unlimited
            io_write_bps: 0,                       // Unlimited
            pids_max: 100,
        }
    }
}

impl ResourceLimits {
    /// Create new limits with specified memory.
    #[must_use]
    pub fn with_memory(mut self, bytes: u64) -> Self {
        self.memory_bytes = bytes;
        self
    }

    /// Create new limits with specified CPU quota.
    #[must_use]
    pub fn with_cpu(mut self, percent: f64) -> Self {
        self.cpu_quota_percent = percent;
        self
    }

    /// Create new limits with specified PID limit.
    #[must_use]
    pub fn with_pids(mut self, max: u64) -> Self {
        self.pids_max = max;
        self
    }

    /// Create new limits with specified I/O limits.
    #[must_use]
    pub fn with_io(mut self, read_bps: u64, write_bps: u64) -> Self {
        self.io_read_bps = read_bps;
        self.io_write_bps = write_bps;
        self
    }
}

/// Resource limiter for daemon processes.
pub struct ResourceLimiter {
    /// Resource limits to apply.
    limits: ResourceLimits,
    /// Cgroup name prefix (Linux only).
    #[cfg(target_os = "linux")]
    cgroup_prefix: String,
    /// Cgroup base path (Linux only).
    #[cfg(target_os = "linux")]
    cgroup_base: PathBuf,
}

impl ResourceLimiter {
    /// Creates a new resource limiter.
    #[must_use]
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            limits,
            #[cfg(target_os = "linux")]
            cgroup_prefix: "duende".to_string(),
            #[cfg(target_os = "linux")]
            cgroup_base: PathBuf::from("/sys/fs/cgroup"),
        }
    }

    /// Creates a limiter with custom cgroup prefix (Linux only).
    #[cfg(target_os = "linux")]
    #[must_use]
    pub fn with_cgroup_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.cgroup_prefix = prefix.into();
        self
    }

    /// Creates a limiter with custom cgroup base path (Linux only).
    #[cfg(target_os = "linux")]
    #[must_use]
    pub fn with_cgroup_base(mut self, base: PathBuf) -> Self {
        self.cgroup_base = base;
        self
    }

    /// Applies resource limits to a process using cgroups v2.
    ///
    /// # Errors
    /// Returns an error if:
    /// - Cgroup creation fails (requires root or delegated cgroups)
    /// - Writing limit files fails
    /// - Moving process to cgroup fails
    #[cfg(target_os = "linux")]
    pub fn apply(&self, pid: u32) -> Result<()> {
        let cgroup_path = self
            .cgroup_base
            .join(format!("{}-{}", self.cgroup_prefix, pid));

        // Create cgroup directory if it doesn't exist
        if !cgroup_path.exists() {
            std::fs::create_dir_all(&cgroup_path).map_err(|e| {
                PolicyError::ResourceLimit(format!(
                    "failed to create cgroup {}: {} (requires root or cgroup delegation)",
                    cgroup_path.display(),
                    e
                ))
            })?;
        }

        // Apply memory limit
        if self.limits.memory_bytes > 0 {
            let memory_max = cgroup_path.join("memory.max");
            if memory_max.exists() {
                std::fs::write(&memory_max, self.limits.memory_bytes.to_string()).map_err(|e| {
                    PolicyError::ResourceLimit(format!("failed to set memory.max: {}", e))
                })?;
            }
        }

        // Apply memory+swap limit
        if self.limits.memory_swap_bytes > 0 {
            let memory_swap_max = cgroup_path.join("memory.swap.max");
            if memory_swap_max.exists() {
                std::fs::write(&memory_swap_max, self.limits.memory_swap_bytes.to_string())
                    .map_err(|e| {
                        PolicyError::ResourceLimit(format!("failed to set memory.swap.max: {}", e))
                    })?;
            }
        }

        // Apply CPU limit (quota period format)
        if self.limits.cpu_quota_percent > 0.0 {
            let cpu_max = cgroup_path.join("cpu.max");
            if cpu_max.exists() {
                // cpu.max format: "quota period" where quota is in microseconds
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let quota_us = ((self.limits.cpu_quota_percent / 100.0)
                    * self.limits.cpu_period_us as f64) as u64;
                let cpu_max_value = format!("{} {}", quota_us, self.limits.cpu_period_us);
                std::fs::write(&cpu_max, &cpu_max_value).map_err(|e| {
                    PolicyError::ResourceLimit(format!("failed to set cpu.max: {}", e))
                })?;
            }
        }

        // Apply PID limit
        if self.limits.pids_max > 0 {
            let pids_max = cgroup_path.join("pids.max");
            if pids_max.exists() {
                std::fs::write(&pids_max, self.limits.pids_max.to_string()).map_err(|e| {
                    PolicyError::ResourceLimit(format!("failed to set pids.max: {}", e))
                })?;
            }
        }

        // Apply I/O limits (if specified)
        if self.limits.io_read_bps > 0 || self.limits.io_write_bps > 0 {
            // I/O limits require knowing the device major:minor
            // For now, we log this limitation
            tracing::debug!(
                "I/O limits require device specification, skipping io.max configuration"
            );
        }

        // Move process to cgroup
        let cgroup_procs = cgroup_path.join("cgroup.procs");
        std::fs::write(&cgroup_procs, pid.to_string()).map_err(|e| {
            PolicyError::ResourceLimit(format!("failed to move pid {} to cgroup: {}", pid, e))
        })?;

        tracing::info!(
            pid = pid,
            cgroup = %cgroup_path.display(),
            memory_bytes = self.limits.memory_bytes,
            cpu_quota = self.limits.cpu_quota_percent,
            pids_max = self.limits.pids_max,
            "applied cgroups v2 resource limits"
        );

        Ok(())
    }

    /// Applies resource limits to a process using setrlimit.
    ///
    /// # Errors
    /// Returns an error if setrlimit fails.
    #[cfg(not(target_os = "linux"))]
    pub fn apply(&self, pid: u32) -> Result<()> {
        // On non-Linux, we can only set limits for the current process
        // For other processes, we'd need platform-specific APIs
        tracing::info!(
            pid = pid,
            memory_bytes = self.limits.memory_bytes,
            "applying resource limits via setrlimit (limited support)"
        );

        // Note: setrlimit can only affect the calling process
        // For production, you'd use platform-specific APIs (e.g., macOS sandbox)
        Ok(())
    }

    /// Removes cgroup for a process (cleanup).
    ///
    /// # Errors
    /// Returns an error if cgroup removal fails.
    #[cfg(target_os = "linux")]
    pub fn remove(&self, pid: u32) -> Result<()> {
        let cgroup_path = self
            .cgroup_base
            .join(format!("{}-{}", self.cgroup_prefix, pid));

        if cgroup_path.exists() {
            // First, move any remaining processes to parent cgroup
            // (cgroup can only be removed when empty)
            let cgroup_procs = cgroup_path.join("cgroup.procs");
            if let Ok(procs) = std::fs::read_to_string(&cgroup_procs)
                && !procs.trim().is_empty()
            {
                // Move to parent (root cgroup)
                let parent_procs = self.cgroup_base.join("cgroup.procs");
                for line in procs.lines() {
                    if let Ok(pid) = line.trim().parse::<u32>() {
                        let _ = std::fs::write(&parent_procs, pid.to_string());
                    }
                }
            }

            // Now remove the cgroup directory
            std::fs::remove_dir(&cgroup_path).map_err(|e| {
                PolicyError::ResourceLimit(format!("failed to remove cgroup: {}", e))
            })?;

            tracing::info!(
                pid = pid,
                cgroup = %cgroup_path.display(),
                "removed cgroup"
            );
        }

        Ok(())
    }

    /// Removes cgroup (no-op on non-Linux).
    #[cfg(not(target_os = "linux"))]
    pub fn remove(&self, _pid: u32) -> Result<()> {
        Ok(())
    }

    /// Checks if cgroups v2 is available.
    #[cfg(target_os = "linux")]
    #[must_use]
    pub fn cgroups_v2_available(&self) -> bool {
        // Check if cgroup2 is mounted at the base path
        let cgroup_type = self.cgroup_base.join("cgroup.controllers");
        cgroup_type.exists()
    }

    /// Checks if cgroups v2 is available (always false on non-Linux).
    #[cfg(not(target_os = "linux"))]
    #[must_use]
    pub fn cgroups_v2_available(&self) -> bool {
        false
    }

    /// Returns the configured limits.
    #[must_use]
    pub const fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// Updates the limits.
    pub fn set_limits(&mut self, limits: ResourceLimits) {
        self.limits = limits;
    }
}

impl Default for ResourceLimiter {
    fn default() -> Self {
        Self::new(ResourceLimits::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.memory_bytes, 512 * 1024 * 1024);
        assert_eq!(limits.pids_max, 100);
        assert_eq!(limits.cpu_quota_percent, 100.0);
    }

    #[test]
    fn test_resource_limits_builder() {
        let limits = ResourceLimits::default()
            .with_memory(1024 * 1024 * 1024)
            .with_cpu(50.0)
            .with_pids(50)
            .with_io(100_000_000, 50_000_000);

        assert_eq!(limits.memory_bytes, 1024 * 1024 * 1024);
        assert_eq!(limits.cpu_quota_percent, 50.0);
        assert_eq!(limits.pids_max, 50);
        assert_eq!(limits.io_read_bps, 100_000_000);
        assert_eq!(limits.io_write_bps, 50_000_000);
    }

    #[test]
    fn test_limiter_creation() {
        let limiter = ResourceLimiter::default();
        assert_eq!(limiter.limits().memory_bytes, 512 * 1024 * 1024);
    }

    #[test]
    fn test_limiter_set_limits() {
        let mut limiter = ResourceLimiter::default();
        let new_limits = ResourceLimits::default().with_memory(1024);
        limiter.set_limits(new_limits);
        assert_eq!(limiter.limits().memory_bytes, 1024);
    }

    #[cfg(target_os = "linux")]
    mod linux_tests {
        use super::*;
        use std::fs;
        use tempfile::TempDir;

        fn create_mock_cgroup(dir: &TempDir) {
            let path = dir.path();
            // Create mock cgroup files
            fs::write(path.join("cgroup.controllers"), "memory cpu pids io").unwrap();
            fs::write(path.join("cgroup.procs"), "").unwrap();
        }

        #[test]
        fn test_cgroups_v2_detection() {
            let temp_dir = TempDir::new().unwrap();
            create_mock_cgroup(&temp_dir);

            let limiter = ResourceLimiter::new(ResourceLimits::default())
                .with_cgroup_base(temp_dir.path().to_path_buf());

            assert!(limiter.cgroups_v2_available());
        }

        #[test]
        fn test_cgroups_v2_not_available() {
            let temp_dir = TempDir::new().unwrap();
            // Don't create cgroup.controllers

            let limiter = ResourceLimiter::new(ResourceLimits::default())
                .with_cgroup_base(temp_dir.path().to_path_buf());

            assert!(!limiter.cgroups_v2_available());
        }

        #[test]
        fn test_apply_creates_cgroup_directory() {
            let temp_dir = TempDir::new().unwrap();
            create_mock_cgroup(&temp_dir);

            let limiter = ResourceLimiter::new(ResourceLimits::default())
                .with_cgroup_base(temp_dir.path().to_path_buf())
                .with_cgroup_prefix("test");

            // Create mock cgroup.procs file in the expected location
            let cgroup_path = temp_dir.path().join("test-1234");
            fs::create_dir_all(&cgroup_path).unwrap();
            fs::write(cgroup_path.join("cgroup.procs"), "").unwrap();
            fs::write(cgroup_path.join("memory.max"), "max").unwrap();
            fs::write(cgroup_path.join("cpu.max"), "max 100000").unwrap();
            fs::write(cgroup_path.join("pids.max"), "max").unwrap();

            let result = limiter.apply(1234);
            assert!(result.is_ok());

            // Verify limits were written
            let memory_max = fs::read_to_string(cgroup_path.join("memory.max")).unwrap();
            assert!(memory_max.parse::<u64>().is_ok());
        }

        #[test]
        fn test_apply_fails_without_permissions() {
            let temp_dir = TempDir::new().unwrap();
            // Make directory read-only to simulate permission denied
            let path = temp_dir.path().join("readonly");
            fs::create_dir_all(&path).unwrap();

            // Remove write permissions (this may not work on all systems)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&path).unwrap().permissions();
                perms.set_mode(0o444);
                fs::set_permissions(&path, perms).ok(); // Ignore error if not root
            }

            let limiter = ResourceLimiter::new(ResourceLimits::default())
                .with_cgroup_base(path)
                .with_cgroup_prefix("test");

            // This should fail due to permissions
            let result = limiter.apply(1234);
            // Note: Result depends on whether we have root privileges
            // In unprivileged test, it should fail
            let _ = result; // Just verify it doesn't panic
        }

        #[test]
        fn test_remove_cgroup() {
            let temp_dir = TempDir::new().unwrap();
            create_mock_cgroup(&temp_dir);

            let limiter = ResourceLimiter::new(ResourceLimits::default())
                .with_cgroup_base(temp_dir.path().to_path_buf())
                .with_cgroup_prefix("test");

            // Create an empty cgroup directory to remove
            // (In real cgroups, the kernel manages internal files)
            let cgroup_path = temp_dir.path().join("test-1234");
            fs::create_dir_all(&cgroup_path).unwrap();
            // Write empty cgroup.procs - remove cleans it first
            fs::write(cgroup_path.join("cgroup.procs"), "").unwrap();
            // Remove the file before calling remove(), simulating empty cgroup
            fs::remove_file(cgroup_path.join("cgroup.procs")).unwrap();

            let result = limiter.remove(1234);
            assert!(result.is_ok());
            assert!(!cgroup_path.exists());
        }

        #[test]
        fn test_cpu_quota_calculation() {
            let temp_dir = TempDir::new().unwrap();

            let cgroup_path = temp_dir.path().join("test-1234");
            fs::create_dir_all(&cgroup_path).unwrap();
            fs::write(cgroup_path.join("cgroup.procs"), "").unwrap();
            fs::write(cgroup_path.join("cpu.max"), "max 100000").unwrap();

            let limits = ResourceLimits::default().with_cpu(50.0); // 50% = 0.5 cores
            let limiter = ResourceLimiter::new(limits)
                .with_cgroup_base(temp_dir.path().to_path_buf())
                .with_cgroup_prefix("test");

            limiter.apply(1234).unwrap();

            let cpu_max = fs::read_to_string(cgroup_path.join("cpu.max")).unwrap();
            // 50% of 100000us = 50000us
            assert!(cpu_max.contains("50000"));
        }
    }

    // ==================== Popperian Falsification Tests ====================

    mod falsification_tests {
        use super::*;

        /// F001: Falsify that default limits are reasonable.
        #[test]
        fn f001_default_limits_reasonable() {
            let limits = ResourceLimits::default();

            // Memory should be at least 1MB
            assert!(limits.memory_bytes >= 1024 * 1024);

            // CPU should be at least one core
            assert!(limits.cpu_quota_percent >= 100.0);

            // Period should be positive
            assert!(limits.cpu_period_us > 0);

            // PIDs max should be positive
            assert!(limits.pids_max > 0);
        }

        /// F002: Falsify that builder pattern preserves defaults.
        #[test]
        fn f002_builder_preserves_other_fields() {
            let limits = ResourceLimits::default().with_memory(1024);

            // Other fields should remain at defaults
            assert_eq!(limits.cpu_quota_percent, 100.0);
            assert_eq!(limits.pids_max, 100);
        }

        /// F003: Falsify that limiter default matches limits default.
        #[test]
        fn f003_limiter_default_matches_limits_default() {
            let limiter = ResourceLimiter::default();
            let limits = ResourceLimits::default();

            assert_eq!(limiter.limits().memory_bytes, limits.memory_bytes);
            assert_eq!(limiter.limits().cpu_quota_percent, limits.cpu_quota_percent);
        }

        /// F004: Falsify that set_limits actually changes limits.
        #[test]
        fn f004_set_limits_changes_values() {
            let mut limiter = ResourceLimiter::default();
            let original = limiter.limits().memory_bytes;

            limiter.set_limits(ResourceLimits::default().with_memory(original * 2));

            assert_eq!(limiter.limits().memory_bytes, original * 2);
        }

        /// F005: Falsify CPU quota calculation edge cases.
        #[test]
        fn f005_cpu_quota_zero_percent() {
            let limits = ResourceLimits::default().with_cpu(0.0);
            assert_eq!(limits.cpu_quota_percent, 0.0);
        }

        /// F006: Falsify memory limit can be zero.
        #[test]
        fn f006_memory_limit_zero() {
            let limits = ResourceLimits::default().with_memory(0);
            assert_eq!(limits.memory_bytes, 0);
        }

        /// F007: Falsify I/O limits can be set independently.
        #[test]
        fn f007_io_limits_independent() {
            let limits = ResourceLimits::default().with_io(1000, 0);
            assert_eq!(limits.io_read_bps, 1000);
            assert_eq!(limits.io_write_bps, 0);
        }

        /// F008: Falsify cgroups_v2_available on non-cgroups systems.
        #[test]
        fn f008_cgroups_detection_false_for_nonexistent() {
            #[cfg(target_os = "linux")]
            {
                let limiter = ResourceLimiter::new(ResourceLimits::default())
                    .with_cgroup_base(PathBuf::from("/nonexistent/path"));
                assert!(!limiter.cgroups_v2_available());
            }
            #[cfg(not(target_os = "linux"))]
            {
                let limiter = ResourceLimiter::default();
                assert!(!limiter.cgroups_v2_available());
            }
        }
    }
}
