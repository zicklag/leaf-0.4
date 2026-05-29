//! Async IO trait and event processing for the arbiter state machine.
//!
//! Provides an [`Io`] trait that users implement to supply the network
//! layer, and a [`process_event`] helper that drives one event through
//! the state machine recursively — fulfilling IO actions and feeding
//! remote results back until the policy completes.
//!
//! No external runtime dependency: the trait uses native `async fn`
//! (stable in Rust 1.75+, edition 2024).  Callers may build their own
//! event loop on top of `process_event`, or integrate the `Io` impl
//! with any async runtime (tokio, smol, etc.).
//!
//! # Example (conceptual)
//!
//! ```ignore
//! use arbiter_core::{StateMachine, Event, futures::Io};
//!
//! struct MyIo;
//!
//! impl Io for MyIo {
//!     async fn send_response(&mut self, body: Value, status: u16) {
//!         // … send HTTP response …
//!     }
//!     async fn remote_request(
//!         &mut self,
//!         did: &str,
//!         method: &policy_core::XrpcMethod,
//!         nsid: &str,
//!         input: Value,
//!     ) -> (u16, Value) {
//!         // … make HTTP request, return (status, body) …
//!     }
//! }
//!
//! # async fn example() {
//! let mut sm = StateMachine::create(…);
//! let mut io = MyIo;
//!
//! // Drive one incoming request to completion.
//! arbiter_core::futures::process_event(
//!     &mut io,
//!     &mut sm,
//!     Event::IncomingXrpc { … },
//! ).await;
//! # }
//! ```

use std::sync::Arc;

use serde_json::Value;

use crate::{Event, IoAction, StateMachine, policy_core::XrpcMethod};

// ---------------------------------------------------------------------------
// IO trait
// ---------------------------------------------------------------------------

/// Network layer for an arbiter state machine.
///
/// Implement this trait to connect the sans-IO [`StateMachine`] to the
/// real world — sending XRPC responses to clients and making remote
/// XRPC requests to other arbiters or services.
pub trait Io {
    /// Send an XRPC response back to the client that made the original
    /// request.
    fn send_response(
        &self,
        body: Value,
        status: u16,
    ) -> impl std::future::Future<Output = ()> + Send;

    /// Perform a remote XRPC request and return the result as
    /// `(status, body)`.
    ///
    /// This is called when the policy engine invokes `xrpc_remote`.
    /// The implementation should make the network call and return the
    /// response.  Errors (timeouts, network failures) should be reported
    /// as an appropriate HTTP status code so the policy can handle them.
    fn remote_request(
        &self,
        did: &str,
        method: &XrpcMethod,
        nsid: &str,
        input: Value,
    ) -> impl std::future::Future<Output = (u16, Value)> + Send;
}

// ---------------------------------------------------------------------------
// Event processing
// ---------------------------------------------------------------------------

/// Drive a single [`Event`] through the state machine to completion.
///
/// The function processes the event, fulfils every [`IoAction`] that the
/// state machine emits, and feeds remote results back recursively — the
/// returned future resolves only after the policy has finished (either
/// completed or errored).
///
/// If the policy suspends on `xrpc_remote`, this function blocks the
/// current task until the remote request finishes.  For concurrent
/// processing of multiple in-flight requests (stored in the state
/// machine's [`pending_jobs`](StateMachine::pending_jobs)), callers
/// should provide their own event loop that calls this function from
/// separate tasks or uses a [`select`](futures_util::select)-style
/// pattern.
///
/// # No more IO actions
///
/// After the policy completes, the state machine emits exactly one
/// [`IoAction::SendXrpcResponse`] (or errors out).  This function
/// sends that response and returns.
pub async fn process_event(
    io: &impl Io,
    sm: Arc<async_lock::Mutex<StateMachine>>,
    event: Event,
) {
    let mut stack = vec![event];

    while let Some(event) = stack.pop() {
        let actions = sm.lock().await.handle_event(event);
        for action in actions {
            match action {
                IoAction::SendXrpcResponse { body, status } => {
                    io.send_response(body, status).await;
                }
                IoAction::SendXrpcRequest {
                    did,
                    method,
                    nsid,
                    input,
                    job_id,
                } => {
                    let (status, body) = io.remote_request(&did, &method, &nsid, input).await;
                    stack.push(Event::XrpcRemoteResult {
                        status,
                        body,
                        job_id,
                    });
                }
            }
        }
    }
}
