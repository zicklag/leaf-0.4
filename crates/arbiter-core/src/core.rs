//! Core data types and state machine for the Muni Town Arbiter.
//!
//! This module mirrors the logic in `arbiter-simulator/src/lib/simulator.ts`,
//! providing a sans-IO Rust port of the simulator's arbitration logic.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::policy::{
    HostRequest, NSID, VmResult, VmSession, build_data_from_arbiter, build_op_input,
    json_to_regorus, regorus_to_json,
};

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

pub type Did = String;
pub type SpaceKey = String;
pub type JobId = u64;

// ---------------------------------------------------------------------------
// Core data structures
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

// ---------------------------------------------------------------------------
// Snapshot types (serialisable full state)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Operation result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceSummary {
    pub key: SpaceKey,
    pub space_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceRef {
    pub arbiter_did: Did,
    pub space_key: SpaceKey,
}

/// Successful operation result — the fields present depend on the NSID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpOk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<MemberEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_spaces: Option<Vec<SpaceRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spaces: Option<Vec<SpaceSummary>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpError {
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OpResult {
    Ok(OpOk),
    Err(OpError),
}

// ---------------------------------------------------------------------------
// Host request (suspension info surfaced to the IO layer)
// ---------------------------------------------------------------------------

/// A request for the IO layer to resolve.
#[derive(Debug, Clone)]
pub enum CoreRequest {
    /// Proxy an XRPC query to this arbiter's configured backend.
    ///
    /// The policy requested local data via `xrpc_local()`. For native arbiter
    /// NSIDs the policy accesses data directly through Rego rules; this variant
    /// is for foreign NSIDs that need proxying to the backend service.
    Local {
        /// DID of the arbiter whose backend should handle this.
        arbiter_did: Did,
        /// The XRPC method NSID to proxy.
        path: String,
        /// Input parameters for the query.
        input: Value,
    },
    /// Resolve an XRPC query on a remote arbiter.
    Remote {
        /// DID of the caller (used for auth on the remote side).
        caller_did: Did,
        /// DID of the remote arbiter.
        remote_did: Did,
        /// The XRPC method NSID.
        path: String,
        /// Input parameters for the query.
        input: Value,
    },
}

// ---------------------------------------------------------------------------
// Operation step — the result of a single step of processing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum OpStep {
    /// The operation completed with a result.
    Done(OpResult),
    /// The policy needs data from a local or remote XRPC query before it can
    /// continue. The caller resolves it and calls
    /// [`ArbiterCore::resume_operation`].
    Suspended { job_id: JobId, request: CoreRequest },
    /// The arbiter was deleted (only from deleteArbiter).
    Deleted,
    /// Policy check passed for a foreign (non-arbiter) XRPC method.
    /// The IO layer should proxy the request to the arbiter's configured
    /// backend. The backend URL is read from the arbiter's config.
    ProxyRequest {
        arbiter_did: Did,
        caller_did: Did,
        nsid: String,
        params: Value,
    },
}

// ---------------------------------------------------------------------------
// Pending operation state (stored internally for suspension/resume)
// ---------------------------------------------------------------------------

/// What phase the pending operation is in.
enum PendingPhase {
    /// Authorizing the operation — evaluating `data.arbiter.allow`.
    Authorizing {
        caller_did: Did,
        operation_nsid: String,
        params: Value,
        /// What to do if authorization succeeds.
        action: Action,
    },
    /// Evaluating `data.arbiter.resolve_result` for resolveSpaceMembers.
    /// Authorization has already passed.
    ResolvingMembers {
        #[allow(dead_code)]
        caller_did: Did,
    },
    /// Authorizing a foreign (non-arbiter) XRPC method.
    AuthorizingForeign {
        caller_did: Did,
        nsid: String,
        params: Value,
    },
}

struct PendingOp {
    arbiter_did: Did,
    session: VmSession,
    phase: PendingPhase,
}

// ---------------------------------------------------------------------------
// Actions (operations to execute after policy authorization)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Action {
    GetArbiterConfig,
    SetArbiterConfig(Value),
    DeleteArbiter,
    CreateSpace {
        space_key: SpaceKey,
        space_type: String,
        config: Value,
    },
    GetSpaceConfig {
        space_key: SpaceKey,
    },
    SetSpaceConfig {
        space_key: SpaceKey,
        space_type: String,
        config: Value,
    },
    DeleteSpace {
        space_key: SpaceKey,
    },
    ListSpaces,
    GetSpaceMembers {
        space_key: SpaceKey,
    },
    ResolveSpaceMembers {
        space_key: SpaceKey,
    },
    SetSpaceMemberAccess {
        space_key: SpaceKey,
        member_did: Did,
        access: Value,
    },
    RemoveSpaceMember {
        space_key: SpaceKey,
        member_did: Did,
    },
    /// Update the DID document configuration.
    UpdateDidDoc(Value),
}

// ---------------------------------------------------------------------------
// ArbiterCore — main state machine
// ---------------------------------------------------------------------------

/// Sans-IO state machine managing a collection of arbiters.
///
/// Mirrors the `Simulator` class in `arbiter-simulator/src/lib/simulator.ts`.
///
/// # IO layer contract
///
/// Operations that require policy evaluation return [`OpStep::Suspended`]
/// when the policy needs remote data. The IO layer (test harness or server)
/// resolves the request and calls [`resume_operation`](Self::resume_operation).
pub struct ArbiterCore {
    /// All arbiters keyed by DID.
    pub arbiters: HashMap<Did, ArbiterState>,

    /// Default Rego policy used when creating new arbiters.
    pub default_policy: String,

    /// Monotonically increasing time counter, incremented on each mutation.
    pub time: u64,

    /// Pending policy evaluations waiting for remote data.
    pending: HashMap<JobId, PendingOp>,

    /// Next job ID.
    next_job_id: JobId,
}

impl ArbiterCore {
    /// Create a new empty core with the given default policy.
    pub fn new(default_policy: impl Into<String>) -> Self {
        Self {
            arbiters: HashMap::new(),
            default_policy: default_policy.into(),
            time: 0,
            pending: HashMap::new(),
            next_job_id: 1,
        }
    }

    // -------------------------------------------------------------------
    // Synchronous operations (no policy check needed)
    // -------------------------------------------------------------------

    /// Create a new arbiter. Bypasses policy (identity bootstrap).
    pub fn create_arbiter(&mut self, arbiter_did: Did, config: Value, owner_did: Did) -> OpResult {
        if self.arbiters.contains_key(&arbiter_did) {
            return OpResult::Err(OpError {
                error: "ErrArbiterAlreadyExists".into(),
            });
        }

        // Extract policy from config, or use default
        let policy = config
            .get("policy")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.default_policy)
            .to_string();

        let admin_space = Space {
            key: "$admin".into(),
            space_type: "town.muni.arbiter.config.adminSpace".into(),
            config: json!({}),
            members: vec![MemberEntry {
                did: owner_did,
                access: json!({"level": "Owner"}),
            }],
        };

        let mut spaces = HashMap::new();
        spaces.insert(admin_space.key.clone(), admin_space);

        self.arbiters.insert(
            arbiter_did.clone(),
            ArbiterState {
                did: arbiter_did,
                version: 1,
                config,
                policy,
                online: true,
                spaces,
            },
        );

        self.time += 1;
        OpResult::Ok(OpOk {
            config: None,
            members: None,
            missing_spaces: None,
            spaces: None,
        })
    }

    /// Convenience: create an arbiter with default policy.
    pub fn create_default_arbiter(&mut self, arbiter_did: Did, owner_did: Did) -> OpResult {
        self.create_arbiter(arbiter_did, json!({}), owner_did)
    }

    /// Toggle an arbiter's online/offline status.
    pub fn toggle_arbiter_offline(&mut self, did: &str) {
        if let Some(arb) = self.arbiters.get_mut(did) {
            arb.online = !arb.online;
        }
    }

    /// Check if an arbiter is offline.
    pub fn is_arbiter_offline(&self, did: &str) -> bool {
        self.arbiters.get(did).is_none_or(|a| !a.online)
    }

    // -------------------------------------------------------------------
    // Policy-driven operations (may suspend)
    // -------------------------------------------------------------------

    /// Begin processing an operation. Returns a step indicating whether the
    /// operation completed or needs more data before it can continue.
    pub fn process_operation(
        &mut self,
        arbiter_did: &str,
        caller_did: &str,
        nsid: &str,
        params: Value,
    ) -> OpStep {
        let arbiter = match self.arbiters.get(arbiter_did) {
            Some(a) => a,
            None => {
                return OpStep::Done(OpResult::Err(OpError {
                    error: "ErrArbiterNotExists".into(),
                }));
            }
        };

        // Determine the action from the NSID and params.
        // Native arbiter methods get structural validation + action dispatch.
        // Foreign XRPC methods just get policy auth + proxy signal.
        match build_action(nsid, &params) {
            Some(action) => {
                // Structural validation (pre-policy checks)
                if let Some(err) = validate_operation(arbiter, &action) {
                    return OpStep::Done(OpResult::Err(err));
                }
                self.start_auth_check(
                    arbiter_did.to_string(),
                    caller_did.to_string(),
                    nsid.to_string(),
                    params,
                    action,
                )
            }
            None => {
                // Foreign XRPC method — just check policy for auth.
                // If allowed, the IO layer proxies to the arbiter's backend.
                self.start_foreign_auth_check(
                    arbiter_did.to_string(),
                    caller_did.to_string(),
                    nsid.to_string(),
                    params,
                )
            }
        }
    }

    /// Execute a native arbiter query directly on the core, bypassing policy.
    ///
    /// Used by the IO layer to resolve `xrpc_local` calls from the policy
    /// for native arbiter NSIDs (getSpaceMembers, getSpaceConfig, etc.).
    /// The action is built from the NSID and params, then executed without
    /// any authorization check since the policy itself is the caller.
    pub fn execute_query_direct(
        &mut self,
        arbiter_did: &str,
        nsid: &str,
        params: &Value,
    ) -> OpStep {
        let action = match build_action(nsid, params) {
            Some(a) => a,
            None => {
                return OpStep::Done(OpResult::Err(OpError {
                    error: format!("ErrUnknownNSID: {nsid}"),
                }));
            }
        };
        self.execute_authorized_action(
            arbiter_did.to_string(),
            "",
            nsid,
            params,
            action,
        )
    }

    /// Check whether a given NSID is a native arbiter query that can be
    /// handled internally via [`execute_query_direct`].
    pub fn is_native_query(&self, nsid: &str) -> bool {
        build_action(nsid, &Value::Null).is_some()
    }

    /// Resume a suspended operation with resolved data from a remote or
    /// local XRPC query.
    ///
    /// The `job_id` must match a `Suspended` step returned earlier.
    /// `resolved_value` is the JSON value returned by the XRPC query
    /// that the policy requested.
    pub fn resume_operation(&mut self, job_id: JobId, resolved_value: Value) -> OpStep {
        let pending = match self.pending.remove(&job_id) {
            Some(p) => p,
            None => {
                return OpStep::Done(OpResult::Err(OpError {
                    error: "ErrInvalidJobId".into(),
                }));
            }
        };

        self.continue_evaluation(pending, resolved_value)
    }

    // -------------------------------------------------------------------
    // Snapshot / serialisation
    // -------------------------------------------------------------------

    /// Capture a full snapshot of the server state.
    pub fn snapshot(&self) -> ServerSnapshot {
        let arbiters: Vec<ArbiterSnapshot> = self
            .arbiters
            .values()
            .map(|a| {
                let spaces: Vec<SpaceSnapshot> = a
                    .spaces
                    .values()
                    .map(|s| SpaceSnapshot {
                        key: s.key.clone(),
                        space_type: s.space_type.clone(),
                        config: s.config.clone(),
                        members: s.members.clone(),
                    })
                    .collect();
                ArbiterSnapshot {
                    did: a.did.clone(),
                    version: a.version,
                    config: a.config.clone(),
                    policy: a.policy.clone(),
                    online: a.online,
                    spaces,
                }
            })
            .collect();
        ServerSnapshot { arbiters }
    }

    /// Load a snapshot into the core, replacing all current state.
    pub fn load_snapshot(&mut self, snapshot: ServerSnapshot) {
        self.arbiters.clear();
        for a in snapshot.arbiters {
            let spaces: HashMap<SpaceKey, Space> = a
                .spaces
                .into_iter()
                .map(|s| {
                    (
                        s.key.clone(),
                        Space {
                            key: s.key,
                            space_type: s.space_type,
                            config: s.config,
                            members: s.members,
                        },
                    )
                })
                .collect();
            self.arbiters.insert(
                a.did.clone(),
                ArbiterState {
                    did: a.did,
                    version: a.version,
                    config: a.config,
                    policy: a.policy,
                    online: a.online,
                    spaces,
                },
            );
        }
        // Clear any pending operations
        self.pending.clear();
    }

    // -------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------

    /// Start a policy authorization check.
    fn start_auth_check(
        &mut self,
        arbiter_did: Did,
        caller_did: Did,
        nsid: String,
        params: Value,
        action: Action,
    ) -> OpStep {
        let arbiter = match self.arbiters.get(&arbiter_did) {
            Some(a) => a,
            None => {
                return OpStep::Done(OpResult::Err(OpError {
                    error: "ErrArbiterNotExists".into(),
                }));
            }
        };

        let data = json_to_regorus(&build_data_from_arbiter(arbiter));
        let input = json_to_regorus(&build_op_input(&caller_did, &nsid, &params));

        let session = match VmSession::new(&arbiter.policy, &data, &input, &["data.arbiter.allow"])
        {
            Ok(s) => s,
            Err(e) => {
                return OpStep::Done(OpResult::Err(OpError {
                    error: format!("ErrPolicyCompile: {e}"),
                }));
            }
        };

        self.handle_session_result(
            arbiter_did.clone(),
            session,
            PendingPhase::Authorizing {
                caller_did,
                operation_nsid: nsid,
                params,
                action,
            },
        )
    }

    /// Start a policy authorization check for a foreign (non-arbiter) XRPC
    /// method. If allowed, returns [`OpStep::ProxyRequest`]; if denied,
    /// returns an error.
    fn start_foreign_auth_check(
        &mut self,
        arbiter_did: Did,
        caller_did: Did,
        nsid: String,
        params: Value,
    ) -> OpStep {
        let arbiter = match self.arbiters.get(&arbiter_did) {
            Some(a) => a,
            None => {
                return OpStep::Done(OpResult::Err(OpError {
                    error: "ErrArbiterNotExists".into(),
                }));
            }
        };

        let data = json_to_regorus(&build_data_from_arbiter(arbiter));
        let input = json_to_regorus(&build_op_input(&caller_did, &nsid, &params));

        let session = match VmSession::new(&arbiter.policy, &data, &input, &["data.arbiter.allow"])
        {
            Ok(s) => s,
            Err(e) => {
                return OpStep::Done(OpResult::Err(OpError {
                    error: format!("ErrPolicyCompile: {e}"),
                }));
            }
        };

        self.handle_session_result(
            arbiter_did,
            session,
            PendingPhase::AuthorizingForeign {
                caller_did,
                nsid,
                params,
            },
        )
    }

    /// Handle the result of a VmSession step (start or resume).
    fn handle_session_result(
        &mut self,
        arbiter_did: Did,
        mut session: VmSession,
        phase: PendingPhase,
    ) -> OpStep {
        match session.start() {
            Ok(VmResult::Completed(value)) => {
                self.on_policy_completed(arbiter_did, phase, regorus_to_json(&value))
            }
            Ok(VmResult::Suspended(request)) => {
                self.on_policy_suspended(arbiter_did, session, phase, request)
            }
            Err(e) => OpStep::Done(OpResult::Err(OpError {
                error: format!("ErrPolicyEval: {e}"),
            })),
        }
    }

    /// Handle the result of a VmSession resume.
    fn handle_resume_result(
        &mut self,
        arbiter_did: Did,
        mut session: VmSession,
        phase: PendingPhase,
        resume_value: Value,
    ) -> OpStep {
        match session.resume(&json_to_regorus(&resume_value)) {
            Ok(VmResult::Completed(value)) => {
                self.on_policy_completed(arbiter_did, phase, regorus_to_json(&value))
            }
            Ok(VmResult::Suspended(request)) => {
                self.on_policy_suspended(arbiter_did, session, phase, request)
            }
            Err(e) => OpStep::Done(OpResult::Err(OpError {
                error: format!("ErrPolicyEval: {e}"),
            })),
        }
    }

    /// Policy evaluation completed (not suspended). Handle the result.
    fn on_policy_completed(
        &mut self,
        arbiter_did: Did,
        phase: PendingPhase,
        value: Value,
    ) -> OpStep {
        match phase {
            PendingPhase::Authorizing {
                caller_did,
                operation_nsid,
                params,
                action,
            } => {
                // Check if the policy allowed the operation
                let allowed = value.as_bool().unwrap_or(false);
                if !allowed {
                    return OpStep::Done(OpResult::Err(OpError {
                        error: "ErrPermissionDenied".into(),
                    }));
                }

                // Authorization passed — execute or proceed
                self.execute_authorized_action(
                    arbiter_did,
                    &caller_did,
                    &operation_nsid,
                    &params,
                    action,
                )
            }
            PendingPhase::ResolvingMembers { caller_did: _ } => {
                // resolve_result returned the member list
                let obj = value.as_object().cloned().unwrap_or_default();
                let members: Vec<MemberEntry> = obj
                    .get("members")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                let missing: Vec<SpaceRef> = obj
                    .get("missingSpaces")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                OpStep::Done(OpResult::Ok(OpOk {
                    config: None,
                    members: Some(members),
                    missing_spaces: Some(missing),
                    spaces: None,
                }))
            }
            PendingPhase::AuthorizingForeign {
                caller_did,
                nsid,
                params,
            } => {
                let allowed = value.as_bool().unwrap_or(false);
                if !allowed {
                    return OpStep::Done(OpResult::Err(OpError {
                        error: "ErrPermissionDenied".into(),
                    }));
                }
                // Policy passed — signal the IO layer to proxy this request
                // to the arbiter's configured backend.
                OpStep::ProxyRequest {
                    arbiter_did,
                    caller_did,
                    nsid,
                    params,
                }
            }
        }
    }

    /// Policy evaluation suspended — store the pending state and surface
    /// the request to the caller.
    fn on_policy_suspended(
        &mut self,
        arbiter_did: Did,
        session: VmSession,
        phase: PendingPhase,
        request: HostRequest,
    ) -> OpStep {
        let job_id = self.next_job_id;
        self.next_job_id += 1;

        let core_request = match &request {
            HostRequest::XrpcLocal { path, input } => CoreRequest::Local {
                arbiter_did: arbiter_did.clone(),
                path: path.clone(),
                input: regorus_to_json(input),
            },
            HostRequest::XrpcRemote { did, path, input } => CoreRequest::Remote {
                caller_did: arbiter_did.clone(),
                remote_did: did.clone(),
                path: path.clone(),
                input: regorus_to_json(input),
            },
        };

        self.pending.insert(
            job_id,
            PendingOp {
                arbiter_did,
                session,
                phase,
            },
        );

        OpStep::Suspended {
            job_id,
            request: core_request,
        }
    }

    /// Execute the authorized action, which may involve executing a mutation
    /// or starting a secondary policy evaluation (for resolveSpaceMembers).
    fn execute_authorized_action(
        &mut self,
        arbiter_did: Did,
        caller_did: &str,
        _operation_nsid: &str,
        _params: &Value,
        action: Action,
    ) -> OpStep {
        match action {
            // --- Queries (return data, no mutation) ---
            Action::GetArbiterConfig => {
                let arbiter = match self.arbiters.get(&arbiter_did) {
                    Some(a) => a,
                    None => {
                        return OpStep::Done(OpResult::Err(OpError {
                            error: "ErrArbiterNotExists".into(),
                        }));
                    }
                };
                OpStep::Done(OpResult::Ok(OpOk {
                    config: Some(arbiter.config.clone()),
                    members: None,
                    missing_spaces: None,
                    spaces: None,
                }))
            }
            Action::GetSpaceConfig { space_key } => {
                let arbiter = match self.arbiters.get(&arbiter_did) {
                    Some(a) => a,
                    None => {
                        return OpStep::Done(OpResult::Err(OpError {
                            error: "ErrArbiterNotExists".into(),
                        }));
                    }
                };
                let space = match arbiter.spaces.get(&space_key) {
                    Some(s) => s,
                    None => {
                        return OpStep::Done(OpResult::Err(OpError {
                            error: "ErrSpaceNotExists".into(),
                        }));
                    }
                };
                OpStep::Done(OpResult::Ok(OpOk {
                    config: Some(space.config.clone()),
                    members: None,
                    missing_spaces: None,
                    spaces: None,
                }))
            }
            Action::ListSpaces => {
                let arbiter = match self.arbiters.get(&arbiter_did) {
                    Some(a) => a,
                    None => {
                        return OpStep::Done(OpResult::Err(OpError {
                            error: "ErrArbiterNotExists".into(),
                        }));
                    }
                };
                let spaces: Vec<SpaceSummary> = arbiter
                    .spaces
                    .values()
                    .map(|s| SpaceSummary {
                        key: s.key.clone(),
                        space_type: s.space_type.clone(),
                    })
                    .collect();
                OpStep::Done(OpResult::Ok(OpOk {
                    config: None,
                    members: None,
                    missing_spaces: None,
                    spaces: Some(spaces),
                }))
            }
            Action::GetSpaceMembers { space_key } => {
                let arbiter = match self.arbiters.get(&arbiter_did) {
                    Some(a) => a,
                    None => {
                        return OpStep::Done(OpResult::Err(OpError {
                            error: "ErrArbiterNotExists".into(),
                        }));
                    }
                };
                let space = match arbiter.spaces.get(&space_key) {
                    Some(s) => s,
                    None => {
                        return OpStep::Done(OpResult::Err(OpError {
                            error: "ErrSpaceNotExists".into(),
                        }));
                    }
                };
                OpStep::Done(OpResult::Ok(OpOk {
                    config: None,
                    members: Some(space.members.clone()),
                    missing_spaces: None,
                    spaces: None,
                }))
            }
            Action::ResolveSpaceMembers { space_key } => {
                // Authorization passed. Now evaluate resolve_result to get
                // the resolved member list. This may suspend for remote data.
                self.start_resolve_evaluation(arbiter_did, caller_did.to_string(), space_key)
            }

            // --- Procedures (mutations) ---
            Action::SetArbiterConfig(new_config) => {
                self.mutate_arbiter(&arbiter_did, |arb| {
                    arb.config = new_config;
                    arb.version += 1;
                });
                self.time += 1;
                OpStep::Done(OpResult::Ok(OpOk {
                    config: None,
                    members: None,
                    missing_spaces: None,
                    spaces: None,
                }))
            }
            Action::DeleteArbiter => {
                self.arbiters.remove(&arbiter_did);
                self.time += 1;
                OpStep::Deleted
            }
            Action::CreateSpace {
                space_key,
                space_type,
                config,
            } => {
                let space = Space {
                    key: space_key.clone(),
                    space_type,
                    config,
                    members: vec![],
                };
                if let Some(arb) = self.arbiters.get_mut(&arbiter_did) {
                    arb.spaces.insert(space_key, space);
                    arb.version += 1;
                }
                self.time += 1;
                OpStep::Done(OpResult::Ok(OpOk {
                    config: None,
                    members: None,
                    missing_spaces: None,
                    spaces: None,
                }))
            }
            Action::SetSpaceConfig {
                space_key,
                space_type,
                config,
            } => {
                if let Some(arb) = self.arbiters.get_mut(&arbiter_did)
                    && let Some(space) = arb.spaces.get_mut(&space_key) {
                        space.space_type = space_type;
                        space.config = config;
                        arb.version += 1;
                    }
                self.time += 1;
                OpStep::Done(OpResult::Ok(OpOk {
                    config: None,
                    members: None,
                    missing_spaces: None,
                    spaces: None,
                }))
            }
            Action::DeleteSpace { space_key } => {
                if let Some(arb) = self.arbiters.get_mut(&arbiter_did) {
                    arb.spaces.remove(&space_key);
                    arb.version += 1;
                }
                self.time += 1;
                OpStep::Done(OpResult::Ok(OpOk {
                    config: None,
                    members: None,
                    missing_spaces: None,
                    spaces: None,
                }))
            }
            Action::SetSpaceMemberAccess {
                space_key,
                member_did,
                access,
            } => {
                if let Some(arb) = self.arbiters.get_mut(&arbiter_did)
                    && let Some(space) = arb.spaces.get_mut(&space_key) {
                        let idx = space.members.iter().position(|m| m.did == member_did);
                        if let Some(i) = idx {
                            space.members[i].access = access;
                        } else {
                            space.members.push(MemberEntry {
                                did: member_did,
                                access,
                            });
                        }
                        arb.version += 1;
                    }
                self.time += 1;
                OpStep::Done(OpResult::Ok(OpOk {
                    config: None,
                    members: None,
                    missing_spaces: None,
                    spaces: None,
                }))
            }
            Action::RemoveSpaceMember {
                space_key,
                member_did,
            } => {
                if let Some(arb) = self.arbiters.get_mut(&arbiter_did)
                    && let Some(space) = arb.spaces.get_mut(&space_key) {
                        space.members.retain(|m| m.did != member_did);
                        arb.version += 1;
                    }
                self.time += 1;
                OpStep::Done(OpResult::Ok(OpOk {
                    config: None,
                    members: None,
                    missing_spaces: None,
                    spaces: None,
                }))
            }
            Action::UpdateDidDoc(_config) => {
                // Policy check passed.
                // The actual PLC directory interaction happens in the
                // server layer after this returns.
                self.time += 1;
                OpStep::Done(OpResult::Ok(OpOk {
                    config: None,
                    members: None,
                    missing_spaces: None,
                    spaces: None,
                }))
            }
        }
    }

    /// Start evaluating `data.arbiter.resolve_result` for resolveSpaceMembers.
    fn start_resolve_evaluation(
        &mut self,
        arbiter_did: Did,
        caller_did: Did,
        space_key: SpaceKey,
    ) -> OpStep {
        let arbiter = match self.arbiters.get(&arbiter_did) {
            Some(a) => a,
            None => {
                return OpStep::Done(OpResult::Err(OpError {
                    error: "ErrArbiterNotExists".into(),
                }));
            }
        };

        let data = json_to_regorus(&build_data_from_arbiter(arbiter));
        let input = json_to_regorus(&build_op_input(
            &caller_did,
            NSID::RESOLVE_SPACE_MEMBERS,
            &json!({"spaceKey": &space_key}),
        ));

        let session = match VmSession::new(
            &arbiter.policy,
            &data,
            &input,
            &["data.arbiter.resolve_result"],
        ) {
            Ok(s) => s,
            Err(e) => {
                return OpStep::Done(OpResult::Err(OpError {
                    error: format!("ErrPolicyCompile: {e}"),
                }));
            }
        };

        // Run the initial step of the resolve_result evaluation
        self.handle_session_result(
            arbiter_did,
            session,
            PendingPhase::ResolvingMembers { caller_did },
        )
    }

    /// Continue a pending evaluation (called from resume_operation).
    fn continue_evaluation(&mut self, pending: PendingOp, resolved_value: Value) -> OpStep {
        self.handle_resume_result(
            pending.arbiter_did,
            pending.session,
            pending.phase,
            resolved_value,
        )
    }

    /// Apply a mutation to an arbiter.
    fn mutate_arbiter(&mut self, arbiter_did: &str, f: impl FnOnce(&mut ArbiterState)) {
        if let Some(arb) = self.arbiters.get_mut(arbiter_did) {
            f(arb);
        }
    }
}

// ---------------------------------------------------------------------------
// NSID → Action builder (extracts values from params)
// ---------------------------------------------------------------------------

fn build_action(nsid: &str, params: &Value) -> Option<Action> {
    match nsid {
        NSID::GET_ARBITER_CONFIG => Some(Action::GetArbiterConfig),
        NSID::SET_ARBITER_CONFIG => Some(Action::SetArbiterConfig(
            params.get("config").cloned().unwrap_or(Value::Null),
        )),
        NSID::DELETE_ARBITER => Some(Action::DeleteArbiter),
        NSID::CREATE_SPACE => Some(Action::CreateSpace {
            space_key: params
                .get("spaceKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            space_type: params
                .get("spaceType")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            config: params.get("config").cloned().unwrap_or(Value::Null),
        }),
        NSID::GET_SPACE_CONFIG => Some(Action::GetSpaceConfig {
            space_key: params
                .get("spaceKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }),
        NSID::SET_SPACE_CONFIG => Some(Action::SetSpaceConfig {
            space_key: params
                .get("spaceKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            space_type: params
                .get("spaceType")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            config: params.get("config").cloned().unwrap_or(Value::Null),
        }),
        NSID::DELETE_SPACE => Some(Action::DeleteSpace {
            space_key: params
                .get("spaceKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }),
        NSID::LIST_SPACES => Some(Action::ListSpaces),
        NSID::GET_SPACE_MEMBERS => Some(Action::GetSpaceMembers {
            space_key: params
                .get("spaceKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }),
        NSID::RESOLVE_SPACE_MEMBERS => Some(Action::ResolveSpaceMembers {
            space_key: params
                .get("spaceKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }),
        NSID::SET_SPACE_MEMBER_ACCESS => Some(Action::SetSpaceMemberAccess {
            space_key: params
                .get("spaceKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            member_did: params
                .get("memberDid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            access: params.get("access").cloned().unwrap_or(Value::Null),
        }),
        NSID::REMOVE_SPACE_MEMBER => Some(Action::RemoveSpaceMember {
            space_key: params
                .get("spaceKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            member_did: params
                .get("memberDid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }),
        NSID::UPDATE_DID_DOC => Some(Action::UpdateDidDoc(
            params.get("config").cloned().unwrap_or(Value::Null),
        )),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Structural validation
// ---------------------------------------------------------------------------

fn validate_operation(arbiter: &ArbiterState, action: &Action) -> Option<OpError> {
    match action {
        Action::CreateSpace { space_key, .. } => {
            if arbiter.spaces.contains_key(space_key) {
                return Some(OpError {
                    error: "ErrSpaceExists".into(),
                });
            }
        }
        Action::GetSpaceConfig { space_key }
        | Action::SetSpaceConfig { space_key, .. }
        | Action::GetSpaceMembers { space_key }
        | Action::ResolveSpaceMembers { space_key }
        | Action::SetSpaceMemberAccess { space_key, .. }
        | Action::RemoveSpaceMember { space_key, .. } => {
            if !arbiter.spaces.contains_key(space_key) {
                return Some(OpError {
                    error: "ErrSpaceNotExists".into(),
                });
            }
        }
        Action::DeleteSpace { space_key } => {
            if space_key == "$admin" {
                return Some(OpError {
                    error: "ErrCannotDeleteAdminSpace".into(),
                });
            }
            if !arbiter.spaces.contains_key(space_key) {
                return Some(OpError {
                    error: "ErrSpaceNotExists".into(),
                });
            }
        }
        _ => {}
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests — ported from arbiter-simulator/src/lib/simulator.test.ts
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The default access-levels policy embedded from the project policy file.
    const DEFAULT_POLICY: &str = include_str!("../../../policies/arbiter/access-levels.rego");

    // -----------------------------------------------------------------------
    // TestHarness — wraps ArbiterCore with auto-resolution loop
    // -----------------------------------------------------------------------

    struct TestHarness {
        core: ArbiterCore,
    }

    impl TestHarness {
        fn new() -> Self {
            let core = ArbiterCore::new(DEFAULT_POLICY);
            Self { core }
        }

        fn create_default_arbiter(&mut self, did: &str, owner: &str) {
            let result = self
                .core
                .create_arbiter(did.into(), json!({}), owner.into());
            assert!(
                matches!(result, OpResult::Ok(_)),
                "create_arbiter failed for {did}"
            );
        }

        fn create_arbiter(&mut self, did: &str, owner: &str, policy: &str) {
            let result = self.core.create_arbiter(
                did.into(),
                json!({"$type": "town.muni.arbiter.config.regoPolicy", "policy": policy}),
                owner.into(),
            );
            assert!(matches!(result, OpResult::Ok(_)));
        }

        /// Assert an operation succeeds. Handles the suspension loop.
        fn assert_ok(
            &mut self,
            arbiter: &str,
            caller: &str,
            space_key: &str,
            nsid: &str,
            params: Value,
        ) {
            let step = self.run_op(arbiter, caller, nsid, params);
            match step {
                OpStep::Done(OpResult::Ok(_)) => {}
                other => {
                    panic!(
                        "Expected success for {caller}@{arbiter}/{space_key} ({nsid}), got {other:?}"
                    )
                }
            }
        }

        /// Assert an operation is denied.
        fn assert_denied(
            &mut self,
            arbiter: &str,
            caller: &str,
            space_key: &str,
            nsid: &str,
            params: Value,
        ) {
            let step = self.run_op(arbiter, caller, nsid, params);
            match step {
                OpStep::Done(OpResult::Err(e)) => {
                    assert!(
                        e.error.to_lowercase().contains("denied"),
                        "Expected denied error, got '{}'",
                        e.error
                    );
                }
                other => {
                    panic!(
                        "Expected denial for {caller}@{arbiter}/{space_key} ({nsid}), got {other:?}"
                    )
                }
            }
        }

        /// Resolve members for a space and return the entries.
        fn resolved_members(
            &mut self,
            arbiter: &str,
            caller: &str,
            space_key: &str,
        ) -> Vec<MemberEntry> {
            let step = self.run_op(
                arbiter,
                caller,
                NSID::RESOLVE_SPACE_MEMBERS,
                json!({"spaceKey": space_key}),
            );
            match step {
                OpStep::Done(OpResult::Ok(ok)) => ok.members.unwrap_or_default(),
                other => panic!("Expected ok for resolveSpaceMembers, got {other:?}"),
            }
        }

        /// Get a mutable reference to a space (for modifying config).
        fn space_mut(&mut self, arbiter: &str, space_key: &str) -> &mut Space {
            self.core
                .arbiters
                .get_mut(arbiter)
                .and_then(|a| a.spaces.get_mut(space_key))
                .expect("Space not found")
        }

        /// Run an operation through to completion, handling all suspensions.
        fn run_op(&mut self, arbiter: &str, caller: &str, nsid: &str, params: Value) -> OpStep {
            let step = self.core.process_operation(arbiter, caller, nsid, params);
            self.resolve_loop(step)
        }

        /// Recursively resolve all suspensions until completion.
        fn resolve_loop(&mut self, step: OpStep) -> OpStep {
            match step {
                OpStep::Suspended { job_id, request } => {
                    let resolved = self.resolve_request(&request);
                    let next = self.core.resume_operation(job_id, resolved);
                    self.resolve_loop(next)
                }
                done => done,
            }
        }

        /// Resolve a single CoreRequest to a JSON value.
        fn resolve_request(&mut self, request: &CoreRequest) -> Value {
            match request {
                CoreRequest::Local {
                    arbiter_did,
                    path,
                    input,
                } => {
                    // Native arbiter query — execute directly, bypassing policy
                    match self.core.execute_query_direct(arbiter_did, path, input) {
                        OpStep::Done(OpResult::Ok(ok)) => {
                            // Return the full XRPC response object.
                            // The policy extracts the relevant field.
                            serde_json::to_value(ok).unwrap_or(json!([]))
                        }
                        _ => json!([]),
                    }
                }
                CoreRequest::Remote {
                    caller_did,
                    remote_did,
                    path,
                    input,
                } => {
                    // Remote arbiter doesn't exist or is offline
                    if self.core.is_arbiter_offline(remote_did) {
                        return json!([]);
                    }
                    if !self.core.arbiters.contains_key(remote_did.as_str()) {
                        return json!([]);
                    }
                    // Extract space_key from input
                    let space_key = input
                        .get("spaceKey")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if space_key.is_empty() {
                        return json!([]);
                    }
                    // Call resolveSpaceMembers on the remote arbiter with auth
                    // using caller_did (the local arbiter's DID).
                    let params = json!({"spaceKey": space_key});
                    let step = self
                        .core
                        .process_operation(remote_did, caller_did, path, params);
                    let resolved_step = self.resolve_loop(step);
                    match resolved_step {
                        OpStep::Done(OpResult::Ok(ok)) => {
                            // Return the full XRPC response object.
                            // The policy extracts the relevant field (e.g., .members).
                            serde_json::to_value(ok).unwrap_or(json!([]))
                        }
                        _ => json!([]),
                    }
                }
            }
        }
    }

    // -------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------

    fn assert_member_exists(members: &[MemberEntry], expected_did: &str, expected_level: &str) {
        let found = members.iter().find(|m| m.did == expected_did);
        assert!(
            found.is_some(),
            "Member {expected_did} not found in resolved list"
        );
        let found = found.unwrap();
        let level = found
            .access
            .get("level")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(
            level, expected_level,
            "Member {expected_did} expected level {expected_level}, got {level}"
        );
    }

    // ===================================================================
    // Basic owner operations
    // ===================================================================

    #[test]
    fn test_owner_can_create_spaces() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.assert_ok(
      "org",
      "alice",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok(
      "org",
      "alice",
      "docs",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "docs", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
    }

    #[test]
    fn test_non_member_cannot_create_space() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.assert_denied(
      "org",
      "stranger",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
    }

    #[test]
    fn test_owner_can_delete_arbiter() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        let step = h.run_op(
            "org",
            "alice",
            NSID::DELETE_ARBITER,
            json!({"spaceKey": "$admin"}),
        );
        assert!(
            matches!(step, OpStep::Deleted),
            "Expected Deleted, got {step:?}"
        );
        assert!(!h.core.arbiters.contains_key("org"));
    }

    #[test]
    fn test_non_owner_cannot_delete_arbiter() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.assert_denied(
            "org",
            "stranger",
            "$admin",
            NSID::DELETE_ARBITER,
            json!({"spaceKey": "$admin"}),
        );
    }

    #[test]
    fn test_multiple_owners_cannot_delete_arbiter() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.assert_ok(
            "org",
            "alice",
            "$admin",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "$admin", "memberDid": "bob", "access": {"level": "Owner"}}),
        );
        h.assert_denied(
            "org",
            "alice",
            "$admin",
            NSID::DELETE_ARBITER,
            json!({"spaceKey": "$admin"}),
        );
        h.assert_denied(
            "org",
            "bob",
            "$admin",
            NSID::DELETE_ARBITER,
            json!({"spaceKey": "$admin"}),
        );
    }

    #[test]
    fn test_owner_can_delete_space() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.assert_ok(
      "org",
      "alice",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok(
            "org",
            "alice",
            "team",
            NSID::DELETE_SPACE,
            json!({"spaceKey": "team"}),
        );
        assert!(!h.core.arbiters["org"].spaces.contains_key("team"));
    }

    #[test]
    fn test_owner_cannot_delete_admin_space() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        let step = h.run_op(
            "org",
            "alice",
            NSID::DELETE_SPACE,
            json!({"spaceKey": "$admin"}),
        );
        assert!(matches!(step, OpStep::Done(OpResult::Err(_))));
        assert!(h.core.arbiters["org"].spaces.contains_key("$admin"));
    }

    // ===================================================================
    // Access level hierarchy
    // ===================================================================

    #[test]
    fn test_owner_can_add_members() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.assert_ok(
            "org",
            "alice",
            "$admin",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "$admin", "memberDid": "bob", "access": {"level": "Owner"}}),
        );
        h.assert_ok(
            "org",
            "alice",
            "$admin",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "$admin", "memberDid": "carol", "access": {"level": "IsMember"}}),
        );
    }

    #[test]
    fn test_read_member_cannot_create_space() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.assert_ok(
      "org",
      "alice",
      "$admin",
      NSID::SET_SPACE_MEMBER_ACCESS,
      json!({"spaceKey": "$admin", "memberDid": "bob", "access": {"level": "ReadMemberList"}}),
    );
        h.assert_denied(
      "org",
      "bob",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
    }

    #[test]
    fn test_cannot_grant_higher_access_than_own() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.assert_ok(
            "org",
            "alice",
            "$admin",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "$admin", "memberDid": "bob", "access": {"level": "AddMembers"}}),
        );
        h.assert_ok(
            "org",
            "bob",
            "$admin",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "$admin", "memberDid": "carol", "access": {"level": "IsMember"}}),
        );
        h.assert_denied(
            "org",
            "bob",
            "$admin",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "$admin", "memberDid": "dave", "access": {"level": "Owner"}}),
        );
        h.assert_denied(
      "org",
      "bob",
      "$admin",
      NSID::SET_SPACE_MEMBER_ACCESS,
      json!({"spaceKey": "$admin", "memberDid": "eve", "access": {"level": "ConfigureSpace"}}),
    );
    }

    #[test]
    fn test_need_remove_members_to_modify_existing() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.assert_ok(
            "org",
            "alice",
            "$admin",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "$admin", "memberDid": "bob", "access": {"level": "IsMember"}}),
        );
        h.assert_ok(
            "org",
            "alice",
            "$admin",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "$admin", "memberDid": "carol", "access": {"level": "AddMembers"}}),
        );
        h.assert_ok(
      "org",
      "carol",
      "$admin",
      NSID::SET_SPACE_MEMBER_ACCESS,
      json!({"spaceKey": "$admin", "memberDid": "dave", "access": {"level": "ReadMemberList"}}),
    );
        h.assert_denied(
      "org",
      "carol",
      "$admin",
      NSID::SET_SPACE_MEMBER_ACCESS,
      json!({"spaceKey": "$admin", "memberDid": "bob", "access": {"level": "ReadMemberList"}}),
    );
        h.assert_ok(
      "org",
      "alice",
      "$admin",
      NSID::SET_SPACE_MEMBER_ACCESS,
      json!({"spaceKey": "$admin", "memberDid": "bob", "access": {"level": "ReadMemberList"}}),
    );
    }

    // ===================================================================
    // Resolved member lists
    // ===================================================================

    #[test]
    fn test_owner_sees_self_in_admin_space() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        let members = h.resolved_members("org", "alice", "$admin");
        assert!(!members.is_empty());
        assert_member_exists(&members, "alice", "Owner");
    }

    #[test]
    fn test_resolve_includes_all_direct_members() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.assert_ok(
            "org",
            "alice",
            "$admin",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "$admin", "memberDid": "bob", "access": {"level": "IsMember"}}),
        );
        h.assert_ok(
      "org",
      "alice",
      "$admin",
      NSID::SET_SPACE_MEMBER_ACCESS,
      json!({"spaceKey": "$admin", "memberDid": "carol", "access": {"level": "ReadMemberList"}}),
    );
        let members = h.resolved_members("org", "alice", "$admin");
        assert_member_exists(&members, "alice", "Owner");
        assert_member_exists(&members, "bob", "IsMember");
        assert_member_exists(&members, "carol", "ReadMemberList");
    }

    // ===================================================================
    // Local space delegation
    // ===================================================================

    #[test]
    fn test_access_limited_by_parent_delegation() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.assert_ok(
      "org",
      "alice",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok(
            "org",
            "alice",
            "team",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "team", "memberDid": "bob", "access": {"level": "Owner"}}),
        );
        h.assert_ok("org", "alice", "$admin", NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "$admin", "memberDid": "space:team", "access": {"level": "ReadMemberList"}}));
        let members = h.resolved_members("org", "alice", "$admin");
        assert_member_exists(&members, "bob", "ReadMemberList");
    }

    #[test]
    fn test_members_of_child_space_inherit_access() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.assert_ok(
      "org",
      "alice",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok(
            "org",
            "alice",
            "team",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "team", "memberDid": "bob", "access": {"level": "IsMember"}}),
        );
        h.assert_ok(
      "org",
      "alice",
      "$admin",
      NSID::SET_SPACE_MEMBER_ACCESS,
      json!({"spaceKey": "$admin", "memberDid": "space:team", "access": {"level": "IsMember"}}),
    );
        let members = h.resolved_members("org", "alice", "$admin");
        assert_member_exists(&members, "bob", "IsMember");
    }

    #[test]
    fn test_public_members_allows_non_member_access() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.assert_ok(
      "org",
      "alice",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok(
            "org",
            "alice",
            "team",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "team", "memberDid": "bob", "access": {"level": "IsMember"}}),
        );
        h.space_mut("org", "team").config = json!({"publicMembers": true});
        let members = h.resolved_members("org", "stranger", "team");
        assert!(!members.is_empty());
        assert_member_exists(&members, "bob", "IsMember");
    }

    // ===================================================================
    // Remote space resolution
    // ===================================================================

    #[test]
    fn test_remote_space_resolution() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.create_default_arbiter("partner", "carol");

        h.assert_ok(
      "partner",
      "carol",
      "shared",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "shared", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.space_mut("partner", "shared").config = json!({"publicMembers": true});
        h.assert_ok(
            "partner",
            "carol",
            "shared",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "shared", "memberDid": "dave", "access": {"level": "Owner"}}),
        );

        h.assert_ok(
      "org",
      "alice",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok(
      "org",
      "alice",
      "team",
      NSID::SET_SPACE_MEMBER_ACCESS,
      json!({"spaceKey": "team", "memberDid": "partner|shared", "access": {"level": "IsMember"}}),
    );

        let members = h.resolved_members("org", "alice", "team");
        assert_member_exists(&members, "dave", "IsMember");
    }

    #[test]
    fn test_remote_access_limited_by_parent() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.create_default_arbiter("partner", "carol");

        h.assert_ok(
      "partner",
      "carol",
      "shared",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "shared", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.space_mut("partner", "shared").config = json!({"publicMembers": true});
        h.assert_ok(
            "partner",
            "carol",
            "shared",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "shared", "memberDid": "dave", "access": {"level": "Owner"}}),
        );

        h.assert_ok(
      "org",
      "alice",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok("org", "alice", "team", NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "team", "memberDid": "partner|shared", "access": {"level": "ReadMemberList"}}));

        let members = h.resolved_members("org", "alice", "team");
        assert_member_exists(&members, "dave", "ReadMemberList");
    }

    #[test]
    fn test_deep_remote_chain_resolves() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.create_default_arbiter("partner", "carol");

        h.assert_ok(
      "partner",
      "carol",
      "users",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "users", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.space_mut("partner", "users").config = json!({"publicMembers": true});
        h.assert_ok(
            "partner",
            "carol",
            "users",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "users", "memberDid": "dave", "access": {"level": "Owner"}}),
        );

        h.assert_ok(
      "org",
      "alice",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok(
      "org",
      "alice",
      "team",
      NSID::SET_SPACE_MEMBER_ACCESS,
      json!({"spaceKey": "team", "memberDid": "partner|users", "access": {"level": "IsMember"}}),
    );

        let members = h.resolved_members("org", "alice", "team");
        assert_member_exists(&members, "dave", "IsMember");
    }

    #[test]
    fn test_remote_arbiter_denies_unauthorised_caller() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.create_default_arbiter("partner", "carol");

        h.assert_ok("partner", "carol", "restricted", NSID::CREATE_SPACE,
            json!({"spaceKey": "restricted", "spaceType": "town.muni.arbiter.config.space", "config": {}}));
        h.assert_ok(
            "partner",
            "carol",
            "restricted",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "restricted", "memberDid": "dave", "access": {"level": "Owner"}}),
        );

        h.assert_ok(
      "org",
      "alice",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok("org", "alice", "team", NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "team", "memberDid": "partner|restricted", "access": {"level": "IsMember"}}));

        let members = h.resolved_members("org", "alice", "team");
        assert!(
            !members.iter().any(|m| m.did == "dave"),
            "Dave should NOT appear — remote arbiter should deny org"
        );
    }

    #[test]
    fn test_remote_arbiter_grants_caller_via_member_access() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.create_default_arbiter("partner", "carol");

        h.assert_ok(
      "partner",
      "carol",
      "shared",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "shared", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok(
            "partner",
            "carol",
            "shared",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "shared", "memberDid": "dave", "access": {"level": "Owner"}}),
        );
        h.assert_ok(
      "partner",
      "carol",
      "shared",
      NSID::SET_SPACE_MEMBER_ACCESS,
      json!({"spaceKey": "shared", "memberDid": "org", "access": {"level": "ReadMemberList"}}),
    );

        h.assert_ok(
      "org",
      "alice",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok(
      "org",
      "alice",
      "team",
      NSID::SET_SPACE_MEMBER_ACCESS,
      json!({"spaceKey": "team", "memberDid": "partner|shared", "access": {"level": "IsMember"}}),
    );

        let members = h.resolved_members("org", "alice", "team");
        assert_member_exists(&members, "dave", "IsMember");
    }

    // ===================================================================
    // Custom policies
    // ===================================================================

    #[test]
    fn test_allow_all_policy() {
        let allow_all = r#"
            package arbiter
            import rego.v1
            default allow := true
            resolved_members contains {"did": input.caller.did, "access": {"level": "Owner"}} if { true }
            missing_spaces contains false if { false }
            resolve_result := {"members": resolved_members, "missingSpaces": missing_spaces}
        "#;
        let mut h = TestHarness::new();
        h.create_arbiter("org", "alice", allow_all);
        h.assert_ok(
      "org",
      "stranger",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok(
            "org",
            "stranger",
            "team",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "team", "memberDid": "alice", "access": {"level": "Owner"}}),
        );
        let members = h.resolved_members("org", "stranger", "$admin");
        assert_member_exists(&members, "stranger", "Owner");
    }

    #[test]
    fn test_deny_all_policy() {
        let deny_all = r#"
            package arbiter
            import rego.v1
            default allow := false
            resolved_members contains {"did": "noone", "access": {"level": "ReadMemberList"}} if { false }
        "#;
        let mut h = TestHarness::new();
        h.create_arbiter("org", "alice", deny_all);
        h.assert_denied(
      "org",
      "alice",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
    }

    // ===================================================================
    // Access control edge cases
    // ===================================================================

    #[test]
    fn test_remote_arbiter_offline_excludes_remote_members() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.create_default_arbiter("partner", "carol");

        h.assert_ok(
      "partner",
      "carol",
      "shared",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "shared", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.space_mut("partner", "shared").config = json!({"publicMembers": true});
        h.assert_ok(
            "partner",
            "carol",
            "shared",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "shared", "memberDid": "dave", "access": {"level": "Owner"}}),
        );

        h.assert_ok(
      "org",
      "alice",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok("org", "alice", "team", NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "team", "memberDid": "partner|shared", "access": {"level": "ReadMemberList"}}));

        // Online: Dave visible
        let online = h.resolved_members("org", "alice", "team");
        assert_member_exists(&online, "dave", "ReadMemberList");

        // Offline: Dave absent
        h.core.toggle_arbiter_offline("partner");
        let offline = h.resolved_members("org", "alice", "team");
        assert!(
            !offline.iter().any(|m| m.did == "dave"),
            "Dave should be absent when partner is offline"
        );

        // Back online: Dave returns
        h.core.toggle_arbiter_offline("partner");
        let back_online = h.resolved_members("org", "alice", "team");
        assert_member_exists(&back_online, "dave", "ReadMemberList");
    }

    #[test]
    fn test_public_members_toggle_controls_stranger_access() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.assert_ok(
      "org",
      "alice",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok(
            "org",
            "alice",
            "team",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "team", "memberDid": "bob", "access": {"level": "IsMember"}}),
        );

        // Not public: stranger denied
        h.assert_denied(
            "org",
            "stranger",
            "team",
            NSID::RESOLVE_SPACE_MEMBERS,
            json!({"spaceKey": "team"}),
        );

        // Make public: stranger can see
        h.space_mut("org", "team").config = json!({"publicMembers": true});
        let members = h.resolved_members("org", "stranger", "team");
        assert_member_exists(&members, "bob", "IsMember");

        // Un-public: stranger denied again
        h.space_mut("org", "team").config = json!({"publicMembers": false});
        h.assert_denied(
            "org",
            "stranger",
            "team",
            NSID::RESOLVE_SPACE_MEMBERS,
            json!({"spaceKey": "team"}),
        );
    }

    #[test]
    fn test_space_scoped_owner_cannot_create_spaces_globally() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");

        h.assert_ok(
      "org",
      "alice",
      "team",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "team", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok(
            "org",
            "alice",
            "team",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "team", "memberDid": "bob", "access": {"level": "Owner"}}),
        );

        h.assert_ok(
            "org",
            "bob",
            "team",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "team", "memberDid": "carol", "access": {"level": "IsMember"}}),
        );

        h.assert_denied(
      "org",
      "bob",
      "newspace",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "newspace", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );

        h.assert_ok(
      "org",
      "alice",
      "newspace",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "newspace", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
    }

    // ===================================================================
    // UI flow regression tests
    // ===================================================================

    #[test]
    fn test_create_arbiter_with_ui_style_config_then_resolve_members() {
        let mut core = ArbiterCore::new(DEFAULT_POLICY);
        let result = core.create_arbiter(
            "arbiter1".into(),
            json!({"$type": "town.muni.arbiter.config.regoPolicy"}),
            "alice".into(),
        );
        assert!(matches!(result, OpResult::Ok(_)));

        let mut h = TestHarness { core };
        let members = h.resolved_members("arbiter1", "alice", "$admin");
        assert_eq!(members.len(), 1);
        assert_member_exists(&members, "alice", "Owner");
    }

    #[test]
    fn test_add_member_to_admin_space() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.assert_ok("org", "alice", "$admin", NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "$admin", "memberDid": "bob", "access": {"$type": "town.muni.arbiter.config.accessLevel", "level": "IsMember"}}));
        let members = h.resolved_members("org", "alice", "$admin");
        assert_member_exists(&members, "bob", "IsMember");
    }

    #[test]
    fn test_create_space_with_explicit_key() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("org", "alice");
        h.assert_ok("org", "alice", "test", NSID::CREATE_SPACE,
            json!({"spaceKey": "test", "spaceType": "town.muni.arbiter.config.space", "config": {"$type": "town.muni.arbiter.config.space", "publicRecords": false, "publicMembers": false}}));
        let space = h.core.arbiters["org"].spaces.get("test").cloned();
        assert!(space.is_some(), "Space 'test' should exist");
        assert_eq!(space.unwrap().key, "test");
    }

    // ===================================================================
    // Nested local delegation
    // ===================================================================

    #[test]
    fn test_resolves_deeply_nested_local_delegations() {
        let mut h = TestHarness::new();
        h.create_default_arbiter("arb1", "alice");

        h.assert_ok(
      "arb1",
      "alice",
      "members",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "members", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok("arb1", "alice", "moderators", NSID::CREATE_SPACE,
            json!({"spaceKey": "moderators", "spaceType": "town.muni.arbiter.config.space", "config": {}}));
        h.assert_ok(
      "arb1",
      "alice",
      "#general",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "#general", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );

        h.assert_ok("arb1", "alice", "members", NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "members", "memberDid": "space:moderators", "access": {"level": "RemoveMembers"}}));
        h.assert_ok("arb1", "alice", "#general", NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "#general", "memberDid": "space:members", "access": {"level": "RemoveMembers"}}));
        h.assert_ok(
      "arb1",
      "alice",
      "moderators",
      NSID::SET_SPACE_MEMBER_ACCESS,
      json!({"spaceKey": "moderators", "memberDid": "carol", "access": {"level": "RemoveMembers"}}),
    );
        h.assert_ok(
            "arb1",
            "alice",
            "members",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "members", "memberDid": "george", "access": {"level": "IsMember"}}),
        );

        let members = h.resolved_members("arb1", "alice", "#general");
        assert_member_exists(&members, "alice", "Owner");
        assert_member_exists(&members, "george", "IsMember");
        assert_member_exists(&members, "carol", "RemoveMembers");
    }

    // ===================================================================
    // Cross-arbiter remote delegation
    // ===================================================================

    #[test]
    fn test_resolves_members_across_arbiter_boundaries() {
        let mut h = TestHarness::new();

        h.create_default_arbiter("muni-town", "alice");
        h.assert_ok(
      "muni-town",
      "alice",
      "members",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "members", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok("muni-town", "alice", "moderators", NSID::CREATE_SPACE,
            json!({"spaceKey": "moderators", "spaceType": "town.muni.arbiter.config.space", "config": {}}));

        h.space_mut("muni-town", "members").config = json!({"publicMembers": true});

        h.assert_ok("muni-town", "alice", "members", NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "members", "memberDid": "space:moderators", "access": {"level": "RemoveMembers"}}));
        h.assert_ok(
            "muni-town",
            "alice",
            "members",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "members", "memberDid": "george", "access": {"level": "IsMember"}}),
        );
        h.assert_ok(
      "muni-town",
      "alice",
      "moderators",
      NSID::SET_SPACE_MEMBER_ACCESS,
      json!({"spaceKey": "moderators", "memberDid": "carol", "access": {"level": "RemoveMembers"}}),
    );

        h.create_default_arbiter("spicy-lobster", "bob");
        h.assert_ok(
      "spicy-lobster",
      "bob",
      "members",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "members", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );
        h.assert_ok(
      "spicy-lobster",
      "bob",
      "#general",
      NSID::CREATE_SPACE,
      json!({"spaceKey": "#general", "spaceType": "town.muni.arbiter.config.space", "config": {}}),
    );

        h.assert_ok(
            "spicy-lobster",
            "bob",
            "members",
            NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "members", "memberDid": "mary", "access": {"level": "IsMember"}}),
        );
        h.assert_ok("spicy-lobster", "bob", "members", NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "members", "memberDid": "muni-town|members", "access": {"level": "IsMember"}}));
        h.assert_ok("spicy-lobster", "bob", "#general", NSID::SET_SPACE_MEMBER_ACCESS,
            json!({"spaceKey": "#general", "memberDid": "space:members", "access": {"level": "RemoveMembers"}}));

        let members = h.resolved_members("spicy-lobster", "bob", "#general");
        assert_member_exists(&members, "bob", "Owner");
        assert_member_exists(&members, "mary", "IsMember");
        assert_member_exists(&members, "alice", "IsMember");
        assert_member_exists(&members, "george", "IsMember");
        assert_member_exists(&members, "carol", "IsMember");
    }
}
