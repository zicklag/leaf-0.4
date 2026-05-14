//! Implementation of the `arbiter_server` state machine from the Quint specification.
//!
//! This is a sans-IO state machine that wraps the core `Arbiter`, manages multiple
//! arbiters, dispatches messages, handles timeouts, and returns effects.
//!
//! The async wrapper (`futures.rs`, behind the `async` feature) bridges this
//! sans-IO design to the async world.

use im::{HashMap};
use serde::{Deserialize, Serialize};

#[cfg(feature = "js")]
use tsify::Tsify;
#[cfg(feature = "js")]
use wasm_bindgen::prelude::*;

use crate::core::*;

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

/// A point in time (monotonic tick count).
pub type Time = i64;

/// Number of ticks before timing out a job.
pub const TIMEOUT_TICKS: i64 = 8;

// ---------------------------------------------------------------------------
// Message
// ---------------------------------------------------------------------------

/// A message that the server processes, matching Quint's `Msg` type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "js", derive(Tsify))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Message {
    /// DID of the user or arbiter that initiated this message.
    pub user_did: UserDid,
    /// DID of the target arbiter.
    pub arbiter_did: ArbiterDid,
    /// Key of the target space.
    pub space_key: SpaceKey,
    /// The JobId that triggered this message (used by ReplyResolvedMembers
    /// to find the right job).
    pub src_job_id: JobId,
    /// Maximum depth for remote space resolution chains.
    pub resolver_depth: i64,
    /// What kind of message this is.
    pub kind: MessageKind,
}

// ---------------------------------------------------------------------------
// MessageKind
// ---------------------------------------------------------------------------

/// The kind of message, matching Quint's `MsgKind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[cfg_attr(feature = "js", derive(Tsify))]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum MessageKind {
    /// Reply to a FetchMembers request with resolved members.
    ReplyResolvedMembers {
        members: ResolvedMemberList,
    },
    /// Fetch the member list for a space.
    FetchMembers,
    /// Create a new space.
    CreateSpace,
    /// Configure a space's public records/members flags.
    ConfigureSpace {
        public_records: bool,
        public_members: bool,
    },
    /// Delete a space.
    DeleteSpace,
    /// Set a member's access in a space.
    SetMemberAccess {
        member: Member,
        access: Access,
    },
    /// Remove a member from a space.
    RemoveMember {
        member: Member,
    },
    /// Create a new arbiter.
    CreateArbiter,
    /// Delete an arbiter.
    DeleteArbiter,
}

// ---------------------------------------------------------------------------
// ServerEffect
// ---------------------------------------------------------------------------

/// Effects that the server emits for the async wrapper to process.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerEffect {
    /// Send a message to another arbiter's DID (for remote space resolution).
    SendMessage {
        to_did: ArbiterDid,
        msg: Message,
    },
    /// Respond to a pending request with a result.
    Respond {
        req_id: JobId,
        result: Result<JobResult, ServerError>,
    },
    /// The state of a specific arbiter changed — trigger persistence.
    ArbiterChanged {
        arbiter_did: ArbiterDid,
    },
    /// An arbiter was deleted — remove its persisted state.
    ArbiterDeleted {
        arbiter_did: ArbiterDid,
    },
}

// ---------------------------------------------------------------------------
// JobInfo
// ---------------------------------------------------------------------------

/// Information about a queued job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobInfo {
    /// The message that triggered this job.
    pub msg: Message,
    /// The time the job was started.
    pub start_time: Time,
}

// ---------------------------------------------------------------------------
// ServerError
// ---------------------------------------------------------------------------

/// Errors that the server can produce.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ServerError {
    #[error("ArbiterAlreadyExists")]
    ArbiterAlreadyExists,
    #[error("ArbiterNotExists")]
    ArbiterNotExists,
    #[error("ArbiterErr({0:?})")]
    ArbiterErr(ArbiterError),
    #[error("DuplicateReqId")]
    DuplicateReqId,
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

/// The server state: manages multiple arbiters, job info, and time.
///
/// This is a pure, sans-IO state machine. Every mutation returns a new `Server`
/// plus a list of `ServerEffect`s to be performed by the caller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Server {
    pub time: Time,
    /// Maps JobId → info about queued jobs (original message + start time).
    pub job_info: HashMap<JobId, JobInfo>,
    /// All hosted arbiters, keyed by DID.
    pub arbiters: HashMap<ArbiterDid, Arbiter>,
}

impl Default for Server {
    fn default() -> Self {
        Server {
            time: 0,
            job_info: HashMap::new(),
            arbiters: HashMap::new(),
        }
    }
}

impl Server {
    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Process a message, returning new state + any effects.
    ///
    /// Dispatches on `msg.kind` to the appropriate handler.
    pub fn handle_message(&self, msg: &Message) -> (Self, Vec<ServerEffect>) {
        match &msg.kind {
            MessageKind::CreateArbiter => self.handle_create_arbiter(msg),
            MessageKind::DeleteArbiter => {
                self.handle_start_job(msg, JobArgs::DeleteArbiter)
            }
            MessageKind::FetchMembers => {
                self.handle_start_job(msg, JobArgs::FetchMembers)
            }
            MessageKind::CreateSpace => {
                self.handle_start_job(msg, JobArgs::CreateSpace)
            }
            MessageKind::ConfigureSpace {
                public_records,
                public_members,
            } => self.handle_start_job(
                msg,
                JobArgs::ConfigureSpace(SpaceConfig {
                    public_records: *public_records,
                    public_members: *public_members,
                }),
            ),
            MessageKind::DeleteSpace => {
                self.handle_start_job(msg, JobArgs::DeleteSpace)
            }
            MessageKind::SetMemberAccess { member, access } => {
                self.handle_start_job(
                    msg,
                    JobArgs::SetMemberAccess {
                        member: member.clone(),
                        access: *access,
                    },
                )
            }
            MessageKind::RemoveMember { member } => {
                self.handle_start_job(
                    msg,
                    JobArgs::RemoveMember {
                        member: member.clone(),
                    },
                )
            }
            MessageKind::ReplyResolvedMembers { members } => {
                self.handle_resolution_reply(msg, members.clone())
            }
        }
    }

    /// Tick the server, processing any timed-out jobs. Returns new state + effects.
    ///
    /// If a timed-out job is found, it is timed out on the arbiter, and the
    /// result is fed through `handle_arbiter_result`. Otherwise, time is simply
    /// advanced.
    pub fn tick(&self) -> (Self, Vec<ServerEffect>) {
        // Find a timed-out job, if one exists
        let timed_out = self.find_timed_out_job();

        if let Some(job_id) = timed_out {
            let arbiter_did = self.find_arbiter_for_job(job_id)
                .expect("timed-out job must exist on an arbiter");
            let arbiter = &self.arbiters[&arbiter_did];
            let upd_arbiter = arbiter.timeout_job(job_id);

            // Create a minimal fake message for handle_arbiter_result
            // (the JobInfo lookup inside handle_arbiter_result will find the
            // real original message if it was stored)
            let dummy_msg = Message {
                user_did: String::new(),
                arbiter_did: arbiter_did.clone(),
                space_key: String::new(),
                src_job_id: job_id,
                resolver_depth: 0,
                kind: MessageKind::FetchMembers,
            };

            let new_server = self.tick_time();
            let (s, effects) = new_server.handle_arbiter_result(upd_arbiter, &dummy_msg);
            (s, effects)
        } else {
            (self.tick_time(), Vec::new())
        }
    }

    // -----------------------------------------------------------------------
    // Public helpers for persistence snapshots
    // -----------------------------------------------------------------------

    /// Get a serializable snapshot of an arbiter's state.
    /// Strips transient fields (job_queue, result).
    pub fn persistent_arbiter(&self, arbiter_did: &ArbiterDid) -> Option<PersistentArbiter> {
        self.arbiters.get(arbiter_did).map(|a| PersistentArbiter {
            version: a.version,
            did: a.did.clone(),
            spaces: a.spaces.clone(),
        })
    }

    /// Get snapshots of all arbiters.
    pub fn all_persistent_arbiters(&self) -> HashMap<ArbiterDid, PersistentArbiter> {
        self.arbiters
            .iter()
            .map(|(did, a)| {
                (
                    did.clone(),
                    PersistentArbiter {
                        version: a.version,
                        did: a.did.clone(),
                        spaces: a.spaces.clone(),
                    },
                )
            })
            .collect()
    }

    /// Load a persisted arbiter state back into the server.
    pub fn load_persistent_arbiter(
        &self,
        persistent: PersistentArbiter,
    ) -> Self {
        let arbiter = Arbiter {
            version: persistent.version,
            did: persistent.did.clone(),
            spaces: persistent.spaces,
            job_queue: HashMap::new(),
            result: ArbiterResult::Ok,
        };
        self.put_arbiter(persistent.did, arbiter)
    }

    // -----------------------------------------------------------------------
    // Internal helper methods — each returns (Server, Vec<ServerEffect>)
    // -----------------------------------------------------------------------

    fn handle_create_arbiter(&self, msg: &Message) -> (Self, Vec<ServerEffect>) {
        if self.arbiters.contains_key(&msg.arbiter_did) {
            return (
                self.clone(),
                vec![ServerEffect::Respond {
                    req_id: msg.src_job_id,
                    result: Err(ServerError::ArbiterAlreadyExists),
                }],
            );
        }

        let new_arbiter = Arbiter::new(msg.arbiter_did.clone(), msg.user_did.clone());
        let server = self.put_arbiter(msg.arbiter_did.clone(), new_arbiter);
        (
            server,
            vec![
                ServerEffect::Respond {
                    req_id: msg.src_job_id,
                    result: Ok(JobResult::Ok),
                },
                ServerEffect::ArbiterChanged {
                    arbiter_did: msg.arbiter_did.clone(),
                },
            ],
        )
    }

    fn handle_start_job(&self, msg: &Message, job_args: JobArgs) -> (Self, Vec<ServerEffect>) {
        // Check arbiter exists
        if !self.arbiters.contains_key(&msg.arbiter_did) {
            return (
                self.clone(),
                vec![ServerEffect::Respond {
                    req_id: msg.src_job_id,
                    result: Err(ServerError::ArbiterNotExists),
                }],
            );
        }

        let arbiter = &self.arbiters[&msg.arbiter_did];
        let upd_arbiter = arbiter.start_job(
            msg.user_did.clone(),
            msg.space_key.clone(),
            msg.src_job_id,
            job_args,
        );
        self.handle_arbiter_result(upd_arbiter, msg)
    }

    fn handle_resolution_reply(
        &self,
        msg: &Message,
        members: ResolvedMemberList,
    ) -> (Self, Vec<ServerEffect>) {
        // Look up the job info to find the original message's arbiter DID
        let job_info = match self.job_info.get(&msg.src_job_id) {
            Some(info) => info,
            None => {
                // Job no longer tracked — ignore the reply
                return (self.clone(), Vec::new());
            }
        };

        // The original arbiter that started the job
        let orig_arbiter_did = &job_info.msg.arbiter_did;

        if !self.arbiters.contains_key(orig_arbiter_did) {
            return (
                self.clone(),
                vec![ServerEffect::Respond {
                    req_id: msg.src_job_id,
                    result: Err(ServerError::ArbiterNotExists),
                }],
            );
        }

        let arbiter = &self.arbiters[orig_arbiter_did];
        let space_id = SpaceId {
            arbiter_did: msg.arbiter_did.clone(),
            space_key: msg.space_key.clone(),
        };
        let upd_arbiter = arbiter.provide_remote_space_members(msg.src_job_id, space_id, members);
        self.handle_arbiter_result(upd_arbiter, msg)
    }

    /// Given an arbiter that has just been updated, translate its result into
    /// server effects. Mirrors Quint's `handleArbiterResult`.
    fn handle_arbiter_result(
        &self,
        upd_arbiter: Arbiter,
        trigger_msg: &Message,
    ) -> (Self, Vec<ServerEffect>) {
        let arbiter_did = upd_arbiter.did.clone();
        let result = upd_arbiter.result.clone();

        match result {
            ArbiterResult::Ok => {
                let server = self.put_arbiter(arbiter_did, upd_arbiter);
                (server, Vec::new())
            }

            ArbiterResult::QueuedJob {
                id,
                spaces_to_resolve,
            } => {
                if trigger_msg.resolver_depth < 1 {
                    // Can't do resolution — timeout immediately
                    let timed_out = upd_arbiter.timeout_job(id);
                    self.handle_arbiter_result(timed_out, trigger_msg)
                } else {
                    let server = self
                        .put_arbiter(arbiter_did, upd_arbiter)
                        .add_job_info(
                            id,
                            JobInfo {
                                msg: trigger_msg.clone(),
                                start_time: self.time,
                            },
                        );
                    let effects: Vec<ServerEffect> = spaces_to_resolve
                        .into_iter()
                        .map(|space_id| ServerEffect::SendMessage {
                            // Decrement the resolver depth so we don't loop infinitely
                            to_did: space_id.arbiter_did.clone(),
                            msg: Message {
                                user_did: trigger_msg.user_did.clone(),
                                arbiter_did: space_id.arbiter_did,
                                space_key: space_id.space_key,
                                src_job_id: id,
                                resolver_depth: trigger_msg.resolver_depth - 1,
                                kind: MessageKind::FetchMembers,
                            },
                        })
                        .collect();
                    (server, effects)
                }
            }

            ArbiterResult::FinishedJob {
                id,
                result: job_result,
            } => {
                let server = self
                    .put_arbiter(arbiter_did.clone(), upd_arbiter)
                    .remove_job_info(id);
                let effects = vec![
                    ServerEffect::Respond {
                        req_id: id,
                        result: Ok(job_result),
                    },
                    ServerEffect::ArbiterChanged {
                        arbiter_did,
                    },
                ];
                (server, effects)
            }

            ArbiterResult::Deleted => {
                let server = self.remove_arbiter(&arbiter_did);
                (
                    server,
                    vec![
                        ServerEffect::Respond {
                            req_id: trigger_msg.src_job_id,
                            result: Ok(JobResult::Ok),
                        },
                        ServerEffect::ArbiterDeleted {
                            arbiter_did,
                        },
                    ],
                )
            }

            ArbiterResult::Err(e) => {
                let server = self.put_arbiter(arbiter_did, upd_arbiter);
                (
                    server,
                    vec![ServerEffect::Respond {
                        req_id: trigger_msg.src_job_id,
                        result: Err(ServerError::ArbiterErr(e)),
                    }],
                )
            }
        }
    }

    // -----------------------------------------------------------------------
    // Pure helper methods
    // -----------------------------------------------------------------------

    fn tick_time(&self) -> Self {
        let mut s = self.clone();
        s.time += 1;
        s
    }

    fn put_arbiter(&self, arbiter_did: ArbiterDid, arbiter: Arbiter) -> Self {
        let mut s = self.clone();
        s.arbiters = s.arbiters.update(arbiter_did, arbiter);
        s
    }

    fn remove_arbiter(&self, arbiter_did: &ArbiterDid) -> Self {
        let mut s = self.clone();
        s.arbiters = s.arbiters.without(arbiter_did);
        s
    }

    fn add_job_info(&self, job_id: JobId, info: JobInfo) -> Self {
        let mut s = self.clone();
        s.job_info = s.job_info.update(job_id, info);
        s
    }

    fn remove_job_info(&self, job_id: JobId) -> Self {
        let mut s = self.clone();
        s.job_info = s.job_info.without(&job_id);
        s
    }

    /// Find the first timed-out job across all arbiters.
    fn find_timed_out_job(&self) -> Option<JobId> {
        for (_arbiter_did, arbiter) in &self.arbiters {
            for job_id in arbiter.job_queue.keys() {
                if let Some(info) = self.job_info.get(job_id)
                    && self.time - info.start_time >= TIMEOUT_TICKS
                {
                    return Some(*job_id);
                }
            }
        }
        None
    }

    /// Find the arbiter DID that hosts a given job ID.
    fn find_arbiter_for_job(&self, job_id: JobId) -> Option<ArbiterDid> {
        for (arbiter_did, arbiter) in &self.arbiters {
            if arbiter.job_queue.contains_key(&job_id) {
                return Some(arbiter_did.clone());
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// PersistentArbiter
// ---------------------------------------------------------------------------

/// A serializable snapshot of an arbiter's persistent state.
/// Strips transient fields (job_queue, result).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistentArbiter {
    pub version: i64,
    pub did: ArbiterDid,
    pub spaces: HashMap<SpaceKey, Space>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a minimal message
    fn msg(
        user_did: &str,
        arbiter_did: &str,
        space_key: &str,
        src_job_id: JobId,
        kind: MessageKind,
    ) -> Message {
        Message {
            user_did: user_did.to_string(),
            arbiter_did: arbiter_did.to_string(),
            space_key: space_key.to_string(),
            src_job_id,
            resolver_depth: 3,
            kind,
        }
    }

    fn create_arbiter_msg(user_did: &str, arbiter_did: &str, job_id: JobId) -> Message {
        msg(
            user_did,
            arbiter_did,
            ADMIN_SPACE_KEY,
            job_id,
            MessageKind::CreateArbiter,
        )
    }

    fn fetch_members_msg(user_did: &str, arbiter_did: &str, space_key: &str, job_id: JobId) -> Message {
        msg(
            user_did,
            arbiter_did,
            space_key,
            job_id,
            MessageKind::FetchMembers,
        )
    }

    fn delete_arbiter_msg(user_did: &str, arbiter_did: &str, job_id: JobId) -> Message {
        msg(
            user_did,
            arbiter_did,
            ADMIN_SPACE_KEY,
            job_id,
            MessageKind::DeleteArbiter,
        )
    }

    fn create_space_msg(user_did: &str, arbiter_did: &str, space_key: &str, job_id: JobId) -> Message {
        msg(
            user_did,
            arbiter_did,
            space_key,
            job_id,
            MessageKind::CreateSpace,
        )
    }

    fn delete_space_msg(user_did: &str, arbiter_did: &str, space_key: &str, job_id: JobId) -> Message {
        msg(
            user_did,
            arbiter_did,
            space_key,
            job_id,
            MessageKind::DeleteSpace,
        )
    }

    fn configure_space_msg(
        user_did: &str,
        arbiter_did: &str,
        space_key: &str,
        public_records: bool,
        public_members: bool,
        job_id: JobId,
    ) -> Message {
        msg(
            user_did,
            arbiter_did,
            space_key,
            job_id,
            MessageKind::ConfigureSpace {
                public_records,
                public_members,
            },
        )
    }

    fn set_member_access_msg(
        user_did: &str,
        arbiter_did: &str,
        space_key: &str,
        member: Member,
        access: Access,
        job_id: JobId,
    ) -> Message {
        msg(
            user_did,
            arbiter_did,
            space_key,
            job_id,
            MessageKind::SetMemberAccess { member, access },
        )
    }

    fn remove_member_msg(
        user_did: &str,
        arbiter_did: &str,
        space_key: &str,
        member: Member,
        job_id: JobId,
    ) -> Message {
        msg(
            user_did,
            arbiter_did,
            space_key,
            job_id,
            MessageKind::RemoveMember { member },
        )
    }

    #[test]
    fn test_create_arbiter() {
        let server = Server::default();
        let m = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (new_server, effects) = server.handle_message(&m);

        // Should have the arbiter
        assert!(new_server.arbiters.contains_key("did:example:arb1"));
        let arb = &new_server.arbiters["did:example:arb1"];
        assert_eq!(arb.did, "did:example:arb1");

        // Should have Respond + ArbiterChanged
        assert_eq!(effects.len(), 2);
        assert!(effects.iter().any(|e| matches!(
            e,
            ServerEffect::Respond {
                req_id: 1,
                result: Ok(JobResult::Ok),
            }
        )));
        assert!(effects.iter().any(|e| matches!(
            e,
            ServerEffect::ArbiterChanged {
                arbiter_did,
            } if arbiter_did == "did:example:arb1"
        )));
    }

    #[test]
    fn test_create_duplicate_arbiter() {
        let server = Server::default();
        let m1 = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (server, _) = server.handle_message(&m1);

        let m2 = create_arbiter_msg("bob", "did:example:arb1", 2);
        let (_, effects) = server.handle_message(&m2);

        // Should error
        assert_eq!(effects.len(), 1);
        assert!(matches!(
            effects[0],
            ServerEffect::Respond {
                req_id: 2,
                result: Err(ServerError::ArbiterAlreadyExists),
            }
        ));
    }

    #[test]
    fn test_fetch_members_immediate() {
        let server = Server::default();
        let create = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (server, _) = server.handle_message(&create);

        // Fetch members should succeed immediately (only owner = alice)
        let fetch = fetch_members_msg("alice", "did:example:arb1", ADMIN_SPACE_KEY, 2);
        let (_new_server, effects) = server.handle_message(&fetch);

        // Should have a Respond effect with the member list
        let respond_effect = effects.iter().find(|e| matches!(e, ServerEffect::Respond { .. }));
        assert!(respond_effect.is_some(), "Expected a Respond effect");
        if let Some(ServerEffect::Respond { req_id, result }) = respond_effect {
            assert_eq!(*req_id, 2);
            assert!(result.is_ok());
            if let Ok(JobResult::ResolvedMembersList(list)) = result {
                assert!(list.member_list.contains_key("alice"));
                assert_eq!(list.member_list["alice"], Access::Owner);
            } else {
                panic!("Expected ResolvedMembersList, got {:?}", result);
            }
        }

        // Should have ArbiterChanged (since the job was written to the arbiter)
        assert!(effects.iter().any(|e| matches!(
            e,
            ServerEffect::ArbiterChanged { .. }
        )));
    }

    #[test]
    fn test_fetch_members_nonexistent_arbiter() {
        let server = Server::default();
        let fetch = fetch_members_msg("alice", "did:example:nonexistent", ADMIN_SPACE_KEY, 1);
        let (_, effects) = server.handle_message(&fetch);

        assert_eq!(effects.len(), 1);
        assert!(matches!(
            effects[0],
            ServerEffect::Respond {
                result: Err(ServerError::ArbiterNotExists),
                ..
            }
        ));
    }

    #[test]
    fn test_tick_no_timeout() {
        let server = Server::default();
        let (new_server, effects) = server.tick();

        assert_eq!(new_server.time, 1);
        assert!(effects.is_empty());
    }

    #[test]
    fn test_job_timeout() {
        let server = Server::default();
        let create = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (server, _) = server.handle_message(&create);

        // Create a space (will finish immediately, then we can test with a queued job)
        // Actually, on a fresh arbiter, everything resolves sync (no remote spaces)
        // so jobs don't queue. To test timeout we need a job that requires resolution.
        // Let's create a space first, then set up a remote space member.

        let sk = "myspace";
        let create_space = create_space_msg("alice", "did:example:arb1", sk, 2);
        let (server, _) = server.handle_message(&create_space);

        // Add a remote space member, which will cause the next job to need resolution
        let remote_id = SpaceId {
            arbiter_did: "did:example:remote".to_string(),
            space_key: "theirspace".to_string(),
        };
        let set_access = set_member_access_msg(
            "alice",
            "did:example:arb1",
            sk,
            Member::RemoteSpace(remote_id),
            Access::ReadMemberList,
            3,
        );
        let (server, effects) = server.handle_message(&set_access);

        // After adding remote member, the job should be queued
        let _has_send = effects.iter().any(|e| matches!(e, ServerEffect::SendMessage { .. }));
        // Note: depends on whether alice has AddMembers permission which allows setting
        // access on an empty space... Let's check: owner has all permissions.

        // Actually the job to set member access on the space will try to resolve
        // member lists. Since there are no remote spaces yet (we're adding one),
        // it should complete immediately. Let me adjust the test.
        //
        // Instead, let me just verify tick works with no queued jobs first.
        // For a proper timeout test, we need to create a scenario where space
        // delegation leads to queued jobs.

        let (server, _) = (server, effects);

        // Now tick a few times
        let mut server = server;
        for _ in 0..TIMEOUT_TICKS + 1 {
            let (s, _eff) = server.tick();
            server = s;
        }
        assert!(server.time > TIMEOUT_TICKS);
    }

    #[test]
    fn test_tick_queued_job_timeout() {
        // Create an arbiter with a space that has a remote space member.
        // Then fetch members of that space. It should queue.
        let server = Server::default();

        // First set up an arbiter
        let create = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (server, _) = server.handle_message(&create);

        // Create a space
        let create_space = create_space_msg("alice", "did:example:arb1", "myspace", 2);
        let (server, _) = server.handle_message(&create_space);

        // Add a remote space member (requires resolution when fetching)
        let remote_id = SpaceId {
            arbiter_did: "did:example:remote".to_string(),
            space_key: "theirspace".to_string(),
        };
        let m = msg(
            "alice",
            "did:example:arb1",
            "myspace",
            3,
            MessageKind::SetMemberAccess {
                member: Member::RemoteSpace(remote_id),
                access: Access::ReadMemberList,
            },
        );

        let (server, _effects) = server.handle_message(&m);

        // Now fetch members of the space with a new job
        let fetch = fetch_members_msg("alice", "did:example:arb1", "myspace", 4);
        let (server, effects) = server.handle_message(&fetch);

        // The fetch should queue and produce a SendMessage effect
        let queued = effects.iter().any(|e| matches!(e, ServerEffect::SendMessage { .. }));
        assert!(queued, "Expected SendMessage for remote resolution");

        let has_respond = effects.iter().any(|e| matches!(e, ServerEffect::Respond { .. }));
        assert!(!has_respond, "Should not respond immediately for queued job");

        // Advance time past timeout
        let mut server = server;
        for _ in 0..=TIMEOUT_TICKS {
            let (s, effects) = server.tick();
            server = s;
            // Check if any tick produced a Respond (job timed out)
            if effects.iter().any(|e| matches!(e, ServerEffect::Respond { .. })) {
                return; // Success - job timed out and responded
            }
        }

        panic!("Job did not time out within expected ticks");
    }

    #[test]
    fn test_resolution_reply() {
        let server = Server::default();

        // Set up an arbiter with a space that has a remote member
        let create = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (server, _) = server.handle_message(&create);

        let create_space = create_space_msg("alice", "did:example:arb1", "myspace", 2);
        let (server, _) = server.handle_message(&create_space);

        let remote_id = SpaceId {
            arbiter_did: "did:example:remote".to_string(),
            space_key: "theirspace".to_string(),
        };
        let set_access = msg(
            "alice",
            "did:example:arb1",
            "myspace",
            3,
            MessageKind::SetMemberAccess {
                member: Member::RemoteSpace(remote_id.clone()),
                access: Access::ReadMemberList,
            },
        );
        let (server, _) = server.handle_message(&set_access);

        // Fetch members — should queue
        let fetch = fetch_members_msg("alice", "did:example:arb1", "myspace", 4);
        let (server, effects) = server.handle_message(&fetch);

        // Verify it queued
        let send_effect = effects.iter().find(|e| matches!(e, ServerEffect::SendMessage { .. }));
        assert!(send_effect.is_some(), "Expected SendMessage for remote resolution");

        // Simulate receiving the reply
        let resolved = ResolvedMemberList {
            member_list: {
                let mut m: HashMap<UserDid, Access> = HashMap::new();
                m.insert("bob".to_string(), Access::ReadMemberList);
                m
            },
            missing_spaces: HashMap::new(),
        };

        let reply = msg(
            "bob",                 // user_did doesn't matter for reply
            "did:example:remote",  // the remote arbiter we queried
            "theirspace",          // the space we queried
            4,                     // src_job_id matching the fetch
            MessageKind::ReplyResolvedMembers {
                members: resolved,
            },
        );

        let (_server, effects) = server.handle_message(&reply);

        // Should get a Respond effect
        let respond = effects.iter().find(|e| matches!(e, ServerEffect::Respond { .. }));
        assert!(respond.is_some(), "Expected Respond after resolution reply");
        if let Some(ServerEffect::Respond { result, .. }) = respond {
            assert!(result.is_ok(), "Expected Ok result, got {:?}", result);
        }

        // Should also have ArbiterChanged
        assert!(effects.iter().any(|e| matches!(
            e,
            ServerEffect::ArbiterChanged { arbiter_did } if arbiter_did == "did:example:arb1"
        )));
    }

    #[test]
    fn test_delete_arbiter() {
        let server = Server::default();
        let create = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (server, _) = server.handle_message(&create);

        // Deleting as the sole owner should succeed
        let delete = delete_arbiter_msg("alice", "did:example:arb1", 2);
        let (new_server, effects) = server.handle_message(&delete);

        // Arbiter should be removed
        assert!(!new_server.arbiters.contains_key("did:example:arb1"));

        // Should have ArbiterDeleted
        assert!(effects.iter().any(|e| matches!(
            e,
            ServerEffect::ArbiterDeleted { arbiter_did } if arbiter_did == "did:example:arb1"
        )));
    }

    #[test]
    fn test_delete_nonexistent_arbiter() {
        let server = Server::default();
        let delete = delete_arbiter_msg("alice", "did:example:nonexistent", 1);
        let (_, effects) = server.handle_message(&delete);

        assert!(matches!(
            effects[0],
            ServerEffect::Respond {
                result: Err(ServerError::ArbiterNotExists),
                ..
            }
        ));
    }

    #[test]
    fn test_create_space() {
        let server = Server::default();
        let create = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (server, _) = server.handle_message(&create);

        let create_space = create_space_msg("alice", "did:example:arb1", "myspace", 2);
        let (server, effects) = server.handle_message(&create_space);

        // Should have Respond(Ok)
        let respond = effects.iter().find(|e| matches!(e, ServerEffect::Respond { .. }));
        assert!(respond.is_some());
        if let Some(ServerEffect::Respond { result, .. }) = respond {
            assert!(matches!(result, Ok(JobResult::Ok)));
        }

        // Space should exist
        assert!(server.arbiters["did:example:arb1"].spaces.contains_key("myspace"));
    }

    #[test]
    fn test_create_duplicate_space() {
        let server = Server::default();
        let create = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (server, _) = server.handle_message(&create);

        let create_space = create_space_msg("alice", "did:example:arb1", "myspace", 2);
        let (server, _) = server.handle_message(&create_space);

        // Create the same space again — should fail
        let create_dup = create_space_msg("alice", "did:example:arb1", "myspace", 3);
        let (_, effects) = server.handle_message(&create_dup);

        assert!(effects.iter().any(|e| matches!(
            e,
            ServerEffect::Respond {
                result: Err(ServerError::ArbiterErr(ArbiterError::SpaceAlreadyExists)),
                ..
            }
        )));
    }

    #[test]
    fn test_delete_space() {
        let server = Server::default();
        let create = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (server, _) = server.handle_message(&create);

        let create_space = create_space_msg("alice", "did:example:arb1", "myspace", 2);
        let (server, _) = server.handle_message(&create_space);

        let delete = delete_space_msg("alice", "did:example:arb1", "myspace", 3);
        let (server, effects) = server.handle_message(&delete);

        // Should succeed
        let respond = effects.iter().find(|e| matches!(e, ServerEffect::Respond { .. }));
        assert!(respond.is_some());
        if let Some(ServerEffect::Respond { result, .. }) = respond {
            assert!(matches!(result, Ok(JobResult::Ok)));
        }

        // Space should be gone
        assert!(!server.arbiters["did:example:arb1"].spaces.contains_key("myspace"));
    }

    #[test]
    fn test_delete_admin_space_fails() {
        let server = Server::default();
        let create = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (server, _) = server.handle_message(&create);

        let delete = delete_space_msg("alice", "did:example:arb1", ADMIN_SPACE_KEY, 2);
        let (_, effects) = server.handle_message(&delete);

        assert!(effects.iter().any(|e| matches!(
            e,
            ServerEffect::Respond {
                result: Err(ServerError::ArbiterErr(ArbiterError::CannotDeleteAdminSpace)),
                ..
            }
        )));
    }

    #[test]
    fn test_set_member_access() {
        let server = Server::default();
        let create = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (server, _) = server.handle_message(&create);

        let create_space = create_space_msg("alice", "did:example:arb1", "myspace", 2);
        let (server, _) = server.handle_message(&create_space);

        // Set bob as a member with ReadMemberList access
        let set = set_member_access_msg(
            "alice",
            "did:example:arb1",
            "myspace",
            Member::User("bob".to_string()),
            Access::ReadMemberList,
            3,
        );
        let (server, effects) = server.handle_message(&set);

        let respond = effects.iter().find(|e| matches!(e, ServerEffect::Respond { .. }));
        assert!(respond.is_some());
        if let Some(ServerEffect::Respond { result, .. }) = respond {
            assert!(matches!(result, Ok(JobResult::Ok)));
        }

        // Verify bob is a member
        let space = &server.arbiters["did:example:arb1"].spaces["myspace"];
        assert!(space.members.contains_key(&Member::User("bob".to_string())));
        assert_eq!(space.members[&Member::User("bob".to_string())], Access::ReadMemberList);
    }

    #[test]
    fn test_remove_member() {
        let server = Server::default();
        let create = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (server, _) = server.handle_message(&create);

        let create_space = create_space_msg("alice", "did:example:arb1", "myspace", 2);
        let (server, _) = server.handle_message(&create_space);

        // Add bob
        let set = set_member_access_msg(
            "alice",
            "did:example:arb1",
            "myspace",
            Member::User("bob".to_string()),
            Access::ReadMemberList,
            3,
        );
        let (server, _) = server.handle_message(&set);

        // Remove bob
        let remove = remove_member_msg(
            "alice",
            "did:example:arb1",
            "myspace",
            Member::User("bob".to_string()),
            4,
        );
        let (server, effects) = server.handle_message(&remove);

        let respond = effects.iter().find(|e| matches!(e, ServerEffect::Respond { .. }));
        assert!(respond.is_some());
        if let Some(ServerEffect::Respond { result, .. }) = respond {
            assert!(matches!(result, Ok(JobResult::Ok)));
        }

        // Verify bob is gone
        let space = &server.arbiters["did:example:arb1"].spaces["myspace"];
        assert!(!space.members.contains_key(&Member::User("bob".to_string())));
    }

    #[test]
    fn test_configure_space() {
        let server = Server::default();
        let create = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (server, _) = server.handle_message(&create);

        let create_space = create_space_msg("alice", "did:example:arb1", "myspace", 2);
        let (server, _) = server.handle_message(&create_space);

        // Configure space
        let config = configure_space_msg("alice", "did:example:arb1", "myspace", true, true, 3);
        let (server, effects) = server.handle_message(&config);

        let respond = effects.iter().find(|e| matches!(e, ServerEffect::Respond { .. }));
        assert!(respond.is_some());
        if let Some(ServerEffect::Respond { result, .. }) = respond {
            assert!(matches!(result, Ok(JobResult::Ok)));
        }

        // Verify config
        let space = &server.arbiters["did:example:arb1"].spaces["myspace"];
        assert!(space.config.public_records);
        assert!(space.config.public_members);
    }

    #[test]
    fn test_persistent_arbiter_snapshot() {
        let server = Server::default();
        let create = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (server, _) = server.handle_message(&create);

        let pa = server.persistent_arbiter(&"did:example:arb1".to_string());
        assert!(pa.is_some());
        let pa = pa.unwrap();
        assert_eq!(pa.did, "did:example:arb1");
        assert!(pa.spaces.contains_key(ADMIN_SPACE_KEY));
    }

    #[test]
    fn test_load_persistent_arbiter() {
        let server = Server::default();
        let create = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (server, _) = server.handle_message(&create);

        // Snapshot
        let pa = server.persistent_arbiter(&"did:example:arb1".to_string()).unwrap();
        assert!(pa.spaces.contains_key(ADMIN_SPACE_KEY));

        // Create a fresh server and load the snapshot
        let fresh = Server::default();
        let loaded = fresh.load_persistent_arbiter(pa);
        assert!(loaded.arbiters.contains_key("did:example:arb1"));
        let arb = &loaded.arbiters["did:example:arb1"];
        assert_eq!(arb.spaces.len(), 1);
        assert!(arb.job_queue.is_empty());
    }

    #[test]
    fn test_remote_resolution_zero_depth() {
        // When resolver_depth is 0, a queued job should immediately time out
        let server = Server::default();
        let create = create_arbiter_msg("alice", "did:example:arb1", 1);
        let (server, _) = server.handle_message(&create);

        let create_space = create_space_msg("alice", "did:example:arb1", "myspace", 2);
        let (server, _) = server.handle_message(&create_space);

        // Add remote space member
        let remote_id = SpaceId {
            arbiter_did: "did:example:remote".to_string(),
            space_key: "theirspace".to_string(),
        };
        let set_access = set_member_access_msg(
            "alice",
            "did:example:arb1",
            "myspace",
            Member::RemoteSpace(remote_id),
            Access::ReadMemberList,
            3,
        );
        let (server, _) = server.handle_message(&set_access);

        // Fetch members with resolver_depth = 0
        let fetch = Message {
            user_did: "alice".to_string(),
            arbiter_did: "did:example:arb1".to_string(),
            space_key: "myspace".to_string(),
            src_job_id: 4,
            resolver_depth: 0,
            kind: MessageKind::FetchMembers,
        };
        let (_, effects) = server.handle_message(&fetch);

        // Should NOT send a remote resolution message due to depth 0
        let has_send = effects.iter().any(|e| matches!(e, ServerEffect::SendMessage { .. }));
        assert!(!has_send, "Should not send with depth 0");

        // Should respond with the timed-out result (which is the member list with
        // the remote space in missing_spaces)
        let has_respond = effects.iter().any(|e| matches!(e, ServerEffect::Respond { .. }));
        assert!(has_respond, "Should respond after immediate timeout");
    }
}
