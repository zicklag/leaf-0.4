//! Policy evaluation — suspension loop, data building, and NSID routing.
//!
//! [`evaluate`] runs a Rego entry point through its suspension loop, resolving
//! local queries against the arbiter collection and remote queries via HTTP.
//! No state machine, no pending ops — just a loop.

use std::collections::BTreeMap;

use policy_core::{HostRequest, VmResult, VmSession};
use regorus::Value as RegoValue;
use serde_json::Value;
use tracing;

use crate::state::ArbiterCollection;

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
    pub const CREATE_DID: &'static str = "town.muni.arbiter.createDid";
    pub const UPDATE_DID_DOC: &'static str = "town.muni.arbiter.updateDidDoc";
}

pub fn is_native_nsid(nsid: &str) -> bool {
    matches!(nsid,
        NSID::GET_ARBITER_CONFIG
        | NSID::GET_SPACE_CONFIG
        | NSID::GET_SPACE_MEMBERS
        | NSID::RESOLVE_SPACE_MEMBERS
        | NSID::LIST_SPACES
    )
}

// ---------------------------------------------------------------------------
// Rego ↔ JSON conversion
// ---------------------------------------------------------------------------

fn json_to_rego(val: &Value) -> RegoValue {
    match val {
        Value::Null => RegoValue::Null,
        Value::Bool(b) => RegoValue::from(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() { RegoValue::from(i) }
            else if let Some(f) = n.as_f64() { RegoValue::from(f) }
            else { RegoValue::from(n.to_string()) }
        }
        Value::String(s) => RegoValue::from(s.as_str()),
        Value::Array(arr) => RegoValue::from(arr.iter().map(json_to_rego).collect::<Vec<_>>()),
        Value::Object(obj) => {
            let mut map = BTreeMap::new();
            for (k, v) in obj { map.insert(RegoValue::from(k.as_str()), json_to_rego(v)); }
            RegoValue::from(map)
        }
    }
}

fn rego_to_json(val: &RegoValue) -> Value {
    match val {
        RegoValue::Null => Value::Null,
        RegoValue::Bool(b) => Value::Bool(*b),
        RegoValue::Number(n) => {
            let s = n.format_decimal();
            if let Ok(i) = s.parse::<i64>() { Value::Number(i.into()) }
            else if let Ok(f) = s.parse::<f64>() {
                serde_json::Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null)
            }
            else { Value::String(s) }
        }
        RegoValue::String(s) => Value::String(s.to_string()),
        RegoValue::Array(arr) => Value::Array(arr.iter().map(rego_to_json).collect()),
        RegoValue::Object(obj) => {
            let mut map = serde_json::Map::new();
            for (k, v) in obj.iter() {
                let key = k.as_string().map(|s| s.to_string()).unwrap_or_else(|_| format!("{k:?}"));
                map.insert(key, rego_to_json(v));
            }
            Value::Object(map)
        }
        RegoValue::Set(s) => Value::Array(s.iter().map(rego_to_json).collect()),
        RegoValue::Undefined => Value::Null,
    }
}

// ---------------------------------------------------------------------------
// Data builders
// ---------------------------------------------------------------------------

pub fn build_data(arbiter_config: &Value) -> Value {
    serde_json::json!({ "arbiter": { "config": arbiter_config } })
}

pub fn build_input(caller_did: &str, nsid: &str, params: &Value) -> Value {
    serde_json::json!({
        "caller": { "did": caller_did },
        "operation": { "nsid": nsid, "params": params },
    })
}

// ---------------------------------------------------------------------------
// Local query resolution
// ---------------------------------------------------------------------------

fn resolve_local(
    arbiter_did: &str,
    nsid: &str,
    params: &Value,
    collection: &ArbiterCollection,
) -> Value {
    let space_key = params.get("spaceKey").and_then(|v| v.as_str()).unwrap_or("");
    match nsid {
        NSID::GET_SPACE_MEMBERS => {
            let members = collection.space_members(arbiter_did, space_key);
            serde_json::json!({ "members": members })
        }
        NSID::GET_SPACE_CONFIG => {
            let config = collection.space_config(arbiter_did, space_key);
            serde_json::json!({ "config": config })
        }
        NSID::GET_ARBITER_CONFIG => {
            serde_json::json!({ "config": collection.get(arbiter_did).map(|a| &a.config) })
        }
        NSID::LIST_SPACES => {
            let spaces: Vec<Value> = collection.get(arbiter_did)
                .map(|a| a.spaces.values().map(|s| serde_json::json!({
                    "key": s.key, "spaceType": s.space_type,
                })).collect())
                .unwrap_or_default();
            serde_json::json!({ "spaces": spaces })
        }
        _ => {
            tracing::warn!(%nsid, "Unhandled xrpc_local in policy suspension");
            serde_json::json!({})
        }
    }
}

// ---------------------------------------------------------------------------
// Main evaluation loop
// ---------------------------------------------------------------------------

/// Run a policy entry point to completion, resolving suspensions inline.
///
/// `collection` is borrowed and may be read during local query resolution.
/// The caller must hold the collection lock for the duration.
pub async fn evaluate(
    policy_source: &str,
    arbiter_config: &Value,
    caller_did: &str,
    nsid: &str,
    params: &Value,
    entry_points: &[&str],
    arbiter_did: &str,
    collection: &ArbiterCollection,
    http_client: &reqwest::Client,
) -> Result<Value, String> {
    let data = json_to_rego(&build_data(arbiter_config));
    let input = json_to_rego(&build_input(caller_did, nsid, params));

    let mut session = VmSession::new(policy_source, &data, &input, entry_points)
        .map_err(|e| format!("ErrPolicyCompile: {e}"))?;

    let mut result = session.start()
        .map_err(|e| format!("ErrPolicyEval: {e}"))?;

    loop {
        match result {
            VmResult::Completed(val) => return Ok(rego_to_json(&val)),
            VmResult::Suspended(req) => {
                let resolved = match &req {
                    HostRequest::XrpcLocal { path, input } => {
                        let params = rego_to_json(input);
                        if is_native_nsid(path) {
                            resolve_local(arbiter_did, path, &params, collection)
                        } else {
                            tracing::warn!(%path, "Foreign xrpc_local in policy eval");
                            serde_json::json!({})
                        }
                    }
                    HostRequest::XrpcRemote { did, path, input } => {
                        resolve_remote(http_client, did, path, rego_to_json(input)).await
                    }
                };
                result = session.resume(&json_to_rego(&resolved))
                    .map_err(|e| format!("ErrPolicyResume: {e}"))?;
            }
        }
    }
}

/// Fetch data from a remote arbiter.
async fn resolve_remote(client: &reqwest::Client, remote_did: &str, path: &str, input: Value) -> Value {
    let host = remote_did.strip_prefix("did:web:").unwrap_or(remote_did).replace("%3A", ":");
    let space_key = input.get("spaceKey").and_then(|v| v.as_str()).unwrap_or("");
    let url = format!("https://{host}/xrpc/{path}?arbiterDid={remote_did}&spaceKey={space_key}");

    match client.get(&url).send().await {
        Ok(resp) => resp.json::<Value>().await.unwrap_or(serde_json::json!({ "members": [] })),
        Err(e) => {
            tracing::warn!(%remote_did, %e, "Remote query failed");
            serde_json::json!({ "members": [] })
        }
    }
}
