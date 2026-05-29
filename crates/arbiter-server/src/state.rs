//! Server state — collection of arbiter state machines.
//!
//! Each arbiter is a full [`arbiter_core::StateMachine`].  The collection is
//! locked behind a single `tokio::sync::Mutex` in [`ServerState`].
//!
//! Persistence is handled via [`ArbiterCollection::snapshot`] /
//! [`ArbiterCollection::load_snapshot`], which serialize only the
//! [`ArbiterState`](arbiter_core::ArbiterState) data (not transient VM
//! sessions).

use std::collections::HashMap;

use arbiter_core::{ArbiterState, StateMachine, SpaceId, Space, MemberEntry};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type Did = String;

// ---------------------------------------------------------------------------
// Snapshot types (JSON-serializable full state)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceSnapshot {
    pub key: String,
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
    pub spaces: Vec<SpaceSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSnapshot {
    pub arbiters: Vec<ArbiterSnapshot>,
}

// ---------------------------------------------------------------------------
// ArbiterCollection — all state machines
// ---------------------------------------------------------------------------

/// All arbiter state machines managed by this server.
///
/// Locked behind [`tokio::sync::Mutex`] in [`ServerState`](crate::ServerState).
#[derive(Default)]
pub struct ArbiterCollection {
    pub arbiters: HashMap<Did, StateMachine>,
}

impl ArbiterCollection {
    pub fn new() -> Self {
        Self {
            arbiters: HashMap::new(),
        }
    }

    pub fn get(&self, did: &str) -> Option<&StateMachine> {
        self.arbiters.get(did)
    }

    pub fn get_mut(&mut self, did: &str) -> Option<&mut StateMachine> {
        self.arbiters.get_mut(did)
    }

    pub fn create_arbiter(&mut self, did: Did, config: Value, policy: String, owner_did: Did) {
        let sm = StateMachine::create(did.clone(), config, policy, owner_did);
        self.arbiters.insert(did, sm);
    }

    // -------------------------------------------------------------------
    // Snapshot / serialisation
    // -------------------------------------------------------------------

    pub fn snapshot(&self) -> ServerSnapshot {
        let arbiters: Vec<ArbiterSnapshot> = self
            .arbiters
            .values()
            .map(|sm| {
                let arb = &sm.arbiter;
                let spaces: Vec<SpaceSnapshot> = arb
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
                    did: arb.did.clone(),
                    version: arb.version,
                    config: arb.config.clone(),
                    policy: arb.policy.clone(),
                    spaces,
                }
            })
            .collect();
        ServerSnapshot { arbiters }
    }

    pub fn load_snapshot(&mut self, snapshot: ServerSnapshot) {
        self.arbiters.clear();
        for a in snapshot.arbiters {
            let spaces: HashMap<SpaceId, Space> = a
                .spaces
                .into_iter()
                .map(|s| {
                    let space = Space {
                        key: s.key.clone(),
                        space_type: s.space_type.clone(),
                        config: s.config,
                        members: s.members,
                    };
                    let id = SpaceId {
                        space_key: space.key.clone(),
                        space_type: space.space_type.clone(),
                    };
                    (id, space)
                })
                .collect();
            let arb_state = ArbiterState {
                did: a.did.clone(),
                version: a.version,
                config: a.config,
                policy: a.policy,
                spaces,
            };
            self.arbiters
                .insert(a.did, StateMachine::new(arb_state));
        }
    }

    /// Remove an arbiter by DID.  Returns `true` if it existed.
    pub fn remove(&mut self, did: &str) -> bool {
        self.arbiters.remove(did).is_some()
    }
}
