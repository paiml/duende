//! # duende-platform
//!
//! Platform adapters for the Duende cross-platform daemon framework.
//!
//! This crate provides platform-specific implementations for spawning,
//! signaling, and monitoring daemons across:
//!
//! - **Linux** (systemd): Service units with cgroup resource control
//! - **macOS** (launchd): Property lists with keep-alive support
//! - **Container** (Docker/OCI): Container runtime integration
//! - **pepita** (MicroVM): Virtio-vsock communication
//! - **WOS** (WebAssembly OS): Process scheduling with priority levels
//! - **Native** (fallback): Direct process spawning
//!
//! ## Iron Lotus Framework
//!
//! This crate follows the Iron Lotus Framework principles:
//! - **Genchi Genbutsu**: Platform detection via direct observation
//! - **Poka-Yoke**: Feature-gated platform code prevents misuse
//! - **Standardized Work**: Unified `PlatformAdapter` trait
//!
//! ## Example
//!
//! ```rust,ignore
//! use duende_platform::{detect_platform, create_adapter, Platform};
//!
//! let platform = detect_platform();
//! let adapter = create_adapter(platform)?;
//!
//! let handle = adapter.spawn(my_daemon).await?;
//! adapter.signal(&handle, Signal::Term).await?;
//! ```

#![warn(missing_docs)]

pub mod adapter;
pub mod detect;
pub mod error;
pub mod memory;
pub mod native;

#[cfg(feature = "linux")]
pub mod linux;

#[cfg(feature = "macos")]
pub mod macos;

#[cfg(feature = "container")]
pub mod container;

#[cfg(feature = "pepita")]
pub mod pepita;

#[cfg(feature = "wos")]
pub mod wos;

pub use adapter::{DaemonHandle, PlatformAdapter, TracerHandle};
pub use detect::{Platform, detect_platform};
pub use error::{PlatformError, Result};
pub use memory::{MlockResult, apply_memory_config, is_memory_locked, lock_daemon_memory};
pub use native::NativeAdapter;
