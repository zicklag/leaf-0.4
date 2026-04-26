//! Implementation of the `arbiter_core` state machine from the Quint specification.
//!
//! This module handles role computation and resolution logic. It is designed to be wrapped
//! by the arbiter server that drives it.

use im::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

pub type ArbiterDid = String;
pub type UserDid = String;
pub type SpaceKey = String;
pub type JobId = i64;

/// The ID of the special `$admin` space that controls access to the arbiter itself.
pub const ADMIN_SPACE_KEY: &str = "$admin";

// ---------------------------------------------------------------------------
// Access
// ---------------------------------------------------------------------------

/// The access a member may have to a permissioned space.
///
/// All later permissions include the ones before. There are two "tiers":
/// space permissions and arbiter permissions. The arbiter permissions only have
/// effect when inherited into the `$admin` space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Access {
    ReadMemberList = 0,
    IsMember = 1,
    AddMembers = 2,
    RemoveMembers = 3,
    ConfigureSpace = 4,
    CreateSpaces = 5,
    RemoveSpace = 6,
    Owner = 7,
}

/// All access levels in order.
pub const ALL_ACCESSES: [Access; 8] = [
    Access::ReadMemberList,
    Access::IsMember,
    Access::AddMembers,
    Access::RemoveMembers,
    Access::ConfigureSpace,
    Access::CreateSpaces,
    Access::RemoveSpace,
    Access::Owner,
];

impl Access {
    /// Numeric level corresponding to this access.
    pub fn level(self) -> i64 {
        self as i64
    }

    /// Returns the minimum (lower level) of two accesses.
    pub fn min(self, other: Access) -> Access {
        if self.level() < other.level() { self } else { other }
    }

    /// Returns the maximum (higher level) of two accesses.
    pub fn max(self, other: Access) -> Access {
        if self.level() > other.level() { self } else { other }
    }

    /// Returns true if `self` includes `other` (i.e. `self` >= `other`).
    pub fn includes(self, other: Access) -> bool {
        self.level() >= other.level()
    }
}

// ---------------------------------------------------------------------------
// Member
// ---------------------------------------------------------------------------

/// A member in a space may be a user with a DID, another local space, or a remote space.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Member {
    User(UserDid),
    LocalSpace(SpaceKey),
    RemoteSpace(SpaceId),
}

// ---------------------------------------------------------------------------
// SpaceId
// ---------------------------------------------------------------------------

/// A reference to a space, qualified by the arbiter that owns it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpaceId {
    pub arbiter_did: ArbiterDid,
    pub space_key: SpaceKey,
}

// ---------------------------------------------------------------------------
// SpaceConfig
// ---------------------------------------------------------------------------

/// Extra configuration associated to a space.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpaceConfig {
    pub public_records: bool,
    pub public_members: bool,
}

// ---------------------------------------------------------------------------
// Space
// ---------------------------------------------------------------------------

/// A permissioned space.
#[derive(Debug, Clone, PartialEq)]
pub struct Space {
    pub config: SpaceConfig,
    pub members: HashMap<Member, Access>,
}

// ---------------------------------------------------------------------------
// UnresolvedMemberListItem / UnresolvedMemberList / ResolvedMemberList
// ---------------------------------------------------------------------------

/// An item in the unresolved member list: either a user or a remote space that must be resolved.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnresolvedMemberListItem {
    User(UserDid),
    Space(SpaceId),
}

/// A mapping from unresolved items to their access level.
pub type UnresolvedMemberList = HashMap<UnresolvedMemberListItem, Access>;

/// A fully resolved member list, plus any spaces that are still missing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResolvedMemberList {
    pub member_list: HashMap<UserDid, Access>,
    pub missing_spaces: HashMap<SpaceId, Access>,
}

impl ResolvedMemberList {
    /// Returns the set of users that have at least `IsMember` access.
    pub fn permissioned_space_members(&self) -> HashSet<UserDid> {
        self.member_list
            .iter()
            .filter(|&(_, access)| access.includes(Access::IsMember))
            .map(|(did, _)| did.clone())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// ArbiterError
// ---------------------------------------------------------------------------

/// Errors produced by the arbiter state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArbiterError {
    JobIdAlreadyUsed,
    JobNotExists,
    SpaceNotNeeded,
    SpaceAlreadyResolved,
    SpaceAlreadyExists,
    SpaceNotExists,
    PermissionDenied,
    CannotDeleteAdminSpace,
    MemberNotExist,
    PermissionChanged,
    ArbiterDeletionMustSpecifyAdminSpace,
    WriteOperationAlreadyInProgress,
    RemoteSpaceReferencesLocalArbiter,
    OnlyLastOwnerCanDeleteArbiter,
    JobsTimedOut(HashSet<JobId>),
}

// ---------------------------------------------------------------------------
// JobResult
// ---------------------------------------------------------------------------

/// The result of a finished job.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum JobResult {
    Ok,
    ResolvedMembersList(ResolvedMemberList),
}

// ---------------------------------------------------------------------------
// ArbiterResult
// ---------------------------------------------------------------------------

/// The result of an action on the arbiter state machine.
#[derive(Debug, Clone, PartialEq)]
pub enum ArbiterResult {
    Ok,
    QueuedJob {
        id: JobId,
        spaces_to_resolve: HashSet<SpaceId>,
    },
    FinishedJob {
        id: JobId,
        result: JobResult,
    },
    Deleted,
    Err(ArbiterError),
}

// ---------------------------------------------------------------------------
// JobArgs
// ---------------------------------------------------------------------------

/// Arguments for a job to perform some action on the arbiter.
#[derive(Debug, Clone, PartialEq)]
pub enum JobArgs {
    FetchMembers,
    CreateSpace,
    ConfigureSpace(SpaceConfig),
    DeleteSpace,
    SetMemberAccess {
        member: Member,
        access: Access,
    },
    RemoveMember {
        member: Member,
    },
    DeleteArbiter,
}

// ---------------------------------------------------------------------------
// Job
// ---------------------------------------------------------------------------

/// A work-in-progress action such as adding or removing a member.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub id: JobId,
    pub user_did: UserDid,
    pub space_key: SpaceKey,
    pub args: JobArgs,
    pub unresolved_members: UnresolvedMemberList,
    pub resolved_spaces: HashMap<SpaceId, ResolvedMemberList>,
    pub arbiter_version: i64,
}

// ---------------------------------------------------------------------------
// Arbiter (core state)
// ---------------------------------------------------------------------------

/// The core state of an arbiter.
#[derive(Debug, Clone, PartialEq)]
pub struct Arbiter {
    /// Monotonic version used for compare-and-swap updates.
    pub version: i64,
    /// The arbiter's DID.
    pub did: ArbiterDid,
    /// Spaces managed by the arbiter.
    pub spaces: HashMap<SpaceKey, Space>,
    /// Work-in-progress resolution jobs.
    pub job_queue: HashMap<JobId, Job>,
    /// The result of the last operation.
    pub result: ArbiterResult,
}

// ---------------------------------------------------------------------------
// Version arithmetic helpers
// ---------------------------------------------------------------------------

const VERSION_BITS: i64 = 32;
pub(crate) const VERSION_MAX: i64 = (1 << VERSION_BITS) - 1;

/// Increment the arbiter version with wrapping add.
fn next_version(a: &Arbiter) -> Arbiter {
    let mut a = a.clone();
    a.version = (a.version + 1) % (1 << VERSION_BITS);
    a
}

/// Calculate how many version increments are needed before `job.arbiter_version` collides
/// with `a.version` (wrapping around).
pub(crate) fn version_diff(a: &Arbiter, job: &Job) -> i64 {
    if a.version < job.arbiter_version {
        job.arbiter_version - a.version
    } else if a.version > job.arbiter_version {
        VERSION_MAX - a.version + job.arbiter_version
    } else {
        VERSION_MAX
    }
}

// ---------------------------------------------------------------------------
// Pure helper functions matching Quint definitions
// ---------------------------------------------------------------------------

/// Get the smaller of the provided accesses.
pub fn min_access(a: Access, b: Access) -> Access {
    a.min(b)
}

/// Get the larger of the provided accesses.
pub fn max_access(a: Access, b: Access) -> Access {
    a.max(b)
}

/// Whether access `a` includes access `b`.
pub fn includes_access(a: Access, b: Access) -> bool {
    a.includes(b)
}

/// Filter out the remote spaces from a member list.
pub fn remote_spaces_from_members(s: &HashSet<Member>) -> HashSet<SpaceId> {
    s.iter()
        .filter_map(|m| match m {
            Member::RemoteSpace(id) => Some(id.clone()),
            _ => None,
        })
        .collect()
}

/// Filter out the users from a member list.
pub fn local_users_from_members(s: &HashSet<Member>) -> HashSet<UserDid> {
    s.iter()
        .filter_map(|m| match m {
            Member::User(u) => Some(u.clone()),
            _ => None,
        })
        .collect()
}

/// Filter out the local spaces from a member list.
pub fn local_spaces_from_members(s: &HashSet<Member>) -> HashSet<SpaceKey> {
    s.iter()
        .filter_map(|m| match m {
            Member::LocalSpace(k) => Some(k.clone()),
            _ => None,
        })
        .collect()
}

/// Synchronously resolve a member list if it contains no remote spaces.
pub fn resolve_sync(unresolved: &UnresolvedMemberList) -> Option<ResolvedMemberList> {
    if unresolved.keys().all(|item| matches!(item, UnresolvedMemberListItem::User(_))) {
        let member_list: HashMap<UserDid, Access> = unresolved
            .iter()
            .map(|(item, &access)| match item {
                UnresolvedMemberListItem::User(u) => (u.clone(), access),
                _ => unreachable!(),
            })
            .collect();
        Some(ResolvedMemberList {
            member_list,
            missing_spaces: HashMap::new(),
        })
    } else {
        None
    }
}

/// Get the list of spaces that need resolution from an unresolved member list.
pub fn spaces_to_resolve(unresolved: &UnresolvedMemberList) -> HashSet<SpaceId> {
    unresolved
        .keys()
        .filter_map(|item| match item {
            UnresolvedMemberListItem::Space(id) => Some(id.clone()),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Arbiter constructors and actions
// ---------------------------------------------------------------------------

impl Arbiter {
    /// Create a new arbiter with the given DID and initial owner.
    pub fn new(arbiter_did: ArbiterDid, user_did: UserDid) -> Self {
        let mut spaces = HashMap::new();
        let mut admin_members = HashMap::new();
        admin_members.insert(Member::User(user_did), Access::Owner);

        spaces.insert(
            ADMIN_SPACE_KEY.to_string(),
            Space {
                config: SpaceConfig::default(),
                members: admin_members,
            },
        );

        Arbiter {
            version: 0,
            did: arbiter_did,
            job_queue: HashMap::new(),
            spaces,
            result: ArbiterResult::Ok,
        }
    }

    /// Set the result to an error (used internally).
    fn throw(&self, err: ArbiterError) -> Self {
        let mut a = self.clone();
        a.result = ArbiterResult::Err(err);
        a
    }

    /// Remove a job from the job queue and set the result to an error.
    fn throw_job_err(&self, err: ArbiterError, job_id: JobId) -> Self {
        let mut a = self.clone();
        a.job_queue = a.job_queue.without(&job_id);
        a.result = ArbiterResult::Err(err);
        a
    }

    /// Remove a job from the queue.
    fn remove_job(&self, job_id: JobId) -> Self {
        let mut a = self.clone();
        a.job_queue = a.job_queue.without(&job_id);
        a
    }

    // -----------------------------------------------------------------------
    // Start a new job
    // -----------------------------------------------------------------------

    /// Process a new job.
    ///
    /// NOTE: `user_did` is required to be authenticated by the wrapper as having
    /// actually come from that DID.
    pub fn start_job(
        &self,
        user_did: UserDid,
        space_key: SpaceKey,
        job_id: JobId,
        args: JobArgs,
    ) -> Self {
        // Check Job ID not already used
        if self.job_queue.contains_key(&job_id) {
            return self.throw(ArbiterError::JobIdAlreadyUsed);
        }

        // Check for colliding jobs that need timeout to avoid version collision
        let colliding_jobs: HashSet<JobId> = self
            .job_queue
            .keys()
            .filter(|&jid| {
                let job = &self.job_queue[jid];
                version_diff(self, job) == 1
            })
            .cloned()
            .collect();

        if !colliding_jobs.is_empty() {
            let mut a = self.clone();
            // Remove all colliding jobs
            for jid in &colliding_jobs {
                a.job_queue = a.job_queue.without(jid);
            }
            a.result = ArbiterResult::Err(ArbiterError::JobsTimedOut(colliding_jobs));
            return a;
        }

        // Validate the job
        if let Some(err) = job_validation_error(self, &space_key, &args) {
            return self.throw(err);
        }

        // Get the unresolved member list
        let unresolved_members = self.members(&space_key);

        // Try to resolve synchronously
        if let Some(resolved) = resolve_sync(&unresolved_members) {
            let job = Job {
                id: job_id,
                user_did,
                space_key,
                args,
                unresolved_members,
                resolved_spaces: HashMap::new(),
                arbiter_version: self.version,
            };
            return execute_job(self, resolved, &job);
        }

        // We need to queue the job.
        // Check for existing write job from this user.
        let user_has_write_job = self.job_queue.keys().any(|jid| {
            let job = &self.job_queue[jid];
            job.user_did == user_did
                && !matches!(job.args, JobArgs::FetchMembers)
        });

        if user_has_write_job {
            return self.throw(ArbiterError::WriteOperationAlreadyInProgress);
        }

        // Queue the job
        let mut a = next_version(self);
        let spaces_to_resolve_set = spaces_to_resolve(&unresolved_members);
        a.job_queue = a.job_queue.update(
            job_id,
            Job {
                id: job_id,
                user_did,
                space_key,
                args,
                unresolved_members,
                resolved_spaces: HashMap::new(),
                arbiter_version: self.version,
            },
        );
        a.result = ArbiterResult::QueuedJob {
            id: job_id,
            spaces_to_resolve: spaces_to_resolve_set,
        };
        a
    }

    // -----------------------------------------------------------------------
    // Provide remote space members
    // -----------------------------------------------------------------------

    /// Provide the resolved space member list for a job waiting on resolution.
    ///
    /// NOTE: This action is assumed to be called by a trusted wrapper.
    /// The resolved member list must be validated using mechanisms like HTTPS.
    pub fn provide_remote_space_members(
        &self,
        job_id: JobId,
        space_id: SpaceId,
        members: ResolvedMemberList,
    ) -> Self {
        // Check job exists
        if !self.job_queue.contains_key(&job_id) {
            return self.throw(ArbiterError::JobNotExists);
        }

        let job = &self.job_queue[&job_id];

        // Check that this job actually needs the space
        if !job
            .unresolved_members
            .contains_key(&UnresolvedMemberListItem::Space(space_id.clone()))
        {
            return self.throw(ArbiterError::SpaceNotNeeded);
        }

        // Check that we haven't already resolved the space
        if job.resolved_spaces.contains_key(&space_id) {
            return self.throw(ArbiterError::SpaceAlreadyResolved);
        }

        // Create updated resolved spaces
        let mut resolved_spaces = job.resolved_spaces.clone();
        resolved_spaces.insert(space_id, members);

        // Check whether all spaces are resolved
        let unresolved_spaces = spaces_to_resolve(&job.unresolved_members);
        let job_is_ready = unresolved_spaces
            .difference(job.resolved_spaces.keys().cloned().collect())
            .is_empty();

        if job_is_ready {
            let resolved = join_resolved_spaces(&job.unresolved_members, &resolved_spaces);
            let updated_job = Job {
                resolved_spaces,
                ..job.clone()
            };
            execute_job(self, resolved, &updated_job)
        } else {
            let mut a = next_version(self);
            a.job_queue = a.job_queue.update(
                job_id,
                Job {
                    resolved_spaces,
                    ..job.clone()
                },
            );
            a.result = ArbiterResult::Ok;
            a
        }
    }

    // -----------------------------------------------------------------------
    // Timeout a job
    // -----------------------------------------------------------------------

    /// Timeout a job in the queue. Executes the job without waiting for
    /// remote member lists to finish resolving.
    pub fn timeout_job(&self, job_id: JobId) -> Self {
        // Check job exists
        if !self.job_queue.contains_key(&job_id) {
            return self.throw(ArbiterError::JobNotExists);
        }

        let job = &self.job_queue[&job_id];
        let resolved = join_resolved_spaces(&job.unresolved_members, &job.resolved_spaces);
        execute_job(self, resolved, job)
    }

    // -----------------------------------------------------------------------
    // Compute the member list for a space (breadth-first traversal)
    // -----------------------------------------------------------------------

    /// Calculate the member list synchronously without resolving remote spaces.
    ///
    /// If the returned list contains remote spaces they will need to be resolved
    /// to complete the effective member list.
    pub fn members(&self, space_key: &str) -> UnresolvedMemberList {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        struct QueuedSpace {
            space_key: SpaceKey,
            access: Access,
        }

        let space_count = self.spaces.len() as i64;
        let access_count = ALL_ACCESSES.len() as i64;
        let recursion_depth = space_count * access_count;

        let mut state = {
            let member_list: UnresolvedMemberList = HashMap::new();
            let mut queue = HashSet::new();
            queue.insert(QueuedSpace {
                space_key: ADMIN_SPACE_KEY.to_string(),
                access: Access::Owner,
            });
            queue.insert(QueuedSpace {
                space_key: space_key.to_string(),
                access: Access::Owner,
            });

            (member_list, HashSet::new(), queue)
        };

        for _ in 0..recursion_depth {
            // Take the current queue
            let current_queue = std::mem::take(&mut state.2);
            if current_queue.is_empty() {
                state.2 = current_queue;
                break;
            }

            for qs in current_queue {
                // If this space doesn't exist, skip it and mark visited
                if !self.spaces.contains_key(&qs.space_key) {
                    state.1.insert(qs);
                    continue;
                }

                let space = &self.spaces[&qs.space_key];

                // Collect local users
                let local_users_set: HashSet<Member> = space.members.keys().cloned().collect();
                let local_users = local_users_from_members(&local_users_set);

                // Collect local child spaces
                let local_spaces: Vec<QueuedSpace> = space
                    .members
                    .iter()
                    .filter_map(|(member, &child_access)| match member {
                        Member::LocalSpace(k) => Some(QueuedSpace {
                            space_key: k.clone(),
                            access: min_access(child_access, qs.access),
                        }),
                        _ => None,
                    })
                    .collect();

                // Collect remote spaces
                let remote_spaces: HashMap<SpaceId, Access> = space
                    .members
                    .iter()
                    .filter_map(|(member, &child_access)| match member {
                        Member::RemoteSpace(id) => {
                            Some((id.clone(), min_access(child_access, qs.access)))
                        }
                        _ => None,
                    })
                    .collect();

                // Mark visited
                state.1.insert(qs.clone());

                // Add local spaces to queue (if not visited with >= access)
                for lqs in local_spaces {
                    let already_visited = state.1.iter().any(|v: &QueuedSpace| {
                        v.space_key == lqs.space_key
                            && v.access.level() >= lqs.access.level()
                    });
                    if !already_visited {
                        state.2.insert(lqs);
                    }
                }

                // Add users to member list
                for user_did in &local_users {
                    let user_access_in_space = space.members[&Member::User(user_did.clone())];
                    let space_limited_access = min_access(qs.access, user_access_in_space);

                    let uml_user = UnresolvedMemberListItem::User(user_did.clone());
                    let entry = state.0.entry(uml_user).or_insert(Access::ReadMemberList);
                    *entry = max_access(*entry, space_limited_access);
                }

                // Add remote spaces to member list
                for (space_id, access) in &remote_spaces {
                    let uml_space = UnresolvedMemberListItem::Space(space_id.clone());
                    let entry = state.0.entry(uml_space).or_insert(Access::ReadMemberList);
                    *entry = max_access(*entry, *access);
                }
            }
        }

        state.0
    }

    // -----------------------------------------------------------------------
    // Invariants
    // -----------------------------------------------------------------------

    /// Core arbiter invariants.
    pub fn invariants(&self) -> bool {
        self.inv_arbiter_space_always_exists()
            && self.inv_arbiter_has_at_least_one_owner()
            && self.inv_remote_space_member_does_not_reference_local_arbiter()
    }

    /// The special `$admin` space always exists.
    pub fn inv_arbiter_space_always_exists(&self) -> bool {
        self.spaces.contains_key(ADMIN_SPACE_KEY)
    }

    /// There is at least one owner in the `$admin` space.
    pub fn inv_arbiter_has_at_least_one_owner(&self) -> bool {
        if let Some(space) = self.spaces.get(ADMIN_SPACE_KEY) {
            space.members.values().any(|&access| access == Access::Owner)
        } else {
            false
        }
    }

    /// No remote space members reference the local arbiter.
    pub fn inv_remote_space_member_does_not_reference_local_arbiter(&self) -> bool {
        self.spaces.values().all(|space| {
            !space.members.keys().any(|member| match member {
                Member::RemoteSpace(id) => id.arbiter_did == self.did,
                _ => false,
            })
        })
    }
}

// ---------------------------------------------------------------------------
// Job validation
// ---------------------------------------------------------------------------

/// Return any validation errors for the job if there are issues.
fn job_validation_error(
    arbiter: &Arbiter,
    space_key: &SpaceKey,
    args: &JobArgs,
) -> Option<ArbiterError> {
    match args {
        JobArgs::FetchMembers => {
            if !arbiter.spaces.contains_key(space_key) {
                Some(ArbiterError::SpaceNotExists)
            } else {
                None
            }
        }
        JobArgs::CreateSpace => {
            if arbiter.spaces.contains_key(space_key) {
                Some(ArbiterError::SpaceAlreadyExists)
            } else {
                None
            }
        }
        JobArgs::ConfigureSpace(_) => {
            if !arbiter.spaces.contains_key(space_key) {
                Some(ArbiterError::SpaceNotExists)
            } else {
                None
            }
        }
        JobArgs::DeleteSpace => {
            if space_key == ADMIN_SPACE_KEY {
                Some(ArbiterError::CannotDeleteAdminSpace)
            } else if !arbiter.spaces.contains_key(space_key) {
                Some(ArbiterError::SpaceNotExists)
            } else {
                None
            }
        }
        JobArgs::SetMemberAccess { member, .. } => {
            if !arbiter.spaces.contains_key(space_key) {
                Some(ArbiterError::SpaceNotExists)
            } else if let Member::RemoteSpace(id) = member {
                if id.arbiter_did == arbiter.did {
                    Some(ArbiterError::RemoteSpaceReferencesLocalArbiter)
                } else {
                    None
                }
            } else {
                None
            }
        }
        JobArgs::RemoveMember { member } => {
            if !arbiter.spaces.contains_key(space_key) {
                Some(ArbiterError::SpaceNotExists)
            } else {
                let space = &arbiter.spaces[space_key];
                if !space.members.contains_key(member) {
                    Some(ArbiterError::MemberNotExist)
                } else {
                    None
                }
            }
        }
        JobArgs::DeleteArbiter => {
            if space_key != ADMIN_SPACE_KEY {
                Some(ArbiterError::ArbiterDeletionMustSpecifyAdminSpace)
            } else if let Some(space) = arbiter.spaces.get(space_key) {
                if space.members.len() > 1 {
                    Some(ArbiterError::OnlyLastOwnerCanDeleteArbiter)
                } else {
                    None
                }
            } else {
                Some(ArbiterError::SpaceNotExists)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Job execution dispatch
// ---------------------------------------------------------------------------

/// Finish the execution of a job, applying changes or erroring.
pub(crate) fn execute_job(arbiter: &Arbiter, resolved: ResolvedMemberList, job: &Job) -> Arbiter {
    match &job.args {
        JobArgs::FetchMembers => fetch_members(arbiter, resolved, job),
        JobArgs::CreateSpace => create_space(arbiter, resolved, job),
        JobArgs::ConfigureSpace(config) => configure_space(arbiter, resolved, job, config),
        JobArgs::DeleteSpace => delete_space(arbiter, resolved, job),
        JobArgs::SetMemberAccess { member, access } => {
            set_member_access(arbiter, resolved, job, member, *access)
        }
        JobArgs::RemoveMember { member } => remove_member(arbiter, resolved, job, member),
        JobArgs::DeleteArbiter => delete_arbiter(arbiter, resolved, job),
    }
}

// ---------------------------------------------------------------------------
// Individual job execution functions
// ---------------------------------------------------------------------------

fn check_permissions_and_version<'a>(
    arbiter: &'a Arbiter,
    resolved: &ResolvedMemberList,
    job: &Job,
) -> Result<(&'a Space, Access), Box<Arbiter>> {
    let user_in_list = resolved.member_list.contains_key(&job.user_did);
    if !user_in_list {
        return Err(Box::new(arbiter.throw_job_err(ArbiterError::PermissionDenied, job.id)));
    }

    if arbiter.version != job.arbiter_version {
        return Err(Box::new(arbiter.throw_job_err(ArbiterError::PermissionChanged, job.id)));
    }

    let user_access = resolved.member_list[&job.user_did];

    if !arbiter.spaces.contains_key(&job.space_key) {
        return Err(Box::new(arbiter.throw_job_err(ArbiterError::SpaceNotExists, job.id)));
    }

    let space = &arbiter.spaces[&job.space_key];
    Ok((space, user_access))
}

fn fetch_members(arbiter: &Arbiter, resolved: ResolvedMemberList, job: &Job) -> Arbiter {
    // Check space exists
    if !arbiter.spaces.contains_key(&job.space_key) {
        return arbiter.throw_job_err(ArbiterError::SpaceNotExists, job.id);
    }

    let space = &arbiter.spaces[&job.space_key];
    let member_access = resolved.member_list.get(&job.user_did).copied();

    let has_public_members = space.config.public_members;
    let has_access = member_access
        .map(|a| a.includes(Access::ReadMemberList))
        .unwrap_or(false)
        && resolved.member_list.contains_key(&job.user_did);

    if !has_public_members && !has_access {
        return arbiter.throw_job_err(ArbiterError::PermissionDenied, job.id);
    }

    let mut a = arbiter.remove_job(job.id);
    a.result = ArbiterResult::FinishedJob {
        id: job.id,
        result: JobResult::ResolvedMembersList(resolved),
    };
    a
}

fn create_space(arbiter: &Arbiter, resolved: ResolvedMemberList, job: &Job) -> Arbiter {
    // Check that the user is in the member list
    if !resolved.member_list.contains_key(&job.user_did) {
        return arbiter.throw_job_err(ArbiterError::PermissionDenied, job.id);
    }

    let user_access = resolved.member_list[&job.user_did];

    // Check that arbiter has not been modified since the job was started
    if arbiter.version != job.arbiter_version {
        return arbiter.throw_job_err(ArbiterError::PermissionChanged, job.id);
    }

    // Check space doesn't already exist
    if arbiter.spaces.contains_key(&job.space_key) {
        return arbiter.throw_job_err(ArbiterError::SpaceAlreadyExists, job.id);
    }

    // Check that the user has access to create spaces
    if !user_access.includes(Access::CreateSpaces) {
        return arbiter.throw_job_err(ArbiterError::PermissionDenied, job.id);
    }

    let mut a = next_version(arbiter);
    a.job_queue = a.job_queue.without(&job.id);
    a.spaces = a.spaces.update(
        job.space_key.clone(),
        Space {
            config: SpaceConfig::default(),
            members: HashMap::new(),
        },
    );
    a.result = ArbiterResult::FinishedJob {
        id: job.id,
        result: JobResult::Ok,
    };
    a
}

fn configure_space(
    arbiter: &Arbiter,
    resolved: ResolvedMemberList,
    job: &Job,
    config: &SpaceConfig,
) -> Arbiter {
    let check = check_permissions_and_version(arbiter, &resolved, job);
    let (_, user_access) = match check {
        Ok(v) => v,
        Err(a) => return *a,
    };

    if !user_access.includes(Access::ConfigureSpace) {
        return arbiter.throw_job_err(ArbiterError::PermissionDenied, job.id);
    }

    let space = &arbiter.spaces[&job.space_key];
    let mut a = next_version(arbiter);
    a.job_queue = a.job_queue.without(&job.id);
    a.spaces = a.spaces.update(
        job.space_key.clone(),
        Space {
            config: config.clone(),
            members: space.members.clone(),
        },
    );
    a.result = ArbiterResult::FinishedJob {
        id: job.id,
        result: JobResult::Ok,
    };
    a
}

fn delete_space(arbiter: &Arbiter, resolved: ResolvedMemberList, job: &Job) -> Arbiter {
    let check = check_permissions_and_version(arbiter, &resolved, job);
    let (_, user_access) = match check {
        Ok(v) => v,
        Err(a) => return *a,
    };

    if job.space_key == ADMIN_SPACE_KEY {
        return arbiter.throw_job_err(ArbiterError::CannotDeleteAdminSpace, job.id);
    }

    if !user_access.includes(Access::RemoveSpace) {
        return arbiter.throw_job_err(ArbiterError::PermissionDenied, job.id);
    }

    let mut a = next_version(arbiter);
    a.job_queue = a.job_queue.without(&job.id);
    a.spaces = a.spaces.without(&job.space_key);
    a.result = ArbiterResult::FinishedJob {
        id: job.id,
        result: JobResult::Ok,
    };
    a
}

fn set_member_access(
    arbiter: &Arbiter,
    resolved: ResolvedMemberList,
    job: &Job,
    updated_member: &Member,
    updated_member_access: Access,
) -> Arbiter {
    let check = check_permissions_and_version(arbiter, &resolved, job);
    let (space, user_access) = match check {
        Ok(v) => v,
        Err(a) => return *a,
    };

    if !user_access.includes(Access::AddMembers) {
        return arbiter.throw_job_err(ArbiterError::PermissionDenied, job.id);
    }

    // Cannot grant access higher than your own
    if !user_access.includes(updated_member_access) {
        return arbiter.throw_job_err(ArbiterError::PermissionDenied, job.id);
    }

    let updated_member_exists = space.members.contains_key(updated_member);
    if updated_member_exists {
        let existing_access = space.members[updated_member];

        // Can't modify existing member without RemoveMembers
        if !user_access.includes(Access::RemoveMembers) {
            return arbiter.throw_job_err(ArbiterError::PermissionDenied, job.id);
        }

        // Can't modify a member with higher access than you
        if existing_access.level() > user_access.level() {
            return arbiter.throw_job_err(ArbiterError::PermissionDenied, job.id);
        }
    }

    let mut a = next_version(arbiter);
    a.job_queue = a.job_queue.without(&job.id);
    let mut new_members = space.members.clone();
    new_members.insert(updated_member.clone(), updated_member_access);
    a.spaces = a.spaces.update(
        job.space_key.clone(),
        Space {
            config: space.config.clone(),
            members: new_members,
        },
    );
    a.result = ArbiterResult::FinishedJob {
        id: job.id,
        result: JobResult::Ok,
    };
    a
}

fn remove_member(
    arbiter: &Arbiter,
    resolved: ResolvedMemberList,
    job: &Job,
    removed_member: &Member,
) -> Arbiter {
    let check = check_permissions_and_version(arbiter, &resolved, job);
    let (space, user_access) = match check {
        Ok(v) => v,
        Err(a) => return *a,
    };

    if !user_access.includes(Access::RemoveMembers) {
        return arbiter.throw_job_err(ArbiterError::PermissionDenied, job.id);
    }

    let removed_member_access = space.members[removed_member];
    if removed_member_access.level() > user_access.level() {
        return arbiter.throw_job_err(ArbiterError::PermissionDenied, job.id);
    }

    let mut a = next_version(arbiter);
    a.job_queue = a.job_queue.without(&job.id);
    let mut new_members = space.members.clone();
    new_members.remove(removed_member);
    a.spaces = a.spaces.update(
        job.space_key.clone(),
        Space {
            config: space.config.clone(),
            members: new_members,
        },
    );
    a.result = ArbiterResult::FinishedJob {
        id: job.id,
        result: JobResult::Ok,
    };
    a
}

fn delete_arbiter(arbiter: &Arbiter, resolved: ResolvedMemberList, job: &Job) -> Arbiter {
    // Check that the user is in the member list
    if !resolved.member_list.contains_key(&job.user_did) {
        return arbiter.throw_job_err(ArbiterError::PermissionDenied, job.id);
    }

    let user_access = resolved.member_list[&job.user_did];

    // Check that arbiter has not been modified since the job was started
    if arbiter.version != job.arbiter_version {
        return arbiter.throw_job_err(ArbiterError::PermissionChanged, job.id);
    }

    if !arbiter.spaces.contains_key(&job.space_key) {
        return arbiter.throw_job_err(ArbiterError::SpaceNotExists, job.id);
    }

    let space = &arbiter.spaces[&job.space_key];

    if job.space_key != ADMIN_SPACE_KEY {
        return arbiter.throw_job_err(ArbiterError::ArbiterDeletionMustSpecifyAdminSpace, job.id);
    }

    if !user_access.includes(Access::Owner) {
        return arbiter.throw_job_err(ArbiterError::PermissionDenied, job.id);
    }

    if space.members.len() > 1 {
        return arbiter.throw_job_err(ArbiterError::OnlyLastOwnerCanDeleteArbiter, job.id);
    }

    let mut a = arbiter.clone();
    a.result = ArbiterResult::Deleted;
    a
}

// ---------------------------------------------------------------------------
// join_resolved_spaces
// ---------------------------------------------------------------------------

/// Takes an unresolved member list and a map of resolved member lists for external
/// spaces, and returns the final fully-resolved member list.
///
/// If not all remote spaces are resolved, the missing spaces are returned too.
pub fn join_resolved_spaces(
    unresolved: &UnresolvedMemberList,
    resolved_spaces: &HashMap<SpaceId, ResolvedMemberList>,
) -> ResolvedMemberList {
    let mut member_list: HashMap<UserDid, Access> = HashMap::new();
    let mut missing_spaces: HashMap<SpaceId, Access> = HashMap::new();

    for (list_item, &member_access) in unresolved.iter() {
        match list_item {
            UnresolvedMemberListItem::User(user_did) => {
                let entry = member_list.entry(user_did.clone()).or_insert(Access::ReadMemberList);
                *entry = max_access(*entry, member_access);
            }
            UnresolvedMemberListItem::Space(space_id) => {
                if let Some(resolved_space) = resolved_spaces.get(space_id) {
                    // Merge missing spaces from the resolved space
                    for (ms_id, &ms_access) in &resolved_space.missing_spaces {
                        let effective = min_access(ms_access, member_access);
                        let entry = missing_spaces.entry(ms_id.clone()).or_insert(Access::ReadMemberList);
                        *entry = max_access(*entry, effective);
                    }
                    // Merge members from the resolved space
                    for (user_did, &user_access) in &resolved_space.member_list {
                        let effective = min_access(user_access, member_access);
                        let entry = member_list.entry(user_did.clone()).or_insert(Access::ReadMemberList);
                        *entry = max_access(*entry, effective);
                    }
                } else {
                    // Space not resolved => add to missing list
                    let entry = missing_spaces
                        .entry(space_id.clone())
                        .or_insert(Access::ReadMemberList);
                    *entry = max_access(*entry, member_access);
                }
            }
        }
    }

    ResolvedMemberList {
        member_list,
        missing_spaces,
    }
}
