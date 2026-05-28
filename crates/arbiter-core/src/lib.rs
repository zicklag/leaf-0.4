//! Pure sans-IO arbiter state machine.
//!
//! The state machine ([`StateMachine`]) manages a **single arbiter** — its
//! spaces, members, and policy evaluation.  Everything outside that
//! (routing, persistence, cross-arbiter remote resolution) belongs to the
//! IO layer / harness.
//!
//! # Architecture
//!
//! ```text
//! Event ──→ StateMachine ──→ Vec<IoAction>
//! ```
//!
//! Feed the machine an [`Event`]; it returns zero or more [`IoAction`]s.
//! The IO layer fulfills those actions and feeds results back as new
//! events.
//!
//! Policy evaluation via [`policy_core::VmSession`] lives inside the
//! machine.  `xrpc_local` queries resolve against the local arbiter's
//! state.  `xrpc_remote` queries produce an [`IoAction::XrpcRemote`];
//! the IO layer resolves them by consulting the appropriate remote
//! arbiter (or a local stand-in, in tests).

#![deny(rust_2018_idioms)]

use std::collections::{BTreeMap, HashMap};

use policy_core::{HostRequest, VmResult, VmSession};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use policy_core;

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

pub type Did = String;
pub type SpaceKey = String;
pub type JobId = u64;

// ---------------------------------------------------------------------------
// Events  (input to the state machine)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Event {
    /// An incoming XRPC call from the HTTP server, with caller DID already
    /// resolved by the IO layer.
    IncomingXrpc { method: String, params: Value, caller_did: Did },

    /// A remote XRPC query (from policy evaluation) completed.
    XrpcRemoteResult { result: Result<Value, String>, job_id: JobId },
}

// ---------------------------------------------------------------------------
// IO actions  (output of the state machine)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum IoAction {
    /// Send an XRPC response to the client.
    SendResponse { body: Value, status: u16, job_id: JobId },

    /// Resolve a remote XRPC query from the policy engine.
    XrpcRemote { did: String, path: String, input: Value, job_id: JobId },

    /// Proxy an XRPC call to a backend service.
    ProxyXrpc { backend_url: String, path: String, params: Value, job_id: JobId },
}

// ---------------------------------------------------------------------------
// JSON-serialisable types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberEntry {
    pub did: Did,
    pub access: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Space {
    pub key: SpaceKey,
    pub space_type: String,
    pub config: Value,
    pub members: Vec<MemberEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbiterState {
    pub did: Did,
    pub version: u64,
    pub config: Value,
    pub policy: String,
    pub spaces: HashMap<SpaceKey, Space>,
}

impl ArbiterState {
    /// Create a new arbiter with the given DID and initial owner.
    pub fn create(did: Did, config: Value, policy: String, owner_did: Did) -> Self {
        let admin_space = Space {
            key: "$admin".into(),
            space_type: "town.muni.arbiter.config.adminSpace".into(),
            config: Value::Null,
            members: vec![MemberEntry {
                did: owner_did,
                access: serde_json::json!({"level": "Owner"}),
            }],
        };
        let mut spaces = HashMap::new();
        spaces.insert("$admin".into(), admin_space);
        ArbiterState {
            did,
            version: 1,
            config,
            policy,
            spaces,
        }
    }
}

// ---------------------------------------------------------------------------
// NSID constants
// ---------------------------------------------------------------------------

pub struct NSID;
impl NSID {
    pub const CREATE_ARBITER: &'static str = "town.muni.arbiter.createArbiter";
    pub const GET_ARBITER_CONFIG: &'static str = "town.muni.arbiter.getArbiterConfig";
    pub const SET_ARBITER_CONFIG: &'static str = "town.muni.arbiter.setArbiterConfig";
    pub const DELETE_ARBITER: &'static str = "town.muni.arbiter.deleteArbiter";
    pub const CREATE_SPACE: &'static str = "town.muni.arbiter.createSpace";
    pub const GET_SPACE_CONFIG: &'static str = "town.muni.arbiter.getSpaceConfig";
    pub const SET_SPACE_CONFIG: &'static str = "town.muni.arbiter.setSpaceConfig";
    pub const DELETE_SPACE: &'static str = "town.muni.arbiter.deleteSpace";
    pub const LIST_SPACES: &'static str = "town.muni.arbiter.listSpaces";
    pub const GET_SPACE_MEMBERS: &'static str = "town.muni.arbiter.getSpaceMembers";
    pub const RESOLVE_SPACE_MEMBERS: &'static str = "town.muni.arbiter.resolveSpaceMembers";
    pub const SET_SPACE_MEMBER_ACCESS: &'static str = "town.muni.arbiter.setSpaceMemberAccess";
    pub const REMOVE_SPACE_MEMBER: &'static str = "town.muni.arbiter.removeSpaceMember";
    pub const UPDATE_DID_DOC: &'static str = "town.muni.arbiter.updateDidDoc";
    pub const CREATE_DID: &'static str = "town.muni.arbiter.createDid";
}

pub fn is_readonly_nsid(nsid: &str) -> bool {
    matches!(nsid,
        NSID::GET_ARBITER_CONFIG
        | NSID::GET_SPACE_CONFIG
        | NSID::GET_SPACE_MEMBERS
        | NSID::RESOLVE_SPACE_MEMBERS
        | NSID::LIST_SPACES
    )
}

// ---------------------------------------------------------------------------
// Rego ↔ serde_json conversion  (pub for consumers like integration tests)
// ---------------------------------------------------------------------------

pub fn serde_to_rego(val: &Value) -> regorus::Value {
    match val {
        Value::Null => regorus::Value::Null,
        Value::Bool(b) => regorus::Value::from(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() { regorus::Value::from(i) }
            else if let Some(f) = n.as_f64() { regorus::Value::from(f) }
            else { regorus::Value::from(n.to_string()) }
        }
        Value::String(s) => regorus::Value::from(s.as_str()),
        Value::Array(arr) => regorus::Value::from(arr.iter().map(serde_to_rego).collect::<Vec<_>>()),
        Value::Object(obj) => {
            let mut map = BTreeMap::new();
            for (k, v) in obj { map.insert(regorus::Value::from(k.as_str()), serde_to_rego(v)); }
            regorus::Value::from(map)
        }
    }
}

pub fn rego_to_serde(val: &regorus::Value) -> Value {
    match val {
        regorus::Value::Null => Value::Null,
        regorus::Value::Bool(b) => Value::Bool(*b),
        regorus::Value::Number(n) => {
            let s = n.format_decimal();
            if let Ok(i) = s.parse::<i64>() { Value::Number(i.into()) }
            else if let Ok(f) = s.parse::<f64>() {
                serde_json::Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null)
            } else { Value::String(s) }
        }
        regorus::Value::String(s) => Value::String(s.to_string()),
        regorus::Value::Array(arr) => Value::Array(arr.iter().map(rego_to_serde).collect()),
        regorus::Value::Object(obj) => {
            let mut map = serde_json::Map::new();
            for (k, v) in obj.iter() {
                let key: String = match k.as_string() {
                    Ok(s) => s.to_string(),
                    Err(_) => format!("{k:?}"),
                };
                map.insert(key, rego_to_serde(v));
            }
            Value::Object(map)
        }
        regorus::Value::Set(s) => Value::Array(s.iter().map(rego_to_serde).collect()),
        regorus::Value::Undefined => Value::Null,
    }
}

// ---------------------------------------------------------------------------
// Pending state for in-progress operations
// ---------------------------------------------------------------------------

struct PendingEval {
    session: VmSession,
    ctx: EvalContext,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum EvalPhase { Allow, ResolveResult }

#[derive(Clone)]
struct EvalContext {
    caller_did: String,
    method: String,
    params: Value,
    phase: EvalPhase,
}

// ---------------------------------------------------------------------------
// State machine  (one per arbiter)
// ---------------------------------------------------------------------------

pub struct StateMachine {
    pub arbiter: ArbiterState,
    next_job_id: JobId,
    pending_eval: Option<PendingEval>,
}

impl StateMachine {
    pub fn new(arbiter: ArbiterState) -> Self {
        Self {
            arbiter,
            next_job_id: 1,
            pending_eval: None,
        }
    }

    pub fn create(did: Did, config: Value, policy: String, owner_did: Did) -> Self {
        Self::new(ArbiterState::create(did, config, policy, owner_did))
    }

    fn alloc_job_id(&mut self) -> JobId {
        let id = self.next_job_id;
        self.next_job_id += 1;
        id
    }

    // -------------------------------------------------------------------
    // Main entry point
    // -------------------------------------------------------------------

    pub fn handle_event(&mut self, event: Event) -> Vec<IoAction> {
        match event {
            Event::IncomingXrpc { method, params, caller_did } => {
                self.start_eval_or_execute(method, params, caller_did)
            }
            Event::XrpcRemoteResult { result, .. } => self.resume_pending_eval(result),
        }
    }

    // -------------------------------------------------------------------
    // Start or continue policy evaluation
    // -------------------------------------------------------------------

    fn start_eval_or_execute(&mut self, method: String, params: Value, caller_did: Did) -> Vec<IoAction> {
        if method == NSID::CREATE_ARBITER {
            // Already created — nothing to do (the harness creates us).
            let j = self.alloc_job_id();
            return vec![IoAction::SendResponse { body: serde_json::json!({}), status: 200, job_id: j }];
        }

        let entry_points: Vec<String> = if method == NSID::RESOLVE_SPACE_MEMBERS {
            vec!["data.arbiter.allow".into(), "data.arbiter.resolve_result".into()]
        } else {
            vec!["data.arbiter.allow".into()]
        };

        let ep_refs: Vec<&str> = entry_points.iter().map(|s| s.as_str()).collect();

        let data = serde_json::json!({ "arbiter": { "config": &self.arbiter.config } });
        let input = serde_json::json!({
            "caller": { "did": &caller_did },
            "operation": { "nsid": &method, "params": &params },
        });

        let rego_data = serde_to_rego(&data);
        let rego_input = serde_to_rego(&input);

        let session = match VmSession::new(&self.arbiter.policy, &rego_data, &rego_input, &ep_refs) {
            Ok(s) => s,
            Err(e) => {
                let j = self.alloc_job_id();
                return vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": format!("ErrPolicyCompile: {e}")}),
                    status: 500, job_id: j,
                }];
            }
        };

        let ctx = EvalContext { caller_did, method, params, phase: EvalPhase::Allow };
        self.continue_eval(session, ctx)
    }

    fn continue_eval(&mut self, mut session: VmSession, ctx: EvalContext) -> Vec<IoAction> {
        match session.start() {
            Ok(VmResult::Completed(val)) => self.on_eval_completed(val, ctx),
            Ok(VmResult::Suspended(req)) => self.handle_vm_suspension(req, session, ctx),
            Err(e) => {
                let j = self.alloc_job_id();
                vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": format!("ErrPolicyEval: {e}")}),
                    status: 500, job_id: j,
                }]
            }
        }
    }

    fn continue_eval_resume(&mut self, mut session: VmSession, ctx: EvalContext, resume_val: &regorus::Value) -> Vec<IoAction> {
        match session.resume(resume_val) {
            Ok(VmResult::Completed(val)) => self.on_eval_completed(val, ctx),
            Ok(VmResult::Suspended(req)) => self.handle_vm_suspension(req, session, ctx),
            Err(e) => {
                let j = self.alloc_job_id();
                vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": format!("ErrPolicyResume: {e}")}),
                    status: 500, job_id: j,
                }]
            }
        }
    }

    // -------------------------------------------------------------------
    // Policy completed
    // -------------------------------------------------------------------

    fn on_eval_completed(&mut self, val: regorus::Value, ctx: EvalContext) -> Vec<IoAction> {
        // ResolveResult phase — the value is the result object.
        if ctx.phase == EvalPhase::ResolveResult {
            let result = rego_to_serde(&val);
            let members = result.get("members").cloned().unwrap_or(Value::Array(vec![]));
            let missing = result.get("missingSpaces").cloned().unwrap_or(Value::Array(vec![]));
            let body = serde_json::json!({"members": members, "missingSpaces": missing});
            let j = self.alloc_job_id();
            return vec![IoAction::SendResponse { body, status: 200, job_id: j }];
        }

        // Allow phase — check if true.
        let allowed = val == regorus::Value::Bool(true) || val == regorus::Value::from(true);
        if !allowed {
            let j = self.alloc_job_id();
            return vec![IoAction::SendResponse {
                body: serde_json::json!({"error": "ErrPermissionDenied"}),
                status: 403, job_id: j,
            }];
        }

        if ctx.method == NSID::RESOLVE_SPACE_MEMBERS {
            self.eval_resolve_result(ctx)
        } else if ctx.method == NSID::GET_ARBITER_CONFIG
            || ctx.method == NSID::GET_SPACE_CONFIG
            || ctx.method == NSID::GET_SPACE_MEMBERS
            || ctx.method == NSID::LIST_SPACES
        {
            self.execute_read(&ctx.method, &ctx.params)
        } else {
            self.execute_operation(ctx.method, ctx.params)
        }
    }

    // -------------------------------------------------------------------
    // VmSession suspension handling
    // -------------------------------------------------------------------

    fn handle_vm_suspension(&mut self, req: HostRequest, session: VmSession, ctx: EvalContext) -> Vec<IoAction> {
        match req {
            HostRequest::XrpcLocal { path, input } => {
                let resolved = self.resolve_local(&path, &rego_to_serde(&input));
                let resolved_rego = serde_to_rego(&resolved);
                self.continue_eval_resume(session, ctx, &resolved_rego)
            }
            HostRequest::XrpcRemote { did, path, input } => {
                let j = self.alloc_job_id();
                self.pending_eval = Some(PendingEval { session, ctx });
                vec![IoAction::XrpcRemote {
                    did: did.to_string(),
                    path: path.to_string(),
                    input: rego_to_serde(&input),
                    job_id: j,
                }]
            }
        }
    }

    fn resume_pending_eval(&mut self, result: Result<Value, String>) -> Vec<IoAction> {
        let Some(pending) = self.pending_eval.take() else { return vec![] };
        match result {
            Err(msg) => {
                let j = self.alloc_job_id();
                vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": format!("ErrRemoteXrpc: {msg}")}),
                    status: 502, job_id: j,
                }]
            }
            Ok(data) => {
                let resolved_rego = serde_to_rego(&data);
                self.continue_eval_resume(pending.session, pending.ctx, &resolved_rego)
            }
        }
    }

    // -------------------------------------------------------------------
    // Local XRPC resolution  (reads from our own arbiter state)
    // -------------------------------------------------------------------

    fn resolve_local(&self, nsid: &str, params: &Value) -> Value {
        let space_key = params.get("spaceKey").and_then(|v| v.as_str()).unwrap_or("");
        match nsid {
            NSID::GET_SPACE_MEMBERS => {
                let members: Vec<Value> = self.arbiter.spaces.get(space_key)
                    .map(|s| s.members.iter()
                        .map(|m| serde_json::json!({"did": m.did, "access": m.access}))
                        .collect())
                    .unwrap_or_default();
                serde_json::json!({"members": members})
            }
            NSID::GET_SPACE_CONFIG => {
                let config = self.arbiter.spaces.get(space_key).map(|s| &s.config);
                serde_json::json!({"config": config})
            }
            NSID::GET_ARBITER_CONFIG => {
                serde_json::json!({"config": &self.arbiter.config})
            }
            NSID::LIST_SPACES => {
                let spaces: Vec<Value> = self.arbiter.spaces.values()
                    .map(|s| serde_json::json!({"key": s.key, "spaceType": s.space_type}))
                    .collect();
                serde_json::json!({"spaces": spaces})
            }
            _ => serde_json::json!({}),
        }
    }

    // -------------------------------------------------------------------
    // Evaluate resolve_result entry point
    // -------------------------------------------------------------------

    fn eval_resolve_result(&mut self, ctx: EvalContext) -> Vec<IoAction> {
        let data = serde_json::json!({"arbiter": {"config": &self.arbiter.config}});
        let input = serde_json::json!({
            "caller": {"did": &ctx.caller_did},
            "operation": {"nsid": &ctx.method, "params": &ctx.params},
        });

        let rego_data = serde_to_rego(&data);
        let rego_input = serde_to_rego(&input);

        let mut session = match VmSession::new(
            &self.arbiter.policy, &rego_data, &rego_input,
            &["data.arbiter.resolve_result"],
        ) {
            Ok(s) => s,
            Err(e) => {
                let j = self.alloc_job_id();
                return vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": format!("ErrPolicyCompile: {e}")}),
                    status: 500, job_id: j,
                }];
            }
        };

        let resolve_ctx = EvalContext { phase: EvalPhase::ResolveResult, ..ctx };

        match session.start() {
            Ok(VmResult::Completed(val)) => {
                let result = rego_to_serde(&val);
                let members = result.get("members").cloned().unwrap_or(Value::Array(vec![]));
                let missing = result.get("missingSpaces").cloned().unwrap_or(Value::Array(vec![]));
                let body = serde_json::json!({"members": members, "missingSpaces": missing});
                let j = self.alloc_job_id();
                vec![IoAction::SendResponse { body, status: 200, job_id: j }]
            }
            Ok(VmResult::Suspended(req)) => self.handle_vm_suspension(req, session, resolve_ctx),
            Err(e) => {
                let j = self.alloc_job_id();
                vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": format!("ErrPolicyEval: {e}")}),
                    status: 500, job_id: j,
                }]
            }
        }
    }

    // -------------------------------------------------------------------
    // Execute a permitted operation
    // -------------------------------------------------------------------

    fn execute_read(&mut self, method: &str, params: &Value) -> Vec<IoAction> {
        let body = match method {
            NSID::GET_ARBITER_CONFIG => serde_json::json!({"config": &self.arbiter.config}),
            NSID::GET_SPACE_CONFIG => {
                let sk = params.get("spaceKey").and_then(|v| v.as_str()).unwrap_or("");
                let config = self.arbiter.spaces.get(sk).map(|s| &s.config);
                serde_json::json!({"config": config, "spaceType": ""})
            }
            NSID::GET_SPACE_MEMBERS => {
                let sk = params.get("spaceKey").and_then(|v| v.as_str()).unwrap_or("");
                let members: Vec<Value> = self.arbiter.spaces.get(sk)
                    .map(|s| s.members.iter().map(|m| serde_json::json!({
                        "member": {"did": m.did}, "access": m.access,
                    })).collect())
                    .unwrap_or_default();
                serde_json::json!({"members": members})
            }
            NSID::LIST_SPACES => {
                let spaces: Vec<Value> = self.arbiter.spaces.values().map(|s| serde_json::json!({
                    "spaceKey": s.key, "spaceType": s.space_type, "config": s.config,
                })).collect();
                serde_json::json!({"spaces": spaces})
            }
            _ => unreachable!(),
        };
        let j = self.alloc_job_id();
        vec![IoAction::SendResponse { body, status: 200, job_id: j }]
    }

    fn execute_operation(&mut self, method: String, params: Value) -> Vec<IoAction> {
        if method == NSID::DELETE_ARBITER {
            // The harness should delete us.  Signal completion.
            let j = self.alloc_job_id();
            return vec![IoAction::SendResponse { body: serde_json::json!({}), status: 200, job_id: j }];
        }

        if method == NSID::SET_ARBITER_CONFIG {
            if let Some(new_config) = params.get("config") {
                self.arbiter.config = new_config.clone();
                self.arbiter.version += 1;
            }
        } else if method == NSID::CREATE_SPACE {
            let sk = params.get("spaceKey").and_then(|v| v.as_str()).map(String::from);
            if let Some(sk) = sk {
                if self.arbiter.spaces.contains_key(&sk) {
                    let j = self.alloc_job_id();
                    return vec![IoAction::SendResponse {
                        body: serde_json::json!({"error": "ErrSpaceExists"}), status: 409, job_id: j,
                    }];
                }
                let space_type = params.get("spaceType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("town.muni.arbiter.config.space")
                    .to_string();
                let config = params.get("config").cloned().unwrap_or_default();
                self.arbiter.spaces.insert(sk.clone(), Space {
                    key: sk, space_type, config, members: vec![],
                });
                self.arbiter.version += 1;
            }
        } else if method == NSID::SET_SPACE_CONFIG {
            let sk = params.get("spaceKey").and_then(|v| v.as_str()).map(String::from);
            if let Some(sk) = sk {
                if let Some(space) = self.arbiter.spaces.get_mut(&sk) {
                    if let Some(st) = params.get("spaceType").and_then(|v| v.as_str()) {
                        space.space_type = st.to_string();
                    }
                    if let Some(c) = params.get("config") { space.config = c.clone(); }
                    self.arbiter.version += 1;
                } else {
                    let j = self.alloc_job_id();
                    return vec![IoAction::SendResponse {
                        body: serde_json::json!({"error": "ErrSpaceNotExists"}), status: 404, job_id: j,
                    }];
                }
            }
        } else if method == NSID::DELETE_SPACE {
            let sk = params.get("spaceKey").and_then(|v| v.as_str()).map(String::from);
            if let Some(sk) = sk {
                if sk == "$admin" {
                    let j = self.alloc_job_id();
                    return vec![IoAction::SendResponse {
                        body: serde_json::json!({"error": "ErrCannotDeleteAdminSpace"}), status: 403, job_id: j,
                    }];
                }
                self.arbiter.spaces.remove(&sk);
                self.arbiter.version += 1;
            }
        } else if method == NSID::SET_SPACE_MEMBER_ACCESS {
            let sk = params.get("spaceKey").and_then(|v| v.as_str()).map(String::from);
            let md = params.get("memberDid")
                .or_else(|| params.get("member").and_then(|m| m.get("did")))
                .and_then(|v| v.as_str()).map(String::from);
            if let (Some(sk), Some(md)) = (sk, md) {
                if let Some(space) = self.arbiter.spaces.get_mut(&sk) {
                    let access = params.get("access").cloned().unwrap_or_default();
                    if let Some(existing) = space.members.iter_mut().find(|m| m.did == md) {
                        existing.access = access;
                    } else {
                        space.members.push(MemberEntry { did: md, access });
                    }
                }
                self.arbiter.version += 1;
            }
        } else if method == NSID::REMOVE_SPACE_MEMBER {
            let sk = params.get("spaceKey").and_then(|v| v.as_str()).map(String::from);
            let md = params.get("memberDid")
                .or_else(|| params.get("member").and_then(|m| m.get("did")))
                .and_then(|v| v.as_str()).map(String::from);
            if let (Some(sk), Some(md)) = (sk, md) {
                if let Some(space) = self.arbiter.spaces.get_mut(&sk) {
                    space.members.retain(|m| m.did != md);
                }
                self.arbiter.version += 1;
            }
        } else {
            // Unknown method — proxy.
            return self.execute_proxy(&method, &params);
        }

        let j = self.alloc_job_id();
        vec![IoAction::SendResponse { body: serde_json::json!({}), status: 200, job_id: j }]
    }

    fn execute_proxy(&mut self, method: &str, params: &Value) -> Vec<IoAction> {
        let backend_url = self.arbiter.config.get("backendUrl").and_then(|v| v.as_str()).map(String::from);
        if let Some(url) = backend_url {
            let j = self.alloc_job_id();
            vec![IoAction::ProxyXrpc { backend_url: url, path: method.to_string(), params: params.clone(), job_id: j }]
        } else {
            let j = self.alloc_job_id();
            vec![IoAction::SendResponse {
                body: serde_json::json!({"error": "ErrBackendNotConfigured"}), status: 502, job_id: j,
            }]
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_arbiter_state() {
        let arb = ArbiterState::create(
            "did:plc:abc".into(),
            serde_json::json!({"foo": "bar"}),
            "package arbiter".into(),
            "did:plc:alice".into(),
        );
        assert_eq!(arb.did, "did:plc:abc");
        assert!(arb.spaces.contains_key("$admin"));
    }

    #[test]
    fn test_is_readonly_nsid() {
        assert!(is_readonly_nsid(NSID::GET_ARBITER_CONFIG));
        assert!(!is_readonly_nsid(NSID::SET_ARBITER_CONFIG));
    }

    #[test]
    fn test_serde_rego_roundtrip() {
        let original = serde_json::json!({"s": "hello", "n": 42, "b": true, "a": [1, 2]});
        let rego = serde_to_rego(&original);
        let back = rego_to_serde(&rego);
        assert_eq!(original, back);
    }

    #[test]
    fn test_get_arbiter_config_with_caller_did() {
        let mut sm = StateMachine::create(
            "did:plc:abc".into(),
            serde_json::json!({"key": "val"}),
            "package arbiter\ndefault allow := true".into(),
            "did:plc:alice".into(),
        );
        let actions = sm.handle_event(Event::IncomingXrpc {
            method: NSID::GET_ARBITER_CONFIG.into(),
            params: serde_json::json!({}),
            caller_did: "did:plc:alice".into(),
        });
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            IoAction::SendResponse { body, status, .. } => {
                assert_eq!(*status, 200);
                assert_eq!(body.get("config").and_then(|c| c.get("key")).and_then(|v| v.as_str()), Some("val"));
            }
            _ => panic!("expected SendResponse"),
        }
    }

    #[test]
    fn test_incoming_xrpc_carries_caller_did() {
        let mut sm = StateMachine::create(
            "did:plc:abc".into(),
            serde_json::json!({}),
            "package arbiter\ndefault allow := true".into(),
            "did:plc:alice".into(),
        );
        let actions = sm.handle_event(Event::IncomingXrpc {
            method: NSID::GET_ARBITER_CONFIG.into(),
            params: serde_json::json!({}),
            caller_did: "did:plc:bob".into(),
        });
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], IoAction::SendResponse { status: 200, .. }));
    }


}
