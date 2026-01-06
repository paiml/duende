# Platform Adapters

Duende supports multiple platforms through the `PlatformAdapter` trait. All 6 adapters are fully implemented.

## Supported Platforms

| Platform | Adapter | Status | Falsification |
|----------|---------|--------|---------------|
| Native | `NativeAdapter` | Complete | `cargo run --example daemon` |
| [Linux (systemd)](./platforms/linux.md) | `SystemdAdapter` | Complete | `systemctl --user status duende-*` |
| [macOS (launchd)](./platforms/macos.md) | `LaunchdAdapter` | Complete | `launchctl list \| grep duende` |
| [Container](./platforms/container.md) | `ContainerAdapter` | Complete | `docker ps \| grep duende` |
| [pepita (MicroVM)](./platforms/pepita.md) | `PepitaAdapter` | Complete | `pepita list \| grep duende-vm` |
| [WOS](./platforms/wos.md) | `WosAdapter` | Complete | `wos-ctl ps \| grep duende` |

## Platform Detection

```rust
use duende_core::platform::{detect_platform, Platform};

let platform = detect_platform();
match platform {
    Platform::Linux => println!("Running on Linux (systemd)"),
    Platform::MacOS => println!("Running on macOS (launchd)"),
    Platform::Container => println!("Running in container"),
    Platform::PepitaMicroVM => println!("Running in pepita microVM"),
    Platform::Wos => println!("Running on WOS"),
    Platform::Native => println!("Native fallback"),
}
```

## Automatic Adapter Selection

```rust
use duende_core::adapters::select_adapter;
use duende_core::platform::detect_platform;

// Auto-detect platform and get appropriate adapter
let platform = detect_platform();
let adapter = select_adapter(platform);

// Use the adapter
let handle = adapter.spawn(Box::new(my_daemon)).await?;
```

## PlatformAdapter Trait

All adapters implement this trait:

```rust
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    /// Returns the platform this adapter handles
    fn platform(&self) -> Platform;

    /// Spawns a daemon and returns a handle
    async fn spawn(&self, daemon: Box<dyn Daemon>) -> PlatformResult<DaemonHandle>;

    /// Sends a signal to the daemon
    async fn signal(&self, handle: &DaemonHandle, sig: Signal) -> PlatformResult<()>;

    /// Returns current daemon status
    async fn status(&self, handle: &DaemonHandle) -> PlatformResult<DaemonStatus>;

    /// Attaches a tracer for syscall monitoring
    async fn attach_tracer(&self, handle: &DaemonHandle) -> PlatformResult<TracerHandle>;
}
```

## Supported Signals

```rust
pub enum Signal {
    Term,   // SIGTERM (15) - graceful shutdown
    Kill,   // SIGKILL (9)  - force kill
    Int,    // SIGINT (2)   - interrupt
    Quit,   // SIGQUIT (3)  - quit with core dump
    Hup,    // SIGHUP (1)   - reload config
    Usr1,   // SIGUSR1 (10) - user-defined
    Usr2,   // SIGUSR2 (12) - user-defined
    Stop,   // SIGSTOP (19) - pause
    Cont,   // SIGCONT (18) - resume
}
```

## Native Adapter

The `NativeAdapter` is the fallback for all platforms. It spawns daemons as regular OS processes using `tokio::process`:

```rust
use duende_core::adapters::NativeAdapter;
use duende_core::types::Signal;

let adapter = NativeAdapter::new();
let handle = adapter.spawn(Box::new(my_daemon)).await?;

// Check status
let status = adapter.status(&handle).await?;
println!("Status: {:?}", status);

// Send graceful shutdown
adapter.signal(&handle, Signal::Term).await?;
```

## Handle Types

Each adapter returns platform-specific handle data:

```rust
pub enum HandleData {
    Native { pid: u32 },
    Systemd { unit_name: String },
    Launchd { label: String },
    Container { runtime: String, container_id: String },
    Pepita { vm_id: String, vsock_cid: u32 },
    Wos { pid: u32 },
}
```

## Error Handling

All adapter operations return `PlatformResult<T>`:

```rust
pub enum PlatformError {
    SpawnFailed(String),
    SignalFailed(String),
    StatusFailed(String),
    TracerFailed(String),
    NotSupported(String),
    Config(String),
}
```
