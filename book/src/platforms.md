# Platform Adapters

Duende supports multiple platforms through the `PlatformAdapter` trait.

## Supported Platforms

| Platform | Adapter | Status |
|----------|---------|--------|
| [Linux (systemd)](./platforms/linux.md) | `LinuxAdapter` | Partial |
| [macOS (launchd)](./platforms/macos.md) | `MacOSAdapter` | Stub |
| [Container](./platforms/container.md) | `ContainerAdapter` | Stub |
| [pepita (MicroVM)](./platforms/pepita.md) | `PepitaAdapter` | Stub |
| [WOS](./platforms/wos.md) | `WosAdapter` | Stub |
| Native | `NativeAdapter` | Complete |

## Platform Detection

```rust
use duende_platform::{detect_platform, Platform};

let platform = detect_platform();
match platform {
    Platform::Linux => println!("Running on Linux"),
    Platform::MacOS => println!("Running on macOS"),
    Platform::Container => println!("Running in container"),
    Platform::PepitaMicroVM => println!("Running in pepita"),
    Platform::Wos => println!("Running on WOS"),
    Platform::Native => println!("Native fallback"),
}
```

## PlatformAdapter Trait

```rust
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    fn platform(&self) -> Platform;
    async fn spawn(&self, daemon: Box<dyn Daemon>) -> Result<DaemonHandle>;
    async fn signal(&self, handle: &DaemonHandle, sig: Signal) -> Result<()>;
    async fn status(&self, handle: &DaemonHandle) -> Result<DaemonStatus>;
    async fn attach_tracer(&self, handle: &DaemonHandle) -> Result<TracerHandle>;
}
```

## Native Adapter

The `NativeAdapter` is the fallback for all platforms. It spawns daemons as regular OS processes:

```rust
use duende_platform::NativeAdapter;

let adapter = NativeAdapter::new();
let handle = adapter.spawn(Box::new(my_daemon)).await?;
adapter.signal(&handle, Signal::Term).await?;
```
