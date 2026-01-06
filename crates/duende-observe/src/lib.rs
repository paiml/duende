// Iron Lotus: Allow unwrap/expect in tests for clear failure messages
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! # duende-observe
//!
//! Observability integration for the Duende daemon framework.
//!
//! This crate provides:
//! - **Renacer integration**: Syscall tracing with source correlation
//! - **ttop integration**: Real-time resource monitoring via trueno-viz collectors
//! - **Metrics export**: Prometheus and OTLP format support
//!
//! ## Iron Lotus Framework
//!
//! - **Genchi Genbutsu** (現地現物): Direct observation via syscall tracing
//! - **Visual Management** (目で見る管理): Real-time metrics dashboards
//! - **Kaizen** (改善): Continuous improvement via metrics collection
//!
//! ## Example
//!
//! ```rust,ignore
//! use duende_observe::{DaemonTracer, DaemonMonitor};
//!
//! // Attach tracer to daemon
//! let mut tracer = DaemonTracer::new();
//! tracer.attach(daemon_pid).await?;
//!
//! // Collect syscall trace
//! let report = tracer.collect().await?;
//! println!("Critical path: {:?}", report.critical_path);
//! ```

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod monitor;
pub mod tracer;

pub use error::{ObserveError, Result};
pub use monitor::{DaemonMonitor, DaemonSnapshot, ProcessState};
pub use tracer::{AnomalyKind, DaemonTracer, TraceReport};
