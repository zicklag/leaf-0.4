//! Rust implementation of the arbiter core and arbiter server state machines,
//! closely following the Quint specification in `spec/arbiter/arbiter.qnt`.
//!
//! Uses the `im` crate for persistent, functional data structures (HashMap, HashSet, Vector)
//! to mirror Quint's immutable-update semantics.

pub mod core;
#[cfg(test)]
pub mod mbt;
pub mod server;
pub mod service;

pub use core::*;
pub use server::*;
pub use service::*;
