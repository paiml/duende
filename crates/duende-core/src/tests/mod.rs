//! Test infrastructure for falsification testing.
//!
//! # Certeza Methodology - Popperian Falsification
//!
//! Per the Iron Lotus Framework and daemon-tools-spec.md Section 9.2,
//! this module provides infrastructure for 110 falsification tests
//! organized by category:
//!
//! | Category | ID Range | Description | Count |
//! |----------|----------|-------------|-------|
//! | A | F001-F020 | Daemon Lifecycle | 20 |
//! | B | F021-F040 | Signal Handling | 20 |
//! | C | F041-F060 | Resource Limits | 20 |
//! | D | F061-F080 | Health Checks | 20 |
//! | E | F081-F100 | Observability | 20 |
//! | F | F101-F110 | Platform Adapters | 10 |

pub mod falsification;
pub mod harness;
pub mod health;
pub mod lifecycle;
pub mod mocks;
pub mod observability;
pub mod platform;
pub mod resource;
pub mod signal;

pub use falsification::FalsificationTest;
pub use harness::TestHarness;
pub use mocks::MockDaemon;
