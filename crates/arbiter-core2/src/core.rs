//! Data model and state machine for the arbiter core.
//!
//! Contains the core types (Arbiter, Space, Member) and the state machine
//! that wraps the Rego policy engine for authorization.
//!
//! Policy evaluation is driven by a [`PolicyVmPool`] that manages suspended
//! `RegoVM` instances across async resolution cycles. The `Arbiter` itself
//! only holds handles (JobIds) into the pool, keeping the state machine
//! fully `Clone`+`Serialize`.

use serde::{Deserialize, Serialize};

pub use crate::policy::*;
use crate::policy_vm::{HostRequest, PolicyVmPool, VmResult};

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

// ---------------------------------------------------------------------------
// Suspension info (serializable, returned to the caller)
// ---------------------------------------------------------------------------

/// Information about a suspension that the caller must resolve.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuspensionInfo {
    /// The type of host intervention needed.
    #[serde(rename = "type")]
    pub kind: SuspensionKind,
    /// The remote arbiter DID (for `ResolveRemote`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_arbiter_did: Option<String>,
    /// The space key on the remote arbiter (for `ResolveRemote`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuspensionKind {
    ResolveRemote,
}

impl From<HostRequest> for SuspensionInfo {
    fn from(req: HostRequest) -> Self {
        match req {
            HostRequest::ResolveRemote {
                remote_arbiter_did,
                space_key,
            } => SuspensionInfo {
                kind: SuspensionKind::ResolveRemote,
                remote_arbiter_did: Some(remote_arbiter_did),
                space_key: Some(space_key),
            },
        }
    }
}

impl From<&HostRequest> for SuspensionInfo {
    fn from(req: &HostRequest) -> Self {
        match req {
            HostRequest::ResolveRemote {
                remote_arbiter_did,
                space_key,
            } => SuspensionInfo {
                kind: SuspensionKind::ResolveRemote,
                remote_arbiter_did: Some(remote_arbiter_did.clone()),
                space_key: Some(space_key.clone()),
            },
        }
    }
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
    JobNotExists,
    SpaceAlreadyExists,
    SpaceNotExists,
    PermissionDenied,
    CannotDeleteAdminSpace,
    ArbiterDeletionMustSpecifyAdminSpace,
    OnlyLastOwnerCanDeleteArbiter,
    InvalidConfig,
    UnsupportedConfigLexicon,
    ArbiterAlreadyExists,
    PolicyEvaluationError,
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
    /// Policy evaluation needs async resolution before continuing.
    Suspended {
        job_id: JobId,
        request: SuspensionInfo,
    },
    /// A job finished with a result.
    Finished(JobResult),
    /// The arbiter was deleted.
    Deleted,
    /// An error occurred.
    Err(ArbiterError),
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
    pub result: ArbiterResult,
}

impl Arbiter {
    /// Create a new arbiter with the given DID, initial owner, and config.
    ///
    /// The config must contain a valid Rego policy at `config.policy`.
    pub fn new(did: Did, owner_did: Did, config: ArbiterConfig) -> Result<Self, PolicyError> {
        // Validate the config has a valid policy
        let _policy = extract_policy(&config)?;
        // Quick validation that the policy parses
        validate_policy(extract_policy(&config)?)?;

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
            result: ArbiterResult::Ok,
        })
    }

    /// Process an operation on the arbiter.
    ///
    /// Takes a snapshot of the arbiter's spaces, evaluates the policy in a
    /// RegoVM (suspendable mode), and either executes the operation immediately
    /// or suspends waiting for remote resolution.
    ///
    /// If the result is `Suspended`, the caller must resolve the remote and call
    /// [`resume_operation`] with the resolved data.
    pub fn process_operation(
        &mut self,
        user_did: &str,
        space_key: &str,
        args: JobArgs,
        pool: &mut PolicyVmPool,
    ) {
        // 1. Structural validation
        if let Some(err) = validate_operation(self, space_key, &args) {
            self.result = ArbiterResult::Err(err);
            return;
        }

        // 2. Build input for the policy
        let action = args_to_action(&args);
        let params = args_to_policy_params(&args);
        let input = build_policy_input(action, user_did, space_key, params.as_ref());

        // 3. Get the policy source and build job context for suspension
        let policy_source = match extract_policy(&self.config) {
            Ok(s) => s,
            Err(_) => {
                self.result = ArbiterResult::Err(ArbiterError {
                    kind: ArbiterErrorKind::InvalidConfig,
                    job_id: None,
                });
                return;
            }
        };

        // Build the job context (used if the VM suspends, returned if it completes)
        let job_context = crate::policy_vm::JobContext {
            user_did: user_did.to_string(),
            space_key: space_key.to_string(),
            args,
        };

        // 4. Start evaluation in the VM pool
        match pool.start_evaluation(policy_source, crate::policy_vm::POLICY_EXTENSIONS, &input, &self.spaces, Some(job_context)) {
            VmResult::Completed(value, ctx) => {
                let allowed = value.as_bool().copied().unwrap_or(false);
                if allowed {
                    // Use the returned context (which has the original args)
                    match ctx {
                        Some(ctx) => {
                            self.execute_operation(&ctx.user_did, &ctx.space_key, ctx.args, pool);
                        }
                        None => {
                            self.result = ArbiterResult::Err(ArbiterError {
                                kind: ArbiterErrorKind::PolicyEvaluationError,
                                job_id: None,
                            });
                        }
                    }
                } else {
                    self.result = ArbiterResult::Err(ArbiterError {
                        kind: ArbiterErrorKind::PermissionDenied,
                        job_id: None,
                    });
                }
            }
            VmResult::Suspended { job_id, request } => {
                self.result = ArbiterResult::Suspended {
                    job_id,
                    request: SuspensionInfo::from(&request),
                };
            }
            VmResult::Error(_e) => {
                self.result = ArbiterResult::Err(ArbiterError {
                    kind: ArbiterErrorKind::PolicyEvaluationError,
                    job_id: None,
                });
            }
        }
    }

    /// Resume a suspended operation with resolved data.
    ///
    /// The caller must call this after resolving the `SuspensionInfo` from a
    /// previous `Suspended` result, providing the resolved value (e.g., the
    /// remote member list as a JSON value).
    pub fn resume_operation(
        &mut self,
        job_id: JobId,
        resolved_value: &serde_json::Value,
        pool: &mut PolicyVmPool,
    ) {
        match pool.resume_evaluation(job_id, resolved_value) {
            VmResult::Completed(value, context) => {
                let allowed = value.as_bool().copied().unwrap_or(false);
                if allowed {
                    // Use the stored context from the pool
                    match context {
                        Some(ctx) => {
                            self.execute_operation(&ctx.user_did, &ctx.space_key, ctx.args, pool);
                        }
                        None => {
                            self.result = ArbiterResult::Err(ArbiterError {
                                kind: ArbiterErrorKind::PolicyEvaluationError,
                                job_id: Some(job_id),
                            });
                        }
                    }
                } else {
                    pool.cancel(job_id);
                    self.result = ArbiterResult::Err(ArbiterError {
                        kind: ArbiterErrorKind::PermissionDenied,
                        job_id: None,
                    });
                }
            }
            VmResult::Suspended { job_id: new_job_id, request } => {
                self.result = ArbiterResult::Suspended {
                    job_id: new_job_id,
                    request: SuspensionInfo::from(&request),
                };
            }
            VmResult::Error(_e) => {
                pool.cancel(job_id);
                self.result = ArbiterResult::Err(ArbiterError {
                    kind: ArbiterErrorKind::PolicyEvaluationError,
                    job_id: None,
                });
            }
        }
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Execute the operation (mutate state). Assumes authorization is already done.
    fn execute_operation(
        &mut self,
        user_did: &str,
        space_key: &str,
        args: JobArgs,
        pool: &mut PolicyVmPool,
    ) {
        match &args {
            JobArgs::ResolveMembers => {
                // Policy already validated access. Query resolved members from
                // the policy using a fresh VM run in run-to-completion mode.
                let action = PolicyAction::ResolveSpaceMembers;
                let input = build_policy_input(action, user_did, space_key, None);
                let policy_source = match extract_policy(&self.config) {
                    Ok(s) => s,
                    Err(_) => {
                        self.result = ArbiterResult::Err(ArbiterError {
                            kind: ArbiterErrorKind::InvalidConfig,
                            job_id: None,
                        });
                        return;
                    }
                };

                match pool.query_resolved_members(policy_source, &input, &self.spaces) {
                    Ok(members) => {
                        self.result = ArbiterResult::Finished(JobResult::ResolvedMembersList(members));
                    }
                    Err(_) => {
                        self.result = ArbiterResult::Err(ArbiterError {
                            kind: ArbiterErrorKind::PolicyEvaluationError,
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
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the Rego policy source from the arbiter config.
pub fn extract_policy(config: &ArbiterConfig) -> Result<&str, PolicyError> {
    let obj = config
        .as_object()
        .ok_or_else(|| PolicyError::EvalError("Config must be an object".into()))?;

    // Try `policy` field (for $type = town.muni.arbiter.config.regoPolicy)
    if let Some(policy) = obj.get("policy").and_then(|v| v.as_str()) {
        return Ok(policy);
    }

    Err(PolicyError::EvalError(
        "Config missing 'policy' field".into(),
    ))
}

/// Build the policy input JSON from action, requester, and params.
fn build_policy_input(
    action: PolicyAction,
    requester: &str,
    space_key: &str,
    params: Option<&PolicyParams>,
) -> serde_json::Value {
    let mut input = serde_json::json!({
        "requester": requester,
        "action": action.as_str(),
        "resource": {
            "arbiterDid": "",
            "spaceKey": space_key,
        },
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
    use crate::policy_vm::PolicyVmPool;

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
        let mut pool = PolicyVmPool::new();

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
            &mut pool,
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
        let mut pool = PolicyVmPool::new();

        let mut arb2 = Arbiter::new(
            "did:plc:other".into(),
            "did:plc:alice".into(),
            config,
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
            &mut pool,
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
        let mut pool = PolicyVmPool::new();

        arbiter.process_operation(
            "did:plc:alice",
            ADMIN_SPACE_KEY,
            JobArgs::DeleteArbiter,
            &mut pool,
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
        let mut pool = PolicyVmPool::new();

        arbiter.process_operation(
            "did:plc:stranger",
            ADMIN_SPACE_KEY,
            JobArgs::ResolveMembers,
            &mut pool,
        );

        match &arbiter.result {
            ArbiterResult::Err(e) => {
                assert!(matches!(e.kind, ArbiterErrorKind::PermissionDenied));
            }
            other => panic!("Expected PermissionDenied, got {:?}", other),
        }
    }
}
