//! Sans-IO state machine for the Muni Town Arbiter v2.
//!
//! This crate implements the `town.muni.arbiter.*` XRPC lexicons using an
//! embedded Regorus (Rego) engine with suspendable RegoVM execution for
//! pluggable authorization policies.
//!
//! Architecture:
//! - **`core`** — Data model types (Arbiter, Space, Member)
//! - **`policy`** — Rego policy type definitions and helpers
//! - **`policy_vm`** — Policy execution pool using suspendable RegoVM

pub mod core;
pub mod policy;
pub mod policy_vm;

pub use core::*;
