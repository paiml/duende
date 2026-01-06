// Iron Lotus: Allow unwrap/expect in tests for clear failure messages
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! # duende-core
//!
//! Core daemon lifecycle primitives for the Duende cross-platform daemon framework.
//!
//! This crate provides the foundational types and traits for daemon management:
//!
//! - [`Daemon`] trait for implementing daemon lifecycle
//! - [`DaemonConfig`] for daemon configuration
//! - [`DaemonMetrics`] for RED method metrics (Rate, Errors, Duration)
//! - [`DaemonContext`] for runtime context and signal handling
//!
//! ## Iron Lotus Framework
//!
//! This crate follows the Iron Lotus Framework principles:
//! - **Genchi Genbutsu**: All operations traceable to syscalls via renacer
//! - **Jidoka**: Explicit error handling, no panics
//! - **Kaizen**: Continuous metrics for improvement
//! - **Muda**: Zero-waste resource allocation
//!
//! ## Example
//!
//! ```rust,ignore
//! use duende_core::{Daemon, DaemonConfig, DaemonContext, DaemonId, ExitReason};
//! use async_trait::async_trait;
//!
//! struct MyDaemon {
//!     id: DaemonId,
//! }
//!
//! #[async_trait]
//! impl Daemon for MyDaemon {
//!     fn id(&self) -> DaemonId { self.id }
//!     fn name(&self) -> &str { "my-daemon" }
//!     // ... implement other methods
//! }
//! ```

#![deny(unsafe_code)]
#![warn(missing_docs)]
// Allow significant_drop_tightening - overly aggressive for async code with locks
#![allow(clippy::significant_drop_tightening)]

pub mod adapter;
pub mod adapters;
pub mod config;
pub mod daemon;
pub mod error;
pub mod manager;
pub mod metrics;
pub mod platform;
#[cfg(test)]
pub mod tests;
pub mod types;

pub use adapter::{
    DaemonHandle, HandleData, PlatformAdapter, PlatformError, PlatformResult, TracerHandle,
    TracerType,
};
pub use adapters::{
    ContainerAdapter, ContainerRuntime, LaunchdAdapter, NativeAdapter, PepitaAdapter,
    SystemdAdapter, WosAdapter, select_adapter, select_adapter_auto,
};
pub use config::{DaemonConfig, ResourceConfig};
pub use daemon::{Daemon, DaemonContext, DaemonContextHandle};
pub use error::{DaemonError, Result};
pub use manager::{BackoffConfig, DaemonManager, ManagedDaemon, RestartPolicy};
pub use metrics::DaemonMetrics;
pub use platform::{Platform, detect_platform};
pub use types::{DaemonId, DaemonStatus, ExitReason, FailureReason, HealthStatus, Signal};
