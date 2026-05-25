//! Rego policy evaluation helpers for the arbiter.
//!
//! Policy evaluation is now driven by [`PolicyVmPool`] (in `policy_vm.rs`)
//! using the RegoVM with suspendable execution. This module retains only
//! the type definitions, constants, and helper functions.

use regorus::{Engine, Value};
use std::collections::BTreeMap;

/// The default access-level policy embedded in the binary.
pub const DEFAULT_POLICY: &str = include_str!("../../../policies/arbiter/access-levels.rego");

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

// ---------------------------------------------------------------------------
// Value conversion helpers
// ---------------------------------------------------------------------------

/// Convert a serde_json::Value to a regorus::Value.
pub fn value_to_regorus(val: &serde_json::Value) -> Value {
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_policy() {
        assert!(validate_policy(DEFAULT_POLICY).is_ok());
        assert!(validate_policy("garbage {").is_err());
    }
}
