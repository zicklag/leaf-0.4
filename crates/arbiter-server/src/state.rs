//! Server state — collection of arbiter state machines.
//!
//! Each arbiter is a full [`arbiter_core::StateMachine`] backed by a PDS
//! account.  The collection is locked behind a single `tokio::sync::Mutex`
//! in [`ServerState`].
//!
//! Persistence stores [`ArbiterState`](arbiter_core::ArbiterState) data in
//! `state.json` and PDS account credentials in `pds-arbiters.json`
//! (separate file so it can be encrypted independently in future).

use std::collections::HashMap;

use arbiter_core::{ArbiterState, MemberEntry, Space, SpaceId, StateMachine};
use serde::{Deserialize, Serialize};
use serde_json::Value;

type Did = String;

// ---------------------------------------------------------------------------
// PDS account — stored alongside app-password arbiters
// ---------------------------------------------------------------------------

/// PDS account — stored alongside every arbiter.
///
/// Used to proxy XRPC requests during policy evaluation through the
/// associated PDS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdsCredentials {
    /// The app password (stored locally for proxied auth).
    pub app_password: String,
}

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

/// Snapshot of an arbiter's PDS credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdsArbiterSnapshot {
    pub did: Did,
    pub pds_account: PdsCredentials,
}

/// All arbiter PDS accounts.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PdsArbiterSnapshotSet {
    pub arbiters: Vec<PdsArbiterSnapshot>,
}

// ---------------------------------------------------------------------------
// ArbiterCollection — all state machines
// ---------------------------------------------------------------------------

/// All arbiter state machines managed by this server.
///
/// Every arbiter has an associated PDS account for proxying XRPC requests.
/// Locked behind [`tokio::sync::Mutex`] in [`ServerState`](crate::ServerState).
#[derive(Default)]
pub struct ArbiterCollection {
    pub arbiters: HashMap<Did, StateMachine>,
    /// PDS accounts for proxying XRPC requests, keyed by arbiter DID.
    pub pds_accounts: HashMap<Did, PdsCredentials>,
}

impl ArbiterCollection {
    pub fn new() -> Self {
        Self {
            arbiters: HashMap::new(),
            pds_accounts: HashMap::new(),
        }
    }

    pub fn get(&self, did: &str) -> Option<&StateMachine> {
        self.arbiters.get(did)
    }

    pub fn get_mut(&mut self, did: &str) -> Option<&mut StateMachine> {
        self.arbiters.get_mut(did)
    }

    /// Create an arbiter backed by a PDS account.  All XRPC requests
    /// triggered during policy evaluation are proxied through the PDS.
    pub fn create_arbiter_with_app_password(
        &mut self,
        did: Did,
        config: Value,
        pds_account: PdsCredentials,
    ) -> anyhow::Result<()> {
        let sm = StateMachine::create(did.clone(), config)?;
        self.pds_accounts.insert(did.clone(), pds_account);
        self.arbiters.insert(did, sm);

        Ok(())
    }

    /// Get the PDS account associated with an arbiter, if any.
    pub fn get_pds_account(&self, arbiter_did: &str) -> Option<&PdsCredentials> {
        self.pds_accounts.get(arbiter_did)
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
                spaces,
            };
            self.arbiters.insert(a.did, StateMachine::new(arb_state));
        }
    }

    /// Load PDS account data from a snapshot (loaded separately from main state).
    pub fn load_pds_snapshot(&mut self, snapshot: PdsArbiterSnapshotSet) {
        for a in snapshot.arbiters {
            self.pds_accounts.insert(a.did, a.pds_account);
        }
    }

    /// Snapshot just the PDS accounts (for separate persistence).
    pub fn pds_snapshot(&self) -> PdsArbiterSnapshotSet {
        let arbiters = self
            .pds_accounts
            .iter()
            .map(|(did, account)| PdsArbiterSnapshot {
                did: did.clone(),
                pds_account: account.clone(),
            })
            .collect();
        PdsArbiterSnapshotSet { arbiters }
    }

    /// Remove an arbiter by DID.  Returns `true` if it existed.
    pub fn remove(&mut self, did: &str) -> bool {
        self.pds_accounts.remove(did);
        self.arbiters.remove(did).is_some()
    }
}
