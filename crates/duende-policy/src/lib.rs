//! # duende-policy
//!
//! Policy enforcement for the Duende daemon framework.
//!
//! This crate provides:
//! - **Quality gates**: PMAT-based code quality enforcement
//! - **Circuit breakers**: 3-state failure protection
//! - **Resource limiters**: cgroups/setrlimit enforcement
//! - **Jidoka automation**: Stop-on-error with recommendations
//!
//! ## Iron Lotus Framework
//!
//! - **Jidoka** (自働化): Automatic stop on quality violations
//! - **Poka-Yoke** (ポカヨケ): Mistake-proofing via policy enforcement
//! - **Standardized Work**: Consistent policy application
//!
//! ## Example
//!
//! ```rust,ignore
//! use duende_policy::{CircuitBreaker, CircuitState};
//!
//! let mut breaker = CircuitBreaker::new(5, Duration::from_secs(30));
//!
//! if breaker.allow() {
//!     match do_work().await {
//!         Ok(_) => breaker.record_success(),
//!         Err(_) => breaker.record_failure(),
//!     }
//! }
//! ```

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod circuit_breaker;
pub mod error;
pub mod gate;
pub mod jidoka;
pub mod limiter;

pub use circuit_breaker::{CircuitBreaker, CircuitState};
pub use error::{PolicyError, Result};
pub use gate::{GateConfig, GateResult, QualityAnalysis, QualityGate, QualityViolation};
pub use jidoka::{
    CheckItem, Evidence, JidokaCheck, JidokaGate, JidokaResult, JidokaViolation, ViolationKind,
};
pub use limiter::{ResourceLimiter, ResourceLimits};
