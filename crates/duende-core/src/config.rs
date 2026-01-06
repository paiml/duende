//! Daemon configuration types.
//!
//! Per Iron Lotus Framework: Configuration is validated at load time (Poka-Yoke),
//! with sensible defaults and clear error messages.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::error::{DaemonError, Result};

/// Daemon configuration.
///
/// # Toyota Way: Standardized Work (標準作業)
/// Every daemon follows the same configuration contract, enabling
/// predictable behavior across platforms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Daemon name (must be valid identifier).
    pub name: String,

    /// Daemon version (semver).
    pub version: String,

    /// Human-readable description.
    #[serde(default)]
    pub description: String,

    /// Path to the daemon binary.
    pub binary_path: PathBuf,

    /// Path to the configuration file.
    #[serde(default)]
    pub config_path: Option<PathBuf>,

    /// Command-line arguments.
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables.
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// User to run as (Unix).
    #[serde(default)]
    pub user: Option<String>,

    /// Group to run as (Unix).
    #[serde(default)]
    pub group: Option<String>,

    /// Working directory.
    #[serde(default)]
    pub working_dir: Option<PathBuf>,

    /// Resource limits.
    #[serde(default)]
    pub resources: ResourceConfig,

    /// Health check configuration.
    #[serde(default)]
    pub health_check: HealthCheckConfig,

    /// Restart policy.
    #[serde(default)]
    pub restart: RestartPolicy,

    /// Graceful shutdown timeout.
    #[serde(default = "default_shutdown_timeout")]
    #[serde(with = "humantime_serde")]
    pub shutdown_timeout: Duration,

    /// Platform-specific configuration.
    #[serde(default)]
    pub platform: PlatformConfig,
}

fn default_shutdown_timeout() -> Duration {
    Duration::from_secs(30)
}

impl DaemonConfig {
    /// Creates a new daemon configuration with required fields.
    #[must_use]
    pub fn new(name: impl Into<String>, binary_path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            version: "0.1.0".to_string(),
            description: String::new(),
            binary_path: binary_path.into(),
            config_path: None,
            args: vec![],
            env: HashMap::new(),
            user: None,
            group: None,
            working_dir: None,
            resources: ResourceConfig::default(),
            health_check: HealthCheckConfig::default(),
            restart: RestartPolicy::default(),
            shutdown_timeout: default_shutdown_timeout(),
            platform: PlatformConfig::default(),
        }
    }

    /// Validates the configuration.
    ///
    /// # Errors
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> Result<()> {
        // Name must be a valid identifier
        if self.name.is_empty() {
            return Err(DaemonError::config("name cannot be empty"));
        }
        if !self
            .name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(DaemonError::config(
                "name must contain only alphanumeric characters, hyphens, and underscores",
            ));
        }

        // Binary path must be specified
        if self.binary_path.as_os_str().is_empty() {
            return Err(DaemonError::config("binary_path cannot be empty"));
        }

        // Resource limits must be sensible
        self.resources.validate()?;

        Ok(())
    }

    /// Loads configuration from a TOML file.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed.
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| DaemonError::config(format!("failed to read config: {e}")))?;
        let config: Self = toml::from_str(&content)
            .map_err(|e| DaemonError::config(format!("failed to parse config: {e}")))?;
        config.validate()?;
        Ok(config)
    }
}

/// Resource limits configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    /// Memory limit in bytes.
    #[serde(default = "default_memory_limit")]
    pub memory_bytes: u64,

    /// Memory + swap limit in bytes.
    #[serde(default = "default_memory_swap_limit")]
    pub memory_swap_bytes: u64,

    /// CPU quota as percentage (100 = 1 core).
    #[serde(default = "default_cpu_quota")]
    pub cpu_quota_percent: f64,

    /// CPU shares (relative weight).
    #[serde(default = "default_cpu_shares")]
    pub cpu_shares: u64,

    /// I/O read limit in bytes per second.
    #[serde(default)]
    pub io_read_bps: u64,

    /// I/O write limit in bytes per second.
    #[serde(default)]
    pub io_write_bps: u64,

    /// Maximum number of processes.
    #[serde(default = "default_pids_max")]
    pub pids_max: u64,

    /// Maximum open file descriptors.
    #[serde(default = "default_open_files")]
    pub open_files_max: u64,

    /// Lock all daemon memory to prevent swapping (mlockall).
    ///
    /// # DT-007: Swap Deadlock Prevention
    /// CRITICAL for daemons that serve as swap devices (e.g., trueno-ublk).
    /// Without memory locking, a deadlock can occur:
    /// 1. Kernel needs to swap pages OUT to the daemon's device
    /// 2. Daemon needs memory to process I/O request
    /// 3. Kernel tries to swap out daemon's pages to free memory
    /// 4. Swap goes to the same daemon → waiting for itself → DEADLOCK
    ///
    /// When enabled, calls `mlockall(MCL_CURRENT | MCL_FUTURE)` to pin
    /// all daemon memory, preventing it from being swapped.
    ///
    /// Requires CAP_IPC_LOCK capability or root privileges.
    #[serde(default)]
    pub lock_memory: bool,

    /// Whether memory locking failure is fatal.
    ///
    /// - `true`: Daemon fails to start if mlock() fails
    /// - `false`: Warning logged but daemon continues (may deadlock under pressure)
    ///
    /// Default: `false` for backwards compatibility.
    #[serde(default)]
    pub lock_memory_required: bool,
}

fn default_memory_limit() -> u64 {
    512 * 1024 * 1024 // 512 MB
}

fn default_memory_swap_limit() -> u64 {
    1024 * 1024 * 1024 // 1 GB
}

fn default_cpu_quota() -> f64 {
    100.0 // 1 core
}

fn default_cpu_shares() -> u64 {
    1024
}

fn default_pids_max() -> u64 {
    100
}

fn default_open_files() -> u64 {
    1024
}

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            memory_bytes: default_memory_limit(),
            memory_swap_bytes: default_memory_swap_limit(),
            cpu_quota_percent: default_cpu_quota(),
            cpu_shares: default_cpu_shares(),
            io_read_bps: 0,  // Unlimited
            io_write_bps: 0, // Unlimited
            pids_max: default_pids_max(),
            open_files_max: default_open_files(),
            lock_memory: false,
            lock_memory_required: false,
        }
    }
}

impl ResourceConfig {
    /// Validates resource limits.
    ///
    /// # Errors
    /// Returns an error if limits are invalid.
    pub fn validate(&self) -> Result<()> {
        if self.memory_bytes == 0 {
            return Err(DaemonError::config("memory_bytes must be greater than 0"));
        }
        if self.cpu_quota_percent <= 0.0 {
            return Err(DaemonError::config("cpu_quota_percent must be positive"));
        }
        if self.pids_max == 0 {
            return Err(DaemonError::config("pids_max must be greater than 0"));
        }
        Ok(())
    }
}

/// Health check configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Whether health checks are enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Health check interval.
    #[serde(default = "default_health_interval")]
    #[serde(with = "humantime_serde")]
    pub interval: Duration,

    /// Health check timeout.
    #[serde(default = "default_health_timeout")]
    #[serde(with = "humantime_serde")]
    pub timeout: Duration,

    /// Number of retries before marking unhealthy.
    #[serde(default = "default_health_retries")]
    pub retries: u32,
}

fn default_true() -> bool {
    true
}

fn default_health_interval() -> Duration {
    Duration::from_secs(30)
}

fn default_health_timeout() -> Duration {
    Duration::from_secs(10)
}

fn default_health_retries() -> u32 {
    3
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            interval: default_health_interval(),
            timeout: default_health_timeout(),
            retries: default_health_retries(),
        }
    }
}

/// Restart policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    /// Never restart.
    Never,
    /// Restart on failure only.
    #[default]
    OnFailure,
    /// Always restart.
    Always,
    /// Restart unless stopped manually.
    UnlessStopped,
}

/// Platform-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlatformConfig {
    /// Container image (for Container platform).
    #[serde(default)]
    pub container_image: Option<String>,

    /// Number of vCPUs (for pepita platform).
    #[serde(default)]
    pub vcpus: Option<u32>,

    /// Kernel path (for pepita platform).
    #[serde(default)]
    pub kernel_path: Option<PathBuf>,

    /// Root filesystem path (for pepita platform).
    #[serde(default)]
    pub rootfs_path: Option<PathBuf>,

    /// Priority level 0-7 (for WOS platform).
    #[serde(default)]
    pub priority: Option<u8>,
}

/// Serde helper for humantime durations.
mod humantime_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    /// Serializes a duration as a human-readable string.
    ///
    /// # Errors
    /// Returns an error if serialization fails.
    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&humantime::format_duration(*duration).to_string())
    }

    /// Deserializes a duration from a human-readable string.
    ///
    /// # Errors
    /// Returns an error if the string cannot be parsed.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        humantime::parse_duration(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_new() {
        let config = DaemonConfig::new("test-daemon", "/usr/bin/test");
        assert_eq!(config.name, "test-daemon");
        assert_eq!(config.binary_path, PathBuf::from("/usr/bin/test"));
    }

    #[test]
    fn test_config_new_defaults() {
        let config = DaemonConfig::new("test", "/bin/test");
        assert_eq!(config.version, "0.1.0");
        assert!(config.description.is_empty());
        assert!(config.args.is_empty());
        assert!(config.env.is_empty());
        assert!(config.user.is_none());
        assert!(config.group.is_none());
        assert!(config.working_dir.is_none());
        assert!(config.config_path.is_none());
        assert_eq!(config.shutdown_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_config_validate_empty_name() {
        let mut config = DaemonConfig::new("test", "/bin/test");
        config.name = String::new();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_invalid_name() {
        let mut config = DaemonConfig::new("test", "/bin/test");
        config.name = "invalid name!".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_empty_binary_path() {
        let config = DaemonConfig::new("test", "");
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validate_valid() {
        let config = DaemonConfig::new("valid-name", "/bin/test");
        assert!(config.validate().is_ok());

        let config = DaemonConfig::new("valid_name", "/bin/test");
        assert!(config.validate().is_ok());

        let config = DaemonConfig::new("valid123", "/bin/test");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_resource_config_defaults() {
        let config = ResourceConfig::default();
        assert_eq!(config.memory_bytes, 512 * 1024 * 1024);
        assert!((config.cpu_quota_percent - 100.0).abs() < f64::EPSILON);
        assert_eq!(config.memory_swap_bytes, 1024 * 1024 * 1024);
        assert_eq!(config.cpu_shares, 1024);
        assert_eq!(config.io_read_bps, 0);
        assert_eq!(config.io_write_bps, 0);
        assert_eq!(config.pids_max, 100);
        assert_eq!(config.open_files_max, 1024);
        assert!(!config.lock_memory);
        assert!(!config.lock_memory_required);
    }

    #[test]
    fn test_resource_config_validate() {
        let config = ResourceConfig {
            memory_bytes: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_resource_config_validate_cpu_quota() {
        let config = ResourceConfig {
            cpu_quota_percent: 0.0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = ResourceConfig {
            cpu_quota_percent: -1.0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_resource_config_validate_pids_max() {
        let config = ResourceConfig {
            pids_max: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_health_check_config_defaults() {
        let config = HealthCheckConfig::default();
        assert!(config.enabled);
        assert_eq!(config.interval, Duration::from_secs(30));
        assert_eq!(config.timeout, Duration::from_secs(10));
        assert_eq!(config.retries, 3);
    }

    #[test]
    fn test_restart_policy_default() {
        let policy = RestartPolicy::default();
        assert!(matches!(policy, RestartPolicy::OnFailure));
    }

    #[test]
    fn test_platform_config_default() {
        let config = PlatformConfig::default();
        assert!(config.container_image.is_none());
        assert!(config.vcpus.is_none());
        assert!(config.kernel_path.is_none());
        assert!(config.rootfs_path.is_none());
        assert!(config.priority.is_none());
    }

    #[test]
    fn test_config_serialize_roundtrip() {
        let config = DaemonConfig::new("test", "/bin/test");
        let toml = toml::to_string(&config).unwrap();
        let deserialized: DaemonConfig = toml::from_str(&toml).unwrap();
        assert_eq!(config.name, deserialized.name);
    }

    #[test]
    fn test_resource_config_serialize_roundtrip() {
        let config = ResourceConfig::default();
        let toml = toml::to_string(&config).unwrap();
        let deserialized: ResourceConfig = toml::from_str(&toml).unwrap();
        assert_eq!(config.memory_bytes, deserialized.memory_bytes);
    }
}
