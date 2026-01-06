# Configuration

Duende uses a structured configuration system with sensible defaults and validation.

## DaemonConfig

The main configuration structure:

```rust
pub struct DaemonConfig {
    pub name: String,              // Daemon identifier
    pub version: String,           // Version (semver)
    pub description: String,       // Human-readable description
    pub binary_path: PathBuf,      // Path to daemon binary
    pub config_path: Option<PathBuf>,
    pub args: Vec<String>,         // Command-line arguments
    pub env: HashMap<String, String>, // Environment variables
    pub user: Option<String>,      // Unix user
    pub group: Option<String>,     // Unix group
    pub working_dir: Option<PathBuf>,
    pub resources: ResourceConfig, // Resource limits
    pub health_check: HealthCheckConfig,
    pub restart: RestartPolicy,
    pub shutdown_timeout: Duration,
    pub platform: PlatformConfig,
}
```

## ResourceConfig

Resource limits including memory locking:

```rust
pub struct ResourceConfig {
    pub memory_bytes: u64,         // Memory limit (default: 512MB)
    pub memory_swap_bytes: u64,    // Memory + swap limit (default: 1GB)
    pub cpu_quota_percent: f64,    // CPU quota (default: 100%)
    pub cpu_shares: u64,           // CPU shares (default: 1024)
    pub io_read_bps: u64,          // I/O read limit
    pub io_write_bps: u64,         // I/O write limit
    pub pids_max: u64,             // Max processes (default: 100)
    pub open_files_max: u64,       // Max FDs (default: 1024)
    pub lock_memory: bool,         // Enable mlock (default: false)
    pub lock_memory_required: bool, // Fail if mlock fails (default: false)
}
```

## TOML Configuration

Load configuration from a TOML file:

```toml
name = "my-daemon"
version = "1.0.0"
description = "My awesome daemon"
binary_path = "/usr/bin/my-daemon"

[resources]
memory_bytes = 536870912  # 512MB
cpu_quota_percent = 200.0  # 2 cores
lock_memory = true
lock_memory_required = true

[health_check]
enabled = true
interval = "30s"
timeout = "10s"
retries = 3

[restart]
policy = "on-failure"

[platform]
# Linux-specific
# container_image = "my-daemon:latest"
```

Load in code:

```rust
let config = DaemonConfig::load("daemon.toml")?;
config.validate()?;
```

## Restart Policies

| Policy | Behavior |
|--------|----------|
| `never` | Never restart |
| `on-failure` | Restart only on non-zero exit |
| `always` | Always restart |
| `unless-stopped` | Restart unless manually stopped |

See [DaemonManager](./api.md) for advanced restart policies with backoff.
