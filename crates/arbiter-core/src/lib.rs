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

use std::collections::HashMap;

use policy_core::{HostRequest, VmResult, VmSession};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use policy_core;

pub type Did = String;
pub type SpaceKey = String;
pub type JobId = u64;

/// Unique identifier for a space, combining its key and type.
/// Both fields are required — `space_type` is part of the space's identity.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct SpaceId {
    pub space_type: String,
    pub key: SpaceKey,
}

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
    pub spaces: HashMap<SpaceId, Space>,
}

impl ArbiterState {
    /// Create a new arbiter with the given DID and initial owner.
    pub fn create(did: Did, config: Value, policy: String, owner_did: Did) -> Self {
        let admin_id = SpaceId {
            key: "$admin".into(),
            space_type: "town.muni.arbiter.config.adminSpace".into(),
        };
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
        spaces.insert(admin_id, admin_space);
        ArbiterState {
            did,
            version: 1,
            config,
            policy,
            spaces,
        }
    }

    /// Look up a space by its full [`SpaceId`].
    pub fn get_space(&self, id: &SpaceId) -> Option<&Space> {
        self.spaces.get(id)
    }

    /// Mutable variant of [`get_space`](Self::get_space).
    pub fn get_space_mut(&mut self, id: &SpaceId) -> Option<&mut Space> {
        self.spaces.get_mut(id)
    }
}

// ---------------------------------------------------------------------------
// NSID constants
// ---------------------------------------------------------------------------

pub struct NSID;
impl NSID {
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

fn serde_to_rego(val: Value) -> regorus::Value {
    serde_json::from_value(val).unwrap()
}
fn rego_to_serde(val: regorus::Value) -> Value {
    serde_json::to_value(val).unwrap()
}

/// Extract the `members` and `missingSpaces` fields from a completed
/// `resolve_result` policy evaluation and format the response body.
fn format_resolve_result(val: regorus::Value) -> Value {
    let result = rego_to_serde(val);
    let members = result.get("members").cloned().unwrap_or(Value::Array(vec![]));
    let missing = result.get("missingSpaces").cloned().unwrap_or(Value::Array(vec![]));
    serde_json::json!({"members": members, "missingSpaces": missing})
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

/// Describes how to step a [`VmSession`] — either starting fresh or
/// resuming with a value from a previous suspension.
enum EvalStep<'a> {
    Start,
    Resume(&'a regorus::Value),
}

#[derive(Clone)]
struct EvalContext {
    caller_did: Did,
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

        let rego_data = serde_to_rego(data);
        let rego_input = serde_to_rego(input);

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
        self.continue_eval(session, ctx, EvalStep::Start)
    }

    fn continue_eval(&mut self, mut session: VmSession, ctx: EvalContext, step: EvalStep<'_>) -> Vec<IoAction> {
        let result = match &step {
            EvalStep::Start => session.start(),
            EvalStep::Resume(val) => session.resume(val),
        };
        let error_label = match &step {
            EvalStep::Start => "ErrPolicyEval",
            EvalStep::Resume(_) => "ErrPolicyResume",
        };
        match result {
            Ok(VmResult::Completed(val)) => self.on_eval_completed(val, ctx),
            Ok(VmResult::Suspended(req)) => self.handle_vm_suspension(req, session, ctx),
            Err(e) => {
                let j = self.alloc_job_id();
                vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": format!("{error_label}: {e}")}),
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
            let body = format_resolve_result(val);
            let j = self.alloc_job_id();
            return vec![IoAction::SendResponse { body, status: 200, job_id: j }];
        }

        // Allow phase — check if true.
        let allowed = val == regorus::Value::Bool(true);
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
                let resolved = self.resolve_local(&path, &rego_to_serde(input));
                let resolved_rego = serde_to_rego(resolved);
                self.continue_eval(session, ctx, EvalStep::Resume(&resolved_rego))
            }
            HostRequest::XrpcRemote { did, path, input } => {
                let j = self.alloc_job_id();
                self.pending_eval = Some(PendingEval { session, ctx });
                vec![IoAction::XrpcRemote {
                    did: did.to_string(),
                    path: path.to_string(),
                    input: rego_to_serde(input),
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
                let resolved_rego = serde_to_rego(data);
                self.continue_eval(pending.session, pending.ctx, EvalStep::Resume(&resolved_rego))
            }
        }
    }

    // -------------------------------------------------------------------
    // Local XRPC resolution  (reads from our own arbiter state)
    // -------------------------------------------------------------------

    fn resolve_local(&self, nsid: &str, params: &Value) -> Value {
        match nsid {
            NSID::GET_SPACE_MEMBERS | NSID::GET_SPACE_CONFIG => {
                let Some(space_id) = self.space_id_from_params(params) else {
                    return serde_json::json!({});
                };
                let space = self.arbiter.get_space(&space_id);
                match nsid {
                    NSID::GET_SPACE_MEMBERS => {
                        let members: Vec<Value> = space
                            .map(|s| s.members.iter()
                                .map(|m| serde_json::json!({"did": m.did, "access": m.access}))
                                .collect())
                            .unwrap_or_default();
                        serde_json::json!({"members": members})
                    }
                    _ => { // GET_SPACE_CONFIG
                        let config = space.map(|s| &s.config);
                        serde_json::json!({"config": config})
                    }
                }
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

        let rego_data = serde_to_rego(data);
        let rego_input = serde_to_rego(input);

        let session = match VmSession::new(
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

        self.continue_eval(session, resolve_ctx, EvalStep::Start)
    }

    // -------------------------------------------------------------------
    // Execute a permitted operation
    // -------------------------------------------------------------------

    fn execute_read(&mut self, method: &str, params: &Value) -> Vec<IoAction> {
        let body = match method {
            NSID::GET_ARBITER_CONFIG => serde_json::json!({"config": &self.arbiter.config}),
            NSID::GET_SPACE_CONFIG => {
                let (st, sk) = match self.space_params(params) {
                    Some(p) => p,
                    None => return self.missing_param("spaceKey/spaceType"),
                };
                let space_id = SpaceId { key: sk, space_type: st };
                let config = self.arbiter.get_space(&space_id).map(|s| &s.config);
                serde_json::json!({"config": config, "spaceType": &space_id.space_type})
            }
            NSID::GET_SPACE_MEMBERS => {
                let (st, sk) = match self.space_params(params) {
                    Some(p) => p,
                    None => return self.missing_param("spaceKey/spaceType"),
                };
                let space_id = SpaceId { key: sk, space_type: st };
                let members: Vec<Value> = self.arbiter.get_space(&space_id)
                    .map(|s| s.members.iter().map(|m| serde_json::json!({
                        "member": {"did": m.did}, "access": m.access,
                    })).collect())
                    .unwrap_or_default();
                serde_json::json!({"members": members})
            }
            NSID::LIST_SPACES => {
                let spaces: Vec<Value> = self.arbiter.spaces.iter().map(|(id, s)| serde_json::json!({
                    "spaceKey": id.key, "spaceType": id.space_type, "config": s.config,
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
            let (st, sk) = match self.space_params(&params) {
                Some(p) => p,
                None => return self.missing_param("spaceKey/spaceType"),
            };
            let space_id = SpaceId { key: sk, space_type: st };
            if self.arbiter.spaces.contains_key(&space_id) {
                let j = self.alloc_job_id();
                return vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": "ErrSpaceExists"}), status: 409, job_id: j,
                }];
            }
            let config = params.get("config").cloned().unwrap_or_default();
            let space = Space {
                key: space_id.key.clone(),
                space_type: space_id.space_type.clone(),
                config,
                members: vec![],
            };
            self.arbiter.spaces.insert(space_id, space);
            self.arbiter.version += 1;
        } else if method == NSID::SET_SPACE_CONFIG {
            let (st, sk) = match self.space_params(&params) {
                Some(p) => p,
                None => return self.missing_param("spaceKey/spaceType"),
            };
            let space_id = SpaceId { key: sk, space_type: st };
            if let Some(space) = self.arbiter.spaces.get_mut(&space_id) {
                if let Some(c) = params.get("config") { space.config = c.clone(); }
                self.arbiter.version += 1;
            } else {
                let j = self.alloc_job_id();
                return vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": "ErrSpaceNotExists"}), status: 404, job_id: j,
                }];
            }
        } else if method == NSID::DELETE_SPACE {
            let (st, sk) = match self.space_params(&params) {
                Some(p) => p,
                None => return self.missing_param("spaceKey/spaceType"),
            };
            if sk == "$admin" && st == "town.muni.arbiter.config.adminSpace" {
                let j = self.alloc_job_id();
                return vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": "ErrCannotDeleteAdminSpace"}), status: 403, job_id: j,
                }];
            }
            let space_id = SpaceId { key: sk, space_type: st };
            if self.arbiter.spaces.remove(&space_id).is_none() {
                let j = self.alloc_job_id();
                return vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": "ErrSpaceNotExists"}), status: 404, job_id: j,
                }];
            }
            self.arbiter.version += 1;
        } else if method == NSID::SET_SPACE_MEMBER_ACCESS {
            let (st, sk) = match self.space_params(&params) {
                Some(p) => p,
                None => return self.missing_param("spaceKey/spaceType"),
            };
            let space_id = SpaceId { key: sk, space_type: st };
            let md = params.get("memberDid")
                .or_else(|| params.get("member").and_then(|m| m.get("did")))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let md = match md {
                Some(d) => d,
                None => return self.missing_param("memberDid"),
            };
            if let Some(space) = self.arbiter.spaces.get_mut(&space_id) {
                let access = params.get("access").cloned().unwrap_or_default();
                if let Some(existing) = space.members.iter_mut().find(|m| m.did == md) {
                    existing.access = access;
                } else {
                    space.members.push(MemberEntry { did: md, access });
                }
            } else {
                let j = self.alloc_job_id();
                return vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": "ErrSpaceNotExists"}), status: 404, job_id: j,
                }];
            }
            self.arbiter.version += 1;
        } else if method == NSID::REMOVE_SPACE_MEMBER {
            let (st, sk) = match self.space_params(&params) {
                Some(p) => p,
                None => return self.missing_param("spaceKey/spaceType"),
            };
            let space_id = SpaceId { key: sk, space_type: st };
            let md = params.get("memberDid")
                .or_else(|| params.get("member").and_then(|m| m.get("did")))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let md = match md {
                Some(d) => d,
                None => return self.missing_param("memberDid"),
            };
            if let Some(space) = self.arbiter.spaces.get_mut(&space_id) {
                space.members.retain(|m| m.did != md);
            } else {
                let j = self.alloc_job_id();
                return vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": "ErrSpaceNotExists"}), status: 404, job_id: j,
                }];
            }
            self.arbiter.version += 1;
        } else {
            // Unknown method — proxy.
            return self.execute_proxy(&method, &params);
        }

        let j = self.alloc_job_id();
        vec![IoAction::SendResponse { body: serde_json::json!({}), status: 200, job_id: j }]
    }

    /// Extract `(spaceType, spaceKey)` from XRPC params.
    fn space_params(&self, params: &Value) -> Option<(String, String)> {
        let sk = params.get("spaceKey").and_then(|v| v.as_str())?;
        let st = params.get("spaceType").and_then(|v| v.as_str())?;
        Some((st.to_string(), sk.to_string()))
    }

    /// Extract a [`SpaceId`] from XRPC params.
    fn space_id_from_params(&self, params: &Value) -> Option<SpaceId> {
        let (space_type, key) = self.space_params(params)?;
        Some(SpaceId { key, space_type })
    }

    fn missing_param(&mut self, name: &str) -> Vec<IoAction> {
        let j = self.alloc_job_id();
        vec![IoAction::SendResponse {
            body: serde_json::json!({"error": format!("ErrMissingParam: {name}")}),
            status: 400, job_id: j,
        }]
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
        let admin_id = SpaceId {
            key: "$admin".into(),
            space_type: "town.muni.arbiter.config.adminSpace".into(),
        };
        assert!(arb.spaces.contains_key(&admin_id));
    }

    #[test]
    fn test_is_readonly_nsid() {
        assert!(is_readonly_nsid(NSID::GET_ARBITER_CONFIG));
        assert!(!is_readonly_nsid(NSID::SET_ARBITER_CONFIG));
    }

    #[test]
    fn test_serde_rego_roundtrip() {
        let original = serde_json::json!({"s": "hello", "n": 42, "b": true, "a": [1, 2]});
        let rego = serde_to_rego(original.clone());
        let back = rego_to_serde(rego);
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
