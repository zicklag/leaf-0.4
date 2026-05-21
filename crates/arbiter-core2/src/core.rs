//! Data model and state machine for the arbiter core.
//!
//! Contains the core types (Arbiter, Space, Member, Job) and the state machine
//! that wraps the Rego policy engine for authorization.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub use crate::policy::*;

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

pub type Did = String;
pub type SpaceKey = String;
pub type JobId = i64;
pub type ArbiterVersion = u64;

pub const ADMIN_SPACE_KEY: &str = "$admin";

// ---------------------------------------------------------------------------
// Member types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceId {
    pub arbiter_did: Did,
    pub space_key: SpaceKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "tag", content = "value")]
pub enum Member {
    #[serde(rename = "MemberDid")]
    MemberDid(Did),
    #[serde(rename = "MemberLocalSpace")]
    MemberLocalSpace(SpaceKey),
    #[serde(rename = "MemberRemoteSpace")]
    MemberRemoteSpace(SpaceId),
}

// ---------------------------------------------------------------------------
// Config types
// ---------------------------------------------------------------------------

pub type ArbiterConfig = serde_json::Value;
pub type SpaceConfig = serde_json::Value;
pub type MemberAccess = serde_json::Value;

// ---------------------------------------------------------------------------
// Job types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum JobArgs {
    ResolveMembers,
    CreateSpace { space_type: String, config: SpaceConfig },
    SetSpaceConfig { space_type: String, config: SpaceConfig },
    DeleteSpace,
    SetSpaceMemberAccess { member: Member, access: serde_json::Value },
    RemoveSpaceMember { member: Member },
    DeleteArbiter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobResult {
    Ok,
    ResolvedMembersList(serde_json::Value),
}

/// A remote space that still needs async resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolutionEntry {
    pub remote_arbiter_did: Did,
    pub space_key: SpaceKey,
}

// ---------------------------------------------------------------------------
// Space
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Space {
    pub space_type: String,
    pub config: SpaceConfig,
    pub members: im::HashMap<Member, MemberAccess>,
}

impl Space {
    pub fn new(space_type: String, config: SpaceConfig) -> Self {
        Self {
            space_type,
            config,
            members: im::HashMap::new(),
        }
    }

    pub fn admin_space() -> Self {
        Self::new(
            lexicon::CONFIG_SPACE.to_string(),
            serde_json::json!({
                "$type": lexicon::CONFIG_SPACE,
                "publicRecords": false,
                "publicMembers": false,
            }),
        )
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArbiterErrorKind {
    JobIdExists,
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
    JobsTimedOut(im::HashSet<JobId>),
    InvalidConfig,
    UnsupportedConfigLexicon,
    ArbiterAlreadyExists,
    RaceCondition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbiterError {
    pub kind: ArbiterErrorKind,
    pub job_id: Option<JobId>,
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

/// The result of processing an operation on an arbiter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArbiterResult {
    /// Operation succeeded.
    Ok,
    /// A job was queued and needs remote space resolution.
    NeedsResolution {
        job_id: JobId,
        spaces: Vec<ResolutionEntry>,
    },
    /// A job finished with a result.
    Finished(JobResult),
    /// The arbiter was deleted.
    Deleted,
    /// An error occurred.
    Err(ArbiterError),
}

// ---------------------------------------------------------------------------
// Job
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub user_did: Did,
    pub space_key: SpaceKey,
    pub args: JobArgs,
    /// Version of the arbiter when this job was started.
    pub arbiter_version: ArbiterVersion,
}

// ---------------------------------------------------------------------------
// Arbiter
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arbiter {
    pub version: ArbiterVersion,
    pub did: Did,
    pub config: ArbiterConfig,
    pub spaces: im::HashMap<SpaceKey, Space>,
    pub job_queue: im::HashMap<JobId, Job>,
    pub result: ArbiterResult,
}

impl Arbiter {
    /// Create a new arbiter with the given DID, initial owner, and config.
    ///
    /// The config must contain a valid Rego policy at `config.policy`.
    pub fn new(did: Did, owner_did: Did, config: ArbiterConfig) -> Result<Self, PolicyError> {
        // Validate the config has a valid policy
        let policy = extract_policy(&config)?;
        // Quick validation that the policy parses
        validate_policy(policy)?;

        let mut admin = Space::admin_space();
        admin.members.insert(
            Member::MemberDid(owner_did),
            serde_json::json!({
                "$type": lexicon::CONFIG_ACCESS_LEVEL,
                "level": "Owner",
            }),
        );

        let mut spaces = im::HashMap::new();
        spaces.insert(ADMIN_SPACE_KEY.to_string(), admin);

        Ok(Self {
            version: 0,
            did,
            config,
            spaces,
            job_queue: im::HashMap::new(),
            result: ArbiterResult::Ok,
        })
    }

    /// Process an operation on the arbiter.
    ///
    /// Takes a snapshot of the arbiter's spaces, evaluates the policy,
    /// and either executes the operation immediately or queues a job
    /// if remote resolution is needed.
    pub fn process_operation(
        &mut self,
        user_did: &str,
        space_key: &str,
        args: JobArgs,
        resolved_remotes: &serde_json::Value,
    ) {
        // 1. Structural validation
        if let Some(err) = validate_operation(self, space_key, &args) {
            self.result = ArbiterResult::Err(err);
            return;
        }

        // 2. Map args to policy action
        let action = args_to_action(&args);

        // 3. Build policy params
        let params = args_to_policy_params(&args);

        // 4. Evaluate policy with snapshot
        match self.evaluate_policy(action, user_did, space_key, params.as_ref(), resolved_remotes) {
            PolicyOutcome::NeedsResolution(spaces) => {
                // Queue the job
                let job_id = self.next_job_id();
                self.job_queue = self.job_queue.update(job_id, Job {
                    id: job_id,
                    user_did: user_did.to_string(),
                    space_key: space_key.to_string(),
                    args,
                    arbiter_version: self.next_version(),
                });
                self.result = ArbiterResult::NeedsResolution {
                    job_id,
                    spaces,
                };
            }
            PolicyOutcome::Denied => {
                self.result = ArbiterResult::Err(ArbiterError {
                    kind: ArbiterErrorKind::PermissionDenied,
                    job_id: None,
                });
            }
            PolicyOutcome::Allowed => {
                self.execute_operation(user_did, space_key, args, resolved_remotes);
            }
            PolicyOutcome::Error(_e) => {
                self.result = ArbiterResult::Err(ArbiterError {
                    kind: ArbiterErrorKind::InvalidConfig,
                    job_id: None,
                });
            }
        }
    }

    /// Provide resolved remote members for a queued job.
    ///
    /// Re-evaluates the policy with the updated resolved_remotes.
    pub fn provide_resolved_remotes(
        &mut self,
        job_id: JobId,
        resolved_remotes: &serde_json::Value,
    ) {
        let job = match self.job_queue.get(&job_id) {
            Some(j) => j.clone(),
            None => {
                self.result = ArbiterResult::Err(ArbiterError {
                    kind: ArbiterErrorKind::JobNotExists,
                    job_id: Some(job_id),
                });
                return;
            }
        };

        let action = args_to_action(&job.args);

        match self.evaluate_policy(
            action,
            &job.user_did,
            &job.space_key,
            args_to_policy_params(&job.args).as_ref(),
            resolved_remotes,
        ) {
            PolicyOutcome::NeedsResolution(spaces) => {
                // Still need more resolution
                self.result = ArbiterResult::NeedsResolution {
                    job_id,
                    spaces,
                };
            }
            PolicyOutcome::Denied => {
                self.job_queue = self.job_queue.without(&job_id);
                self.result = ArbiterResult::Err(ArbiterError {
                    kind: ArbiterErrorKind::PermissionDenied,
                    job_id: Some(job_id),
                });
            }
            PolicyOutcome::Allowed => {
                self.job_queue = self.job_queue.without(&job_id);
                self.execute_operation(&job.user_did, &job.space_key, job.args, resolved_remotes);
            }
            PolicyOutcome::Error(_) => {
                self.job_queue = self.job_queue.without(&job_id);
                self.result = ArbiterResult::Err(ArbiterError {
                    kind: ArbiterErrorKind::InvalidConfig,
                    job_id: Some(job_id),
                });
            }
        }
    }

    /// Timeout a queued job (remotes that didn't resolve).
    pub fn timeout_job(&mut self, job_id: JobId) {
        if !self.job_queue.contains_key(&job_id) {
            self.result = ArbiterResult::Err(ArbiterError {
                kind: ArbiterErrorKind::JobNotExists,
                job_id: Some(job_id),
            });
            return;
        }

        self.job_queue = self.job_queue.without(&job_id);
        self.result = ArbiterResult::Finished(JobResult::Ok);
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Evaluate the policy with a snapshot of the current state.
    fn evaluate_policy(
        &self,
        action: PolicyAction,
        user_did: &str,
        space_key: &str,
        params: Option<&PolicyParams>,
        resolved_remotes: &serde_json::Value,
    ) -> PolicyOutcome {
        let policy_source = match extract_policy(&self.config) {
            Ok(s) => s.to_string(),
            Err(_) => return PolicyOutcome::Error("Invalid config".into()),
        };

        // Snapshot: clone is O(1) via im::HashMap structural sharing
        let snapshot = Arc::new(self.spaces.clone());

        let mut engine = match PolicyEngine::new(&policy_source, snapshot) {
            Ok(e) => e,
            Err(_) => return PolicyOutcome::Error("Invalid policy".into()),
        };

        // Check needs_resolution first
        let needs = match engine.get_needs_resolution(user_did, space_key, resolved_remotes) {
            Ok(n) => n,
            Err(_) => return PolicyOutcome::Error("Policy eval error".into()),
        };

        if !needs.is_empty() {
            let spaces: Vec<ResolutionEntry> = needs
                .iter()
                .filter_map(|v| {
                    let obj = v.as_object()?;
                    Some(ResolutionEntry {
                        remote_arbiter_did: obj.get("remoteArbiterDid")?.as_str()?.to_string(),
                        space_key: obj.get("spaceKey")?.as_str()?.to_string(),
                    })
                })
                .collect();
            return PolicyOutcome::NeedsResolution(spaces);
        }

        // Check allow
        let allowed = match engine.evaluate(action, user_did, space_key, params, resolved_remotes) {
            Ok(a) => a,
            Err(_) => return PolicyOutcome::Error("Policy eval error".into()),
        };

        if allowed {
            PolicyOutcome::Allowed
        } else {
            PolicyOutcome::Denied
        }
    }

    /// Execute the operation (mutate state). Assumes authorization is already done.
    fn execute_operation(&mut self, user_did: &str, space_key: &str, args: JobArgs, resolved_remotes: &serde_json::Value) {
        match &args {
            JobArgs::ResolveMembers => {
                // Policy already validated access. Return resolved members.
                let snapshot = Arc::new(self.spaces.clone());
                let policy = match extract_policy(&self.config) {
                    Ok(s) => s.to_string(),
                    Err(_) => {
                        self.result = ArbiterResult::Err(ArbiterError {
                            kind: ArbiterErrorKind::InvalidConfig,
                            job_id: None,
                        });
                        return;
                    }
                };
                let mut engine = match PolicyEngine::new(&policy, snapshot) {
                    Ok(e) => e,
                    Err(_) => {
                        self.result = ArbiterResult::Err(ArbiterError {
                            kind: ArbiterErrorKind::InvalidConfig,
                            job_id: None,
                        });
                        return;
                    }
                };
                match engine.get_resolved_members(user_did, space_key, resolved_remotes) {
                    Ok(members) => {
                        self.result = ArbiterResult::Finished(JobResult::ResolvedMembersList(members));
                    }
                    Err(_) => {
                        self.result = ArbiterResult::Err(ArbiterError {
                            kind: ArbiterErrorKind::PermissionDenied,
                            job_id: None,
                        });
                    }
                }
            }
            JobArgs::CreateSpace { space_type, config } => {
                self.version = self.next_version();
                self.spaces = self.spaces.update(
                    space_key.to_string(),
                    Space::new(space_type.clone(), config.clone()),
                );
                self.result = ArbiterResult::Finished(JobResult::Ok);
            }
            JobArgs::SetSpaceConfig { space_type, config } => {
                self.version = self.next_version();
                if let Some(space) = self.spaces.get_mut(&space_key.to_string()) {
                    space.space_type = space_type.clone();
                    space.config = config.clone();
                }
                self.result = ArbiterResult::Finished(JobResult::Ok);
            }
            JobArgs::DeleteSpace => {
                self.version = self.next_version();
                self.spaces = self.spaces.without(space_key);
                self.result = ArbiterResult::Finished(JobResult::Ok);
            }
            JobArgs::SetSpaceMemberAccess { member, access } => {
                self.version = self.next_version();
                if let Some(space) = self.spaces.get_mut(&space_key.to_string()) {
                    space.members = space.members.update(member.clone(), access.clone());
                }
                self.result = ArbiterResult::Finished(JobResult::Ok);
            }
            JobArgs::RemoveSpaceMember { member } => {
                self.version = self.next_version();
                if let Some(space) = self.spaces.get_mut(&space_key.to_string()) {
                    space.members = space.members.without(member);
                }
                self.result = ArbiterResult::Finished(JobResult::Ok);
            }
            JobArgs::DeleteArbiter => {
                self.version = self.next_version();
                self.result = ArbiterResult::Deleted;
            }
        }
    }

    fn next_version(&self) -> ArbiterVersion {
        self.version.wrapping_add(1)
    }

    fn next_job_id(&self) -> JobId {
        // Simple: max existing + 1, or 1 if empty
        self.job_queue.keys().max().copied().unwrap_or(0) + 1
    }
}

// ---------------------------------------------------------------------------
// Policy evaluation outcomes
// ---------------------------------------------------------------------------

enum PolicyOutcome {
    /// Remote spaces need to be resolved first.
    NeedsResolution(Vec<ResolutionEntry>),
    /// Operation is denied by policy.
    Denied,
    /// Operation is allowed.
    Allowed,
    /// Policy evaluation error.
    Error(String),
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the Rego policy source from the arbiter config.
fn extract_policy(config: &ArbiterConfig) -> Result<&str, PolicyError> {
    let obj = config
        .as_object()
        .ok_or_else(|| PolicyError::EvalError("Config must be an object".into()))?;

    // Try `policy` field (for $type = town.muni.arbiter.config.regoPolicy)
    if let Some(policy) = obj.get("policy").and_then(|v| v.as_str()) {
        return Ok(policy);
    }

    // Try $type-specific extraction
    if let Some(policy) = obj.get("policy").and_then(|v| v.as_str()) {
        return Ok(policy);
    }

    Err(PolicyError::EvalError(
        "Config missing 'policy' field".into(),
    ))
}

/// Validate the operation before policy evaluation (structural checks).
fn validate_operation(arbiter: &Arbiter, space_key: &str, args: &JobArgs) -> Option<ArbiterError> {
    let err = |kind: ArbiterErrorKind| -> Option<ArbiterError> {
        Some(ArbiterError { kind, job_id: None })
    };

    match args {
        JobArgs::ResolveMembers => {
            if !arbiter.spaces.contains_key(space_key) {
                return err(ArbiterErrorKind::SpaceNotExists);
            }
        }
        JobArgs::CreateSpace { .. } => {
            if arbiter.spaces.contains_key(space_key) {
                return err(ArbiterErrorKind::SpaceAlreadyExists);
            }
        }
        JobArgs::SetSpaceConfig { .. } | JobArgs::SetSpaceMemberAccess { .. } | JobArgs::RemoveSpaceMember { .. } => {
            if !arbiter.spaces.contains_key(space_key) {
                return err(ArbiterErrorKind::SpaceNotExists);
            }
        }
        JobArgs::DeleteSpace => {
            if space_key == ADMIN_SPACE_KEY {
                return err(ArbiterErrorKind::CannotDeleteAdminSpace);
            }
            if !arbiter.spaces.contains_key(space_key) {
                return err(ArbiterErrorKind::SpaceNotExists);
            }
        }
        JobArgs::DeleteArbiter => {
            if space_key != ADMIN_SPACE_KEY {
                return err(ArbiterErrorKind::ArbiterDeletionMustSpecifyAdminSpace);
            }
        }
    }

    None
}

/// Map JobArgs to a PolicyAction.
fn args_to_action(args: &JobArgs) -> PolicyAction {
    match args {
        JobArgs::ResolveMembers => PolicyAction::ResolveSpaceMembers,
        JobArgs::CreateSpace { .. } => PolicyAction::CreateSpace,
        JobArgs::SetSpaceConfig { .. } => PolicyAction::SetSpaceConfig,
        JobArgs::DeleteSpace => PolicyAction::DeleteSpace,
        JobArgs::SetSpaceMemberAccess { .. } => PolicyAction::SetSpaceMemberAccess,
        JobArgs::RemoveSpaceMember { .. } => PolicyAction::RemoveSpaceMember,
        JobArgs::DeleteArbiter => PolicyAction::DeleteArbiter,
    }
}

/// Build PolicyParams from JobArgs (for policy evaluation).
fn args_to_policy_params(args: &JobArgs) -> Option<PolicyParams> {
    match args {
        JobArgs::SetSpaceMemberAccess { member, access } => Some(PolicyParams {
            target_member: Some(serde_json::to_value(member).unwrap_or_default()),
            target_access: Some(access.clone()),
        }),
        JobArgs::RemoveSpaceMember { member } => Some(PolicyParams {
            target_member: Some(serde_json::to_value(member).unwrap_or_default()),
            target_access: None,
        }),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// ServerState (multi-arbiter)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerState {
    pub time: i64,
    pub arbiters: im::HashMap<Did, Arbiter>,
}

impl Default for ServerState {
    fn default() -> Self {
        Self {
            time: 0,
            arbiters: im::HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_default_config() -> serde_json::Value {
        serde_json::json!({
            "$type": lexicon::CONFIG_REGO_POLICY,
            "policy": DEFAULT_POLICY,
        })
    }

    #[test]
    fn test_create_arbiter() {
        let config = make_default_config();
        let arbiter = Arbiter::new(
            "did:plc:my-arb".into(),
            "did:plc:alice".into(),
            config,
        )
        .unwrap();

        assert_eq!(arbiter.did, "did:plc:my-arb");
        assert!(arbiter.spaces.contains_key(ADMIN_SPACE_KEY));
    }

    #[test]
    fn test_owner_can_create_space() {
        let config = make_default_config();
        let mut arbiter = Arbiter::new(
            "did:plc:my-arb".into(),
            "did:plc:alice".into(),
            config,
        )
        .unwrap();

        arbiter.process_operation(
            "did:plc:alice",
            "my-space",
            JobArgs::CreateSpace {
                space_type: lexicon::CONFIG_SPACE.to_string(),
                config: serde_json::json!({
                    "$type": lexicon::CONFIG_SPACE,
                    "publicRecords": false,
                    "publicMembers": false,
                }),
            },
            &serde_json::json!({}),
        );

        match &arbiter.result {
            ArbiterResult::Finished(JobResult::Ok) => {
                assert!(arbiter.spaces.contains_key("my-space"));
            }
            other => panic!("Expected Finished(Ok), got {:?}", other),
        }
    }

    #[test]
    fn test_non_owner_cannot_create_space() {
        let config = make_default_config();
        let mut arbiter = Arbiter::new(
            "did:plc:my-arb".into(),
            "did:plc:alice".into(),
            config,
        )
        .unwrap();

        // Alice created with $admin with 'ReadMemberList' instead... actually
        // for this test let's just check the policy denies a stranger creating a space.

        // Add a non-admin space
        arbiter.process_operation(
            "did:plc:alice",
            "my-space",
            JobArgs::CreateSpace {
                space_type: lexicon::CONFIG_SPACE.to_string(),
                config: serde_json::json!({
                    "$type": lexicon::CONFIG_SPACE,
                    "publicRecords": false,
                    "publicMembers": false,
                }),
            },
            &serde_json::json!({}),
        );

        // Now add $admin space with only ReadMemberList for stranger
        // Actually, let's just test that someone NOT in the admin list gets denied.
        // In our setup, any stranger not resolved gets no access, so they can't create spaces.

        let mut arb2 = Arbiter::new(
            "did:plc:other".into(),
            "did:plc:alice".into(),
            make_default_config(),
        )
        .unwrap();

        arb2.process_operation(
            "did:plc:stranger",
            "new-space",
            JobArgs::CreateSpace {
                space_type: lexicon::CONFIG_SPACE.to_string(),
                config: serde_json::json!({
                    "$type": lexicon::CONFIG_SPACE,
                }),
            },
            &serde_json::json!({}),
        );

        match &arb2.result {
            ArbiterResult::Err(e) => {
                assert!(matches!(e.kind, ArbiterErrorKind::PermissionDenied));
            }
            other => panic!("Expected PermissionDenied, got {:?}", other),
        }
    }

    #[test]
    fn test_owner_can_delete_arbiter() {
        let config = make_default_config();
        let mut arbiter = Arbiter::new(
            "did:plc:my-arb".into(),
            "did:plc:alice".into(),
            config,
        )
        .unwrap();

        arbiter.process_operation(
            "did:plc:alice",
            ADMIN_SPACE_KEY,
            JobArgs::DeleteArbiter,
            &serde_json::json!({}),
        );

        match &arbiter.result {
            ArbiterResult::Deleted => {} // correct!
            other => panic!("Expected Deleted, got {:?}", other),
        }
    }

    #[test]
    fn test_non_member_cannot_read() {
        let config = make_default_config();
        let mut arbiter = Arbiter::new(
            "did:plc:my-arb".into(),
            "did:plc:alice".into(),
            config,
        )
        .unwrap();

        arbiter.process_operation(
            "did:plc:stranger",
            ADMIN_SPACE_KEY,
            JobArgs::ResolveMembers,
            &serde_json::json!({}),
        );

        match &arbiter.result {
            ArbiterResult::Err(e) => {
                assert!(matches!(e.kind, ArbiterErrorKind::PermissionDenied));
            }
            other => panic!("Expected PermissionDenied, got {:?}", other),
        }
    }
}
