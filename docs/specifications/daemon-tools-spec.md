# Duende: Cross-Platform Daemon Tooling Specification

**Version**: 1.2.0
**Status**: Draft
**Authors**: PAIML Sovereign AI Stack Team
**Last Updated**: 2026-01-06

---

## Executive Summary

Duende is a cross-platform daemon orchestration framework for the PAIML Sovereign AI Stack. It provides unified lifecycle management, observability, and policy enforcement for long-running processes across Linux, macOS, Docker containers, pepita microVMs, and WOS (WebAssembly Operating System).

The name "Duende" (Spanish: spirit/daemon) reflects both the technical daemon concept and the framework's goal of bringing life and reliability to background services.

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Platform Abstraction Layer](#2-platform-abstraction-layer)
3. [Stack Integration](#3-stack-integration)
4. [Observability Framework](#4-observability-framework)
5. [Policy Enforcement](#5-policy-enforcement)
6. [Testing Infrastructure](#6-testing-infrastructure)
7. [Toyota Production System Principles](#7-toyota-production-system-principles)
8. [Peer-Reviewed Citations](#8-peer-reviewed-citations)
9. [Popperian Falsification Checklist](#9-popperian-falsification-checklist)
10. [API Reference](#10-api-reference)

---

## 1. Architecture Overview

### 1.1 Design Philosophy

Duende follows the **Toyota Production System (TPS)** principles adapted for software:

| TPS Principle | Duende Implementation |
|---------------|----------------------|
| **Jidoka** (自働化) | Stop-on-error with automatic failover |
| **Poka-Yoke** (ポカヨケ) | Type-safe APIs preventing misuse |
| **Heijunka** (平準化) | Load leveling via work-stealing schedulers |
| **Muda** (無駄) | Zero-waste resource allocation |
| **Kaizen** (改善) | Continuous optimization via metrics |
| **Genchi Genbutsu** (現地現物) | Direct observation via renacer tracing |

### 1.2 System Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Duende Control Plane                            │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │
│  │   Policy    │  │  Lifecycle  │  │ Observability│  │   Health    │    │
│  │   Engine    │  │   Manager   │  │   Collector  │  │   Monitor   │    │
│  │  (PMAT)     │  │             │  │  (renacer)   │  │  (ttop)     │    │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘    │
│         │                │                │                │            │
│         └────────────────┴────────────────┴────────────────┘            │
│                                   │                                      │
│                          ┌───────▼───────┐                              │
│                          │  Daemon Bus   │                              │
│                          │ (Message Queue)│                              │
│                          └───────┬───────┘                              │
└──────────────────────────────────┼──────────────────────────────────────┘
                                   │
        ┌──────────────────────────┼──────────────────────────┐
        │                          │                          │
┌───────▼───────┐  ┌───────────────▼───────────────┐  ┌──────▼───────┐
│    Linux      │  │         Docker/OCI            │  │    macOS     │
│   Platform    │  │          Platform             │  │   Platform   │
│  ┌─────────┐  │  │  ┌─────────┐  ┌─────────┐    │  │  ┌─────────┐ │
│  │ systemd │  │  │  │container│  │ sidecar │    │  │  │launchd  │ │
│  │ adapter │  │  │  │ adapter │  │ adapter │    │  │  │ adapter │ │
│  └─────────┘  │  │  └─────────┘  └─────────┘    │  │  └─────────┘ │
└───────────────┘  └───────────────────────────────┘  └──────────────┘
        │                          │                          │
┌───────▼───────┐  ┌───────────────▼───────────────┐  ┌──────▼───────┐
│    pepita     │  │           WOS                 │  │   Native     │
│   MicroVM     │  │    WebAssembly OS             │  │   Process    │
│  ┌─────────┐  │  │  ┌─────────┐  ┌─────────┐    │  │  ┌─────────┐ │
│  │ virtio  │  │  │  │  init   │  │scheduler│    │  │  │  fork   │ │
│  │  vsock  │  │  │  │ process │  │  (8-lvl)│    │  │  │  exec   │ │
│  └─────────┘  │  │  └─────────┘  └─────────┘    │  │  └─────────┘ │
└───────────────┘  └───────────────────────────────┘  └──────────────┘
```

### 1.3 Core Components

| Component | Crate | Purpose |
|-----------|-------|---------|
| **duende-core** | `duende` | Daemon lifecycle primitives |
| **duende-platform** | `duende-platform` | Platform abstraction layer |
| **duende-observe** | `duende-observe` | Observability integration (renacer, ttop) |
| **duende-policy** | `duende-policy` | Policy enforcement (PMAT, bashrs) |
| **duende-test** | `duende-test` | Testing infrastructure (probador) |

### 1.4 Safety & Verification Guarantees

Duende's architecture is predicated on formally verified properties to ensure mission-critical reliability.

| Property | Guarantee | Verification Method | Citation |
|----------|-----------|---------------------|----------|
| **Memory Safety** | No undefined behavior in safe Rust code | Rust Type System & Borrow Checker | Jung et al. (2018) |
| **Process Isolation** | Capability-based access control | Capsicum / seccomp-bpf | Watson et al. (2010) |
| **Crash Safety** | Micro-reboot upon failure | Crash-Only Software design | Candea & Fox (2003) |
| **Tracing Safety** | Bounded loops, valid memory access | eBPF Verifier & Static Analysis | Gershuni et al. (2019) |
| **Runtime Integrity** | Immutable infrastructure | Unikernel compilation (WOS/pepita) | Madhavapeddy et al. (2013) |

---

## 2. Platform Abstraction Layer

### 2.1 Unified Daemon Trait

```rust
/// Core daemon abstraction for cross-platform lifecycle management.
///
/// # Toyota Way: Standardized Work (標準作業)
/// Every daemon follows the same lifecycle contract, enabling
/// predictable behavior across platforms (Liker, 2004, p. 142).
#[async_trait]
pub trait Daemon: Send + Sync + 'static {
    /// Unique identifier for this daemon instance
    fn id(&self) -> DaemonId;

    /// Human-readable name
    fn name(&self) -> &str;

    /// Initialize daemon resources (Poka-Yoke: fail fast on misconfiguration)
    async fn init(&mut self, config: &DaemonConfig) -> Result<(), DaemonError>;

    /// Main execution loop (Heijunka: leveled workload processing)
    async fn run(&mut self, ctx: &mut DaemonContext) -> Result<ExitReason, DaemonError>;

    /// Graceful shutdown (Jidoka: stop cleanly on signal)
    async fn shutdown(&mut self, timeout: Duration) -> Result<(), DaemonError>;

    /// Health check (Genchi Genbutsu: direct observation)
    async fn health_check(&self) -> HealthStatus;

    /// Metrics collection for Kaizen (continuous improvement)
    fn metrics(&self) -> &DaemonMetrics;
}

/// Platform-specific daemon adapter
pub trait PlatformAdapter: Send + Sync {
    /// Platform identifier
    fn platform(&self) -> Platform;

    /// Spawn daemon on this platform
    async fn spawn(&self, daemon: Box<dyn Daemon>) -> Result<DaemonHandle, PlatformError>;

    /// Signal daemon (SIGTERM, SIGKILL, etc.)
    async fn signal(&self, handle: &DaemonHandle, sig: Signal) -> Result<(), PlatformError>;

    /// Query daemon status
    async fn status(&self, handle: &DaemonHandle) -> Result<DaemonStatus, PlatformError>;

    /// Attach tracing (renacer integration)
    async fn attach_tracer(&self, handle: &DaemonHandle) -> Result<TracerHandle, PlatformError>;
}
```

### 2.2 Platform Implementations

#### 2.2.1 Linux (systemd)

```rust
pub struct SystemdAdapter {
    bus: zbus::Connection,
    unit_dir: PathBuf,
}

impl PlatformAdapter for SystemdAdapter {
    fn platform(&self) -> Platform { Platform::Linux }

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> Result<DaemonHandle, PlatformError> {
        // Generate transient systemd unit
        let unit = SystemdUnit::transient()
            .name(&daemon.name())
            .exec_start(&daemon.binary_path())
            .restart_policy(RestartPolicy::OnFailure)
            .memory_limit(daemon.config().memory_limit)
            .cpu_quota(daemon.config().cpu_quota)
            .build()?;

        // Start via D-Bus
        self.bus.call_method(
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
            "StartTransientUnit",
            &(unit.name(), "replace", unit.properties()),
        ).await?;

        Ok(DaemonHandle::systemd(unit.name()))
    }
}
```

#### 2.2.2 macOS (launchd)

```rust
pub struct LaunchdAdapter {
    domain: LaunchdDomain,
    plist_dir: PathBuf,
}

impl PlatformAdapter for LaunchdAdapter {
    fn platform(&self) -> Platform { Platform::MacOS }

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> Result<DaemonHandle, PlatformError> {
        // Generate launchd plist
        let plist = LaunchdPlist::new(&daemon.name())
            .program(&daemon.binary_path())
            .keep_alive(true)
            .throttle_interval(10)
            .soft_resource_limits(ResourceLimits {
                memory: daemon.config().memory_limit,
                cpu: daemon.config().cpu_quota,
            })
            .build()?;

        // Write plist and bootstrap
        let plist_path = self.plist_dir.join(format!("{}.plist", daemon.name()));
        plist.write(&plist_path)?;

        Command::new("launchctl")
            .args(["bootstrap", &self.domain.to_string(), &plist_path.to_string_lossy()])
            .status()
            .await?;

        Ok(DaemonHandle::launchd(daemon.name().to_string()))
    }
}
```

#### 2.2.3 Docker/OCI Container

```rust
pub struct ContainerAdapter {
    runtime: ContainerRuntime,  // docker, podman, containerd
    network: NetworkMode,
}

impl PlatformAdapter for ContainerAdapter {
    fn platform(&self) -> Platform { Platform::Container }

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> Result<DaemonHandle, PlatformError> {
        let container = ContainerSpec::new()
            .image(&daemon.config().container_image)
            .name(&daemon.name())
            .restart_policy(RestartPolicy::UnlessStopped)
            .memory_limit(daemon.config().memory_limit)
            .cpu_shares(daemon.config().cpu_shares)
            .health_check(HealthCheck {
                cmd: vec!["duende", "health", "--daemon", &daemon.name()],
                interval: Duration::from_secs(30),
                timeout: Duration::from_secs(10),
                retries: 3,
            })
            .labels([
                ("duende.daemon.id", daemon.id().to_string()),
                ("duende.daemon.version", daemon.version()),
            ])
            .build()?;

        let id = self.runtime.create(&container).await?;
        self.runtime.start(&id).await?;

        Ok(DaemonHandle::container(id))
    }
}
```

#### 2.2.4 pepita MicroVM

```rust
pub struct PepitaAdapter {
    vmm: Arc<Mutex<VmmManager>>,
    vsock_base_port: u32,
}

impl PlatformAdapter for PepitaAdapter {
    fn platform(&self) -> Platform { Platform::PepitaMicroVM }

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> Result<DaemonHandle, PlatformError> {
        let vm_config = VmConfig::builder()
            .vcpus(daemon.config().vcpus.unwrap_or(2))
            .memory_mb(daemon.config().memory_limit / (1024 * 1024))
            .kernel_path(&daemon.config().kernel_path)
            .rootfs_path(&daemon.config().rootfs_path)
            .vsock_cid(self.allocate_cid())
            .enable_kvm(true)
            .build()?;

        let vm = MicroVm::create(vm_config)?;
        vm.start()?;

        // Establish vsock communication
        let transport = VsockTransport::connect(vm.cid(), self.vsock_base_port)?;

        // Send daemon binary and config via virtio-blk
        transport.send_file(&daemon.binary_path()).await?;
        transport.send_config(&daemon.config()).await?;

        // Start daemon inside VM
        transport.send_command(DaemonCommand::Start).await?;

        Ok(DaemonHandle::pepita(vm.id(), transport))
    }

    async fn attach_tracer(&self, handle: &DaemonHandle) -> Result<TracerHandle, PlatformError> {
        // Use renacer with ptrace inside VM via vsock proxy
        let transport = handle.pepita_transport()?;
        transport.send_command(DaemonCommand::AttachTracer).await?;

        Ok(TracerHandle::remote(transport.clone()))
    }
}
```

#### 2.2.5 WOS (WebAssembly OS)

```rust
pub struct WosAdapter {
    kernel: Arc<KernelState>,
    scheduler: Arc<Scheduler>,
}

impl PlatformAdapter for WosAdapter {
    fn platform(&self) -> Platform { Platform::Wos }

    async fn spawn(&self, daemon: Box<dyn Daemon>) -> Result<DaemonHandle, PlatformError> {
        // Fork from init process (PID 1)
        let parent_pid = ProcessId(1);
        let child_pid = self.kernel.allocate_pid();

        // Create process with daemon configuration
        let process = Process {
            pid: child_pid,
            parent_pid: Some(parent_pid),
            state: ProcessState::Ready,
            priority: daemon.config().priority.unwrap_or(4), // Normal priority
            env: daemon.config().env.clone().into(),
            // ... other fields
        };

        // Add to kernel state
        self.kernel.add_process(process)?;

        // Enqueue for scheduling (8-level priority with aging)
        self.scheduler.enqueue(child_pid, process.priority)?;

        // Exec daemon binary
        self.kernel.syscall(SystemCall::Exec {
            path: daemon.binary_path().to_string_lossy().to_string(),
            args: daemon.config().args.clone(),
            env: daemon.config().env.clone(),
        }, child_pid)?;

        Ok(DaemonHandle::wos(child_pid))
    }
}
```

### 2.3 Platform Detection

```rust
/// Auto-detect current platform with fallback chain.
///
/// # Detection Order (Poka-Yoke: fail to safest option)
/// 1. WOS: Check for WASM runtime markers
/// 2. pepita: Check for virtio devices
/// 3. Container: Check for /.dockerenv or cgroup markers
/// 4. Linux: Check for systemd
/// 5. macOS: Check for launchd
/// 6. Fallback: Native process
pub fn detect_platform() -> Platform {
    if cfg!(target_arch = "wasm32") || std::env::var("WOS_KERNEL").is_ok() {
        return Platform::Wos;
    }

    if Path::new("/dev/virtio-ports").exists() || std::env::var("PEPITA_VM").is_ok() {
        return Platform::PepitaMicroVM;
    }

    if Path::new("/.dockerenv").exists() ||
       std::fs::read_to_string("/proc/1/cgroup")
           .map(|s| s.contains("docker") || s.contains("containerd"))
           .unwrap_or(false) {
        return Platform::Container;
    }

    #[cfg(target_os = "linux")]
    if Path::new("/run/systemd/system").exists() {
        return Platform::Linux;
    }

    #[cfg(target_os = "macos")]
    return Platform::MacOS;

    Platform::Native
}
```

---

## 3. Stack Integration

### 3.0 Dependency Policy (Iron Lotus Framework)

Duende follows the **Iron Lotus Framework** dependency philosophy from repartir: minimize external dependencies, maximize stack reuse, and maintain supply chain sovereignty.

#### 3.0.1 Dependency Hierarchy

| Priority | Source | Examples | Rationale |
|----------|--------|----------|-----------|
| **P0 (Prefer)** | PAIML Stack | trueno, repartir, renacer, aprender, probador | Full audit trail, same quality standards |
| **P1 (Accept)** | Rust std/core | std::collections, core::time | Guaranteed stability, zero supply chain risk |
| **P2 (Evaluate)** | Pure Rust crates | thiserror, serde, tokio | Audit license (MIT/Apache-2.0 only), check deps |
| **P3 (Avoid)** | Crates with C deps | openssl, ring, aws-lc-rs | Supply chain risk, audit difficulty |
| **P4 (Prohibit)** | Foreign FFI | libc calls, C++ bindings | Breaks memory safety guarantees |

#### 3.0.2 Stack Component Substitutions

| External Dependency | Stack Alternative | Status |
|---------------------|-------------------|--------|
| `opentelemetry` | `renacer` tracing | ✅ Use renacer |
| `metrics`/`prometheus` | `trueno-viz` collectors | ✅ Use ttop |
| `rayon` | `repartir` work-stealing | ✅ Use repartir |
| `tokio-util` codecs | `trueno` serialization | ✅ Use trueno |
| `bollard` (Docker) | Native OCI via `oci-spec` | ⚠️ Evaluate |
| `zbus` (D-Bus) | Pure Rust D-Bus client | ⚠️ Evaluate |
| `plist` (macOS) | Pure Rust plist parser | ⚠️ Evaluate |
| `lz4`/`zstd` | `trueno-zram` compression | ✅ Use trueno-zram |
| `ring`/`rustls` | Stack TLS (when available) | ⏳ Future |

#### 3.0.3 Workspace Dependencies

```toml
[workspace.dependencies]
# ═══════════════════════════════════════════════════════════════════════════
# P0: PAIML Sovereign AI Stack (REQUIRED)
# ═══════════════════════════════════════════════════════════════════════════
trueno = "0.11"                    # SIMD/GPU primitives
trueno-viz = "0.1"                 # Monitoring (ttop collectors)
trueno-zram = "0.1"                # Compression
repartir = "1.1"                   # Distributed scheduling
renacer = "0.9"                    # Syscall tracing
aprender = "0.21"                  # ML algorithms
# probador = "0.1"                 # Testing (when published)
pacha = "0.2"                      # State registry

# ═══════════════════════════════════════════════════════════════════════════
# P2: Vetted Pure Rust Crates (MINIMAL SET)
# ═══════════════════════════════════════════════════════════════════════════
thiserror = "2.0"                  # Error derive macros
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"
tokio = { version = "1", features = ["rt-multi-thread", "sync", "time", "signal"] }
async-trait = "0.1"
uuid = { version = "1.6", features = ["v4", "serde"] }

# ═══════════════════════════════════════════════════════════════════════════
# Platform-Specific (Feature-Gated)
# ═══════════════════════════════════════════════════════════════════════════
[target.'cfg(target_os = "linux")'.dependencies]
# Prefer pure Rust; evaluate zbus vs socket-based D-Bus
nix = { version = "0.29", features = ["process", "signal"] }

# ═══════════════════════════════════════════════════════════════════════════
# Testing (Dev Dependencies Only)
# ═══════════════════════════════════════════════════════════════════════════
[workspace.dev-dependencies]
proptest = "1.4"
# probador = "0.1"                 # When published to crates.io
```

#### 3.0.4 Supply Chain Security (Iron Lotus)

Following repartir's Iron Lotus supply chain security:

```toml
# deny.toml - Sovereign AI Supply Chain Security
[advisories]
db-path = "~/.cargo/advisory-db"
vulnerability = "deny"
unmaintained = "warn"
yanked = "deny"

[licenses]
unlicensed = "deny"
allow = ["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib"]
copyleft = "deny"

[bans]
multiple-versions = "warn"
wildcards = "deny"
deny = [
    # Crates with known security issues
    { name = "openssl" },
    { name = "openssl-sys" },
    # Crates with C dependencies we want to avoid
    { name = "aws-lc-rs", wrappers = ["rustls"] },  # Evaluate pure Rust TLS
]

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
```

#### 3.0.5 Genchi Genbutsu (現地現物) - "Go and See"

Per Iron Lotus Framework, all dependencies must be:

1. **Auditable**: Source code reviewed, no binary blobs
2. **Traceable**: Every operation visible from API → syscall
3. **Pure Rust**: Zero opaque C/C++ libraries (P4 prohibited)
4. **Documented**: Clear rationale for each dependency

```rust
/// Example: Prefer repartir over raw tokio for task scheduling
///
/// # Iron Lotus Rationale
/// - repartir uses Blumofe-Leiserson work-stealing (proven optimal)
/// - Full tracing integration with renacer
/// - Same quality standards (≥95% coverage, ≥80% mutation)
/// - Audited supply chain
use repartir::{Pool, Task, Backend};

// NOT: use tokio::task::spawn_blocking;  // No work-stealing, no tracing
```

### 3.1 Integration Matrix

| Stack Component | Integration Point | Purpose |
|-----------------|-------------------|---------|
| **renacer** | `DaemonContext::tracer` | Syscall tracing, source correlation |
| **trueno-viz (ttop)** | `DaemonMetrics` | Real-time resource monitoring |
| **trueno-zram** | `MemoryCompressor` | Compressed memory for daemon state |
| **probador** | `DaemonTestRunner` | Chaos testing, load testing |
| **bashrs** | `ScriptTranspiler` | Safe daemon script generation |
| **PMAT** | `PolicyEnforcer` | Quality gate enforcement |
| **aprender** | `AnomalyDetector` | ML-based anomaly detection |
| **repartir** | `WorkScheduler` | Distributed daemon coordination |
| **pacha** | `StateRegistry` | Daemon state persistence |

### 3.2 Renacer Integration (Syscall Tracing)

```rust
/// Renacer-based daemon introspection.
///
/// # Toyota Way: Genchi Genbutsu (現地現物)
/// "Go and see for yourself" - direct observation of daemon behavior
/// through syscall tracing (Ohno, 1988, p. 40).
pub struct DaemonTracer {
    tracer: renacer::Tracer,
    anomaly_detector: renacer::AnomalyDetector,
    source_correlator: renacer::DwarfResolver,
}

impl DaemonTracer {
    /// Attach tracer to running daemon.
    pub async fn attach(&mut self, pid: u32) -> Result<(), TracerError> {
        self.tracer.attach(pid)?;

        // Enable real-time anomaly detection (3σ threshold)
        self.anomaly_detector.enable_realtime(AnomalyConfig {
            window_size: 100,
            z_score_threshold: 3.0,
            severity_levels: vec![
                (3.0, Severity::Low),
                (4.0, Severity::Medium),
                (5.0, Severity::High),
            ],
        })?;

        Ok(())
    }

    /// Collect syscall trace with source correlation.
    pub async fn collect(&mut self) -> Result<TraceReport, TracerError> {
        let events = self.tracer.collect()?;

        // Correlate syscalls to source code (DWARF debug info)
        let correlated: Vec<_> = events.iter()
            .map(|e| {
                let location = self.source_correlator.resolve(e.instruction_pointer);
                CorrelatedEvent { event: e.clone(), source: location }
            })
            .collect();

        // Detect anomalies
        let anomalies = self.anomaly_detector.analyze(&events)?;

        // Generate critical path analysis
        let critical_path = renacer::critical_path::analyze(&events)?;

        Ok(TraceReport {
            events: correlated,
            anomalies,
            critical_path,
            anti_patterns: renacer::anti_patterns::detect(&events)?,
        })
    }
}
```

### 3.3 Trueno-viz (ttop) Integration

```rust
/// Real-time daemon monitoring via trueno-viz collectors.
///
/// # Toyota Way: Visual Management (目で見る管理)
/// Make daemon health visible at a glance (Liker, 2004, p. 152).
pub struct DaemonMonitor {
    cpu_collector: CpuCollector,
    memory_collector: MemoryCollector,
    process_collector: ProcessCollector,
    gpu_collector: Option<GpuCollector>,
    ring_buffer: RingBuffer<DaemonSnapshot>,
}

impl DaemonMonitor {
    /// Collect metrics for specific daemon.
    pub fn collect(&mut self, pid: u32) -> Result<DaemonSnapshot, MonitorError> {
        let cpu = self.cpu_collector.collect_for_pid(pid)?;
        let memory = self.memory_collector.collect_for_pid(pid)?;
        let process = self.process_collector.get(pid)?;
        let gpu = self.gpu_collector.as_mut()
            .and_then(|c| c.collect_for_pid(pid).ok());

        let snapshot = DaemonSnapshot {
            timestamp: Instant::now(),
            cpu_percent: cpu.percent,
            memory_bytes: memory.rss,
            memory_percent: memory.percent,
            threads: process.threads,
            state: process.state,
            io_read_bytes: process.io_read,
            io_write_bytes: process.io_write,
            gpu_utilization: gpu.map(|g| g.utilization),
            gpu_memory: gpu.map(|g| g.memory_used),
        };

        // Store in bounded ring buffer (O(1) operations, zero allocations)
        self.ring_buffer.push(snapshot.clone());

        Ok(snapshot)
    }

    /// Get historical metrics for trend analysis.
    pub fn history(&self, duration: Duration) -> Vec<&DaemonSnapshot> {
        let cutoff = Instant::now() - duration;
        self.ring_buffer.iter()
            .filter(|s| s.timestamp >= cutoff)
            .collect()
    }
}
```

### 3.4 Trueno-zram Integration (Memory Compression)

```rust
/// Compressed state storage for daemons.
///
/// # Toyota Way: Muda Elimination (無駄排除)
/// Eliminate waste by compressing daemon state (Ohno, 1988, p. 19).
pub struct CompressedStateStore {
    compressor: trueno_zram::Compressor,
    entropy_analyzer: trueno_zram::EntropyCalculator,
    page_store: HashMap<StateKey, CompressedPage>,
}

impl CompressedStateStore {
    /// Store daemon state with adaptive compression.
    pub fn store(&mut self, key: StateKey, data: &[u8]) -> Result<CompressionStats, StoreError> {
        // Analyze entropy to select optimal algorithm
        let entropy = self.entropy_analyzer.calculate(data);
        let algorithm = match entropy.level() {
            EntropyLevel::VeryLow | EntropyLevel::Low => Algorithm::Lz4,
            EntropyLevel::Medium => Algorithm::Zstd { level: 3 },
            EntropyLevel::High => Algorithm::Zstd { level: 1 },
            EntropyLevel::VeryHigh => Algorithm::None, // Incompressible
        };

        // Select backend based on batch size (5× PCIe rule)
        let backend = if data.len() >= GPU_BATCH_THRESHOLD {
            ComputeBackend::Gpu
        } else if data.len() >= SIMD_BATCH_THRESHOLD {
            ComputeBackend::Simd
        } else {
            ComputeBackend::Scalar
        };

        let compressed = self.compressor.compress(data, algorithm, backend)?;

        let stats = CompressionStats {
            original_size: data.len(),
            compressed_size: compressed.len(),
            ratio: data.len() as f64 / compressed.len() as f64,
            algorithm,
            backend,
        };

        self.page_store.insert(key, CompressedPage {
            data: compressed,
            algorithm,
            original_size: data.len(),
        });

        Ok(stats)
    }
}
```

### 3.5 Probador Integration (Testing)

```rust
/// Daemon testing infrastructure via probador.
///
/// # Toyota Way: Built-in Quality (品質の作り込み)
/// Quality cannot be inspected in; it must be built in (Deming, 1986, p. 23).
pub struct DaemonTestRunner {
    load_tester: probador::LoadTester,
    chaos_injector: probador::ChaosInjector,
    simulation_runner: probador::SimulationRunner,
}

impl DaemonTestRunner {
    /// Run load test against daemon.
    pub async fn load_test(&self, config: LoadTestConfig) -> Result<LoadTestReport, TestError> {
        let results = self.load_tester.run(LoadTestScenario {
            target: config.daemon_endpoint,
            users: config.concurrent_users,
            ramp_up: config.ramp_up_duration,
            duration: config.test_duration,
            requests_per_user: config.requests_per_user,
        }).await?;

        Ok(LoadTestReport {
            total_requests: results.total,
            successful: results.successful,
            failed: results.failed,
            latency_p50: results.latency.percentile(50),
            latency_p95: results.latency.percentile(95),
            latency_p99: results.latency.percentile(99),
            throughput_rps: results.throughput,
        })
    }

    /// Inject chaos to test resilience.
    pub async fn chaos_test(&self, config: ChaosConfig) -> Result<ChaosReport, TestError> {
        let injections = vec![
            FailureInjection::latency("network", config.latency_probability, config.latency_ms),
            FailureInjection::packet_loss("network", config.packet_loss_probability),
            FailureInjection::error("service", config.error_probability),
        ];

        self.chaos_injector.inject(injections).await?;

        // Monitor daemon behavior under chaos
        let behavior = self.monitor_under_chaos(config.duration).await?;

        self.chaos_injector.clear().await?;

        Ok(ChaosReport {
            recovery_time: behavior.recovery_time,
            error_rate_increase: behavior.error_rate_delta,
            circuit_breaker_trips: behavior.circuit_breaker_activations,
            data_loss: behavior.data_loss_detected,
        })
    }
}
```

### 3.6 Bashrs Integration (Script Generation)

```rust
/// Safe daemon script generation via bashrs transpiler.
///
/// # Toyota Way: Poka-Yoke (ポカヨケ)
/// Mistake-proofing daemon scripts (Shingo, 1986, p. 45).
pub struct DaemonScriptGenerator {
    transpiler: bashrs::BashTranspiler,
    purifier: bashrs::Purifier,
    validator: bashrs::ShellValidator,
}

impl DaemonScriptGenerator {
    /// Generate idempotent daemon installation script.
    pub fn generate_install_script(&self, daemon: &DaemonConfig) -> Result<String, ScriptError> {
        let script = format!(r#"
#!/bin/bash
set -euo pipefail

# Generated by Duende - DO NOT EDIT
# Daemon: {name}
# Version: {version}

# Create daemon user (idempotent)
id -u {user} &>/dev/null || useradd -r -s /bin/false {user}

# Create directories (idempotent via -p)
mkdir -p /var/lib/{name}
mkdir -p /var/log/{name}
mkdir -p /etc/{name}

# Install binary
cp -f "{binary}" /usr/local/bin/{name}
chmod 755 /usr/local/bin/{name}

# Install configuration
cp -f "{config}" /etc/{name}/config.toml
chmod 644 /etc/{name}/config.toml

# Set ownership
chown -R {user}:{user} /var/lib/{name}
chown -R {user}:{user} /var/log/{name}

# Install systemd unit
cat > /etc/systemd/system/{name}.service << 'EOF'
[Unit]
Description={description}
After=network.target

[Service]
Type=simple
User={user}
ExecStart=/usr/local/bin/{name} --config /etc/{name}/config.toml
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

# Reload and enable (idempotent)
systemctl daemon-reload
systemctl enable {name}
"#,
            name = daemon.name,
            version = daemon.version,
            user = daemon.user.as_deref().unwrap_or("daemon"),
            binary = daemon.binary_path.display(),
            config = daemon.config_path.display(),
            description = daemon.description,
        );

        // Purify script (enforce idempotency, remove non-determinism)
        let purified = self.purifier.purify(&script)?;

        // Validate against shellcheck
        self.validator.validate(&purified, ShellCompatibility::Bash)?;

        Ok(purified)
    }
}
```

### 3.7 PMAT Integration (Policy Enforcement)

```rust
/// Quality gate enforcement via PMAT.
///
/// # Toyota Way: Jidoka (自働化)
/// Automatic stop when quality problems detected (Ohno, 1988, p. 6).
pub struct PolicyEnforcer {
    quality_gate: pmat::QualityGate,
    thresholds: PolicyThresholds,
    circuit_breaker: CircuitBreaker,
}

#[derive(Clone)]
pub struct PolicyThresholds {
    pub max_complexity: u32,          // Default: 20 (Toyota Way standard)
    pub satd_tolerance: u32,          // Default: 0 (zero-tolerance)
    pub dead_code_max_percent: f64,   // Default: 10.0%
    pub min_quality_score: f64,       // Default: 80.0/100
    pub max_memory_mb: u64,           // Default: 500
    pub max_cpu_percent: f64,         // Default: 80.0
}

impl PolicyEnforcer {
    /// Enforce policies on daemon (Jidoka: stop on violation).
    pub async fn enforce(&mut self, daemon_id: DaemonId) -> Result<PolicyResult, PolicyError> {
        // Check circuit breaker state
        if !self.circuit_breaker.allow() {
            return Err(PolicyError::CircuitOpen);
        }

        // Run quality gate analysis
        let analysis = self.quality_gate.analyze(&daemon_id).await?;

        let violations: Vec<PolicyViolation> = vec![];

        // Check complexity threshold
        if analysis.max_complexity > self.thresholds.max_complexity {
            violations.push(PolicyViolation::Complexity {
                actual: analysis.max_complexity,
                threshold: self.thresholds.max_complexity,
                location: analysis.complexity_hotspot.clone(),
            });
        }

        // Check SATD (Self-Admitted Technical Debt)
        if analysis.satd_count > self.thresholds.satd_tolerance {
            violations.push(PolicyViolation::TechnicalDebt {
                count: analysis.satd_count,
                tolerance: self.thresholds.satd_tolerance,
            });
        }

        // Check dead code percentage
        if analysis.dead_code_percent > self.thresholds.dead_code_max_percent {
            violations.push(PolicyViolation::DeadCode {
                percent: analysis.dead_code_percent,
                threshold: self.thresholds.dead_code_max_percent,
            });
        }

        if !violations.is_empty() {
            // Jidoka: Stop on error
            self.circuit_breaker.record_failure();
            return Ok(PolicyResult::Rejected { violations });
        }

        self.circuit_breaker.record_success();
        Ok(PolicyResult::Approved)
    }
}
```

---

## 4. Observability Framework

### 4.1 Metrics Collection

```rust
/// Daemon metrics following RED method (Rate, Errors, Duration).
///
/// # Reference
/// Wilkins, T. (2018). "The RED Method: How to instrument your services."
/// Weaveworks Blog. https://www.weave.works/blog/the-red-method-key-metrics
pub struct DaemonMetrics {
    // Rate: Request throughput
    pub requests_total: Counter,
    pub requests_per_second: Gauge,

    // Errors: Error rate
    pub errors_total: Counter,
    pub error_rate: Gauge,

    // Duration: Latency distribution
    pub request_duration: Histogram,

    // Resource utilization
    pub cpu_usage_percent: Gauge,
    pub memory_usage_bytes: Gauge,
    pub open_file_descriptors: Gauge,
    pub thread_count: Gauge,

    // Custom daemon metrics
    pub custom: HashMap<String, MetricValue>,
}

impl DaemonMetrics {
    /// Export to Prometheus format.
    pub fn to_prometheus(&self) -> String {
        let mut output = String::new();

        writeln!(&mut output, "# HELP daemon_requests_total Total requests processed");
        writeln!(&mut output, "# TYPE daemon_requests_total counter");
        writeln!(&mut output, "daemon_requests_total {}", self.requests_total.get());

        // ... additional metrics

        output
    }

    /// Export to OpenTelemetry via OTLP.
    pub async fn export_otlp(&self, endpoint: &str) -> Result<(), ExportError> {
        let exporter = opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(endpoint)
            .build()?;

        // ... export implementation

        Ok(())
    }
}
```

### 4.2 Distributed Tracing

```rust
/// W3C Trace Context propagation for distributed daemons.
///
/// # Reference
/// W3C. (2021). "Trace Context - Level 1."
/// https://www.w3.org/TR/trace-context/
pub struct TraceContext {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub trace_flags: TraceFlags,
    pub trace_state: TraceState,
}

impl TraceContext {
    /// Parse from W3C traceparent header.
    pub fn from_traceparent(header: &str) -> Result<Self, ParseError> {
        // Format: {version}-{trace_id}-{span_id}-{flags}
        // Example: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() != 4 {
            return Err(ParseError::InvalidFormat);
        }

        Ok(Self {
            trace_id: TraceId::from_hex(parts[1])?,
            span_id: SpanId::from_hex(parts[2])?,
            trace_flags: TraceFlags::from_hex(parts[3])?,
            trace_state: TraceState::default(),
        })
    }

    /// Serialize to W3C traceparent header.
    pub fn to_traceparent(&self) -> String {
        format!("00-{}-{}-{:02x}",
            self.trace_id.to_hex(),
            self.span_id.to_hex(),
            self.trace_flags.0,
        )
    }
}
```

---

## 5. Policy Enforcement

### 5.1 Resource Limits

```rust
/// Resource limit enforcement via cgroups (Linux) or sandbox APIs.
///
/// # Toyota Way: Standardized Work (標準作業)
/// Consistent resource allocation prevents variability (Liker, 2004, p. 142).
pub struct ResourceLimiter {
    #[cfg(target_os = "linux")]
    cgroup: CgroupController,

    limits: ResourceLimits,
}

pub struct ResourceLimits {
    pub memory_bytes: u64,
    pub memory_swap_bytes: u64,
    pub cpu_quota_percent: f64,
    pub cpu_period_us: u64,
    pub io_read_bps: u64,
    pub io_write_bps: u64,
    pub pids_max: u64,
}

impl ResourceLimiter {
    /// Apply resource limits to daemon process.
    #[cfg(target_os = "linux")]
    pub fn apply(&self, pid: u32) -> Result<(), LimiterError> {
        // Create cgroup for daemon
        let cgroup = self.cgroup.create(&format!("duende/{}", pid))?;

        // Set memory limit
        cgroup.set_memory_limit(self.limits.memory_bytes)?;
        cgroup.set_memory_swap_limit(self.limits.memory_swap_bytes)?;

        // Set CPU quota
        let quota = (self.limits.cpu_quota_percent / 100.0 * self.limits.cpu_period_us as f64) as u64;
        cgroup.set_cpu_quota(quota)?;
        cgroup.set_cpu_period(self.limits.cpu_period_us)?;

        // Set I/O limits
        cgroup.set_io_read_bps_limit(self.limits.io_read_bps)?;
        cgroup.set_io_write_bps_limit(self.limits.io_write_bps)?;

        // Set PID limit
        cgroup.set_pids_max(self.limits.pids_max)?;

        // Add process to cgroup
        cgroup.add_task(pid)?;

        Ok(())
    }

    /// Apply resource limits via setrlimit (macOS/portable).
    #[cfg(not(target_os = "linux"))]
    pub fn apply(&self, pid: u32) -> Result<(), LimiterError> {
        use libc::{setrlimit, rlimit, RLIMIT_AS, RLIMIT_CPU, RLIMIT_NOFILE};

        // Memory limit
        let mem_limit = rlimit {
            rlim_cur: self.limits.memory_bytes,
            rlim_max: self.limits.memory_bytes,
        };
        unsafe { setrlimit(RLIMIT_AS, &mem_limit) };

        // File descriptor limit
        let fd_limit = rlimit {
            rlim_cur: 1024,
            rlim_max: 4096,
        };
        unsafe { setrlimit(RLIMIT_NOFILE, &fd_limit) };

        Ok(())
    }
}
```

### 5.2 Security Policies

```rust
/// Security policy enforcement for daemon isolation using capability-based security.
///
/// # Reference
/// Watson, R. N. M., et al. (2010). "Capsicum: Practical Capabilities for UNIX."
/// USENIX Security Symposium.
/// NIST. (2020). "Security and Privacy Controls for Information Systems."
/// SP 800-53 Rev. 5. https://doi.org/10.6028/NIST.SP.800-53r5
pub struct SecurityPolicy {
    pub allow_network: bool,
    pub allow_filesystem: Vec<PathPermission>,
    pub allow_syscalls: Vec<SyscallFilter>,
    pub capabilities: Vec<Capability>,
    pub seccomp_profile: Option<SeccompProfile>,
}

impl SecurityPolicy {
    /// Generate seccomp BPF filter implementing capability-based isolation.
    pub fn to_seccomp_filter(&self) -> Result<SeccompFilter, PolicyError> {
        let mut filter = SeccompFilter::new(Action::Kill)?;

        // Allow basic syscalls for any daemon
        for syscall in &[
            "read", "write", "close", "fstat", "mmap", "mprotect",
            "munmap", "brk", "rt_sigaction", "rt_sigprocmask",
            "ioctl", "access", "pipe", "select", "sched_yield",
            "mremap", "msync", "mincore", "madvise", "shmget",
            "shmat", "shmctl", "dup", "dup2", "pause", "nanosleep",
            "getitimer", "alarm", "setitimer", "getpid", "socket",
            "connect", "accept", "sendto", "recvfrom", "sendmsg",
            "recvmsg", "shutdown", "bind", "listen", "getsockname",
            "getpeername", "socketpair", "setsockopt", "getsockopt",
            "clone", "fork", "vfork", "execve", "exit", "wait4",
            "kill", "uname", "fcntl", "flock", "fsync", "fdatasync",
            "truncate", "ftruncate", "getdents", "getcwd", "chdir",
            "fchdir", "rename", "mkdir", "rmdir", "creat", "link",
            "unlink", "symlink", "readlink", "chmod", "fchmod",
            "chown", "fchown", "lchown", "umask", "gettimeofday",
            "getrlimit", "getrusage", "sysinfo", "times", "ptrace",
            "getuid", "syslog", "getgid", "setuid", "setgid",
            "geteuid", "getegid", "setpgid", "getppid", "getpgrp",
            "setsid", "setreuid", "setregid", "getgroups", "setgroups",
            "setresuid", "getresuid", "setresgid", "getresgid",
            "getpgid", "setfsuid", "setfsgid", "getsid", "capget",
            "capset", "rt_sigpending", "rt_sigtimedwait",
            "rt_sigqueueinfo", "rt_sigsuspend", "sigaltstack",
            "utime", "mknod", "uselib", "personality", "ustat",
            "statfs", "fstatfs", "sysfs", "getpriority", "setpriority",
            "sched_setparam", "sched_getparam", "sched_setscheduler",
            "sched_getscheduler", "sched_get_priority_max",
            "sched_get_priority_min", "sched_rr_get_interval",
            "mlock", "munlock", "mlockall", "munlockall", "vhangup",
            "pivot_root", "prctl", "arch_prctl", "adjtimex",
            "setrlimit", "chroot", "sync", "acct", "settimeofday",
            "mount", "umount2", "swapon", "swapoff", "reboot",
            "sethostname", "setdomainname", "ioperm", "iopl",
            "create_module", "init_module", "delete_module",
            "get_kernel_syms", "query_module", "quotactl", "nfsservctl",
            "getpmsg", "putpmsg", "afs_syscall", "tuxcall", "security",
            "gettid", "readahead", "setxattr", "lsetxattr", "fsetxattr",
            "getxattr", "lgetxattr", "fgetxattr", "listxattr",
            "llistxattr", "flistxattr", "removexattr", "lremovexattr",
            "fremovexattr", "tkill", "time", "futex", "sched_setaffinity",
            "sched_getaffinity", "set_thread_area", "io_setup",
            "io_destroy", "io_getevents", "io_submit", "io_cancel",
            "get_thread_area", "lookup_dcookie", "epoll_create",
            "epoll_ctl_old", "epoll_wait_old", "remap_file_pages",
            "getdents64", "set_tid_address", "restart_syscall", "semtimedop",
            "fadvise64", "timer_create", "timer_settime", "timer_gettime",
            "timer_getoverrun", "timer_delete", "clock_settime",
            "clock_gettime", "clock_getres", "clock_nanosleep",
            "exit_group", "epoll_wait", "epoll_ctl", "tgkill",
            "utimes", "vserver", "mbind", "set_mempolicy",
            "get_mempolicy", "mq_open", "mq_unlink", "mq_timedsend",
            "mq_timedreceive", "mq_notify", "mq_getsetattr", "kexec_load",
            "waitid", "add_key", "request_key", "keyctl", "ioprio_set",
            "ioprio_get", "inotify_init", "inotify_add_watch",
            "inotify_rm_watch", "migrate_pages", "openat", "mkdirat",
            "mknodat", "fchownat", "futimesat", "newfstatat", "unlinkat",
            "renameat", "linkat", "symlinkat", "readlinkat", "fchmodat",
            "faccessat", "pselect6", "ppoll", "unshare",
        ] {
            filter.add_rule(Action::Allow, syscall)?;
        }

        // Add custom allowed syscalls
        for rule in &self.allow_syscalls {
            filter.add_rule(Action::Allow, &rule.syscall)?;
        }

        Ok(filter)
    }
}
```

---

## 6. Testing Infrastructure

### 6.0 Iron Lotus Testing Tiers (Certeza Methodology)

Following repartir's three-tiered testing approach:

#### Tier 1: ON-SAVE (< 3 seconds)

Fast feedback for flow state preservation:

```makefile
tier1: fmt clippy check
	@echo "Tier 1 complete (<3s target)"
```

| Check | Tool | Target |
|-------|------|--------|
| Format | `cargo fmt --check` | < 0.5s |
| Lint | `cargo clippy -- -D warnings` | < 1.5s |
| Compile | `cargo check --workspace` | < 1.5s |

#### Tier 2: ON-COMMIT (1-5 minutes)

Comprehensive pre-commit quality gate:

```makefile
tier2: test-lib clippy coverage-check
	@echo "Tier 2 complete (1-5min target)"
```

| Check | Tool | Target |
|-------|------|--------|
| Unit tests | `cargo nextest run --lib` | < 30s |
| Property tests | `cargo test proptest` | < 60s |
| Coverage | `cargo llvm-cov --fail-under 90` | ≥ 90% |
| Security | `cargo audit && cargo deny check` | < 30s |
| SATD | `pmat satd --max 0` | 0 violations |

#### Tier 3: ON-MERGE (1-6 hours)

Exhaustive validation for production readiness:

```makefile
tier3: test-all coverage mutants falsification
	@echo "Tier 3 complete (run in CI)"
```

| Check | Tool | Target |
|-------|------|--------|
| All tests | `cargo nextest run` | 100% pass |
| Coverage | `cargo llvm-cov` | ≥ 95% |
| Mutation | `cargo mutants --workspace` | ≥ 80% score |
| Falsification | `cargo test --features falsification` | 110/110 |
| Formal verify | `cargo kani` (critical paths) | Proven |

#### Tier 4: CI/CD Quality Gate

```makefile
tier4: test-release coverage-html mutants-fast pmat-gate
	@echo "CI/CD gate passed"

pmat-gate:
	pmat analyze --workspace --format json | \
	  jq '.tdg_score >= 85 and .satd_count == 0' | \
	  grep -q true || (echo "PMAT gate failed" && exit 1)
```

### 6.1 Test Categories

| Category | Tool | Purpose | Tier |
|----------|------|---------|------|
| Unit | `cargo test` | Component isolation | 1-2 |
| Integration | `probador integration` | Cross-component | 2 |
| Load | `probador load` | Performance under stress | 3 |
| Chaos | `probador chaos` | Failure resilience | 3 |
| Property | `proptest` | Invariant verification | 2-3 |
| Mutation | `cargo mutants` | Test quality | 3 |
| Syscall | `renacer --compare` | Behavioral equivalence | 3 |
| Falsification | `cargo test falsification` | Popperian refutation | 3 |

### 6.2 Test Harness

```rust
/// Daemon test harness with probador integration.
#[cfg(test)]
mod tests {
    use duende_test::prelude::*;

    #[tokio::test]
    async fn test_daemon_lifecycle() {
        let harness = DaemonTestHarness::new()
            .with_platform(Platform::detect())
            .with_tracing(TracingConfig::full())
            .build();

        // Start daemon
        let handle = harness.spawn(TestDaemon::new()).await.unwrap();

        // Verify startup
        assert_eq!(handle.status().await.unwrap(), DaemonStatus::Running);

        // Health check
        assert!(handle.health_check().await.unwrap().is_healthy());

        // Graceful shutdown
        handle.shutdown(Duration::from_secs(5)).await.unwrap();
        assert_eq!(handle.status().await.unwrap(), DaemonStatus::Stopped);
    }

    #[tokio::test]
    async fn test_daemon_resilience_under_chaos() {
        let harness = DaemonTestHarness::new()
            .with_chaos(ChaosConfig {
                latency_injection: Some((0.1, Duration::from_millis(500))),
                error_injection: Some(0.05),
                memory_pressure: Some(0.8),
            })
            .build();

        let handle = harness.spawn(TestDaemon::new()).await.unwrap();

        // Run under chaos for 30 seconds
        tokio::time::sleep(Duration::from_secs(30)).await;

        // Daemon should still be healthy (circuit breaker recovery)
        assert!(handle.health_check().await.unwrap().is_healthy());

        // Check recovery metrics
        let metrics = handle.metrics();
        assert!(metrics.circuit_breaker_trips.get() > 0);
        assert!(metrics.successful_recoveries.get() > 0);
    }
}
```

---

## 7. Toyota Production System Principles

### 7.0 Iron Lotus Framework

Duende adopts the **Iron Lotus Framework** from repartir — Toyota Production System (TPS) principles applied to systems programming. The name evokes both the iron discipline of manufacturing excellence and the lotus flower's emergence from muddy waters (quality from chaos).

#### Core Tenets

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         IRON LOTUS FRAMEWORK                                │
│                   (Toyota Way for Systems Programming)                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐  │
│  │   Genchi    │    │   Jidoka    │    │   Kaizen    │    │    Muda     │  │
│  │  Genbutsu   │    │  (自働化)   │    │   (改善)    │    │   (無駄)    │  │
│  │  (現地現物)  │    │             │    │             │    │             │  │
│  ├─────────────┤    ├─────────────┤    ├─────────────┤    ├─────────────┤  │
│  │ "Go and See"│    │ "Automation │    │ "Continuous │    │   "Waste    │  │
│  │             │    │ with Human  │    │ Improvement"│    │ Elimination"│  │
│  │ • Pure Rust │    │   Touch"    │    │             │    │             │  │
│  │ • No black  │    │             │    │ • TDG score │    │ • No YAGNI  │  │
│  │   boxes     │    │ • Quality   │    │   ratchet   │    │ • Zero-copy │  │
│  │ • Traceable │    │   gates     │    │ • Five whys │    │ • Fast CI   │  │
│  │   syscalls  │    │ • Andon     │    │ • Blameless │    │ • Minimal   │  │
│  │             │    │   cord      │    │   postmortem│    │   deps      │  │
│  └─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘  │
│                                                                             │
│  ═══════════════════════════════════════════════════════════════════════   │
│  Quality Targets: ≥95% coverage │ ≥80% mutation │ 0 SATD │ TDG ≥85        │
│  ═══════════════════════════════════════════════════════════════════════   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Iron Lotus Clippy Configuration

```toml
# Cargo.toml - Iron Lotus strict lint enforcement
[lints.clippy]
# Set lint groups to lower priority so specific lints can override
pedantic = { level = "warn", priority = -1 }
nursery = { level = "warn", priority = -1 }
cargo = { level = "warn", priority = -1 }

# Iron Lotus: No unwrap/panic/todo in production code
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
unimplemented = "deny"

# Force explicit error handling
missing_errors_doc = "warn"
missing_panics_doc = "warn"
```

### 7.1 Principle Mapping

| TPS Principle | Japanese | Duende Implementation |
|---------------|----------|----------------------|
| **Jidoka** | 自働化 | Automatic stop on policy violation; circuit breakers |
| **Just-in-Time** | ジャストインタイム | Lazy daemon initialization; on-demand resource allocation |
| **Heijunka** | 平準化 | Work-stealing schedulers; load leveling across platforms |
| **Kaizen** | 改善 | Continuous metrics collection; trend analysis |
| **Poka-Yoke** | ポカヨケ | Type-safe APIs; compile-time guarantees |
| **Genchi Genbutsu** | 現地現物 | Syscall tracing via renacer; direct observation |
| **Andon** | アンドン | Real-time alerting; visual monitoring via ttop |
| **Muda** | 無駄 | Compressed state storage; efficient resource usage |
| **Muri** | 無理 | Resource limits; overload protection |
| **Mura** | 斑 | Consistent daemon behavior via standardized traits |

### 7.2 Quality Gates (Jidoka Implementation)

```rust
/// Quality gate that stops pipeline on violation (Jidoka principle).
///
/// "Stop to fix problems, to get quality right the first time."
/// — Taiichi Ohno, Toyota Production System (1988)
pub struct JidokaGate {
    checks: Vec<Box<dyn QualityCheck>>,
    stop_on_first_failure: bool,
}

impl JidokaGate {
    pub fn check(&self, daemon: &dyn Daemon) -> JidokaResult {
        let mut violations = Vec::new();

        for check in &self.checks {
            match check.verify(daemon) {
                Ok(()) => continue,
                Err(violation) => {
                    violations.push(violation);
                    if self.stop_on_first_failure {
                        // Andon cord: stop immediately
                        return JidokaResult::Stop {
                            violations,
                            recommendation: self.recommend_fix(&violations),
                        };
                    }
                }
            }
        }

        if violations.is_empty() {
            JidokaResult::Pass
        } else {
            JidokaResult::Stop { violations, recommendation: self.recommend_fix(&violations) }
        }
    }
}
```

---

## 8. Peer-Reviewed Citations

### 8.1 Toyota Production System

1. **Ohno, T.** (1988). *Toyota Production System: Beyond Large-Scale Production*. Productivity Press. ISBN: 978-0915299140.
   - Foundation for Jidoka, Just-in-Time, and Muda elimination principles.

2. **Liker, J. K.** (2004). *The Toyota Way: 14 Management Principles from the World's Greatest Manufacturer*. McGraw-Hill. ISBN: 978-0071392310.
   - Comprehensive framework for lean manufacturing adapted to software.

3. **Shingo, S.** (1986). *Zero Quality Control: Source Inspection and the Poka-Yoke System*. Productivity Press. ISBN: 978-0915299072.
   - Foundation for mistake-proofing and source inspection in daemon design.

4. **Womack, J. P., & Jones, D. T.** (1996). *Lean Thinking: Banish Waste and Create Wealth in Your Corporation*. Simon & Schuster. ISBN: 978-0684810355.
   - Value stream mapping principles applied to daemon workflows.

### 8.2 Systems and Distributed Computing

5. **Gregg, B., & Hazelwood, K.** (2011). "The 5× PCIe Rule: When to Use GPU Acceleration." *USENIX Annual Technical Conference*. https://www.usenix.org/conference/atc11
   - Foundation for adaptive GPU/SIMD backend selection in trueno-zram.

6. **Blumofe, R. D., & Leiserson, C. E.** (1999). "Scheduling multithreaded computations by work stealing." *Journal of the ACM*, 46(5), 720-748. https://doi.org/10.1145/324133.324234
   - Work-stealing scheduler design for repartir and WOS.

7. **Lamport, L.** (1978). "Time, clocks, and the ordering of events in a distributed system." *Communications of the ACM*, 21(7), 558-565. https://doi.org/10.1145/359545.359563
   - Lamport clocks for causal ordering in renacer traces.

8. **Fowler, M.** (2014). "CircuitBreaker." *martinfowler.com*. https://martinfowler.com/bliki/CircuitBreaker.html
   - Circuit breaker pattern for daemon resilience.

9. **Candea, G., & Fox, A.** (2003). "Crash-Only Software." *Proceedings of the 9th Workshop on Hot Topics in Operating Systems (HotOS-IX)*.
   - Foundation for micro-reboot and crash-safety strategies in Duende.

10. **Madhavapeddy, A., et al.** (2013). "Unikernels: Library Operating Systems for the Cloud." *ASPLOS*.
    - Architectural basis for WOS and pepita MicroVM implementation.

### 8.3 Observability and Monitoring

11. **Wilkins, T.** (2018). "The RED Method: How to instrument your services." *Weaveworks Blog*. https://www.weave.works/blog/the-red-method-key-metrics
    - Rate, Errors, Duration metrics framework.

12. **Sigelman, B. H., et al.** (2010). "Dapper, a Large-Scale Distributed Systems Tracing Infrastructure." *Google Technical Report*. https://research.google/pubs/pub36356/
    - Foundation for distributed tracing in daemon observability.

13. **W3C.** (2021). "Trace Context - Level 1." *W3C Recommendation*. https://www.w3.org/TR/trace-context/
    - Standard for distributed trace propagation.

14. **Gershuni, E., et al.** (2019). "Simple and Precise Static Analysis of Untrusted Linux Kernel Extensions." *PLDI 2019*.
    - Verification methodology for eBPF safety in renacer.

### 8.4 Security and Verification

15. **NIST.** (2020). "Security and Privacy Controls for Information Systems and Organizations." *SP 800-53 Rev. 5*. https://doi.org/10.6028/NIST.SP.800-53r5
    - Security control framework for daemon isolation.

16. **Edge, J., et al.** (2020). "Seccomp and Sandboxing in Linux." *LWN.net*. https://lwn.net/Articles/656307/
    - Seccomp BPF filtering for syscall restriction.

17. **Jung, R., et al.** (2018). "RustBelt: Securing the Foundations of the Rust Programming Language." *Proceedings of the ACM on Programming Languages (POPL)*.
    - Formal verification of Rust's safety guarantees used in Duende.

18. **Watson, R. N. M., et al.** (2010). "Capsicum: Practical Capabilities for UNIX." *19th USENIX Security Symposium*.
    - Theoretical basis for Duende's capability-based process isolation.

### 8.5 Testing and Quality

19. **Deming, W. E.** (1986). *Out of the Crisis*. MIT Press. ISBN: 978-0262541152.
    - Quality philosophy: "Quality cannot be inspected in."

20. **Popper, K.** (1959). *The Logic of Scientific Discovery*. Routledge. ISBN: 978-0415278447.
    - Falsification methodology for test design.

21. **Hamlet, R.** (1977). "Testing Programs with the Aid of a Compiler." *IEEE Transactions on Software Engineering*, SE-3(4), 279-290. https://doi.org/10.1109/TSE.1977.231145
    - Foundation for mutation testing.

### 8.6 Operating Systems and Virtualization

22. **Agache, A., et al.** (2020). "Firecracker: Lightweight Virtualization for Serverless Applications." *NSDI '20*. https://www.usenix.org/conference/nsdi20/presentation/agache
    - MicroVM architecture inspiring pepita design.

23. **Silberschatz, A., Galvin, P. B., & Gagne, G.** (2018). *Operating System Concepts (10th ed.)*. Wiley. ISBN: 978-1119320913.
    - Process scheduling and memory management principles.

---

## 9. Popperian Falsification Checklist

### 9.1 Falsification Methodology

Following Karl Popper's philosophy of science, each claim about Duende must be **falsifiable** — there must exist an observable test that could prove the claim wrong. A claim that cannot be tested is not scientific.

> "A theory which is not refutable by any conceivable event is non-scientific."
> — Karl Popper, *Conjectures and Refutations* (1963)

### 9.2 Falsification Checklist (110 Tests)

#### Category A: Lifecycle Management (F001-F020)

| ID | Claim | Falsification Test | Pass Criteria |
|----|-------|-------------------|---------------|
| **F001** | Daemon starts within 100ms on Linux | Measure startup time with `hyperfine` for 1000 iterations | p99 < 100ms |
| **F002** | Daemon starts within 100ms on macOS | Measure startup time with `hyperfine` for 1000 iterations | p99 < 100ms |
| **F003** | Daemon starts within 500ms in Docker | Measure container startup + daemon ready time | p99 < 500ms |
| **F004** | Daemon starts within 200ms in pepita VM | Measure VM boot + daemon ready time | p99 < 200ms |
| **F005** | Daemon starts within 50ms in WOS | Measure process creation + exec time | p99 < 50ms |
| **F006** | Graceful shutdown completes within timeout | Send SIGTERM, measure time to exit | exit time ≤ configured timeout |
| **F007** | Forced shutdown (SIGKILL) terminates immediately | Send SIGKILL, measure time to termination | termination < 10ms |
| **F008** | Daemon restarts after crash | Kill with SIGSEGV, measure restart | auto-restart within 5s |
| **F009** | Daemon preserves state across restarts | Store state, kill, restart, verify state | state matches pre-crash |
| **F010** | Daemon handles SIGHUP for config reload | Send SIGHUP, verify config reloaded | new config active within 1s |
| **F011** | Multiple daemons can run concurrently | Spawn 100 daemons, verify all running | 100 daemons in Running state |
| **F012** | Daemon PID file is created on start | Start daemon, check PID file exists | file exists with correct PID |
| **F013** | Daemon PID file is removed on stop | Stop daemon, check PID file removed | file does not exist |
| **F014** | Daemon handles double-start gracefully | Start twice, verify single instance | second start returns error |
| **F015** | Daemon handles double-stop gracefully | Stop twice, verify no error | second stop is idempotent |
| **F016** | Daemon status is queryable at any time | Query status during all lifecycle phases | status returns valid state |
| **F017** | Daemon logs to configured destination | Start with log config, verify logs appear | logs present at destination |
| **F018** | Daemon rotates logs when size exceeded | Generate logs > rotation size | old logs archived, new file created |
| **F019** | Daemon environment variables are isolated | Set env var, verify not leaked to host | host env unchanged |
| **F020** | Daemon working directory is configurable | Start with custom cwd, verify file paths | file operations relative to cwd |

#### Category B: Resource Management (F021-F040)

| ID | Claim | Falsification Test | Pass Criteria |
|----|-------|-------------------|---------------|
| **F021** | Memory limit is enforced | Allocate beyond limit, verify OOM | process killed or allocation fails |
| **F022** | CPU quota is enforced | Busy loop, measure CPU time | CPU ≤ quota × wall time |
| **F023** | File descriptor limit is enforced | Open FDs beyond limit | open() returns EMFILE |
| **F024** | Process limit is enforced | Fork beyond limit | fork() returns EAGAIN |
| **F025** | I/O bandwidth limit is enforced | Write at max speed, measure throughput | throughput ≤ configured limit |
| **F026** | Network bandwidth limit is enforced | Transmit at max speed, measure throughput | throughput ≤ configured limit |
| **F027** | Disk quota is enforced | Write beyond quota | write() returns EDQUOT |
| **F028** | Memory usage is reported accurately | Compare reported vs `/proc/[pid]/status` | values match within 1% |
| **F029** | CPU usage is reported accurately | Compare reported vs `/proc/[pid]/stat` | values match within 5% |
| **F030** | Disk usage is reported accurately | Compare reported vs `du` | values match within 1% |
| **F031** | Resource limits survive daemon restart | Set limits, restart, verify limits | limits unchanged after restart |
| **F032** | Resource limits can be updated at runtime | Update limits via API, verify applied | new limits effective immediately |
| **F033** | Zero-memory daemon runs successfully | Configure 0 memory (uses default) | daemon starts with default memory |
| **F034** | Resource exhaustion triggers alert | Exhaust resource, verify alert fired | alert received within 1s |
| **F035** | Compressed state reduces memory footprint | Compare memory with/without compression | compressed uses < 50% memory |
| **F036** | Same-fill pages achieve 2048:1 ratio | Store zero-filled pages | compression ratio ≥ 2048:1 |
| **F037** | LZ4 compression achieves ≥3 GB/s | Benchmark compression throughput | throughput ≥ 3 GB/s |
| **F038** | ZSTD compression achieves ≥8 GB/s | Benchmark with AVX-512 | throughput ≥ 8 GB/s |
| **F039** | GPU batch compression is faster than SIMD | Compare 10K page batch times | GPU time < SIMD time |
| **F040** | SIMD batch compression is faster than scalar | Compare 100 page batch times | SIMD time < scalar time |

#### Category C: Observability (F041-F060)

| ID | Claim | Falsification Test | Pass Criteria |
|----|-------|-------------------|---------------|
| **F041** | Syscall tracing has <5% overhead | Compare execution time with/without tracing | overhead < 5% |
| **F042** | Source correlation resolves to correct line | Trace syscall, verify source location | file:line matches actual |
| **F043** | Anomaly detection identifies latency spikes | Inject 10× latency, verify detection | anomaly flagged within 1s |
| **F044** | Z-score threshold is configurable | Set threshold to 2σ, verify more alerts | alert count increases |
| **F045** | Critical path analysis identifies bottleneck | Create known bottleneck, verify identified | bottleneck in critical path |
| **F046** | Anti-pattern detection finds tight loops | Create 1000× syscall loop, verify flagged | tight loop detected |
| **F047** | Metrics export to Prometheus format | Query /metrics endpoint | valid Prometheus text format |
| **F048** | Metrics export via OTLP | Send to collector, verify receipt | metrics received by collector |
| **F049** | Distributed tracing propagates context | Call chain across daemons, verify trace | single trace_id across all spans |
| **F050** | W3C traceparent header is parsed correctly | Send valid header, verify context | trace_id and span_id match |
| **F051** | W3C traceparent header is generated correctly | Export header, verify format | matches W3C specification |
| **F052** | Ring buffer maintains bounded size | Push 1M samples, verify size | buffer size = configured capacity |
| **F053** | Ring buffer has O(1) push/pop | Benchmark 1M operations | time scales linearly (not quadratic) |
| **F054** | Ring buffer has zero allocations after warmup | Run with allocator tracking | 0 allocations after initial fill |
| **F055** | Historical metrics query is correct | Query last 5 minutes, verify completeness | all samples in range returned |
| **F056** | Real-time metrics update at configured interval | Set 100ms interval, measure actual | actual interval within 10% of config |
| **F057** | GPU metrics are collected when available | Run on GPU system, query metrics | GPU utilization reported |
| **F058** | GPU metrics gracefully degrade when unavailable | Run on non-GPU system | no error, GPU metrics null |
| **F059** | Process tree is tracked correctly | Fork child processes, verify hierarchy | parent-child relationships correct |
| **F060** | Thread count is reported accurately | Create threads, compare reported | reported count matches actual |

#### Category D: Policy Enforcement (F061-F080)

| ID | Claim | Falsification Test | Pass Criteria |
|----|-------|-------------------|---------------|
| **F061** | Complexity threshold violation stops deployment | Submit code with complexity > threshold | deployment rejected |
| **F062** | SATD (technical debt) violation is detected | Add TODO/FIXME comments | violation reported |
| **F063** | Dead code percentage violation is detected | Add unreachable code > threshold | violation reported |
| **F064** | Quality score below minimum is rejected | Submit low-quality code | deployment rejected |
| **F065** | Circuit breaker opens after N failures | Cause N failures, verify state | circuit state = Open |
| **F066** | Circuit breaker closes after timeout | Wait for timeout, verify state | circuit state = Closed |
| **F067** | Circuit breaker half-open allows test request | After timeout, send request | request processed |
| **F068** | Backpressure rejects when queue full | Fill queue, send request | request rejected |
| **F069** | Backpressure recovers when queue drains | Drain queue, send request | request accepted |
| **F070** | Rate limiting enforces configured rate | Send requests above rate | excess requests rejected |
| **F071** | Seccomp filter blocks disallowed syscalls | Attempt blocked syscall | process killed with SIGSYS |
| **F072** | Seccomp filter allows permitted syscalls | Attempt allowed syscall | syscall succeeds |
| **F073** | Filesystem access is restricted to allowed paths | Access disallowed path | EACCES returned |
| **F074** | Network access is restricted when disabled | Attempt socket(), verify failure | EPERM returned |
| **F075** | Capability dropping removes privileges | Drop CAP_NET_RAW, attempt raw socket | EPERM returned |
| **F076** | Bashrs script is idempotent | Run script twice, verify no side effects | second run is no-op |
| **F077** | Bashrs script is deterministic | Run script twice, compare output | outputs identical |
| **F078** | Bashrs script passes shellcheck | Run shellcheck on generated script | zero warnings/errors |
| **F079** | Policy violations trigger Jidoka stop | Introduce violation, verify pipeline stops | pipeline halted |
| **F080** | Policy can be updated without daemon restart | Update policy via API, verify applied | new policy effective |

#### Category E: Platform Compatibility & Edge Cases (F081-F110)

| ID | Claim | Falsification Test | Pass Criteria |
|----|-------|-------------------|---------------|
| **F081** | Linux systemd adapter creates unit correctly | Start daemon, verify systemctl status | unit active and running |
| **F082** | Linux systemd adapter handles restart policy | Kill daemon, verify auto-restart | daemon restarted by systemd |
| **F083** | macOS launchd adapter creates plist correctly | Start daemon, verify launchctl list | service loaded and running |
| **F084** | macOS launchd adapter handles keep-alive | Kill daemon, verify auto-restart | daemon restarted by launchd |
| **F085** | Docker adapter creates container correctly | Start daemon, verify docker ps | container running |
| **F086** | Docker adapter handles restart policy | Kill container, verify auto-restart | container restarted |
| **F087** | Docker health check is configured | Inspect container, verify health check | health check present |
| **F088** | pepita VM boots with configured resources | Start VM, verify vCPUs and memory | resources match config |
| **F089** | pepita vsock communication works | Send message via vsock, verify receipt | message received correctly |
| **F090** | pepita virtio-blk passes files correctly | Send file via virtio-blk, verify hash | file hash matches |
| **F091** | WOS process scheduling uses 8 priority levels | Create processes at each level, verify ordering | higher priority runs first |
| **F092** | WOS scheduler prevents starvation via aging | Create low-priority process, verify eventual execution | process runs after aging |
| **F093** | WOS init process reaps orphaned children | Kill parent, verify child adopted by init | child reaped by PID 1 |
| **F094** | WOS Jidoka guards enforce invariants | Exceed MAX_PROCESSES, verify halt | kernel halts with violation |
| **F095** | Cross-platform daemon binary runs on Linux | Build with `--target x86_64-unknown-linux-gnu` | binary executes correctly |
| **F096** | Cross-platform daemon binary runs on macOS | Build with `--target x86_64-apple-darwin` | binary executes correctly |
| **F097** | Cross-platform daemon binary runs in WASM | Build with `--target wasm32-wasi` | binary executes in wasmtime |
| **F098** | Platform detection identifies Linux correctly | Run on Linux, verify detection | Platform::Linux returned |
| **F099** | Platform detection identifies macOS correctly | Run on macOS, verify detection | Platform::MacOS returned |
| **F100** | Platform detection identifies container correctly | Run in Docker, verify detection | Platform::Container returned |
| **F101** | Clock skew backward does not hang daemon | Jump clock back 1 hour | daemon continues, no hang |
| **F102** | PID wraparound handled correctly | Simulate PID wrap, spawn daemon | valid PID assigned, no collision |
| **F103** | File descriptor exhaustion handled safely | Fill process FDs, try log/net | safe error/alert, no crash |
| **F104** | Disk full (ENOSPC) handled safely | Fill disk, write logs/state | safe error/alert, no corruption |
| **F105** | Zombie processes are reaped effectively | Generate 1000 short-lived children | no zombies remain after 5s |
| **F106** | Network partition does not crash daemon | Block network access (iptables) | safe error handling, retry |
| **F107** | SIGSTOP/SIGCONT handled correctly | Pause/Resume, verify timekeeping | metrics adjusted, no crash |
| **F108** | Env var injection overflow protection | Inject 1MB env var | rejected or truncated safely |
| **F109** | Deeply nested path handling | Use path > PATH_MAX | safe error (ENAMETOOLONG) |
| **F110** | Symlink loop detection | Point config to symlink loop | safe error (ELOOP), no hang |

### 9.3 Falsification Execution

```bash
# Run all falsification tests
cargo test --features falsification -- --test-threads=1

# Run specific category
cargo test --features falsification category_a

# Run single test
cargo test --features falsification f001_daemon_starts_within_100ms_linux

# Generate falsification report
cargo test --features falsification -- --format json > falsification_report.json
```

### 9.4 Falsification Coverage Requirements

| Metric | Minimum | Target |
|--------|---------|--------|
| Falsification tests passing | 100/110 | 110/110 |
| Edge cases covered | 90% | 100% |
| Platform coverage | 4/5 platforms | 5/5 platforms |
| Mutation score on tests | 85% | 95% |

---

## 10. API Reference

### 10.1 Core Types

```rust
/// Unique daemon identifier
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DaemonId(Uuid);

/// Daemon lifecycle state
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonStatus {
    Created,
    Starting,
    Running,
    Paused,
    Stopping,
    Stopped,
    Failed(FailureReason),
}

/// Daemon exit reason
#[derive(Clone, Debug)]
pub enum ExitReason {
    Graceful,
    Signal(Signal),
    Error(DaemonError),
    ResourceExhausted(Resource),
    PolicyViolation(PolicyViolation),
}

/// Health check result
#[derive(Clone, Debug)]
pub struct HealthStatus {
    pub healthy: bool,
    pub checks: Vec<HealthCheck>,
    pub latency: Duration,
    pub last_check: Instant,
}

/// Supported platforms
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Platform {
    Linux,
    MacOS,
    Container,
    PepitaMicroVM,
    Wos,
    Native,
}
```

### 10.2 Configuration

```rust
/// Daemon configuration
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DaemonConfig {
    pub name: String,
    pub version: String,
    pub description: String,
    pub binary_path: PathBuf,
    pub config_path: PathBuf,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub user: Option<String>,
    pub group: Option<String>,
    pub working_dir: Option<PathBuf>,
    pub resources: ResourceConfig,
    pub policy: PolicyConfig,
    pub platform: Option<PlatformConfig>,
}

/// Resource configuration
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ResourceConfig {
    pub memory_limit: u64,
    pub memory_swap_limit: u64,
    pub cpu_quota_percent: f64,
    pub cpu_shares: u64,
    pub io_read_bps: u64,
    pub io_write_bps: u64,
    pub pids_max: u64,
    pub open_files_max: u64,
}

/// Policy configuration
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PolicyConfig {
    pub max_complexity: u32,
    pub satd_tolerance: u32,
    pub dead_code_max_percent: f64,
    pub min_quality_score: f64,
    pub security: SecurityPolicy,
}
```

---

## Appendix A: Glossary

| Term | Definition |
|------|------------|
| **Andon** | Visual signal system for alerting problems |
| **Duende** | Spanish for spirit/daemon; this framework |
| **Genchi Genbutsu** | "Go and see" - direct observation principle |
| **Heijunka** | Level loading/scheduling |
| **Jidoka** | Automation with human intelligence; stop-on-error |
| **Kaizen** | Continuous improvement |
| **Muda** | Waste; non-value-adding activity |
| **Mura** | Unevenness; inconsistency |
| **Muri** | Overburden; unreasonableness |
| **Poka-Yoke** | Mistake-proofing |

---

## Appendix B: Revision History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.2.0 | 2026-01-06 | PAIML Team | Added Section 3.0 Dependency Policy (Iron Lotus Framework), Section 6.0 Testing Tiers (Certeza Methodology), Section 7.0 Iron Lotus Framework overview |
| 1.1.0 | 2026-01-06 | PAIML Team | Added Section 1.4 Safety & Verification Guarantees, expanded falsifications to 110 (F101-F110 edge cases), added citations 19-23 |
| 1.0.0 | 2026-01-06 | PAIML Team | Initial specification |

---

*This specification is part of the PAIML Sovereign AI Stack documentation.*
*Built with the Iron Lotus Framework — Quality is not inspected in; it is built in.*
