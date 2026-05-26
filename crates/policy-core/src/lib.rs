//! Sans-IO policy evaluation engine with suspendable XRPC resolution.
//!
//! Provides [`VmSession`], which wraps a single `RegoVM` for evaluating a
//! Rego policy with suspendable execution. Policies can request host data
//! via two built-in functions without blocking:
//!
//! - `xrpc_local(path, input)` — request data from the local host
//! - `xrpc_remote(did, path, input)` — request data from a remote DID
//!
//! When the policy calls one of these functions, the VM suspends and returns
//! a [`HostRequest`]. The caller resolves it and calls
//! [`VmSession::resume`] with the result.
//!
//! **Important:** XRPC requests made from the policy are **always queries**
//! (read-only), never procedures. Evaluating a policy must never change the
//! state of the system. The host should enforce this — if the policy requests
//! a mutation endpoint the host should either reject it or treat it as a
//! no-op query.
//!
//! Because `RegoVM` is `Send + Sync` (with regorus's `arc` feature), each
//! caller owns its own `VmSession` — no pool needed.

use std::collections::BTreeMap;

use regorus::{
  PolicyModule, Value,
  languages::rego::compiler::{Compiler, CompilerError},
  rvm::{
    RegoVM,
    vm::{ExecutionMode, ExecutionState, SuspendReason, VmError},
  },
};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during policy compilation or VM execution.
#[derive(Debug, thiserror::Error)]
pub enum Error {
  /// Policy compilation (parsing, type-checking, RVM bytecode generation) failed.
  #[error("Policy compilation failed: {0}")]
  Compile(Box<dyn std::error::Error + Send + Sync>),

  /// VM execution error (runtime failures from the RegoVM).
  #[error("VM execution error: {0}")]
  Vm(#[from] VmError),

  /// The VM suspended but the suspension reason could not be parsed into a
  /// known [`HostRequest`].
  #[error("VM suspended with unrecognized reason")]
  UnexpectedSuspension,

  /// The VM entered an unexpected state after an execute or resume call.
  #[error("Unexpected VM state: {0}")]
  UnexpectedState(String),

  /// [`resume`](VmSession::resume) was called when the VM was not in a
  /// suspended state.
  #[error("VM is not in a suspended state")]
  VmNotSuspended,
}

impl From<CompilerError> for Error {
  fn from(err: CompilerError) -> Self {
    Error::Compile(Box::new(err))
  }
}

impl From<regorus::languages::rego::compiler::SpannedCompilerError> for Error {
  fn from(err: regorus::languages::rego::compiler::SpannedCompilerError) -> Self {
    Error::Compile(Box::new(err))
  }
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A request from the policy VM to the host.
///
/// Returned inside [`VmResult::Suspended`] when the policy calls one of the
/// built-in XRPC functions. The host must resolve the request and call
/// [`VmSession::resume`] with the result.
#[derive(Debug, Clone, PartialEq)]
pub enum HostRequest {
  /// Resolve an XRPC query on the local host.
  XrpcLocal {
    /// The XRPC method NSID (e.g. `"town.muni.arbiter.resolveSpaceMembers"`).
    path: String,
    /// The input parameters.
    input: Value,
  },
  /// Resolve an XRPC query on a remote host identified by DID.
  XrpcRemote {
    /// The DID of the remote host.
    did: String,
    /// The XRPC method NSID.
    path: String,
    /// The input parameters.
    input: Value,
  },
}

/// Result of evaluating or resuming a policy without errors.
///
/// The non-error outcomes are completion with a [`Value`] or suspension
/// with a [`HostRequest`]. Errors are represented separately so that
/// [`start`](VmSession::start) and [`resume`](VmSession::resume) return
/// `Result<VmResult, Error>`, enabling the `?` operator.
#[derive(Debug)]
pub enum VmResult {
  /// Evaluation completed with a value.
  Completed(Value),
  /// Evaluation suspended, waiting for host to provide data.
  Suspended(HostRequest),
}

// ---------------------------------------------------------------------------
// VmSession
// ---------------------------------------------------------------------------

/// A single RegoVM evaluation session with built-in XRPC support.
///
/// Each session wraps one [`RegoVM`] and a response cache. When the policy
/// calls `xrpc_local` or `xrpc_remote`, the VM suspends and returns a
/// [`HostRequest`] to the caller. The caller resolves it and calls
/// [`resume`](Self::resume).
///
/// If the policy calls the same XRPC function with the same arguments
/// multiple times (e.g., from different rule bodies), the cache
/// auto-resolves the duplicate without returning to the caller.
///
/// `VmSession` is `Send + Sync` because `RegoVM` is, so it can be stored
/// directly in a state machine without an external pool.
#[derive(Debug)]
pub struct VmSession {
  vm: RegoVM,
  /// Cache of resolved XRPC responses keyed by the request argument.
  /// When a duplicate call is made, the cached response auto-resumes.
  response_cache: BTreeMap<Value, Value>,
}

impl VmSession {
  /// Create a new session, compiling the policy and configuring the VM.
  ///
  /// `entry_points` specifies which rules to compile. The first entry
  /// point is the main rule. Pass multiple entry points when the compiled
  /// program needs to support querying different rules (e.g.,
  /// `resolved_members` in addition to `allow`).
  ///
  /// This method only sets up the VM — it does **not** start executing.
  /// Call [`start`](Self::start) to begin evaluation.
  pub fn new(
    policy: &str,
    data: &Value,
    input: &Value,
    entry_points: &[&str],
  ) -> Result<Self, Error> {
    let vm = build_vm(policy, data, input, entry_points)?;
    Ok(Self {
      vm,
      response_cache: BTreeMap::new(),
    })
  }

  /// Start (or restart) evaluation from the first entry point.
  ///
  /// Runs the VM until it either completes or suspends waiting for host
  /// data. If suspended, the caller should resolve the [`HostRequest`]
  /// and call [`resume`](Self::resume).
  ///
  /// Errors from the VM are returned via the `Err` branch so callers can
  /// use the `?` operator.
  pub fn start(&mut self) -> Result<VmResult, Error> {
    self.run()
  }

  /// Resume a suspended evaluation with the host's response.
  ///
  /// If the VM suspends again for the same request (duplicate call from
  /// a different rule body) and we have a cached response, we
  /// auto-resume without returning to the caller.
  ///
  /// Errors from the VM are returned via the `Err` branch so callers can
  /// use the `?` operator.
  pub fn resume(&mut self, resume_value: &Value) -> Result<VmResult, Error> {
    // Cache the response under the current suspension's cache key
    if let Some(key) = self.extract_cache_key() {
      self.response_cache.insert(key, resume_value.clone());
    }

    // First resume with the caller-provided value
    let first = self.resume_one_step(resume_value)?;
    match first {
      VmResult::Completed(_) => return Ok(first),
      VmResult::Suspended(_) => {}
    }

    // Auto-resolve cached duplicates
    loop {
      let key = match self.vm.suspend_reason().and_then(cache_key_from_reason) {
        Some(k) => k,
        None => return Err(Error::VmNotSuspended),
      };

      if let Some(cached) = self.response_cache.get(&key).cloned() {
        let result = self.resume_one_step(&cached)?;
        match result {
          VmResult::Completed(_) => return Ok(result),
          VmResult::Suspended(_) => continue,
        }
      } else {
        // Not cached — ask the caller
        let request = match self.vm.suspend_reason().and_then(host_request_from_reason) {
          Some(r) => r,
          None => return Err(Error::UnexpectedSuspension),
        };
        return Ok(VmResult::Suspended(request));
      }
    }
  }

  // -----------------------------------------------------------------------
  // Internal helpers
  // -----------------------------------------------------------------------

  /// Run the VM to completion or first suspension.
  fn run(&mut self) -> Result<VmResult, Error> {
    match self.vm.execute() {
      Ok(_) => match self.vm.execution_state() {
        ExecutionState::Completed { result } => Ok(VmResult::Completed(result.clone())),
        ExecutionState::Suspended { reason, .. } => match host_request_from_reason(reason) {
          Some(request) => Ok(VmResult::Suspended(request)),
          None => Err(Error::UnexpectedSuspension),
        },
        state => Err(Error::UnexpectedState(format!(
          "Unexpected VM state after execute: {state:?}"
        ))),
      },
      Err(e) => Err(Error::Vm(e)),
    }
  }

  /// Extract the cache key (the argument Value) from the current suspension.
  fn extract_cache_key(&self) -> Option<Value> {
    self.vm.suspend_reason().and_then(cache_key_from_reason)
  }

  /// Resume one step: inject the resolved value and process one continuation.
  fn resume_one_step(&mut self, value: &Value) -> Result<VmResult, Error> {
    match self.vm.resume(Some(value.clone())) {
      Ok(_) => match self.vm.execution_state() {
        ExecutionState::Completed { result } => Ok(VmResult::Completed(result.clone())),
        ExecutionState::Suspended { reason, .. } => match host_request_from_reason(reason) {
          Some(request) => Ok(VmResult::Suspended(request)),
          None => Err(Error::UnexpectedSuspension),
        },
        ExecutionState::Error { error } => Err(Error::Vm(error.clone())),
        _ => panic!("Unexpected Rego VM state."),
      },
      Err(e) => Err(Error::Vm(e)),
    }
  }
}

// ---------------------------------------------------------------------------
// SuspendReason → HostRequest conversion
// ---------------------------------------------------------------------------

/// Parse a [`SuspendReason::HostAwait`] into a [`HostRequest`].
///
/// The argument is expected to be a Rego object produced by the policy
/// extension functions (see [`POLICY_EXTENSIONS`]).
fn host_request_from_reason(reason: &SuspendReason) -> Option<HostRequest> {
  match reason {
    SuspendReason::HostAwait {
      argument,
      identifier,
      ..
    } => {
      let ident = identifier.as_string().ok()?;
      let map = argument.as_object().ok()?;

      match ident.as_ref() {
        "xrpc_local" => Some(HostRequest::XrpcLocal {
          path: map.get(&Value::from("path"))?.as_string().ok()?.to_string(),
          input: map.get(&Value::from("input"))?.clone(),
        }),
        "xrpc_remote" => Some(HostRequest::XrpcRemote {
          did: map.get(&Value::from("did"))?.as_string().ok()?.to_string(),
          path: map.get(&Value::from("path"))?.as_string().ok()?.to_string(),
          input: map.get(&Value::from("input"))?.clone(),
        }),
        _ => None,
      }
    }
    _ => None,
  }
}

/// Derive a cache key from a suspend reason — the argument `Value` itself.
///
/// Duplicate calls with the same argument produce the same key (via `Ord`).
fn cache_key_from_reason(reason: &SuspendReason) -> Option<Value> {
  match reason {
    SuspendReason::HostAwait { argument, .. } => Some(argument.clone()),
    _ => None,
  }
}

// ---------------------------------------------------------------------------
// VM lifecycle
// ---------------------------------------------------------------------------

/// Build and configure a suspendable RegoVM from a policy, data, and input.
fn build_vm(
  policy: &str,
  data: &Value,
  input: &Value,
  entry_points: &[&str],
) -> Result<RegoVM, Error> {
  // Concatenate policy with built-in extensions
  let full_source = {
    let mut s = String::from(policy);
    s.push_str("\n\n");
    s.push_str(POLICY_EXTENSIONS);
    s
  };

  // Primary compilation pass: parses modules and resolves references
  let compiled_policy = regorus::compile_policy_with_entrypoint(
    data.clone(),
    &[PolicyModule {
      id: "".into(),
      content: full_source.into(),
    }],
    entry_points[0].into(),
  )
  .map_err(|e| Error::Compile(e.into()))?;

  // Secondary pass: compile to RVM bytecode with all entry points
  let program = Compiler::compile_from_policy(&compiled_policy, entry_points)?;

  // Configure the VM
  let mut vm = RegoVM::new();
  vm.load_program(program);
  vm.set_data(data.clone())?;
  vm.set_input(input.clone());
  vm.set_execution_mode(ExecutionMode::Suspendable);

  Ok(vm)
}

// ---------------------------------------------------------------------------
// Built-in Rego extensions
// ---------------------------------------------------------------------------

/// Rego extension code defining built-in XRPC functions.
///
/// These are automatically appended to every policy evaluated through
/// [`VmSession`]. They wrap `__builtin_host_await` with structured
/// request arguments that the Rust side parses into [`HostRequest`].
pub const POLICY_EXTENSIONS: &str = r#"
# Request data from the local host via XRPC.
# The VM suspends until the host resolves this and provides the response.
xrpc_local(path, inp) := result if {
    result := __builtin_host_await({"path": path, "input": inp}, "xrpc_local")
}

# Request data from a remote host via XRPC.
# The VM suspends until the host resolves this and provides the response.
xrpc_remote(did, path, inp) := result if {
    result := __builtin_host_await({"did": did, "path": path, "input": inp}, "xrpc_remote")
}
"#;

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate that a policy source parses correctly.
///
/// Tests both the user's policy and the built-in extensions.
pub fn validate_policy(policy: &str) -> Result<(), Error> {
  let mut engine = regorus::Engine::new();
  let full_source = format!("{policy}\n\n{POLICY_EXTENSIONS}");
  engine
    .add_policy("validate.rego".to_string(), full_source)
    .map_err(|e| Error::Compile(e.into()))?;
  Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use std::collections::BTreeMap;

  use super::*;

  fn empty_obj() -> Value {
    Value::new_object()
  }

  fn obj(pairs: Vec<(&str, Value)>) -> Value {
    Value::from(
      pairs
        .into_iter()
        .map(|(k, v)| (Value::from(k), v))
        .collect::<BTreeMap<Value, Value>>(),
    )
  }

  #[test]
  fn test_validate_valid_policy() {
    assert!(validate_policy("package example\nimport rego.v1\ndefault allow := true").is_ok());
  }

  #[test]
  fn test_validate_invalid_policy() {
    assert!(validate_policy("garbage {").is_err());
  }

  #[test]
  fn test_sync_allow() {
    let policy = r#"
            package example
            import rego.v1
            default allow := true
        "#;
    let data = empty_obj();
    let input = obj(vec![("test", Value::from(true))]);

    let mut session =
      VmSession::new(policy, &data, &input, &["data.example.allow"]).unwrap();
    let result = session.start().unwrap();
    match result {
      VmResult::Completed(value) => assert_eq!(value, Value::from(true)),
      VmResult::Suspended(r) => panic!("Unexpected suspension: {r:?}"),
    }
  }

  #[test]
  fn test_custom_entry_point() {
    let policy = r#"
            package example
            import rego.v1
            default allow := false
            custom_result := "hello" if { true }
        "#;
    let data = empty_obj();
    let input = empty_obj();

    let mut session =
      VmSession::new(policy, &data, &input, &["data.example.custom_result"]).unwrap();
    let result = session.start().unwrap();
    match result {
      VmResult::Completed(value) => assert_eq!(value, Value::from("hello")),
      VmResult::Suspended(r) => panic!("Unexpected suspension: {r:?}"),
    }
  }

  #[test]
  fn test_deny() {
    let policy = r#"
            package example
            import rego.v1
            default allow := false
        "#;
    let data = empty_obj();
    let input = empty_obj();

    let mut session =
      VmSession::new(policy, &data, &input, &["data.example.allow"]).unwrap();
    let result = session.start().unwrap();
    match result {
      VmResult::Completed(value) => assert_eq!(value, Value::from(false)),
      VmResult::Suspended(_) => panic!("Unexpected suspension"),
    }
  }

  #[test]
  fn test_multiple_entry_points() {
    let policy = r#"
            package example
            import rego.v1
            default allow := false
            items := ["a", "b", "c"] if { true }
        "#;
    let data = empty_obj();
    let input = empty_obj();

    let mut session = VmSession::new(
      policy,
      &data,
      &input,
      &["data.example.allow", "data.example.items"],
    )
    .unwrap();
    let result = session.start().unwrap();
    match result {
      VmResult::Completed(value) => {
        // Should evaluate the first entry point (allow)
        assert_eq!(value, Value::from(false))
      }
      VmResult::Suspended(_) => panic!("Unexpected suspension"),
    }
  }

  #[test]
  fn test_xrpc_local_suspend_and_resume() {
    let policy = r#"
            package example
            import rego.v1

            default allow := false

            allow if {
                result := xrpc_local("some.query", {"key": "value"})
                result == "resolved"
            }
        "#;
    let data = empty_obj();
    let input = empty_obj();

    let mut session =
      VmSession::new(policy, &data, &input, &["data.example.allow"]).unwrap();
    let result = session.start().unwrap();
    let request = match result {
      VmResult::Suspended(r) => r,
      VmResult::Completed(v) => panic!("Expected suspension, got completed: {v:?}"),
    };

    match &request {
      HostRequest::XrpcLocal { path, .. } => assert_eq!(path, "some.query"),
      _ => panic!("Expected XrpcLocal"),
    }

    let result = session.resume(&Value::from("resolved")).unwrap();
    match result {
      VmResult::Completed(value) => assert_eq!(value, Value::from(true)),
      VmResult::Suspended(r) => panic!("Unexpected suspension: {r:?}"),
    }
  }

  #[test]
  fn test_xrpc_remote_suspend_and_resume() {
    let policy = r#"
            package example
            import rego.v1

            default allow := false

            allow if {
                result := xrpc_remote("did:plc:remote", "remote.query", {"foo": "bar"})
                result == "remote_data"
            }
        "#;
    let data = empty_obj();
    let input = empty_obj();

    let mut session =
      VmSession::new(policy, &data, &input, &["data.example.allow"]).unwrap();
    let result = session.start().unwrap();
    let request = match result {
      VmResult::Suspended(r) => r,
      VmResult::Completed(v) => panic!("Expected suspension, got completed: {v:?}"),
    };

    match &request {
      HostRequest::XrpcRemote { did, path, .. } => {
        assert_eq!(did, "did:plc:remote");
        assert_eq!(path, "remote.query");
      }
      _ => panic!("Expected XrpcRemote"),
    }

    let result = session.resume(&Value::from("remote_data")).unwrap();
    match result {
      VmResult::Completed(value) => assert_eq!(value, Value::from(true)),
      VmResult::Suspended(r) => panic!("Unexpected suspension: {r:?}"),
    }
  }

  #[test]
  fn test_auto_resolve_cached_duplicate() {
    // Policy that calls xrpc_local twice with the same arguments from
    // different rule bodies — the second call should auto-resolve from cache.
    let policy = r#"
            package example
            import rego.v1

            default allow := false

            allow if {
                a := xrpc_local("my.query", {"x": 1})
                b := xrpc_local("my.query", {"x": 1})
                a == "ok"
                b == "ok"
            }
        "#;
    let data = empty_obj();
    let input = empty_obj();

    let mut session =
      VmSession::new(policy, &data, &input, &["data.example.allow"]).unwrap();
    let result = session.start().unwrap();
    let _request = match result {
      VmResult::Suspended(r) => r,
      VmResult::Completed(v) => panic!("Expected suspension, got completed: {v:?}"),
    };

    // Resume once — this should also auto-resolve the duplicate and complete
    let result = session.resume(&Value::from("ok")).unwrap();
    match result {
      VmResult::Completed(value) => assert_eq!(value, Value::from(true)),
      VmResult::Suspended(_) => panic!("Expected completion after cache auto-resolve"),
    }
  }
}
