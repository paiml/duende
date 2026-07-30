// Iron Lotus: Allow unwrap/expect in tests for clear failure messages
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
// clippy's `duration_suboptimal_units` (new in 1.95) wants `Duration::from_mins`
// for the second-valued Durations in chaos.rs and load.rs. That constructor was
// stabilized in Rust 1.87 while this workspace declares `rust-version = "1.83"`,
// so taking the suggestion would silently raise the MSRV. Same rationale as the
// allow in duende-core's manager.rs; revisit if the declared MSRV passes 1.87.
#![allow(clippy::duration_suboptimal_units)]

//! # duende-test
//!
//! Testing infrastructure for the Duende daemon framework.
//!
//! This crate provides:
//! - **Test harness**: Daemon lifecycle testing utilities
//! - **Chaos injection**: Latency, errors, packet loss simulation
//! - **Load testing**: Performance testing under load
//! - **Falsification tests**: 110 Popperian tests for spec compliance
//!
//! ## Iron Lotus Framework
//!
//! - **Built-in Quality** (品質の作り込み): Quality cannot be inspected in
//! - **Popperian Falsification**: Tests designed to refute claims
//! - **Extreme TDD**: Write failing tests first
//!
//! ## Example
//!
//! ```rust,ignore
//! use duende_test::{DaemonTestHarness, ChaosConfig};
//!
//! let harness = DaemonTestHarness::new()
//!     .with_chaos(ChaosConfig {
//!         latency_injection: Some((0.1, Duration::from_millis(500))),
//!         error_injection: Some(0.05),
//!         ..Default::default()
//!     })
//!     .build();
//!
//! let handle = harness.spawn(my_daemon).await?;
//! assert!(handle.health_check().await?.is_healthy());
//! ```

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod chaos;
pub mod error;
pub mod harness;
pub mod load;

pub use chaos::{ChaosConfig, ChaosInjector};
pub use error::{Result, TestError};
pub use harness::DaemonTestHarness;
pub use load::{LoadTestConfig, LoadTester};
