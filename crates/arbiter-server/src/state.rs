//! Server state — the minimal data store for arbiters, spaces, and members.
//!
//! No state machine, no action dispatch. Just data with helpers for
//! snapshot/load and field access.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

pub type Did = String;
pub type SpaceKey = String;

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

impl ArbiterState {
    pub fn space_members(&self, key: &str) -> Vec<MemberEntry> {
        self.spaces.get(key).map(|s| s.members.clone()).unwrap_or_default()
    }

    pub fn space_config(&self, key: &str) -> Value {
        self.spaces.get(key).map(|s| s.config.clone()).unwrap_or(Value::Null)
    }
}

// ---------------------------------------------------------------------------
// Snapshot types (JSON-serializable full state)
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
// ArbiterCollection — the entire in-memory state
// ---------------------------------------------------------------------------

/// All arbiters managed by this server. Locked behind a `tokio::sync::Mutex`
/// in `ServerState`. No internal state machine — just data.
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

    pub fn space_members(&self, arbiter_did: &str, space_key: &str) -> Vec<MemberEntry> {
        self.arbiters.get(arbiter_did)
            .map(|a| a.space_members(space_key))
            .unwrap_or_default()
    }

    pub fn space_config(&self, arbiter_did: &str, space_key: &str) -> Value {
        self.arbiters.get(arbiter_did)
            .map(|a| a.space_config(space_key))
            .unwrap_or(Value::Null)
    }

    // -------------------------------------------------------------------
    // Snapshot / serialisation
    // -------------------------------------------------------------------

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
