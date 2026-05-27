//! Sans-IO arbiter core — data model, policy orchestration, and state machine.
//!
//! This crate provides the core arbiter data types and a suspendable
//! policy-driven state machine for the `town.muni.arbiter.*` XRPC lexicons.
//! It depends on [`policy-core`] for Rego policy evaluation with suspendable
//! XRPC resolution.
//!
//! # Architecture
//!
//! - **`core`** — [`ArbiterCore`], the main state machine. It stores all
//!   arbiters and manages policy evaluation with suspension/resume for
//!   remote member resolution.
//! - **Policy evaluation** uses [`policy_core::VmSession`] with the
//!   suspendable RegoVM. When a policy calls `xrpc_remote`, the core
//!   surfaces a [`HostRequest`] to the caller (IO layer / test harness).
//!
//! # Sans-IO Pattern
//!
//! All methods are synchronous and pure. Methods that require remote data
//! return [`OpStep::NeedRemoteData`] with a `job_id`. The caller resolves
//! the remote data and calls [`ArbiterCore::provide_remote_data`] to resume.

pub mod core;
pub mod policy;

pub use core::*;
pub use policy::*;
