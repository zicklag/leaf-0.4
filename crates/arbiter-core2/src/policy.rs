//! Rego policy evaluation wrapper for the arbiter.
//!
//! Provides a `PolicyEngine` that wraps `regorus::Engine` with custom builtins
//! that query a frozen snapshot of the arbiter state.
//!
//! The snapshot is taken at request dispatch time using `im::HashMap`s cheap
//! structural sharing, ensuring all policy evaluations within a single request
//! see a consistent view of the data.

use regorus::{Engine, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow;

use crate::core::{Member, Space, SpaceKey};

/// The default access-level policy embedded in the binary.
pub const DEFAULT_POLICY: &str = include_str!("../../../policies/access-levels.rego");

/// Standard config NSIDs.
pub mod lexicon {
    /// The arbiter config contains a Rego policy.
    pub const CONFIG_REGO_POLICY: &str = "town.muni.arbiter.config.regoPolicy";
    /// Space config: whether records/members are public.
    pub const CONFIG_SPACE: &str = "town.muni.arbiter.config.space";
    /// Member access config: access level string.
    pub const CONFIG_ACCESS_LEVEL: &str = "town.muni.arbiter.config.accessLevel";
}

/// Actions that the policy can evaluate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyAction {
    ResolveSpaceMembers,
    GetSpaceMembers,
    ListSpaces,
    GetSpaceConfig,
    GetArbiterConfig,
    SetArbiterConfig,
    CreateSpace,
    SetSpaceConfig,
    DeleteSpace,
    SetSpaceMemberAccess,
    RemoveSpaceMember,
    DeleteArbiter,
}

impl PolicyAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ResolveSpaceMembers => "resolveSpaceMembers",
            Self::GetSpaceMembers => "getSpaceMembers",
            Self::ListSpaces => "listSpaces",
            Self::GetSpaceConfig => "getSpaceConfig",
            Self::GetArbiterConfig => "getArbiterConfig",
            Self::SetArbiterConfig => "setArbiterConfig",
            Self::CreateSpace => "createSpace",
            Self::SetSpaceConfig => "setSpaceConfig",
            Self::DeleteSpace => "deleteSpace",
            Self::SetSpaceMemberAccess => "setSpaceMemberAccess",
            Self::RemoveSpaceMember => "removeSpaceMember",
            Self::DeleteArbiter => "deleteArbiter",
        }
    }
}

/// Parameters passed to the policy for action-specific context.
#[derive(Debug, Clone, Default)]
pub struct PolicyParams {
    pub target_member: Option<serde_json::Value>,
    pub target_access: Option<serde_json::Value>,
}

/// Error from policy evaluation.
#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("Policy evaluation error: {0}")]
    EvalError(String),
}

// ---------------------------------------------------------------------------
// Custom builtins — query a frozen arbiter snapshot
// ---------------------------------------------------------------------------

/// Convert a Member to a regorus Value for the policy builtin.
fn member_to_value(member: &Member) -> Value {
    let (tag, value) = match member {
        Member::MemberDid(did) => ("MemberDid", Value::from(did.as_str())),
        Member::MemberLocalSpace(key) => ("MemberLocalSpace", Value::from(key.as_str())),
        Member::MemberRemoteSpace(sid) => {
            let mut obj = BTreeMap::new();
            obj.insert(
                Value::from("arbiterDid"),
                Value::from(sid.arbiter_did.as_str()),
            );
            obj.insert(
                Value::from("spaceKey"),
                Value::from(sid.space_key.as_str()),
            );
            ("MemberRemoteSpace", Value::from(obj))
        }
    };
    let mut obj = BTreeMap::new();
    obj.insert(Value::from("tag"), Value::from(tag));
    obj.insert(Value::from("value"), value);
    Value::from(obj)
}

// ---------------------------------------------------------------------------
// PolicyEngine
// ---------------------------------------------------------------------------

/// Wraps a `regorus::Engine` with custom builtins for lazy querying of a
/// frozen arbiter state snapshot.
///
/// Created per-request with a snapshot of the arbiter's spaces. All policy
/// evaluations during that request see the same snapshot.
pub struct PolicyEngine {
    engine: Engine,
}

impl PolicyEngine {
    /// Create a new policy engine with the given Rego policy and a snapshot
    /// of the arbiter's spaces for the custom builtins to query.
    pub fn new(
        policy_source: &str,
        arbiter_snapshot: Arc<im::HashMap<SpaceKey, Space>>,
    ) -> Result<Self, PolicyError> {
        let mut engine = Engine::new();

        // Register the custom builtin for querying space members.
        // This is a closure that captures the frozen snapshot via Arc,
        // so all evaluations see a consistent view of the data.
        let spaces = arbiter_snapshot;
        let spaces_members = spaces.clone();
        let spaces_config = spaces;
        engine
            .add_extension(
                "arbiter.get_space_members".to_string(),
                1,
                Box::new(move |params: Vec<Value>| -> anyhow::Result<Value> {
                    let space_key = params
                        .first()
                        .and_then(|v| v.as_string().ok())
                        .ok_or_else(|| anyhow::anyhow!("first argument must be a string (space_key)"))?;

                    let space = match spaces_members.get(space_key.as_ref()) {
                        Some(s) => s,
                        None => return Ok(Value::from(Vec::<Value>::new())),
                    };

                    let members: Vec<Value> = space
                        .members
                        .iter()
                        .map(|(member, access)| {
                            let mut obj = BTreeMap::new();
                            obj.insert(Value::from("member"), member_to_value(member));
                            obj.insert(Value::from("access"), value_to_regorus(access));
                            Value::from(obj)
                        })
                        .collect();

                    Ok(Value::from(members))
                }),
            )
            .map_err(|e| PolicyError::EvalError(format!("Failed to register builtin: {e}")))?;

        // Register the custom builtin for querying space config
        engine
            .add_extension(
                "arbiter.get_space_config".to_string(),
                1,
                Box::new(move |params: Vec<Value>| -> anyhow::Result<Value> {
                    let space_key = params
                        .first()
                        .and_then(|v| v.as_string().ok())
                        .ok_or_else(|| anyhow::anyhow!("first argument must be a string (space_key)"))?;

                    let space = match spaces_config.get(space_key.as_ref()) {
                        Some(s) => s,
                        None => return Ok(Value::from(BTreeMap::<Value, Value>::new())),
                    };

                    Ok(value_to_regorus(&space.config))
                }),
            )
            .map_err(|e| PolicyError::EvalError(format!("Failed to register builtin: {e}")))?;

        // Load the policy
        engine
            .add_policy("arbiter-policy.rego".to_string(), policy_source.to_string())
            .map_err(|e| PolicyError::EvalError(format!("Failed to load policy: {e}")))?;

        Ok(Self { engine })
    }

    /// Create a new policy engine with the default access-level policy.
    pub fn default_policy(
        arbiter_snapshot: Arc<im::HashMap<SpaceKey, Space>>,
    ) -> Result<Self, PolicyError> {
        Self::new(DEFAULT_POLICY, arbiter_snapshot)
    }

    /// Set the input for the next evaluation.
    fn set_json_input(&mut self, input: &serde_json::Value) {
        self.engine.set_input(value_to_regorus(input));
    }

    /// Evaluate whether an action is allowed.
    ///
    /// The custom builtins registered during construction query the frozen
    /// arbiter snapshot. For reads that return member lists, use
    /// `get_resolved_members` instead.
    pub fn evaluate(
        &mut self,
        action: PolicyAction,
        requester: &str,
        space_key: &str,
        params: Option<&PolicyParams>,
        resolved_remotes: &serde_json::Value,
    ) -> Result<bool, PolicyError> {
        let input = build_input(action, requester, space_key, params, resolved_remotes);
        self.set_json_input(&input);

        self.engine
            .eval_bool_query("data.arbiter.allow".to_string(), false)
            .map_err(|e| PolicyError::EvalError(format!("Policy evaluation failed: {e}")))
    }

    /// Get the resolved member list for a space (for read operations).
    ///
    /// Returns an array of { did, access }.
    pub fn get_resolved_members(
        &mut self,
        requester: &str,
        space_key: &str,
        resolved_remotes: &serde_json::Value,
    ) -> Result<serde_json::Value, PolicyError> {
        let input = build_input(
            PolicyAction::ResolveSpaceMembers,
            requester,
            space_key,
            None,
            resolved_remotes,
        );
        self.set_json_input(&input);

        // Get resolved members
        let members = self
            .engine
            .eval_query("data.arbiter.resolved_members".to_string(), false)
            .map_err(|e| PolicyError::EvalError(format!("Failed to get resolved members: {e}")))?;

        let member_list: Vec<serde_json::Value> = if members.result.is_empty() {
            vec![]
        } else {
            value_from_regorus(&members.result[0].expressions[0].value)
        };

        // Get missing spaces (remotes that resolved to empty / timed out)
        let missing = self
            .engine
            .eval_query("data.arbiter.missing_spaces".to_string(), false)
            .map_err(|e| PolicyError::EvalError(format!("Failed to get missing spaces: {e}")))?;

        let missing_list: Vec<serde_json::Value> = if missing.result.is_empty() {
            vec![]
        } else {
            value_from_regorus(&missing.result[0].expressions[0].value)
        };

        Ok(serde_json::json!({
            "members": member_list,
            "missingSpaces": missing_list,
        }))
    }

    /// Get the list of remote spaces that still need async resolution.
    pub fn get_needs_resolution(
        &mut self,
        requester: &str,
        space_key: &str,
        resolved_remotes: &serde_json::Value,
    ) -> Result<Vec<serde_json::Value>, PolicyError> {
        let input = build_input(
            PolicyAction::ResolveSpaceMembers,
            requester,
            space_key,
            None,
            resolved_remotes,
        );
        self.set_json_input(&input);

        let result = self
            .engine
            .eval_query("data.arbiter.needs_resolution".to_string(), false)
            .map_err(|e| {
                PolicyError::EvalError(format!("Failed to get needs_resolution: {e}"))
            })?;

        if result.result.is_empty() {
            return Ok(vec![]);
        }
        let val = &result.result[0].expressions[0].value;
        Ok(value_from_regorus(val))
    }
}

// ---------------------------------------------------------------------------
// Input building
// ---------------------------------------------------------------------------

fn build_input(
    action: PolicyAction,
    requester: &str,
    space_key: &str,
    params: Option<&PolicyParams>,
    resolved_remotes: &serde_json::Value,
) -> serde_json::Value {
    let mut input = serde_json::json!({
        "requester": requester,
        "action": action.as_str(),
        "resource": {
            "arbiterDid": "",
            "spaceKey": space_key,
        },
        "resolved_remotes": resolved_remotes,
    });

    if let Some(p) = params {
        if let Some(tm) = &p.target_member {
            input["params"]["targetMember"] = tm.clone();
        }
        if let Some(ta) = &p.target_access {
            input["params"]["targetAccess"] = ta.clone();
        }
    }

    input
}

// ---------------------------------------------------------------------------
// Value conversion helpers
// ---------------------------------------------------------------------------

/// Convert a serde_json::Value to a regorus::Value.
fn value_to_regorus(val: &serde_json::Value) -> Value {
    match val {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::from(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(f) = n.as_f64() {
                Value::from(f)
            } else {
                Value::from(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::from(s.as_str()),
        serde_json::Value::Array(arr) => {
            Value::from(arr.iter().map(value_to_regorus).collect::<Vec<_>>())
        }
        serde_json::Value::Object(obj) => {
            let mut map = BTreeMap::new();
            for (k, v) in obj {
                map.insert(Value::from(k.as_str()), value_to_regorus(v));
            }
            Value::from(map)
        }
    }
}

/// Convert a regorus::Value back to a serde_json::Value.
fn value_from_regorus(val: &Value) -> Vec<serde_json::Value> {
    // This is a simplified conversion. For complex cases, we'd serialize via JSON.
    // regorus::Value supports serde::Serialize, so we can use serde_json.
    let json_str = serde_json::to_string(val).unwrap_or_default();
    serde_json::from_str(&json_str).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate a Rego policy source without running it.
pub fn validate_policy(policy_source: &str) -> Result<(), PolicyError> {
    let mut engine = Engine::new();
    engine
        .add_policy("validate.rego".to_string(), policy_source.to_string())
        .map_err(|e| PolicyError::EvalError(format!("Invalid policy: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Space, ADMIN_SPACE_KEY};

    fn make_test_snapshot() -> Arc<im::HashMap<SpaceKey, Space>> {
        let mut spaces = im::HashMap::new();

        // $admin space with Alice as Owner
        let mut admin = Space::new(
            lexicon::CONFIG_SPACE.to_string(),
            serde_json::json!({
                "$type": lexicon::CONFIG_SPACE,
                "publicRecords": false,
                "publicMembers": false,
            }),
        );
        admin.members.insert(
            Member::MemberDid("did:plc:alice".into()),
            serde_json::json!({
                "$type": lexicon::CONFIG_ACCESS_LEVEL,
                "level": "Owner",
            }),
        );
        admin.members.insert(
            Member::MemberDid("did:plc:bob".into()),
            serde_json::json!({
                "$type": lexicon::CONFIG_ACCESS_LEVEL,
                "level": "ReadMemberList",
            }),
        );
        spaces.insert(ADMIN_SPACE_KEY.to_string(), admin);

        // "team" space with Alice as ConfigureSpace and Bob delegated from admin
        let mut team = Space::new(
            lexicon::CONFIG_SPACE.to_string(),
            serde_json::json!({
                "$type": lexicon::CONFIG_SPACE,
                "publicRecords": true,
                "publicMembers": false,
            }),
        );
        team.members.insert(
            Member::MemberDid("did:plc:alice".into()),
            serde_json::json!({
                "$type": lexicon::CONFIG_ACCESS_LEVEL,
                "level": "ConfigureSpace",
            }),
        );
        spaces.insert("team".to_string(), team);

        Arc::new(spaces)
    }

    fn single_owner_snapshot() -> Arc<im::HashMap<SpaceKey, Space>> {
        let mut spaces = im::HashMap::new();

        // $admin space with Alice as the sole Owner
        let mut admin = Space::new(
            lexicon::CONFIG_SPACE.to_string(),
            serde_json::json!({
                "$type": lexicon::CONFIG_SPACE,
                "publicRecords": false,
                "publicMembers": false,
            }),
        );
        admin.members.insert(
            Member::MemberDid("did:plc:alice".into()),
            serde_json::json!({
                "$type": lexicon::CONFIG_ACCESS_LEVEL,
                "level": "Owner",
            }),
        );
        spaces.insert(ADMIN_SPACE_KEY.to_string(), admin);

        Arc::new(spaces)
    }

    #[test]
    fn test_policy_creation() {
        let snapshot = make_test_snapshot();
        let engine = PolicyEngine::default_policy(snapshot);
        assert!(engine.is_ok(), "{:?}", engine.err());
    }

    #[test]
    fn test_owner_allowed_resolve_members() {
        let snapshot = make_test_snapshot();
        let mut engine = PolicyEngine::default_policy(snapshot).unwrap();
        let empty = serde_json::json!({});

        let allowed = engine
            .evaluate(
                PolicyAction::ResolveSpaceMembers,
                "did:plc:alice",
                "team",
                None,
                &empty,
            )
            .unwrap();
        assert!(allowed);
    }

    #[test]
    fn test_non_member_denied() {
        let snapshot = make_test_snapshot();
        let mut engine = PolicyEngine::default_policy(snapshot).unwrap();
        let empty = serde_json::json!({});

        let allowed = engine
            .evaluate(
                PolicyAction::ResolveSpaceMembers,
                "did:plc:stranger",
                "team",
                None,
                &empty,
            )
            .unwrap();
        assert!(!allowed);
    }

    #[test]
    fn test_owner_can_delete_arbiter() {
        let snapshot = single_owner_snapshot();
        let mut engine = PolicyEngine::default_policy(snapshot).unwrap();
        let empty = serde_json::json!({});

        let allowed = engine
            .evaluate(
                PolicyAction::DeleteArbiter,
                "did:plc:alice",
                "$admin",
                None,
                &empty,
            )
            .unwrap();
        assert!(allowed);
    }

    #[test]
    fn test_non_owner_cannot_delete_arbiter() {
        let snapshot = single_owner_snapshot();
        let mut engine = PolicyEngine::default_policy(snapshot).unwrap();
        let empty = serde_json::json!({});

        let allowed = engine
            .evaluate(
                PolicyAction::DeleteArbiter,
                "did:plc:bob",
                "$admin",
                None,
                &empty,
            )
            .unwrap();
        assert!(!allowed);
    }

    #[test]
    fn test_cannot_grant_higher_access() {
        let snapshot = make_test_snapshot();
        let mut engine = PolicyEngine::default_policy(snapshot).unwrap();
        let empty = serde_json::json!({});

        let params = PolicyParams {
            target_member: Some(serde_json::json!({
                "tag": "MemberDid",
                "value": "did:plc:charlie",
            })),
            target_access: Some(serde_json::json!({
                "$type": lexicon::CONFIG_ACCESS_LEVEL,
                "level": "Owner",
            })),
        };

        // Bob has ReadMemberList only, can't grant Owner
        let allowed = engine
            .evaluate(
                PolicyAction::SetSpaceMemberAccess,
                "did:plc:bob",
                "team",
                Some(&params),
                &empty,
            )
            .unwrap();
        assert!(!allowed);
    }

    #[test]
    fn test_validate_policy() {
        assert!(validate_policy(DEFAULT_POLICY).is_ok());
        assert!(validate_policy("garbage {").is_err());
    }
}
