//! Popperian Falsification Tests for Duende
//!
//! # Reference
//! Popper, K. (1959). *The Logic of Scientific Discovery*. Routledge.
//!
//! > "A theory which is not refutable by any conceivable event is non-scientific."
//!
//! Each test in this module attempts to falsify a specific claim about Duende.
//! A passing test means the claim survived the falsification attempt.

// Allow test-specific patterns that are denied in production code
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::default_trait_access)]

mod lifecycle;
mod observability;
mod platform;
mod policy;
mod resources;
