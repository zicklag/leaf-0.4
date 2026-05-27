//! Sans-IO arbiter state machine.
//!
//! The state machine uses [`asansio`] to express its logic as async
//! functions while remaining completely IO-free.  Every interaction with
//! the outside world flows through [`Req`] / [`Res`] — the IO surface.
//!
//! The IO layer (e.g. the `arbiter-server` crate) is a dumb harness:
//! it forwards incoming XRPC calls, fulfills [`Req`]s, and returns
//! [`Res`]ponses.
//!
//! Policy evaluation using [`policy_core::VmSession`] lives inside the
//! state machine (it is itself sans-IO).  `xrpc_local` calls are resolved
//! from local state; `xrpc_remote` calls surface as [`Req::XrpcRemote`].

#![deny(rust_2018_idioms)]

use std::collections::BTreeMap;
use std::collections::HashMap;

use policy_core::{HostRequest, VmResult, VmSession};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use asansio;
pub use policy_core;

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

pub type Did = String;
pub type SpaceKey = String;
pub type JobId = u64;

/// A request from the state machine to the IO layer.
///
/// Every variant tags a `job_id` so responses can be correlated even
/// if the IO layer reorders them.
#[derive(Debug)]
pub enum Req<'a> {
    // ── Ingress ──────────────────────────────────────────────────────
    /// Poll for the next incoming XRPC call.
    NextXrpcCall,

    /// Send an XRPC response back to the client.
    SendResponse { body: &'a Value, status: u16, job_id: JobId },

    // ── Auth / identity ──────────────────────────────────────────────
    /// Resolve a JWT bearer token (or dev token) to a caller DID.
    ResolveAuth { token: &'a str, job_id: JobId },

    /// Resolve a DID document.
    ResolveDid { did: &'a str, job_id: JobId },

    // ── Policy engine ────────────────────────────────────────────────
    /// The policy engine needs data from a remote service (`xrpc_remote`).
    XrpcRemote { did: &'a str, path: &'a str, input: &'a Value, job_id: JobId },

    // ── PLC directory ────────────────────────────────────────────────
    /// Submit a signed operation.
    PlcSubmitOperation { did: &'a str, operation: &'a Value, job_id: JobId },

    /// Fetch the current PLC state for a DID.
    PlcFetchState { did: &'a str, job_id: JobId },

    /// Fetch PLC state plus the latest operation CID.
    PlcFetchStateWithCid { did: &'a str, job_id: JobId },

    /// Generate new DID keys (rotation + signing).
    GenerateDidKeys { job_id: JobId },

    // ── Persistence ──────────────────────────────────────────────────
    /// Persist a JSON snapshot of the entire collection.
    StoreSnapshot { snapshot_json: &'a Value, job_id: JobId },

    /// Load the previously persisted snapshot.
    LoadSnapshot { job_id: JobId },

    // ── Backend proxy ────────────────────────────────────────────────
    /// Proxy an XRPC call to a backend service.
    ProxyXrpc { backend_url: &'a str, path: &'a str, params: &'a Value, job_id: JobId },
}

/// A response from the IO layer back to the state machine.
///
/// Every variant carries the `job_id` from the corresponding [`Req`]
/// so the state machine can correlate responses.
#[derive(Debug)]
pub enum Res<'a> {
    // ── Ingress / egress ──────────────────────────────────────────────
    /// The next incoming XRPC call.
    IncomingXrpc { method: &'a str, params: &'a Value, token: Option<&'a str> },
    /// Acknowledgement (response sent, store completed, etc.).
    Ack { job_id: JobId },
    /// The IO layer is shutting down.
    Shutdown,

    // ── Auth / identity ──────────────────────────────────────────────
    AuthResolved { caller_did: &'a str, job_id: JobId },
    AuthFailed { job_id: JobId },
    DidDocument { doc: &'a Value, job_id: JobId },

    // ── Policy engine ────────────────────────────────────────────────
    XrpcRemoteResult { data: &'a Value, job_id: JobId },

    // ── PLC directory ────────────────────────────────────────────────
    PlcOperationSubmitted { job_id: JobId },
    PlcState { state: &'a Value, job_id: JobId },
    PlcStateWithCid { state: &'a Value, cid: &'a str, job_id: JobId },
    DidKeys { rotation_key: &'a [u8], signing_key: &'a [u8], job_id: JobId },

    // ── Persistence ──────────────────────────────────────────────────
    LoadedSnapshot { snapshot: Option<&'a Value>, job_id: JobId },

    // ── Backend proxy ────────────────────────────────────────────────
    ProxyResult { data: &'a Value, job_id: JobId },

    // ── Errors ───────────────────────────────────────────────────────
    Error { message: &'a str, job_id: JobId },
}

// ---------------------------------------------------------------------------
// Public type aliases
// ---------------------------------------------------------------------------

pub type ArbiterSans = asansio::Sans<Req<'static>, Res<'static>>;
pub type ArbiterIo = asansio::Io<Req<'static>, Res<'static>>;

pub fn channel() -> (ArbiterSans, ArbiterIo) {
    asansio::new()
}

// ---------------------------------------------------------------------------
// JSON-serialisable types for the snapshot
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
// Policy helpers  (pure — VmSession is itself sans-IO)
// ---------------------------------------------------------------------------

/// Convert a `serde_json::Value` to a `regorus::Value` for policy evaluation.
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

/// Convert a `regorus::Value` back to a `serde_json::Value`.
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
        regorus::Value::Array(arr) => {
            Value::Array(arr.iter().map(rego_to_serde).collect())
        }
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
        regorus::Value::Set(s) => {
            Value::Array(s.iter().map(rego_to_serde).collect())
        }
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
                .map(|a| a.spaces.get(space_key))
                .flatten()
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
                .map(|a| a.spaces.get(space_key))
                .flatten()
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
// The sans-IO state machine
// ---------------------------------------------------------------------------

/// Evaluate a policy entry-point through its suspension loop.
///
/// `xrpc_local` queries are resolved inline from `collection`.
/// `xrpc_remote` queries yield a [`Req::XrpcRemote`] to the IO layer.
///
/// Returns the new handle and the evaluation result.
async fn eval_policy<'a>(
    sans: &asansio::Sans<Req<'a>, Res<'a>>,
    handle: asansio::SansHandle<Res<'a>>,
    collection: &ArbiterCollection,
    arbiter_did: &str,
    policy: &str,
    arbiter_config: &Value,
    caller_did: &str,
    nsid: &str,
    params: &Value,
    entry_points: &[&str],
    next_job_id: &mut JobId,
) -> (asansio::SansHandle<Res<'a>>, Result<Value, String>) {
    let data = serde_json::json!({ "arbiter": { "config": arbiter_config } });
    let input = serde_json::json!({
        "caller": { "did": caller_did },
        "operation": { "nsid": nsid, "params": params },
    });

    let rego_data = serde_to_rego(&data);
    let rego_input = serde_to_rego(&input);

    let mut session = match VmSession::new(policy, &rego_data, &rego_input, entry_points) {
        Ok(s) => s,
        Err(e) => return (handle, Err(format!("ErrPolicyCompile: {e}"))),
    };

    let mut vm_result = match session.start() {
        Ok(r) => r,
        Err(e) => return (handle, Err(format!("ErrPolicyEval: {e}"))),
    };

    let mut handle = handle;

    loop {
        match vm_result {
            VmResult::Completed(val) => {
                return (handle, Ok(rego_to_serde(&val)));
            }
            VmResult::Suspended(req) => match req {
                HostRequest::XrpcLocal { path, input } => {
                    let params_serde = rego_to_serde(&input);
                    let resolved = resolve_local(arbiter_did, &path, &params_serde, collection);
                    let resolved_rego = serde_to_rego(&resolved);
                    vm_result = match session.resume(&resolved_rego) {
                        Ok(r) => r,
                        Err(e) => return (handle, Err(format!("ErrPolicyResume: {e}"))),
                    };
                }
                HostRequest::XrpcRemote { did, path, input } => {
                    let job_id = *next_job_id;
                    *next_job_id += 1;

                    let owned_did = did.to_string();
                    let owned_path = path.to_string();
                    let owned_input = rego_to_serde(&input);

                    let req = Req::XrpcRemote {
                        did: &owned_did,
                        path: &owned_path,
                        input: &owned_input,
                        job_id,
                    };

                    handle = sans.handle(handle, &req).await;

                    // Extract response before using handle again.
                    let xrpc_data = handle.message().and_then(|m| match m {
                        Res::XrpcRemoteResult { data, .. } => Some((*data).clone()),
                        _ => None,
                    });
                    let xrpc_err = handle.message().and_then(|m| match m {
                        Res::Error { message, .. } => Some(message.to_string()),
                        _ => None,
                    });

                    let resolved_serde = match (xrpc_data, xrpc_err) {
                        (Some(data), _) => data,
                        (_, Some(msg)) => return (handle, Err(format!("ErrRemoteXrpc: {msg}"))),
                        _ => Value::Null,
                    };
                    let resolved_rego = serde_to_rego(&resolved_serde);

                    vm_result = match session.resume(&resolved_rego) {
                        Ok(r) => r,
                        Err(e) => return (handle, Err(format!("ErrPolicyResume: {e}"))),
                    };
                }
            },
        }
    }
}

/// Run the arbiter state machine to completion.
///
/// This is a pure async function — it never performs IO directly.
/// Instead it yields [`Req`] values via the [`asansio`] channel and
/// awaits [`Res`]ponses.  The IO layer fulfills those requests.
///
/// # Arguments
///
/// * `sans` — the sans-IO half, received from [`channel`].
/// * `default_policy` — the Rego policy source used when no policy is
///   explicitly configured for an arbiter.
/// * `server_did` — the DID of this server (e.g. `did:web:example.com`).
pub async fn run<'a>(
    sans: asansio::Sans<Req<'a>, Res<'a>>,
    default_policy: &'a str,
    _server_did: &'a str,
) {
    let mut collection = ArbiterCollection::new();
    let mut next_job_id: JobId = 1;

    // ── Phase 1: Load persisted state ────────────────────────────────
    {
        let req = Req::LoadSnapshot { job_id: next_job_id };
        next_job_id += 1;
        let handle = sans.start(&req).await;
        if let Some(Res::LoadedSnapshot { snapshot: Some(snap), .. }) = handle.message() {
            if let Ok(s) = serde_json::from_value::<ServerSnapshot>((*snap).clone()) {
                collection.load_snapshot(s);
            }
        }
    }

    // ── Helper: send a single Req, thread the handle ────────────────
    macro_rules! send {
        ($handle:expr, $req:expr) => {
            sans.handle($handle, $req).await
        };
    }

    // ── Phase 2: Main event loop ─────────────────────────────────────
    let mut handle = sans.start(&Req::NextXrpcCall).await;

    loop {
        // ── Await the next XRPC call (extract owned data to drop borrow) ─
        let (method_str, params_owned, token_owned) = match handle.message() {
            Some(Res::IncomingXrpc { method, params, token }) => {
                (method.to_string(), (*params).clone(), token.map(|s| s.to_string()))
            }
            Some(Res::Shutdown) | None => break,
            _ => {
                handle = send!(handle, &Req::NextXrpcCall);
                continue;
            }
        };

        // ── Resolve auth ──────────────────────────────────────────────
        let caller_did: String = if let Some(ref tok) = token_owned {
            let job_id = next_job_id;
            next_job_id += 1;
            handle = send!(handle, &Req::ResolveAuth { token: tok, job_id });
            match handle.message() {
                Some(Res::AuthResolved { caller_did, .. }) => caller_did.to_string(),
                _ => {
                    let body = serde_json::json!({"error": "ErrAuthRequired"});
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::SendResponse { body: &body, status: 401, job_id: j });
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::NextXrpcCall);
                    continue;
                }
            }
        } else {
            String::new()
        };

        // ── Parse arbiter DID ─────────────────────────────────────────
        let arbiter_did = params_owned
            .get("arbiterDid")
            .or_else(|| params_owned.get("did"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let Some(ref arbiter_did) = arbiter_did else {
            let body = serde_json::json!({"error": "missing arbiterDid"});
            let j = next_job_id;
            next_job_id += 1;
            handle = send!(handle, &Req::SendResponse { body: &body, status: 400, job_id: j });
            let j = next_job_id;
            next_job_id += 1;
            handle = send!(handle, &Req::NextXrpcCall);
            continue;
        };

        // ── Route the XRPC method ────────────────────────────────────
        // Extract into owned for matching
        let method: &str = &method_str;

        if method == NSID::CREATE_ARBITER {
            // Bootstrap — no policy check needed.
            if collection.arbiters.contains_key(arbiter_did) {
                let body = serde_json::json!({"error": "ErrArbiterAlreadyExists"});
                let j = next_job_id;
                next_job_id += 1;
                handle = send!(handle, &Req::SendResponse { body: &body, status: 409, job_id: j });
            } else {
                let config = params_owned.get("config").cloned().unwrap_or_default();
                let policy = params_owned
                    .get("config")
                    .and_then(|c| c.get("policy"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(default_policy)
                    .to_string();

                collection.create_arbiter(arbiter_did.clone(), config, policy, caller_did.clone());

                // Persist and respond
                let snap = collection.snapshot();
                let snap_val = serde_json::to_value(&snap).unwrap_or_default();
                let j = next_job_id;
                next_job_id += 1;
                handle = send!(handle, &Req::StoreSnapshot { snapshot_json: &snap_val, job_id: j });

                let body = serde_json::json!({});
                let j = next_job_id;
                next_job_id += 1;
                handle = send!(handle, &Req::SendResponse { body: &body, status: 200, job_id: j });
            }

        } else if is_readonly_nsid(method) {
            // ── Read-only queries ──────────────────────────────────
            let arbiter = match collection.get(arbiter_did) {
                Some(a) => a,
                None => {
                    let body = serde_json::json!({"error": "ErrArbiterNotExists"});
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::SendResponse { body: &body, status: 404, job_id: j });
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::NextXrpcCall);
                    continue;
                }
            };

            let (new_handle, policy_check) = eval_policy(
                &sans, handle, &collection,
                arbiter_did, &arbiter.policy, &arbiter.config,
                &caller_did, method, &params_owned,
                &["data.arbiter.allow"],
                &mut next_job_id,
            ).await;
            handle = new_handle;

            let allowed = match policy_check {
                Ok(val) => val == Value::Bool(true),
                Err(e) => {
                    let body = serde_json::json!({"error": e});
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::SendResponse { body: &body, status: 403, job_id: j });
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::NextXrpcCall);
                    continue;
                }
            };

            if !allowed {
                let body = serde_json::json!({"error": "ErrPermissionDenied"});
                let j = next_job_id;
                next_job_id += 1;
                handle = send!(handle, &Req::SendResponse { body: &body, status: 403, job_id: j });
                let j = next_job_id;
                next_job_id += 1;
                handle = send!(handle, &Req::NextXrpcCall);
                continue;
            }

            let response_body = if method == NSID::GET_ARBITER_CONFIG {
                serde_json::json!({ "config": &arbiter.config })
            } else if method == NSID::GET_SPACE_CONFIG {
                let sk = params_owned.get("spaceKey").and_then(|v| v.as_str()).unwrap_or("");
                let config = arbiter.spaces.get(sk).map(|s| &s.config);
                serde_json::json!({ "config": config, "spaceType": "" })
            } else if method == NSID::GET_SPACE_MEMBERS {
                let sk = params_owned.get("spaceKey").and_then(|v| v.as_str()).unwrap_or("");
                let members: Vec<Value> = arbiter.spaces.get(sk)
                    .map(|s| s.members.iter().map(|m| serde_json::json!({
                        "member": { "did": m.did }, "access": m.access,
                    })).collect())
                    .unwrap_or_default();
                serde_json::json!({ "members": members })
            } else {
                // LIST_SPACES
                let spaces: Vec<Value> = arbiter.spaces.values().map(|s| serde_json::json!({
                    "spaceKey": s.key, "spaceType": s.space_type, "config": s.config,
                })).collect();
                serde_json::json!({ "spaces": spaces })
            };

            let j = next_job_id;
            next_job_id += 1;
            handle = send!(handle, &Req::SendResponse { body: &response_body, status: 200, job_id: j });

        } else if method == NSID::RESOLVE_SPACE_MEMBERS {
            // ── Resolve space members (two entry points) ──────────────
            let arbiter = match collection.get(arbiter_did) {
                Some(a) => a,
                None => {
                    let body = serde_json::json!({"error": "ErrArbiterNotExists"});
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::SendResponse { body: &body, status: 404, job_id: j });
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::NextXrpcCall);
                    continue;
                }
            };

            // Check allow
            let (new_handle, policy_check) = eval_policy(
                &sans, handle, &collection,
                arbiter_did, &arbiter.policy, &arbiter.config,
                &caller_did, method, &params_owned,
                &["data.arbiter.allow"],
                &mut next_job_id,
            ).await;
            handle = new_handle;

            let allowed = match policy_check {
                Ok(val) => val == Value::Bool(true),
                Err(e) => {
                    let body = serde_json::json!({"error": e});
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::SendResponse { body: &body, status: 403, job_id: j });
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::NextXrpcCall);
                    continue;
                }
            };

            if !allowed {
                let body = serde_json::json!({"error": "ErrPermissionDenied"});
                let j = next_job_id;
                next_job_id += 1;
                handle = send!(handle, &Req::SendResponse { body: &body, status: 403, job_id: j });
                let j = next_job_id;
                next_job_id += 1;
                handle = send!(handle, &Req::NextXrpcCall);
                continue;
            }

            // Evaluate resolve_result entry point
            let (new_handle, eval_result) = eval_policy(
                &sans, handle, &collection,
                arbiter_did, &arbiter.policy, &arbiter.config,
                &caller_did, method, &params_owned,
                &["data.arbiter.resolve_result"],
                &mut next_job_id,
            ).await;
            handle = new_handle;

            let result = eval_result.unwrap_or(serde_json::json!({}));
            let members = result.get("members").cloned().unwrap_or(Value::Array(vec![]));
            let missing = result.get("missingSpaces").cloned().unwrap_or(Value::Array(vec![]));
            let response_body = serde_json::json!({ "members": members, "missingSpaces": missing });

            let j = next_job_id;
            next_job_id += 1;
            handle = send!(handle, &Req::SendResponse { body: &response_body, status: 200, job_id: j });

        } else if method == NSID::SET_ARBITER_CONFIG
            || method == NSID::CREATE_SPACE
            || method == NSID::SET_SPACE_CONFIG
            || method == NSID::DELETE_SPACE
            || method == NSID::SET_SPACE_MEMBER_ACCESS
            || method == NSID::REMOVE_SPACE_MEMBER
            || method == NSID::DELETE_ARBITER
        {
            // ── Procedure endpoints (mutate state) ──────────────────
            let arbiter = match collection.get(arbiter_did) {
                Some(a) => a,
                None => {
                    let body = serde_json::json!({"error": "ErrArbiterNotExists"});
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::SendResponse { body: &body, status: 404, job_id: j });
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::NextXrpcCall);
                    continue;
                }
            };

            let (new_handle, policy_check) = eval_policy(
                &sans, handle, &collection,
                arbiter_did, &arbiter.policy, &arbiter.config,
                &caller_did, method, &params_owned,
                &["data.arbiter.allow"],
                &mut next_job_id,
            ).await;
            handle = new_handle;

            let allowed = match policy_check {
                Ok(val) => val == Value::Bool(true),
                Err(e) => {
                    let body = serde_json::json!({"error": e});
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::SendResponse { body: &body, status: 403, job_id: j });
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::NextXrpcCall);
                    continue;
                }
            };

            if !allowed {
                let body = serde_json::json!({"error": "ErrPermissionDenied"});
                let j = next_job_id;
                next_job_id += 1;
                handle = send!(handle, &Req::SendResponse { body: &body, status: 403, job_id: j });
                let j = next_job_id;
                next_job_id += 1;
                handle = send!(handle, &Req::NextXrpcCall);
                continue;
            }

            // ── Perform the mutation ────────────────────────────────
            if method == NSID::SET_ARBITER_CONFIG {
                if let Some(new_config) = params_owned.get("config") {
                    if let Some(arb) = collection.get_mut(arbiter_did) {
                        arb.config = new_config.clone();
                        arb.version += 1;
                    }
                }
            } else if method == NSID::CREATE_SPACE {
                if let Some(sk) = params_owned.get("spaceKey").and_then(|v| v.as_str()).map(String::from) {
                    if collection.get(arbiter_did)
                        .map(|a| a.spaces.contains_key(&sk))
                        .unwrap_or(false)
                    {
                        let body = serde_json::json!({"error": "ErrSpaceExists"});
                        let j = next_job_id;
                        next_job_id += 1;
                        handle = send!(handle, &Req::SendResponse { body: &body, status: 409, job_id: j });
                        let j = next_job_id;
                        next_job_id += 1;
                        handle = send!(handle, &Req::NextXrpcCall);
                        continue;
                    }
                    if let Some(arb) = collection.get_mut(arbiter_did) {
                        let space_type = params_owned
                            .get("spaceType")
                            .and_then(|v| v.as_str())
                            .unwrap_or("town.muni.arbiter.config.space")
                            .to_string();
                        let config = params_owned.get("config").cloned().unwrap_or_default();
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
                let sk = params_owned.get("spaceKey").and_then(|v| v.as_str()).map(String::from);
                if let Some(sk) = sk {
                    if let Some(arb) = collection.get_mut(arbiter_did) {
                        if let Some(space) = arb.spaces.get_mut(&sk) {
                            if let Some(space_type) = params_owned.get("spaceType").and_then(|v| v.as_str()) {
                                space.space_type = space_type.to_string();
                            }
                            if let Some(config) = params_owned.get("config") {
                                space.config = config.clone();
                            }
                            arb.version += 1;
                        } else {
                            let body = serde_json::json!({"error": "ErrSpaceNotExists"});
                            let j = next_job_id;
                            next_job_id += 1;
                            handle = send!(handle, &Req::SendResponse { body: &body, status: 404, job_id: j });
                            let j = next_job_id;
                            next_job_id += 1;
                            handle = send!(handle, &Req::NextXrpcCall);
                            continue;
                        }
                    }
                }
            } else if method == NSID::DELETE_SPACE {
                if let Some(sk) = params_owned.get("spaceKey").and_then(|v| v.as_str()).map(String::from) {
                    if sk == "$admin" {
                        let body = serde_json::json!({"error": "ErrCannotDeleteAdminSpace"});
                        let j = next_job_id;
                        next_job_id += 1;
                        handle = send!(handle, &Req::SendResponse { body: &body, status: 403, job_id: j });
                        let j = next_job_id;
                        next_job_id += 1;
                        handle = send!(handle, &Req::NextXrpcCall);
                        continue;
                    }
                    if let Some(arb) = collection.get_mut(arbiter_did) {
                        arb.spaces.remove(&sk);
                        arb.version += 1;
                    }
                }
            } else if method == NSID::SET_SPACE_MEMBER_ACCESS {
                let sk = params_owned.get("spaceKey").and_then(|v| v.as_str()).map(String::from);
                let md = params_owned
                    .get("memberDid")
                    .or_else(|| params_owned.get("member").and_then(|m| m.get("did")))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                if let (Some(sk), Some(md)) = (sk, md) {
                    if let Some(arb) = collection.get_mut(arbiter_did) {
                        if let Some(space) = arb.spaces.get_mut(&sk) {
                            let access = params_owned.get("access").cloned().unwrap_or_default();
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
                let sk = params_owned.get("spaceKey").and_then(|v| v.as_str()).map(String::from);
                let md = params_owned
                    .get("memberDid")
                    .or_else(|| params_owned.get("member").and_then(|m| m.get("did")))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                if let (Some(sk), Some(md)) = (sk, md) {
                    if let Some(arb) = collection.get_mut(arbiter_did) {
                        if let Some(space) = arb.spaces.get_mut(&sk) {
                            space.members.retain(|m| m.did != md);
                        }
                        arb.version += 1;
                    }
                }
            } else if method == NSID::DELETE_ARBITER {
                collection.arbiters.remove(arbiter_did);
            }

            // Persist and respond
            let snap = collection.snapshot();
            let snap_val = serde_json::to_value(&snap).unwrap_or_default();
            let j = next_job_id;
            next_job_id += 1;
            handle = send!(handle, &Req::StoreSnapshot { snapshot_json: &snap_val, job_id: j });

            let body = serde_json::json!({});
            let j = next_job_id;
            next_job_id += 1;
            handle = send!(handle, &Req::SendResponse { body: &body, status: 200, job_id: j });

        } else {
            // ── Proxy / fallthrough ─────────────────────────────────
            let arbiter = match collection.get(arbiter_did) {
                Some(a) => a,
                None => {
                    let body = serde_json::json!({"error": "ErrArbiterNotExists"});
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::SendResponse { body: &body, status: 404, job_id: j });
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::NextXrpcCall);
                    continue;
                }
            };

            let (new_handle, policy_check) = eval_policy(
                &sans, handle, &collection,
                arbiter_did, &arbiter.policy, &arbiter.config,
                &caller_did, method, &params_owned,
                &["data.arbiter.allow"],
                &mut next_job_id,
            ).await;
            handle = new_handle;

            let allowed = match policy_check {
                Ok(val) => val == Value::Bool(true),
                Err(e) => {
                    let body = serde_json::json!({"error": e});
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::SendResponse { body: &body, status: 403, job_id: j });
                    let j = next_job_id;
                    next_job_id += 1;
                    handle = send!(handle, &Req::NextXrpcCall);
                    continue;
                }
            };

            if !allowed {
                let body = serde_json::json!({"error": "ErrPermissionDenied"});
                let j = next_job_id;
                next_job_id += 1;
                handle = send!(handle, &Req::SendResponse { body: &body, status: 403, job_id: j });
                let j = next_job_id;
                next_job_id += 1;
                handle = send!(handle, &Req::NextXrpcCall);
                continue;
            }

            let backend_url = arbiter.config
                .get("backendUrl")
                .and_then(|v| v.as_str())
                .map(String::from);

            if let Some(url) = backend_url {
                let j = next_job_id;
                next_job_id += 1;
                handle = send!(handle, &Req::ProxyXrpc {
                    backend_url: &url,
                    path: method,
                    params: &params_owned,
                    job_id: j,
                });

                // Extract proxy result before using handle again.
                let proxy_body = handle.message().and_then(|m| match m {
                    Res::ProxyResult { data, .. } => Some((*data).clone()),
                    _ => None,
                });
                let proxy_err = handle.message().and_then(|m| match m {
                    Res::Error { message, .. } => Some(message.to_string()),
                    _ => None,
                });

                let (response_body, status_code) = match (proxy_body, proxy_err) {
                    (Some(data), _) => (data, 200),
                    (_, Some(msg)) => (serde_json::json!({"error": msg}), 502),
                    _ => (serde_json::json!({"error": "ErrBackendUnreachable"}), 502),
                };
                let j = next_job_id;
                next_job_id += 1;
                handle = send!(handle, &Req::SendResponse { body: &response_body, status: status_code, job_id: j });
            } else {
                let body = serde_json::json!({"error": "ErrBackendNotConfigured"});
                let j = next_job_id;
                next_job_id += 1;
                handle = send!(handle, &Req::SendResponse { body: &body, status: 502, job_id: j });
            }
        }

        // ── Back to the top for the next call ────────────────────────
        let j = next_job_id;
        next_job_id += 1;
        handle = send!(handle, &Req::NextXrpcCall);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_arbiter_snapshot_roundtrip() {
        let mut coll = ArbiterCollection::new();
        coll.create_arbiter(
            "did:plc:abc".into(),
            serde_json::json!({"foo": "bar"}),
            "package arbiter".into(),
            "did:plc:alice".into(),
        );

        let snap = coll.snapshot();
        assert_eq!(snap.arbiters.len(), 1);
        assert_eq!(snap.arbiters[0].did, "did:plc:abc");

        let mut coll2 = ArbiterCollection::new();
        coll2.load_snapshot(snap);
        assert_eq!(coll2.arbiters.len(), 1);
        let arb = coll2.get("did:plc:abc").unwrap();
        assert_eq!(arb.config.get("foo").and_then(|v| v.as_str()), Some("bar"));
    }

    #[test]
    fn test_is_readonly_nsid() {
        assert!(is_readonly_nsid(NSID::GET_ARBITER_CONFIG));
        assert!(is_readonly_nsid(NSID::GET_SPACE_MEMBERS));
        assert!(!is_readonly_nsid(NSID::SET_ARBITER_CONFIG));
        assert!(!is_readonly_nsid(NSID::CREATE_SPACE));
    }

    #[test]
    fn test_resolve_local_get_arbiter_config() {
        let mut coll = ArbiterCollection::new();
        coll.create_arbiter(
            "did:plc:abc".into(),
            serde_json::json!({"key": "val"}),
            "package p".into(),
            "did:plc:alice".into(),
        );
        let params = serde_json::json!({});
        let result = resolve_local("did:plc:abc", NSID::GET_ARBITER_CONFIG, &params, &coll);
        assert_eq!(result.get("config").and_then(|c| c.get("key")).and_then(|v| v.as_str()), Some("val"));
    }

    #[test]
    fn test_serde_rego_roundtrip() {
        let original = serde_json::json!({
            "string": "hello",
            "number": 42,
            "bool": true,
            "nested": { "a": [1, 2, 3] },
            "null": null,
        });
        let rego = serde_to_rego(&original);
        let back = rego_to_serde(&rego);
        assert_eq!(original, back);
    }
}
