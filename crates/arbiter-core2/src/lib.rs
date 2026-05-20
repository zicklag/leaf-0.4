//! Sans-IO state machine for the Muni Town Arbiter v2.
//!
//! This crate implements the `town.muni.arbiter.*` XRPC lexicons using an
//! embedded Regorus (Rego) engine for pluggable authorization policies.
//!
//! Architecture:
//! - **`core`** — Data model types (Arbiter, Space, Member, Job)
//! - **`policy`** — Rego policy evaluation with custom builtins

pub mod core;
pub mod policy;

pub use core::*;
