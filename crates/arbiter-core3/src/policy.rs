//! Policy evaluation helpers — NSID constants, data/input builders.
//!
//! These utilities construct the `data` and `input` JSON documents that
//! the Rego policy expects. The format matches what
//! `arbiter-simulator/src/lib/simulator.ts` produces.

use std::collections::BTreeMap;

use serde_json::{json, Value as JsonValue};

use regorus::Value as RegoValue;

use crate::core::ArbiterState;

// ---------------------------------------------------------------------------
// Re-export policy-core types that the core module uses
// ---------------------------------------------------------------------------

pub use policy_core::{Error as PolicyError, HostRequest, VmResult, VmSession};

// ---------------------------------------------------------------------------
// serde_json::Value ↔ regorus::Value conversion
// ---------------------------------------------------------------------------

/// Convert a `serde_json::Value` to a `regorus::Value`.
///
/// Needed because `policy-core`'s `VmSession` API uses `regorus::Value`.
pub fn json_to_regorus(val: &JsonValue) -> RegoValue {
    match val {
        JsonValue::Null => RegoValue::Null,
        JsonValue::Bool(b) => RegoValue::from(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                RegoValue::from(i)
            } else if let Some(f) = n.as_f64() {
                RegoValue::from(f)
            } else {
                RegoValue::from(n.to_string())
            }
        }
        JsonValue::String(s) => RegoValue::from(s.as_str()),
        JsonValue::Array(arr) => {
            RegoValue::from(arr.iter().map(json_to_regorus).collect::<Vec<_>>())
        }
        JsonValue::Object(obj) => {
            let mut map = BTreeMap::new();
            for (k, v) in obj {
                map.insert(RegoValue::from(k.as_str()), json_to_regorus(v));
            }
            RegoValue::from(map)
        }
    }
}

/// Convert a `regorus::Value` to a `serde_json::Value`.
pub fn regorus_to_json(val: &RegoValue) -> JsonValue {
    match val {
        RegoValue::Null => JsonValue::Null,
        RegoValue::Bool(b) => JsonValue::Bool(*b),
        RegoValue::Number(n) => {
            // Use format_decimal() to get the string representation
            let s = n.format_decimal();
            if let Ok(i) = s.parse::<i64>() {
                JsonValue::Number(serde_json::Number::from(i))
            } else if let Ok(f) = s.parse::<f64>() {
                JsonValue::Number(
                    serde_json::Number::from_f64(f).unwrap_or(serde_json::Number::from(0)),
                )
            } else {
                JsonValue::String(s)
            }
        }
        RegoValue::String(s) => JsonValue::String(s.to_string()),
        RegoValue::Array(arr) => {
            JsonValue::Array(arr.iter().map(regorus_to_json).collect())
        }
        RegoValue::Object(obj) => {
            let mut map = serde_json::Map::new();
            for (k, v) in obj.iter() {
                let key = k
                    .as_string()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|_| format!("{k:?}"));
                map.insert(key, regorus_to_json(v));
            }
            JsonValue::Object(map)
        }
        RegoValue::Set(s) => {
            JsonValue::Array(s.iter().map(regorus_to_json).collect())
        }
        RegoValue::Undefined => JsonValue::Null,
    }
}

// ---------------------------------------------------------------------------
// NSID constants — mirror TS `NSID` object from `types.ts`
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
}

// ---------------------------------------------------------------------------
// NSID type classification (query vs procedure)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NsidType {
    Query,
    Procedure,
}

pub fn nsid_type(nsid: &str) -> NsidType {
    match nsid {
        NSID::GET_ARBITER_CONFIG
        | NSID::GET_SPACE_CONFIG
        | NSID::LIST_SPACES
        | NSID::GET_SPACE_MEMBERS
        | NSID::RESOLVE_SPACE_MEMBERS => NsidType::Query,
        _ => NsidType::Procedure,
    }
}

// ---------------------------------------------------------------------------
// Policy data builder
// ---------------------------------------------------------------------------

/// Build the `data` document for the Rego policy from an arbiter's state.
///
/// Returns a `serde_json::Value` — call [`json_to_regorus`] to pass to `VmSession`.
pub fn build_data_from_arbiter(arbiter: &ArbiterState) -> JsonValue {
    let mut spaces = serde_json::Map::new();
    for (key, space) in &arbiter.spaces {
        let members: Vec<JsonValue> = space
            .members
            .iter()
            .map(|m| {
                json!({
                    "did": m.did,
                    "access": m.access,
                })
            })
            .collect();

        spaces.insert(
            key.clone(),
            json!({
                "spaceType": space.space_type,
                "config": space.config,
                "members": members,
            }),
        );
    }

    json!({
        "arbiter": {
            "config": arbiter.config,
            "spaces": spaces,
        }
    })
}

// ---------------------------------------------------------------------------
// Policy input builder
// ---------------------------------------------------------------------------

/// Build the `input` document for a policy evaluation.
///
/// Returns a `serde_json::Value` — call [`json_to_regorus`] to pass to `VmSession`.
pub fn build_op_input(caller_did: &str, nsid: &str, params: &JsonValue) -> JsonValue {
    json!({
        "caller": {
            "did": caller_did,
        },
        "operation": {
            "nsid": nsid,
            "type": match nsid_type(nsid) {
                NsidType::Query => "query",
                NsidType::Procedure => "procedure",
            },
            "params": params,
        },
    })
}
