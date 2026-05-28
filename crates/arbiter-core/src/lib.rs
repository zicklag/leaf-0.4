//! Pure sans-IO arbiter state machine.
//!
//! The state machine is a struct ([`StateMachine`]) with a single entry
//! point: [`handle_event`](StateMachine::handle_event).  You feed it
//! [`Event`]s (incoming XRPC calls, auth results, remote query results,
//! etc.) and it returns zero or more [`IoAction`]s describing what IO
//! the harness should perform.  The harness fulfills those actions and
//! feeds the results back as new events.
//!
//! Policy evaluation using [`policy_core::VmSession`] lives inside the
//! state machine (it is itself sans-IO).  `xrpc_local` queries are
//! resolved from local state.  `xrpc_remote` queries produce an
//! [`IoAction::XrpcRemote`]; when the harness responds with
//! [`Event::XrpcRemoteResult`], the state machine resumes the suspended
//! `VmSession`.
//!
//! All data is owned — no lifetime parameters, no self-referential borrows,
//! just pure functions: events in → actions out.

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

/// An event that can happen to the state machine.
///
/// All data is owned — no lifetime tracking needed.
#[derive(Debug, Clone)]
pub enum Event {
    /// An incoming XRPC call from the HTTP server.
    IncomingXrpc { method: String, params: Value, token: Option<String> },

    /// Auth resolution completed successfully.
    AuthResolved { caller_did: String, job_id: JobId },
    /// Auth resolution failed.
    AuthFailed { job_id: JobId },

    /// A remote XRPC query (from policy evaluation) returned data.
    XrpcRemoteResult { data: Value, job_id: JobId },
    /// A remote XRPC query failed.
    XrpcRemoteFailed { message: String, job_id: JobId },

    /// A snapshot was loaded from persistence.
    LoadedSnapshot { snapshot: Option<Value>, job_id: JobId },

    /// An XRPC response was sent to the client.
    ResponseSent { job_id: JobId },

    /// A snapshot was persisted.
    SnapshotStored { job_id: JobId },

    /// The IO layer is shutting down.
    Shutdown,
}

// ---------------------------------------------------------------------------
// IO actions  (output of the state machine)
// ---------------------------------------------------------------------------

/// An action the harness should perform.
///
/// All data is owned — no lifetime tracking needed.
#[derive(Debug, Clone)]
pub enum IoAction {
    /// Resolve a JWT bearer token to a caller DID.
    ResolveAuth { token: String, job_id: JobId },

    /// Resolve a DID document.
    ResolveDid { did: String, job_id: JobId },

    /// Send an XRPC response to the client.
    SendResponse { body: Value, status: u16, job_id: JobId },

    /// Resolve a remote XRPC query from the policy engine.
    XrpcRemote { did: String, path: String, input: Value, job_id: JobId },

    /// Submit a signed operation to the PLC directory.
    PlcSubmitOperation { did: String, operation: Value, job_id: JobId },
    /// Fetch PLC state.
    PlcFetchState { did: String, job_id: JobId },
    /// Fetch PLC state with the latest operation CID.
    PlcFetchStateWithCid { did: String, job_id: JobId },

    /// Generate new DID keys (rotation + signing).
    GenerateDidKeys { job_id: JobId },

    /// Persist a JSON snapshot of the entire collection.
    StoreSnapshot { snapshot_json: Value, job_id: JobId },
    /// Load the previously persisted snapshot.
    LoadSnapshot { job_id: JobId },

    /// Proxy an XRPC call to a backend service.
    ProxyXrpc { backend_url: String, path: String, params: Value, job_id: JobId },
}

// ---------------------------------------------------------------------------
// JSON-serialisable snapshot types
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
    pub online: bool,
    pub spaces: HashMap<SpaceKey, Space>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceSnapshot {
    pub key: SpaceKey,
    pub space_type: String,
    pub config: Value,
    pub members: Vec<MemberEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbiterSnapshot {
    pub did: Did,
    pub version: u64,
    pub config: Value,
    pub policy: String,
    pub online: bool,
    pub spaces: Vec<SpaceSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSnapshot {
    pub arbiters: Vec<ArbiterSnapshot>,
}

/// The entire in-memory state: all arbiters, spaces, members.
#[derive(Default)]
pub struct ArbiterCollection {
    pub arbiters: HashMap<Did, ArbiterState>,
}

impl ArbiterCollection {
    pub fn new() -> Self {
        Self { arbiters: HashMap::new() }
    }

    pub fn get(&self, did: &str) -> Option<&ArbiterState> {
        self.arbiters.get(did)
    }

    pub fn get_mut(&mut self, did: &str) -> Option<&mut ArbiterState> {
        self.arbiters.get_mut(did)
    }

    pub fn create_arbiter(&mut self, did: Did, config: Value, policy: String, owner_did: Did) {
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
        self.arbiters.insert(did.clone(), ArbiterState {
            did,
            version: 1,
            config,
            policy,
            online: true,
            spaces,
        });
    }

    pub fn snapshot(&self) -> ServerSnapshot {
        let arbiters: Vec<ArbiterSnapshot> = self.arbiters.values().map(|a| {
            let spaces: Vec<SpaceSnapshot> = a.spaces.values().map(|s| SpaceSnapshot {
                key: s.key.clone(),
                space_type: s.space_type.clone(),
                config: s.config.clone(),
                members: s.members.clone(),
            }).collect();
            ArbiterSnapshot {
                did: a.did.clone(),
                version: a.version,
                config: a.config.clone(),
                policy: a.policy.clone(),
                online: a.online,
                spaces,
            }
        }).collect();
        ServerSnapshot { arbiters }
    }

    pub fn load_snapshot(&mut self, snapshot: ServerSnapshot) {
        self.arbiters.clear();
        for a in snapshot.arbiters {
            let spaces: HashMap<SpaceKey, Space> = a.spaces.into_iter().map(|s| {
                (s.key.clone(), Space {
                    key: s.key,
                    space_type: s.space_type,
                    config: s.config,
                    members: s.members,
                })
            }).collect();
            self.arbiters.insert(a.did.clone(), ArbiterState {
                did: a.did,
                version: a.version,
                config: a.config,
                policy: a.policy,
                online: a.online,
                spaces,
            });
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
// Policy helpers  (pure)
// ---------------------------------------------------------------------------

fn serde_to_rego(val: &Value) -> regorus::Value {
    match val {
        Value::Null => regorus::Value::Null,
        Value::Bool(b) => regorus::Value::from(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                regorus::Value::from(i)
            } else if let Some(f) = n.as_f64() {
                regorus::Value::from(f)
            } else {
                regorus::Value::from(n.to_string())
            }
        }
        Value::String(s) => regorus::Value::from(s.as_str()),
        Value::Array(arr) => {
            regorus::Value::from(arr.iter().map(serde_to_rego).collect::<Vec<_>>())
        }
        Value::Object(obj) => {
            let mut map = BTreeMap::new();
            for (k, v) in obj {
                map.insert(regorus::Value::from(k.as_str()), serde_to_rego(v));
            }
            regorus::Value::from(map)
        }
    }
}

fn rego_to_serde(val: &regorus::Value) -> Value {
    match val {
        regorus::Value::Null => Value::Null,
        regorus::Value::Bool(b) => Value::Bool(*b),
        regorus::Value::Number(n) => {
            let s = n.format_decimal();
            if let Ok(i) = s.parse::<i64>() {
                Value::Number(i.into())
            } else if let Ok(f) = s.parse::<f64>() {
                serde_json::Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null)
            } else {
                Value::String(s)
            }
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

/// Resolve an `xrpc_local` query against our own collection data.
fn resolve_local(
    arbiter_did: &str,
    nsid: &str,
    params: &Value,
    collection: &ArbiterCollection,
) -> Value {
    let space_key = params.get("spaceKey").and_then(|v| v.as_str()).unwrap_or("");
    match nsid {
        NSID::GET_SPACE_MEMBERS => {
            let members: Vec<Value> = collection
                .get(arbiter_did)
                .and_then(|a| a.spaces.get(space_key))
                .map(|s| {
                    s.members
                        .iter()
                        .map(|m| serde_json::json!({ "did": m.did, "access": m.access }))
                        .collect()
                })
                .unwrap_or_default();
            serde_json::json!({ "members": members })
        }
        NSID::GET_SPACE_CONFIG => {
            let config = collection
                .get(arbiter_did)
                .and_then(|a| a.spaces.get(space_key))
                .map(|s| &s.config);
            serde_json::json!({ "config": config })
        }
        NSID::GET_ARBITER_CONFIG => {
            serde_json::json!({ "config": collection.get(arbiter_did).map(|a| &a.config) })
        }
        NSID::LIST_SPACES => {
            let spaces: Vec<Value> = collection
                .get(arbiter_did)
                .map(|a| {
                    a.spaces
                        .values()
                        .map(|s| serde_json::json!({ "key": s.key, "spaceType": s.space_type }))
                        .collect()
                })
                .unwrap_or_default();
            serde_json::json!({ "spaces": spaces })
        }
        _ => serde_json::json!({}),
    }
}

// ---------------------------------------------------------------------------
// Pending state for in-progress operations
// ---------------------------------------------------------------------------

/// Tracks a VmSession suspended waiting for a remote XRPC result.
struct PendingEval {
    session: VmSession,
    /// Context needed to continue when the IO result comes back.
    ctx: EvalContext,
}

/// Everything we need to continue a suspended policy evaluation.
#[derive(Clone)]
struct EvalContext {
    arbiter_did: String,
    arbiter_config: Value,
    caller_did: String,
    method: String,
    params: Value,
}

/// Tracks an incoming call waiting for auth resolution.
struct PendingAuth {
    method: String,
    params: Value,
    arbiter_did: String,
}

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

/// The pure arbiter state machine.
///
/// Owns the arbiter collection and any in-progress operations (policy
/// evaluations suspended waiting for remote data).
#[derive(Default)]
pub struct StateMachine {
    pub collection: ArbiterCollection,
    next_job_id: JobId,
    pending_eval: Option<PendingEval>,
    pending_auth: Option<PendingAuth>,
    /// Set to true when a `Shutdown` event is received.
    pub shutdown: bool,
}

impl StateMachine {
    pub fn new(collection: ArbiterCollection) -> Self {
        Self {
            collection,
            next_job_id: 1,
            pending_eval: None,
            pending_auth: None,
            shutdown: false,
        }
    }

    fn alloc_job_id(&mut self) -> JobId {
        let id = self.next_job_id;
        self.next_job_id += 1;
        id
    }

    // -------------------------------------------------------------------
    // Main entry point
    // -------------------------------------------------------------------

    /// Feed an event into the state machine.
    ///
    /// Returns zero or more [`IoAction`]s that the harness should fulfil.
    /// The harness should then feed the results back via another call to
    /// `handle_event`.
    pub fn handle_event(&mut self, event: Event) -> Vec<IoAction> {
        match event {
            Event::Shutdown => {
                self.shutdown = true;
                vec![]
            }

            Event::IncomingXrpc { method, params, token } => {
                self.handle_incoming(&method, params, token)
            }

            Event::AuthResolved { caller_did, .. } => self.handle_auth_result(caller_did),
            Event::AuthFailed { .. } => self.handle_auth_failed(),

            Event::XrpcRemoteResult { data, .. } => self.resume_pending_eval(Some(data), None),
            Event::XrpcRemoteFailed { message, .. } => self.resume_pending_eval(None, Some(message)),

            Event::LoadedSnapshot { snapshot, .. } => self.handle_loaded_snapshot(snapshot),

            // Acknowledgements — no action needed.
            Event::ResponseSent { .. } | Event::SnapshotStored { .. } => vec![],
        }
    }

    // -------------------------------------------------------------------
    // Incoming XRPC call handling
    // -------------------------------------------------------------------

    fn arbiter_did_from_params<'a>(&self, params: &'a Value) -> Option<&'a str> {
        params
            .get("arbiterDid")
            .or_else(|| params.get("did"))
            .and_then(|v| v.as_str())
    }

    fn handle_incoming(&mut self, method: &str, params: Value, token: Option<String>) -> Vec<IoAction> {
        // Bootstrap: createArbiter has no policy check.
        if method == NSID::CREATE_ARBITER {
            return self.do_create_arbiter(params);
        }

        // All other methods need an arbiter DID.
        let Some(arbiter_did) = self.arbiter_did_from_params(&params).map(String::from) else {
            let j = self.alloc_job_id();
            return vec![IoAction::SendResponse {
                body: serde_json::json!({"error": "missing arbiterDid"}),
                status: 400,
                job_id: j,
            }];
        };

        // If auth is required, defer.
        if let Some(tok) = token {
            let j = self.alloc_job_id();
            self.pending_auth = Some(PendingAuth {
                method: method.to_string(),
                params: params.clone(),
                arbiter_did,
            });
            return vec![IoAction::ResolveAuth { token: tok, job_id: j }];
        }

        // No auth — process immediately with empty caller DID.
        self.start_eval_or_execute(method.to_string(), params, String::new(), arbiter_did)
    }

    // -------------------------------------------------------------------
    // Auth handling
    // -------------------------------------------------------------------

    fn handle_auth_result(&mut self, caller_did: String) -> Vec<IoAction> {
        let Some(pending) = self.pending_auth.take() else {
            return vec![];
        };
        self.start_eval_or_execute(pending.method, pending.params, caller_did, pending.arbiter_did)
    }

    fn handle_auth_failed(&mut self) -> Vec<IoAction> {
        self.pending_auth = None;
        let j = self.alloc_job_id();
        vec![IoAction::SendResponse {
            body: serde_json::json!({"error": "ErrAuthRequired"}),
            status: 401,
            job_id: j,
        }]
    }

    // -------------------------------------------------------------------
    // Start or continue policy evaluation
    // -------------------------------------------------------------------

    fn start_eval_or_execute(
        &mut self,
        method: String,
        params: Value,
        caller_did: String,
        arbiter_did: String,
    ) -> Vec<IoAction> {
        // Look up the arbiter.
        let policy = match self.collection.get(&arbiter_did) {
            Some(a) => a.policy.clone(),
            None => {
                let j = self.alloc_job_id();
                return vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": "ErrArbiterNotExists"}),
                    status: 404,
                    job_id: j,
                }];
            }
        };
        let arbiter_config = self.collection.get(&arbiter_did).map(|a| a.config.clone()).unwrap_or_default();

        let entry_points = if method == NSID::RESOLVE_SPACE_MEMBERS {
            vec!["data.arbiter.allow".to_string(), "data.arbiter.resolve_result".to_string()]
        } else {
            vec!["data.arbiter.allow".to_string()]
        };

        let ep_refs: Vec<&str> = entry_points.iter().map(|s| s.as_str()).collect();

        let data = serde_json::json!({ "arbiter": { "config": &arbiter_config } });
        let input = serde_json::json!({
            "caller": { "did": &caller_did },
            "operation": { "nsid": &method, "params": &params },
        });

        let rego_data = serde_to_rego(&data);
        let rego_input = serde_to_rego(&input);

        let session = match VmSession::new(&policy, &rego_data, &rego_input, &ep_refs) {
            Ok(s) => s,
            Err(e) => {
                let j = self.alloc_job_id();
                return vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": format!("ErrPolicyCompile: {e}")}),
                    status: 500,
                    job_id: j,
                }];
            }
        };

        let ctx = EvalContext {
            arbiter_did,
            arbiter_config,
            caller_did,
            method,
            params,
        };

        self.continue_eval(session, ctx)
    }

    // -------------------------------------------------------------------
    // Continue a VmSession loop (start or after local resolution)
    // -------------------------------------------------------------------

    fn continue_eval(&mut self, mut session: VmSession, ctx: EvalContext) -> Vec<IoAction> {
        match session.start() {
            Ok(VmResult::Completed(val)) => self.policy_completed(val, ctx),
            Ok(VmResult::Suspended(req)) => self.handle_vm_suspension(req, session, ctx),
            Err(e) => {
                let j = self.alloc_job_id();
                vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": format!("ErrPolicyEval: {e}")}),
                    status: 500,
                    job_id: j,
                }]
            }
        }
    }

    fn continue_eval_resume(&mut self, mut session: VmSession, ctx: EvalContext, resume_val: &regorus::Value) -> Vec<IoAction> {
        match session.resume(resume_val) {
            Ok(VmResult::Completed(val)) => self.policy_completed(val, ctx),
            Ok(VmResult::Suspended(req)) => self.handle_vm_suspension(req, session, ctx),
            Err(e) => {
                let j = self.alloc_job_id();
                vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": format!("ErrPolicyResume: {e}")}),
                    status: 500,
                    job_id: j,
                }]
            }
        }
    }

    // -------------------------------------------------------------------
    // Policy completed — check result and execute or deny
    // -------------------------------------------------------------------

    fn policy_completed(&mut self, val: regorus::Value, ctx: EvalContext) -> Vec<IoAction> {
        let allowed = val == regorus::Value::Bool(true) || val == regorus::Value::from(true);
        if !allowed {
            let j = self.alloc_job_id();
            return vec![IoAction::SendResponse {
                body: serde_json::json!({"error": "ErrPermissionDenied"}),
                status: 403,
                job_id: j,
            }];
        }

        if ctx.method == NSID::RESOLVE_SPACE_MEMBERS {
            // Need to evaluate resolve_result now.
            self.eval_resolve_result(ctx)
        } else {
            self.execute_operation(ctx.method, ctx.params, ctx.caller_did, &ctx.arbiter_did)
        }
    }

    // -------------------------------------------------------------------
    // Handle VmSession suspension
    // -------------------------------------------------------------------

    fn handle_vm_suspension(&mut self, req: HostRequest, session: VmSession, ctx: EvalContext) -> Vec<IoAction> {
        match req {
            HostRequest::XrpcLocal { path, input } => {
                let resolved = resolve_local(&ctx.arbiter_did, &path, &rego_to_serde(&input), &self.collection);
                let resolved_rego = serde_to_rego(&resolved);
                self.continue_eval_resume(session, ctx, &resolved_rego)
            }
            HostRequest::XrpcRemote { did, path, input } => {
                let j = self.alloc_job_id();
                let owned_did = did.to_string();
                let owned_path = path.to_string();
                let owned_input = rego_to_serde(&input);

                self.pending_eval = Some(PendingEval { session, ctx });
                vec![IoAction::XrpcRemote {
                    did: owned_did,
                    path: owned_path,
                    input: owned_input,
                    job_id: j,
                }]
            }
        }
    }

    // -------------------------------------------------------------------
    // Resume a suspended policy evaluation (after IO responds)
    // -------------------------------------------------------------------

    fn resume_pending_eval(&mut self, result_data: Option<Value>, error_msg: Option<String>) -> Vec<IoAction> {
        let Some(pending) = self.pending_eval.take() else {
            return vec![];
        };

        if let Some(msg) = error_msg {
            let j = self.alloc_job_id();
            return vec![IoAction::SendResponse {
                body: serde_json::json!({"error": format!("ErrRemoteXrpc: {msg}")}),
                status: 502,
                job_id: j,
            }];
        }

        let resolved = result_data.unwrap_or(Value::Null);
        let resolved_rego = serde_to_rego(&resolved);
        self.continue_eval_resume(pending.session, pending.ctx, &resolved_rego)
    }

    // -------------------------------------------------------------------
    // Evaluate resolve_result entry point
    // -------------------------------------------------------------------

    fn eval_resolve_result(&mut self, ctx: EvalContext) -> Vec<IoAction> {
        let data = serde_json::json!({ "arbiter": { "config": &ctx.arbiter_config } });
        let input = serde_json::json!({
            "caller": { "did": &ctx.caller_did },
            "operation": { "nsid": &ctx.method, "params": &ctx.params },
        });

        let rego_data = serde_to_rego(&data);
        let rego_input = serde_to_rego(&input);

        let mut session = match VmSession::new(
            &self.collection.get(&ctx.arbiter_did).map(|a| &a.policy).cloned().unwrap_or_default(),
            &rego_data,
            &rego_input,
            &["data.arbiter.resolve_result"],
        ) {
            Ok(s) => s,
            Err(e) => {
                let j = self.alloc_job_id();
                return vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": format!("ErrPolicyCompile: {e}")}),
                    status: 500,
                    job_id: j,
                }];
            }
        };

        let resolve_ctx = EvalContext {
            arbiter_did: ctx.arbiter_did,
            arbiter_config: ctx.arbiter_config,
            caller_did: ctx.caller_did,
            method: ctx.method,
            params: ctx.params,
        };

        match session.start() {
            Ok(VmResult::Completed(val)) => {
                let result = rego_to_serde(&val);
                let members = result.get("members").cloned().unwrap_or(Value::Array(vec![]));
                let missing = result.get("missingSpaces").cloned().unwrap_or(Value::Array(vec![]));
                let body = serde_json::json!({ "members": members, "missingSpaces": missing });
                let j = self.alloc_job_id();
                vec![IoAction::SendResponse { body, status: 200, job_id: j }]
            }
            Ok(VmResult::Suspended(req)) => {
                self.handle_vm_suspension(req, session, resolve_ctx)
            }
            Err(e) => {
                let j = self.alloc_job_id();
                vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": format!("ErrPolicyEval: {e}")}),
                    status: 500,
                    job_id: j,
                }]
            }
        }
    }

    // -------------------------------------------------------------------
    // Execute a permitted operation (or proxy)
    // -------------------------------------------------------------------

    fn execute_operation(&mut self, method: String, params: Value, _caller_did: String, arbiter_did: &str) -> Vec<IoAction> {
        if method == NSID::GET_ARBITER_CONFIG
            || method == NSID::GET_SPACE_CONFIG
            || method == NSID::GET_SPACE_MEMBERS
            || method == NSID::LIST_SPACES
        {
            return self.execute_read(&method, &params, arbiter_did);
        }

        if method == NSID::DELETE_ARBITER {
            self.collection.arbiters.remove(arbiter_did);
        } else if method == NSID::SET_ARBITER_CONFIG {
            if let Some(new_config) = params.get("config") {
                if let Some(arb) = self.collection.get_mut(arbiter_did) {
                    arb.config = new_config.clone();
                    arb.version += 1;
                }
            }
        } else if method == NSID::CREATE_SPACE {
            let sk = params.get("spaceKey").and_then(|v| v.as_str()).map(String::from);
            if let Some(sk) = sk {
                if self.collection.get(arbiter_did).map(|a| a.spaces.contains_key(&sk)).unwrap_or(false) {
                    let j = self.alloc_job_id();
                    return vec![IoAction::SendResponse {
                        body: serde_json::json!({"error": "ErrSpaceExists"}),
                        status: 409,
                        job_id: j,
                    }];
                }
                if let Some(arb) = self.collection.get_mut(arbiter_did) {
                    let space_type = params
                        .get("spaceType")
                        .and_then(|v| v.as_str())
                        .unwrap_or("town.muni.arbiter.config.space")
                        .to_string();
                    let config = params.get("config").cloned().unwrap_or_default();
                    arb.spaces.insert(sk.clone(), Space {
                        key: sk,
                        space_type,
                        config,
                        members: vec![],
                    });
                    arb.version += 1;
                }
            }
        } else if method == NSID::SET_SPACE_CONFIG {
            let sk = params.get("spaceKey").and_then(|v| v.as_str()).map(String::from);
            if let Some(sk) = sk {
                if let Some(arb) = self.collection.get_mut(arbiter_did) {
                    if let Some(space) = arb.spaces.get_mut(&sk) {
                        if let Some(space_type) = params.get("spaceType").and_then(|v| v.as_str()) {
                            space.space_type = space_type.to_string();
                        }
                        if let Some(config) = params.get("config") {
                            space.config = config.clone();
                        }
                        arb.version += 1;
                    } else {
                        let j = self.alloc_job_id();
                        return vec![IoAction::SendResponse {
                            body: serde_json::json!({"error": "ErrSpaceNotExists"}),
                            status: 404,
                            job_id: j,
                        }];
                    }
                }
            }
        } else if method == NSID::DELETE_SPACE {
            let sk = params.get("spaceKey").and_then(|v| v.as_str()).map(String::from);
            if let Some(sk) = sk {
                if sk == "$admin" {
                    let j = self.alloc_job_id();
                    return vec![IoAction::SendResponse {
                        body: serde_json::json!({"error": "ErrCannotDeleteAdminSpace"}),
                        status: 403,
                        job_id: j,
                    }];
                }
                if let Some(arb) = self.collection.get_mut(arbiter_did) {
                    arb.spaces.remove(&sk);
                    arb.version += 1;
                }
            }
        } else if method == NSID::SET_SPACE_MEMBER_ACCESS {
            let sk = params.get("spaceKey").and_then(|v| v.as_str()).map(String::from);
            let md = params
                .get("memberDid")
                .or_else(|| params.get("member").and_then(|m| m.get("did")))
                .and_then(|v| v.as_str())
                .map(String::from);
            if let (Some(sk), Some(md)) = (sk, md) {
                if let Some(arb) = self.collection.get_mut(arbiter_did) {
                    if let Some(space) = arb.spaces.get_mut(&sk) {
                        let access = params.get("access").cloned().unwrap_or_default();
                        if let Some(existing) = space.members.iter_mut().find(|m| m.did == md) {
                            existing.access = access;
                        } else {
                            space.members.push(MemberEntry { did: md, access });
                        }
                    }
                    arb.version += 1;
                }
            }
        } else if method == NSID::REMOVE_SPACE_MEMBER {
            let sk = params.get("spaceKey").and_then(|v| v.as_str()).map(String::from);
            let md = params
                .get("memberDid")
                .or_else(|| params.get("member").and_then(|m| m.get("did")))
                .and_then(|v| v.as_str())
                .map(String::from);
            if let (Some(sk), Some(md)) = (sk, md) {
                if let Some(arb) = self.collection.get_mut(arbiter_did) {
                    if let Some(space) = arb.spaces.get_mut(&sk) {
                        space.members.retain(|m| m.did != md);
                    }
                    arb.version += 1;
                }
            }
        } else {
            // Unknown method — proxy to backend.
            return self.execute_proxy(&method, &params, arbiter_did);
        }

        // Persist and respond.
        self.respond_with_persist()
    }

    // -------------------------------------------------------------------
    // Read-only operations
    // -------------------------------------------------------------------

    fn execute_read(&mut self, method: &str, params: &Value, arbiter_did: &str) -> Vec<IoAction> {
        let arbiter = match self.collection.get(arbiter_did) {
            Some(a) => a,
            None => {
                let j = self.alloc_job_id();
                return vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": "ErrArbiterNotExists"}),
                    status: 404,
                    job_id: j,
                }];
            }
        };

        let body = match method {
            NSID::GET_ARBITER_CONFIG => serde_json::json!({ "config": &arbiter.config }),
            NSID::GET_SPACE_CONFIG => {
                let sk = params.get("spaceKey").and_then(|v| v.as_str()).unwrap_or("");
                let config = arbiter.spaces.get(sk).map(|s| &s.config);
                serde_json::json!({ "config": config, "spaceType": "" })
            }
            NSID::GET_SPACE_MEMBERS => {
                let sk = params.get("spaceKey").and_then(|v| v.as_str()).unwrap_or("");
                let members: Vec<Value> = arbiter.spaces.get(sk)
                    .map(|s| s.members.iter().map(|m| serde_json::json!({
                        "member": { "did": m.did }, "access": m.access,
                    })).collect())
                    .unwrap_or_default();
                serde_json::json!({ "members": members })
            }
            NSID::LIST_SPACES => {
                let spaces: Vec<Value> = arbiter.spaces.values().map(|s| serde_json::json!({
                    "spaceKey": s.key, "spaceType": s.space_type, "config": s.config,
                })).collect();
                serde_json::json!({ "spaces": spaces })
            }
            _ => unreachable!(),
        };

        let j = self.alloc_job_id();
        vec![IoAction::SendResponse { body, status: 200, job_id: j }]
    }

    // -------------------------------------------------------------------
    // Proxy to backend
    // -------------------------------------------------------------------

    fn execute_proxy(&mut self, method: &str, params: &Value, arbiter_did: &str) -> Vec<IoAction> {
        let arbiter = match self.collection.get(arbiter_did) {
            Some(a) => a,
            None => {
                let j = self.alloc_job_id();
                return vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": "ErrArbiterNotExists"}),
                    status: 404,
                    job_id: j,
                }];
            }
        };

        let backend_url = arbiter.config
            .get("backendUrl")
            .and_then(|v| v.as_str())
            .map(String::from);

        if let Some(url) = backend_url {
            let j = self.alloc_job_id();
            vec![IoAction::ProxyXrpc {
                backend_url: url,
                path: method.to_string(),
                params: params.clone(),
                job_id: j,
            }]
        } else {
            let j = self.alloc_job_id();
            vec![IoAction::SendResponse {
                body: serde_json::json!({"error": "ErrBackendNotConfigured"}),
                status: 502,
                job_id: j,
            }]
        }
    }

    // -------------------------------------------------------------------
    // Create arbiter (bootstrap — no policy check)
    // -------------------------------------------------------------------

    fn do_create_arbiter(&mut self, params: Value) -> Vec<IoAction> {
        let arbiter_did = match self.arbiter_did_from_params(&params) {
            Some(d) => d.to_string(),
            None => {
                let j = self.alloc_job_id();
                return vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": "missing arbiterDid"}),
                    status: 400,
                    job_id: j,
                }];
            }
        };

        if self.collection.arbiters.contains_key(&arbiter_did) {
            let j = self.alloc_job_id();
            return vec![IoAction::SendResponse {
                body: serde_json::json!({"error": "ErrArbiterAlreadyExists"}),
                status: 409,
                job_id: j,
            }];
        }

        let config = params.get("config").cloned().unwrap_or_default();
        let policy = params
            .get("config")
            .and_then(|c| c.get("policy"))
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();

        self.collection.create_arbiter(arbiter_did, config, policy, String::new());
        self.respond_with_persist()
    }

    // -------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------

    fn respond_with_persist(&mut self) -> Vec<IoAction> {
        let snap = self.collection.snapshot();
        let snap_val = match serde_json::to_value(&snap) {
            Ok(v) => v,
            Err(_) => {
                let j = self.alloc_job_id();
                return vec![IoAction::SendResponse {
                    body: serde_json::json!({"error": "ErrSerialization"}),
                    status: 500,
                    job_id: j,
                }];
            }
        };
        let j_store = self.alloc_job_id();
        let j_resp = self.alloc_job_id();
        vec![
            IoAction::StoreSnapshot { snapshot_json: snap_val, job_id: j_store },
            IoAction::SendResponse { body: serde_json::json!({}), status: 200, job_id: j_resp },
        ]
    }

    fn handle_loaded_snapshot(&mut self, snapshot: Option<Value>) -> Vec<IoAction> {
        if let Some(snap) = snapshot {
            if let Ok(s) = serde_json::from_value::<ServerSnapshot>(snap) {
                self.collection.load_snapshot(s);
            }
        }
        vec![]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_create_arbiter_snapshot_roundtrip() {
        let mut coll = ArbiterCollection::new();
        coll.create_arbiter("did:plc:abc".into(), json!({"foo": "bar"}), "package arbiter".into(), "did:plc:alice".into());
        let snap = coll.snapshot();
        assert_eq!(snap.arbiters.len(), 1);

        let mut coll2 = ArbiterCollection::new();
        coll2.load_snapshot(snap);
        assert_eq!(coll2.get("did:plc:abc").and_then(|a| a.config.get("foo")).and_then(|v| v.as_str()), Some("bar"));
    }

    #[test]
    fn test_is_readonly_nsid() {
        assert!(is_readonly_nsid(NSID::GET_ARBITER_CONFIG));
        assert!(!is_readonly_nsid(NSID::SET_ARBITER_CONFIG));
    }

    #[test]
    fn test_serde_rego_roundtrip() {
        let original = json!({"s": "hello", "n": 42, "b": true, "a": [1, 2]});
        let rego = serde_to_rego(&original);
        let back = rego_to_serde(&rego);
        assert_eq!(original, back);
    }

    #[test]
    fn test_create_arbiter_via_state_machine() {
        let mut sm = StateMachine::new(ArbiterCollection::new());
        let actions = sm.handle_event(Event::IncomingXrpc {
            method: NSID::CREATE_ARBITER.into(),
            params: json!({"arbiterDid": "did:plc:abc", "config": {"foo": "bar"}}),
            token: None,
        });
        assert_eq!(actions.len(), 2); // StoreSnapshot + SendResponse
        assert!(sm.collection.get("did:plc:abc").is_some());
    }

    #[test]
    fn test_get_arbiter_config_no_auth() {
        let mut coll = ArbiterCollection::new();
        coll.create_arbiter("did:plc:abc".into(), json!({"key": "val"}), "package arbiter\ndefault allow := true".into(), "did:plc:alice".into());
        let mut sm = StateMachine::new(coll);
        let actions = sm.handle_event(Event::IncomingXrpc {
            method: NSID::GET_ARBITER_CONFIG.into(),
            params: json!({"arbiterDid": "did:plc:abc"}),
            token: None,
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
    fn test_auth_required_then_resolved() {
        let mut coll = ArbiterCollection::new();
        coll.create_arbiter("did:plc:abc".into(), json!({}), "package arbiter\ndefault allow := true".into(), "did:plc:alice".into());
        let mut sm = StateMachine::new(coll);
        let actions = sm.handle_event(Event::IncomingXrpc {
            method: NSID::GET_ARBITER_CONFIG.into(),
            params: json!({"arbiterDid": "did:plc:abc"}),
            token: Some("some-jwt".into()),
        });
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], IoAction::ResolveAuth { .. }));

        let actions = sm.handle_event(Event::AuthResolved { caller_did: "did:plc:alice".into(), job_id: 1 });
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], IoAction::SendResponse { status: 200, .. }));
    }

    #[test]
    fn test_auth_required_then_failed() {
        let mut coll = ArbiterCollection::new();
        coll.create_arbiter("did:plc:abc".into(), json!({}), "package arbiter\ndefault allow := true".into(), "did:plc:alice".into());
        let mut sm = StateMachine::new(coll);
        let actions = sm.handle_event(Event::IncomingXrpc {
            method: NSID::GET_ARBITER_CONFIG.into(),
            params: json!({"arbiterDid": "did:plc:abc"}),
            token: Some("bad".into()),
        });
        assert!(matches!(&actions[0], IoAction::ResolveAuth { .. }));

        let actions = sm.handle_event(Event::AuthFailed { job_id: 1 });
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], IoAction::SendResponse { status: 401, .. }));
    }

    #[test]
    fn test_missing_arbiter_did() {
        let mut sm = StateMachine::new(ArbiterCollection::new());
        let actions = sm.handle_event(Event::IncomingXrpc {
            method: NSID::GET_ARBITER_CONFIG.into(),
            params: json!({}),
            token: None,
        });
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], IoAction::SendResponse { status: 400, .. }));
    }

    #[test]
    fn test_shutdown() {
        let mut sm = StateMachine::new(ArbiterCollection::new());
        assert!(!sm.shutdown);
        let actions = sm.handle_event(Event::Shutdown);
        assert!(actions.is_empty());
        assert!(sm.shutdown);
    }
}
