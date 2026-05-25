//! WebAssembly bindings for policy-core.
//!
//! Provides a [`PolicySession`] that wraps the sans-IO [`VmSession`] for use
//! in the browser. Complex types are passed as [`JsValue`] and deserialized
//! by `serde-wasm-bindgen` — JS callers pass objects/arrays directly without
//! manual stringification.
//!
//! Errors are thrown as JavaScript exceptions via `Result<_, JsValue>`.

use policy_core::{HostRequest, VmResult, VmSession};
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// PolicySession
// ---------------------------------------------------------------------------

/// A single Rego policy evaluation session, exposed to JavaScript/WASM.
///
/// Create one with [`PolicySession::new`], call [`start`](Self::start) to
/// begin evaluation, and [`resume`](Self::resume) if the policy suspends
/// waiting for host data.
///
/// Errors (compilation failures, VM errors, etc.) are thrown as JavaScript
/// exceptions.
#[wasm_bindgen]
pub struct PolicySession {
  inner: VmSession,
}

#[wasm_bindgen]
impl PolicySession {
  /// Create a new policy session.
  ///
  /// - `policy` — the raw Rego policy source string
  /// - `data` — a JS object with the policy data document
  /// - `input` — a JS object with the policy input document
  /// - `entry_points` — an array of entry-point paths (e.g. `["data.example.allow"]`)
  #[wasm_bindgen(constructor)]
  pub fn new(
    policy: &str,
    data: JsValue,
    input: JsValue,
    entry_points: Vec<String>,
  ) -> Result<PolicySession, JsValue> {
    let data = serde_wasm_bindgen::from_value::<regorus::Value>(data)
      .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let input = serde_wasm_bindgen::from_value::<regorus::Value>(input)
      .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let entry_points_refs: Vec<&str> = entry_points.iter().map(|s| s.as_str()).collect();

    let session = VmSession::new(policy, &data, &input, &entry_points_refs)
      .map_err(|e| JsValue::from_str(&e.to_string()))?;

    Ok(PolicySession { inner: session })
  }

  /// Start evaluation of the policy. Throws on error.
  pub fn start(&mut self) -> Result<PolicyResult, JsValue> {
    let result = self.inner.start().map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(PolicyResult { inner: result })
  }

  /// Resume a suspended evaluation with the host's response.
  ///
  /// `resume_value` is the value to inject as the response to the suspended
  /// XRPC call — pass any JS value (object, array, string, number, etc.).
  /// Throws on error.
  pub fn resume(&mut self, resume_value: JsValue) -> Result<PolicyResult, JsValue> {
    let value: regorus::Value = serde_wasm_bindgen::from_value(resume_value)
      .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let result = self.inner.resume(&value).map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(PolicyResult { inner: result })
  }
}

// ---------------------------------------------------------------------------
// PolicyResult — typed return value for start / resume
// ---------------------------------------------------------------------------

/// The outcome of a policy evaluation step.
///
/// Check [`status`](Self::status) to discriminate:
/// - `"completed"` → read [`value`](Self::value)
/// - `"suspended"` → read [`request`](Self::request)
#[wasm_bindgen]
pub struct PolicyResult {
  inner: VmResult,
}

#[wasm_bindgen]
impl PolicyResult {
  /// Either `"completed"` or `"suspended"`.
  #[wasm_bindgen(getter)]
  pub fn status(&self) -> String {
    match &self.inner {
      VmResult::Completed(_) => "completed",
      VmResult::Suspended(_) => "suspended",
    }
    .into()
  }

  /// The policy's result value. Present only when `status` is `"completed"`.
  #[wasm_bindgen(getter)]
  pub fn value(&self) -> Option<JsValue> {
    match &self.inner {
      VmResult::Completed(v) => serde_wasm_bindgen::to_value(v).ok(),
      VmResult::Suspended(_) => None,
    }
  }

  /// The host request. Present only when `status` is `"suspended"`.
  #[wasm_bindgen(getter)]
  pub fn request(&self) -> Option<HostRequestView> {
    match &self.inner {
      VmResult::Completed(_) => None,
      VmResult::Suspended(r) => Some(HostRequestView { inner: r.clone() }),
    }
  }
}

// ---------------------------------------------------------------------------
// HostRequestView — typed request details
// ---------------------------------------------------------------------------

/// Details of a suspended evaluation waiting for host data.
#[wasm_bindgen]
pub struct HostRequestView {
  inner: HostRequest,
}

#[wasm_bindgen]
impl HostRequestView {
  /// The kind of request: `"xrpc_local"` or `"xrpc_remote"`.
  #[wasm_bindgen(getter)]
  pub fn kind(&self) -> String {
    match &self.inner {
      HostRequest::XrpcLocal { .. } => "xrpc_local",
      HostRequest::XrpcRemote { .. } => "xrpc_remote",
    }
    .into()
  }

  /// The XRPC method NSID.
  #[wasm_bindgen(getter)]
  pub fn path(&self) -> String {
    match &self.inner {
      HostRequest::XrpcLocal { path, .. } | HostRequest::XrpcRemote { path, .. } => path.clone(),
    }
  }

  /// The DID of the remote host. Present only for `"xrpc_remote"` requests.
  #[wasm_bindgen(getter)]
  pub fn did(&self) -> Option<String> {
    match &self.inner {
      HostRequest::XrpcLocal { .. } => None,
      HostRequest::XrpcRemote { did, .. } => Some(did.clone()),
    }
  }

  /// The input parameters for the request.
  #[wasm_bindgen(getter)]
  pub fn input(&self) -> JsValue {
    match &self.inner {
      HostRequest::XrpcLocal { input, .. } | HostRequest::XrpcRemote { input, .. } => {
        serde_wasm_bindgen::to_value(input).unwrap_or(JsValue::UNDEFINED)
      }
    }
  }
}

// ---------------------------------------------------------------------------
// Standalone helpers
// ---------------------------------------------------------------------------

/// Validate a Rego policy string. Throws a JavaScript exception on error.
#[wasm_bindgen]
pub fn validate_policy(policy: &str) -> Result<(), JsValue> {
  policy_core::validate_policy(policy).map_err(|e| JsValue::from_str(&e.to_string()))
}
