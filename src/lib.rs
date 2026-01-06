//! Duende: Cross-Platform Daemon Tooling Framework
//!
//! Part of the PAIML Sovereign AI Stack.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use duende::prelude::*;
//!
//! // Re-exports from sub-crates for convenience
//! ```

pub use duende_core as core;
pub use duende_mlock as mlock;
pub use duende_platform as platform;

/// Prelude module for common imports.
pub mod prelude {
    pub use duende_core::{
        Daemon, DaemonConfig, DaemonContext, DaemonId, DaemonMetrics, DaemonStatus, ExitReason,
        HealthStatus, Signal,
    };
    pub use duende_platform::{
        is_memory_locked, lock_daemon_memory, DaemonHandle, MlockResult, Platform,
        PlatformAdapter,
    };
}
