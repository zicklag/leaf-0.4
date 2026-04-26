//! Implementation of the `arbiter_server` state machine from the Quint specification.
//!
//! The server wraps the core, manages multiple arbiters, handles XRPC requests,
//! timeouts, and remote space resolution dispatch.

use im::{HashMap, HashSet};
use std::fmt;

use crate::core::*;

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

pub type ReqId = i64;
pub type Time = i64;

/// Number of ticks before timing out a job.
pub const TIMEOUT_TICKS: i64 = 8;

// ---------------------------------------------------------------------------
// XrpcEndpoint
// ---------------------------------------------------------------------------

/// An XRPC endpoint with its arguments.
#[derive(Debug, Clone, PartialEq)]
pub enum XrpcEndpoint {
    FetchMembers {
        user_did: UserDid,
        arbiter_did: ArbiterDid,
        space_key: SpaceKey,
        resolver_depth: i64,
    },
    CreateSpace {
        user_did: UserDid,
        arbiter_did: ArbiterDid,
        space_key: SpaceKey,
        resolver_depth: i64,
    },
    ConfigureSpace {
        user_did: UserDid,
        arbiter_did: ArbiterDid,
        space_key: SpaceKey,
        public_records: bool,
        public_members: bool,
        resolver_depth: i64,
    },
    DeleteSpace {
        user_did: UserDid,
        arbiter_did: ArbiterDid,
        space_key: SpaceKey,
        resolver_depth: i64,
    },
    SetMemberAccess {
        user_did: UserDid,
        arbiter_did: ArbiterDid,
        space_key: SpaceKey,
        member: Member,
        access: Access,
        resolver_depth: i64,
    },
    RemoveMember {
        user_did: UserDid,
        arbiter_did: ArbiterDid,
        space_key: SpaceKey,
        member: Member,
        resolver_depth: i64,
    },
    CreateArbiter {
        user_did: UserDid,
        arbiter_did: ArbiterDid,
        resolver_depth: i64,
    },
    DeleteArbiter {
        user_did: UserDid,
        arbiter_did: ArbiterDid,
        resolver_depth: i64,
    },
}

// ---------------------------------------------------------------------------
// Request
// ---------------------------------------------------------------------------

/// An XRPC request.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub id: ReqId,
    pub endpoint: XrpcEndpoint,
}

// ---------------------------------------------------------------------------
// Response
// ---------------------------------------------------------------------------

/// A response for a completed request.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Response {
    pub id: ReqId,
    pub data: JobResult,
}

// ---------------------------------------------------------------------------
// Feedback
// ---------------------------------------------------------------------------

/// Feedback given by the server to the outside world.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Feedback {
    Respond(Response),
    ResolveRemoteList {
        req_id: ReqId,
        space: SpaceId,
        resolver_depth: i64,
    },
    CancelResolvingRemoteList(SpaceId),
}

// ---------------------------------------------------------------------------
// ServerError
// ---------------------------------------------------------------------------

/// Errors that the server can produce.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerError {
    ArbiterAlreadyExists,
    ArbiterNotExists,
    ArbiterErr(ArbiterError),
    DuplicateReqId,
}

// ---------------------------------------------------------------------------
// ServerResult
// ---------------------------------------------------------------------------

/// The result of an action on the server.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerResult {
    Ok(HashSet<Feedback>),
    Err(ServerError),
    Panic,
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

/// The server state: manages multiple arbiters, job start times, and time.
#[derive(Debug, Clone, PartialEq)]
pub struct Server {
    pub time: Time,
    pub job_start_times: HashMap<ReqId, Time>,
    pub arbiters: HashMap<ArbiterDid, Arbiter>,
    pub result: ServerResult,
}

impl Default for Server {
    fn default() -> Self {
        Server {
            time: 0,
            job_start_times: HashMap::new(),
            arbiters: HashMap::new(),
            result: ServerResult::Ok(HashSet::new()),
        }
    }
}

impl Server {

    fn throw(&self, err: ServerError) -> Self {
        let mut s = self.clone();
        s.result = ServerResult::Err(err);
        s
    }

    fn panic(&self) -> Self {
        let mut s = self.clone();
        s.result = ServerResult::Panic;
        s
    }

    fn ok(&self, feedback: HashSet<Feedback>) -> Self {
        let mut s = self.clone();
        s.result = ServerResult::Ok(feedback);
        s
    }

    fn tick_internal(&self) -> Self {
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

    fn add_job_start_time(&self, req_id: ReqId, time: Time) -> Self {
        let mut s = self.clone();
        s.job_start_times = s.job_start_times.update(req_id, time);
        s
    }

    fn remove_job_start_time(&self, req_id: ReqId) -> Self {
        let mut s = self.clone();
        s.job_start_times = s.job_start_times.without(&req_id);
        s
    }

    // -----------------------------------------------------------------------
    // Process an XRPC request
    // -----------------------------------------------------------------------

    /// Process an XRPC request.
    pub fn request(&self, req: &Request) -> Self {
        // Reject requests with an in-use request ID (any arbiter's job queue has this ID)
        let id_in_use = self.arbiters.values().any(|a| a.job_queue.contains_key(&req.id));
        if id_in_use {
            return self.throw(ServerError::DuplicateReqId);
        }

        match &req.endpoint {
            XrpcEndpoint::CreateArbiter { user_did, arbiter_did, .. } => {
                self.create_arbiter(req.id, user_did.clone(), arbiter_did.clone())
            }
            XrpcEndpoint::DeleteArbiter { user_did, arbiter_did, resolver_depth } => {
                self.start_job(
                    req.id,
                    *resolver_depth,
                    user_did.clone(),
                    arbiter_did.clone(),
                    ADMIN_SPACE_KEY.to_string(),
                    JobArgs::DeleteArbiter,
                )
            }
            XrpcEndpoint::FetchMembers { user_did, arbiter_did, space_key, resolver_depth } => {
                self.start_job(
                    req.id,
                    *resolver_depth,
                    user_did.clone(),
                    arbiter_did.clone(),
                    space_key.clone(),
                    JobArgs::FetchMembers,
                )
            }
            XrpcEndpoint::CreateSpace { user_did, arbiter_did, space_key, resolver_depth } => {
                self.start_job(
                    req.id,
                    *resolver_depth,
                    user_did.clone(),
                    arbiter_did.clone(),
                    space_key.clone(),
                    JobArgs::CreateSpace,
                )
            }
            XrpcEndpoint::ConfigureSpace {
                user_did,
                arbiter_did,
                space_key,
                public_records,
                public_members,
                resolver_depth,
            } => self.start_job(
                req.id,
                *resolver_depth,
                user_did.clone(),
                arbiter_did.clone(),
                space_key.clone(),
                JobArgs::ConfigureSpace(SpaceConfig {
                    public_records: *public_records,
                    public_members: *public_members,
                }),
            ),
            XrpcEndpoint::DeleteSpace { user_did, arbiter_did, space_key, resolver_depth } => {
                self.start_job(
                    req.id,
                    *resolver_depth,
                    user_did.clone(),
                    arbiter_did.clone(),
                    space_key.clone(),
                    JobArgs::DeleteSpace,
                )
            }
            XrpcEndpoint::SetMemberAccess {
                user_did,
                arbiter_did,
                space_key,
                member,
                access,
                resolver_depth,
            } => self.start_job(
                req.id,
                *resolver_depth,
                user_did.clone(),
                arbiter_did.clone(),
                space_key.clone(),
                JobArgs::SetMemberAccess {
                    member: member.clone(),
                    access: *access,
                },
            ),
            XrpcEndpoint::RemoveMember {
                user_did,
                arbiter_did,
                space_key,
                member,
                resolver_depth,
            } => self.start_job(
                req.id,
                *resolver_depth,
                user_did.clone(),
                arbiter_did.clone(),
                space_key.clone(),
                JobArgs::RemoveMember {
                    member: member.clone(),
                },
            ),
        }
    }

    // -----------------------------------------------------------------------
    // Tick: process timeouts and advance time
    // -----------------------------------------------------------------------

    /// Tick the server to let it process timeouts and resolution steps.
    pub fn tick(&self) -> Self {
        // Find a timed out job, if one exists
        let timed_out = self.find_timed_out_job();

        if let Some((arbiter_did, job_id)) = timed_out {
            let arbiter = &self.arbiters[&arbiter_did];
            let upd_arbiter = arbiter.timeout_job(job_id);
            self.tick_internal()
                .handle_arbiter_result(upd_arbiter, 0)
        } else {
            self.tick_internal()
        }
    }

    /// Find the first timed-out job across all arbiters.
    fn find_timed_out_job(&self) -> Option<(ArbiterDid, JobId)> {
        for (arbiter_did, arbiter) in &self.arbiters {
            for job_id in arbiter.job_queue.keys() {
                if let Some(&start_time) = self.job_start_times.get(job_id)
                    && self.time - start_time >= TIMEOUT_TICKS
                {
                    return Some((arbiter_did.clone(), *job_id));
                }
            }
        }
        None
    }

    // -----------------------------------------------------------------------
    // Provide resolved member list for a remote space
    // -----------------------------------------------------------------------

    /// Provide the result of a remote server member list resolution.
    pub fn resolved_member_list(
        &self,
        arbiter_did: &ArbiterDid,
        req_id: ReqId,
        space_id: SpaceId,
        members: ResolvedMemberList,
    ) -> Self {
        if !self.arbiters.contains_key(arbiter_did) {
            return self.throw(ServerError::ArbiterNotExists);
        }

        let arbiter = &self.arbiters[arbiter_did];
        let upd_arbiter = arbiter.provide_remote_space_members(req_id, space_id, members);
        self.handle_arbiter_result(upd_arbiter, 0)
    }

    // -----------------------------------------------------------------------
    // Create arbiter
    // -----------------------------------------------------------------------

    /// Create a new arbiter on the server.
    fn create_arbiter(&self, _req_id: ReqId, user_did: UserDid, arbiter_did: ArbiterDid) -> Self {
        if self.arbiters.contains_key(&arbiter_did) {
            return self.throw(ServerError::ArbiterAlreadyExists);
        }

        let new_arbiter = Arbiter::new(arbiter_did.clone(), user_did);
        self.put_arbiter(arbiter_did, new_arbiter)
            .ok(HashSet::new())
    }

    // -----------------------------------------------------------------------
    // Start a job on an arbiter
    // -----------------------------------------------------------------------

    /// Start a job on the given arbiter.
    fn start_job(
        &self,
        req_id: ReqId,
        resolver_depth: i64,
        user_did: UserDid,
        arbiter_did: ArbiterDid,
        space_key: SpaceKey,
        job_args: JobArgs,
    ) -> Self {
        if !self.arbiters.contains_key(&arbiter_did) {
            return self.throw(ServerError::ArbiterNotExists);
        }

        let arbiter = &self.arbiters[&arbiter_did];
        let upd_arbiter = arbiter.start_job(user_did, space_key, req_id, job_args);
        self.handle_arbiter_result(upd_arbiter, resolver_depth)
    }

    // -----------------------------------------------------------------------
    // Handle arbiter result (translate to server feedback)
    // -----------------------------------------------------------------------

    /// Given an arbiter that has just been updated, translate its result into
    /// server feedback.
    fn handle_arbiter_result(&self, upd_arbiter: Arbiter, resolver_depth: i64) -> Self {
        let arbiter_did = upd_arbiter.did.clone();
        let result = upd_arbiter.result.clone();

        match result {
            ArbiterResult::Ok => {
                self.put_arbiter(arbiter_did, upd_arbiter)
                    .ok(HashSet::new())
            }

            ArbiterResult::QueuedJob {
                id,
                spaces_to_resolve,
            } => {
                if resolver_depth < 1 {
                    // Can't resolve remote spaces, timeout immediately
                    let timed_out = upd_arbiter.timeout_job(id);
                    let timed_out_result = timed_out.result.clone();
                    match timed_out_result {
                        ArbiterResult::Deleted => {
                            self.remove_arbiter(&arbiter_did)
                                .ok(HashSet::new())
                        }
                        ArbiterResult::FinishedJob {
                            id: finished_id,
                            result,
                        } => {
                            self.put_arbiter(arbiter_did, timed_out)
                                .remove_job_start_time(finished_id)
                                .ok({
                                    let mut fb = HashSet::new();
                                    fb.insert(Feedback::Respond(Response {
                                        id: finished_id,
                                        data: result,
                                    }));
                                    fb
                                })
                        }
                        ArbiterResult::Err(e) => {
                            self.put_arbiter(arbiter_did, timed_out)
                                .throw(ServerError::ArbiterErr(e))
                        }
                        _ => self.panic(),
                    }
                } else {
                    // We can do resolution
                    let feedback: HashSet<Feedback> = spaces_to_resolve
                        .into_iter()
                        .map(|space_id| {
                            Feedback::ResolveRemoteList {
                                req_id: id,
                                space: space_id,
                                resolver_depth: resolver_depth - 1,
                            }
                        })
                        .collect();

                    self.put_arbiter(arbiter_did, upd_arbiter)
                        .add_job_start_time(id, self.time)
                        .ok(feedback)
                }
            }

            ArbiterResult::FinishedJob { id, result } => {
                let mut fb = HashSet::new();
                fb.insert(Feedback::Respond(Response {
                    id,
                    data: result,
                }));
                self.put_arbiter(arbiter_did, upd_arbiter)
                    .remove_job_start_time(id)
                    .ok(fb)
            }

            ArbiterResult::Deleted => {
                self.remove_arbiter(&arbiter_did)
                    .ok(HashSet::new())
            }

            ArbiterResult::Err(e) => {
                self.put_arbiter(arbiter_did, upd_arbiter)
                    .throw(ServerError::ArbiterErr(e))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Display impls for ergonomic debugging
// ---------------------------------------------------------------------------

impl fmt::Display for Access {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Access::ReadMemberList => write!(f, "ReadMemberList"),
            Access::IsMember => write!(f, "IsMember"),
            Access::AddMembers => write!(f, "AddMembers"),
            Access::RemoveMembers => write!(f, "RemoveMembers"),
            Access::ConfigureSpace => write!(f, "ConfigureSpace"),
            Access::CreateSpaces => write!(f, "CreateSpaces"),
            Access::RemoveSpace => write!(f, "RemoveSpace"),
            Access::Owner => write!(f, "Owner"),
        }
    }
}

impl fmt::Display for Member {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Member::User(did) => write!(f, "User({did})"),
            Member::LocalSpace(k) => write!(f, "LocalSpace({k})"),
            Member::RemoteSpace(id) => write!(f, "RemoteSpace({}:{})", id.arbiter_did, id.space_key),
        }
    }
}

impl fmt::Display for SpaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.arbiter_did, self.space_key)
    }
}

impl fmt::Display for ArbiterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArbiterError::JobIdAlreadyUsed => write!(f, "JobIdAlreadyUsed"),
            ArbiterError::JobNotExists => write!(f, "JobNotExists"),
            ArbiterError::SpaceNotNeeded => write!(f, "SpaceNotNeeded"),
            ArbiterError::SpaceAlreadyResolved => write!(f, "SpaceAlreadyResolved"),
            ArbiterError::SpaceAlreadyExists => write!(f, "SpaceAlreadyExists"),
            ArbiterError::SpaceNotExists => write!(f, "SpaceNotExists"),
            ArbiterError::PermissionDenied => write!(f, "PermissionDenied"),
            ArbiterError::CannotDeleteAdminSpace => write!(f, "CannotDeleteAdminSpace"),
            ArbiterError::MemberNotExist => write!(f, "MemberNotExist"),
            ArbiterError::PermissionChanged => write!(f, "PermissionChanged"),
            ArbiterError::ArbiterDeletionMustSpecifyAdminSpace => {
                write!(f, "ArbiterDeletionMustSpecifyAdminSpace")
            }
            ArbiterError::WriteOperationAlreadyInProgress => {
                write!(f, "WriteOperationAlreadyInProgress")
            }
            ArbiterError::RemoteSpaceReferencesLocalArbiter => {
                write!(f, "RemoteSpaceReferencesLocalArbiter")
            }
            ArbiterError::OnlyLastOwnerCanDeleteArbiter => {
                write!(f, "OnlyLastOwnerCanDeleteArbiter")
            }
            ArbiterError::JobsTimedOut(ids) => write!(f, "JobsTimedOut({ids:?})"),
        }
    }
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerError::ArbiterAlreadyExists => write!(f, "ArbiterAlreadyExists"),
            ServerError::ArbiterNotExists => write!(f, "ArbiterNotExists"),
            ServerError::ArbiterErr(e) => write!(f, "ArbiterErr({e})"),
            ServerError::DuplicateReqId => write!(f, "DuplicateReqId"),
        }
    }
}



