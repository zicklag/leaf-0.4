//! Rust implementation of the arbiter core and arbiter server state machines,
//! closely following the Quint specification in `spec/arbiter/arbiter.qnt`.
//!
//! Uses the `im` crate for persistent, functional data structures (HashMap, HashSet, Vector)
//! to mirror Quint's immutable-update semantics.

pub mod core;
pub mod server;

pub use core::*;
pub use server::*;
