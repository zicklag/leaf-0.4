//! Sans-IO policy evaluation pool using the Rego Virtual Machine.
//!
//! Provides a [`PolicyVmPool`] that manages `RegoVM` instances across
//! suspension/resume cycles. The VM is not `Clone`/`Serialize`, so we keep
//! it here; the `Arbiter` state machine holds only `JobId` handles.
//!
//! All host-provided functions use `__builtin_host_await` in the Rego policy,
//! making the extension mechanism uniform for both sync operations (local
//! space member/config reads) and async operations (remote space resolution).

use std::collections::{BTreeMap, HashMap, VecDeque};

use regorus::{
  PolicyModule, Value,
  languages::rego::compiler::Compiler,
  rvm::{
    RegoVM,
    vm::{ExecutionMode, ExecutionState, SuspendReason},
  },
};

use crate::core::{JobArgs, JobId, Space, SpaceKey};
use crate::policy::*;

// ---------------------------------------------------------------------------
// Job context stored alongside the suspended VM
// ---------------------------------------------------------------------------

/// Context needed to execute the operation after policy evaluation completes.
#[derive(Debug, Clone)]
pub struct JobContext {
    pub user_did: String,
    pub space_key: String,
    pub args: JobArgs,
}

// ---------------------------------------------------------------------------
// Suspension info returned to the caller
// ---------------------------------------------------------------------------

/// Information about a suspension that needs host intervention.
#[derive(Debug, Clone, PartialEq)]
pub enum HostRequest {
    /// Resolve members of a remote space.
    ResolveRemote {
        remote_arbiter_did: String,
        space_key: String,
    },
}

/// Result of running or resuming a policy evaluation.
#[derive(Debug)]
pub enum VmResult {
    /// Evaluation completed with a value and the job context (if any).
    Completed(Value, Option<JobContext>),
    /// Evaluation suspended, waiting for host to provide data.
    Suspended {
        job_id: JobId,
        request: HostRequest,
    },
    /// Evaluation failed with an error.
    Error(String),
}

// ---------------------------------------------------------------------------
// PolicyVmPool
// ---------------------------------------------------------------------------

/// Manages `RegoVM` instances for policy evaluation.
///
/// Each suspended VM is stored here, keyed by `JobId`. The `Arbiter` and
/// `ServerState` (which are `Clone`+`Serialize`) hold only the job ids.
///
/// ## Caching
///
/// When `resolve_remote` is called multiple times with the same remote ID
/// (e.g. from both `resolved_members_raw` and `missing_spaces` rules), the
/// pool caches the response and auto-resumes the VM without asking the caller.
pub struct PolicyVmPool {
    pending: HashMap<JobId, PendingVm>,
    /// Accumulated resolved remote data from suspension/resume cycles.
    /// Keyed by remote_id ("arbiterDid|spaceKey").
    resolved_remotes: HashMap<String, serde_json::Value>,
    /// Per-job response caches for auto-resolving duplicate remote requests.
    response_caches: HashMap<JobId, HashMap<String, serde_json::Value>>,
    next_job_id: JobId,
}

struct PendingVm {
    vm: RegoVM,
    context: Option<JobContext>,
}

impl PolicyVmPool {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            resolved_remotes: HashMap::new(),
            response_caches: HashMap::new(),
            next_job_id: 1,
        }
    }

    /// Compile the policy and start evaluation in a new VM.
    ///
    /// Returns the result immediately if the VM completes without suspending,
    /// or returns `VmResult::Suspended` with a `job_id` if it needs host data.
    pub fn start_evaluation(
        &mut self,
        policy_source: &str,
        extensions: &str,
        input: &serde_json::Value,
        spaces: &im::HashMap<SpaceKey, Space>,
        context: Option<JobContext>,
    ) -> VmResult {
        let vm = match Self::build_vm(policy_source, extensions, input, spaces, &["data.arbiter.allow"]) {
            Ok(v) => v,
            Err(e) => return VmResult::Error(e),
        };

        self.run_or_suspend(vm, context)
    }

    /// Resume a suspended evaluation with the host's response.
    ///
    /// `resume_value` is the value that the `__builtin_host_await` call returns.
    ///
    /// If the VM suspends again for the same remote (duplicate call from a
    /// different rule body), and we have a cached response, we auto-resume
    /// without returning to the caller.
    pub fn resume_evaluation(
        &mut self,
        job_id: JobId,
        resume_value: &serde_json::Value,
    ) -> VmResult {
        // Store the provided response in cache and pool
        let cached_id = self.extract_remote_id_from_suspension(job_id);
        if let Some(ref remote_id) = cached_id {
            self.resolved_remotes.insert(remote_id.clone(), resume_value.clone());
            self.response_caches
                .entry(job_id)
                .or_default()
                .insert(remote_id.clone(), resume_value.clone());
        }

        // First resume with the caller-provided value
        let first_resume = self.resume_one_step(job_id, resume_value);
        match first_resume {
            VmResult::Completed(..) | VmResult::Error(_) => return first_resume,
            VmResult::Suspended { .. } => {}
        }

        // Now loop: for each subsequent suspension, check the cache.
        // If cached, auto-resume. If not, return Suspended to caller.
        loop {
            let entry = match self.pending.get(&job_id) {
                Some(e) => e,
                None => return VmResult::Error(format!("No pending VM for job {job_id}")),
            };

            let reason = match entry.vm.suspend_reason() {
                Some(r) => r.clone(),
                None => return VmResult::Error("VM not suspended".into()),
            };

            // Check cache for this remote
            if let Some(cached) = self.check_cache(job_id, &reason) {
                let result = self.resume_one_step(job_id, &cached);
                match result {
                    VmResult::Completed(..) | VmResult::Error(_) => return result,
                    VmResult::Suspended { .. } => continue,
                }
            } else {
                // Not cached — ask the caller
                let request = match host_request_from_suspend(&reason) {
                    Some(r) => r,
                    None => {
                        self.pending.remove(&job_id);
                        self.response_caches.remove(&job_id);
                        return VmResult::Error("Unexpected suspension reason".into());
                    }
                };
                return VmResult::Suspended { job_id, request };
            }
        }
    }

    /// Resume one step: inject the value and process one VM continuation.
    fn resume_one_step(
        &mut self,
        job_id: JobId,
        value: &serde_json::Value,
    ) -> VmResult {
        let entry = match self.pending.get_mut(&job_id) {
            Some(e) => e,
            None => return VmResult::Error(format!("No pending VM for job {job_id}")),
        };

        let regorus_value = value_to_regorus(value);
        match entry.vm.resume(Some(regorus_value)) {
            Ok(_) => {
                match entry.vm.execution_state() {
                    ExecutionState::Completed { result } => {
                        let result = result.clone();
                        let mut entry = self.pending.remove(&job_id).unwrap();
                        let context = entry.context.take();
                        self.response_caches.remove(&job_id);
                        VmResult::Completed(result, context)
                    }
                    ExecutionState::Suspended { reason, .. } => {
                        let request = match host_request_from_suspend(&reason) {
                            Some(r) => r,
                            None => {
                                self.pending.remove(&job_id);
                                self.response_caches.remove(&job_id);
                                return VmResult::Error("Unexpected suspension reason".into());
                            }
                        };
                        VmResult::Suspended { job_id, request }
                    }
                    ExecutionState::Error { error } => {
                        let err = format!("{error:?}");
                        self.pending.remove(&job_id);
                        self.response_caches.remove(&job_id);
                        VmResult::Error(err)
                    }
                    state => {
                        let err = format!("Unexpected VM state after resume: {state:?}");
                        self.pending.remove(&job_id);
                        self.response_caches.remove(&job_id);
                        VmResult::Error(err)
                    }
                }
            }
            Err(e) => {
                let err = format!("{e:?}");
                self.pending.remove(&job_id);
                self.response_caches.remove(&job_id);
                VmResult::Error(err)
            }
        }
    }

    /// Query the resolved members for a space using the policy.
    ///
    /// This creates a fresh VM and runs it in run-to-completion mode with
    /// the `data.arbiter.resolved_members` entry point. Any previously resolved
    /// remote data (collected during the authorization suspension/resume cycle)
    /// is pre-loaded as host_await responses.
    ///
    /// Call this only after authorization has completed (the suspension loop
    /// is done) and all remote data has been resolved.
    pub fn query_resolved_members(
        &mut self,
        policy_source: &str,
        input: &serde_json::Value,
        spaces: &im::HashMap<SpaceKey, Space>,
    ) -> Result<serde_json::Value, String> {
        // Build a fresh VM compiled with allow, resolved_members, and missing_spaces entry points
        let mut vm = Self::build_vm(
            policy_source,
            crate::policy_vm::POLICY_EXTENSIONS,
            input,
            spaces,
            &["data.arbiter.allow", "data.arbiter.resolved_members", "data.arbiter.missing_spaces"],
        )?;

        // Pre-load responses from accumulated resolved remotes
        // The HostAwait instruction indexes by identifier (2nd arg), so we
        // key by "arbiter.resolve_remote" and queue responses in order.
        if !self.resolved_remotes.is_empty() {
            let mut responses: BTreeMap<Value, VecDeque<Value>> = BTreeMap::new();
            let identifier_key = Value::from("arbiter.resolve_remote");
            let mut queue = VecDeque::new();
            // Push a response for each resolved remote
            // We don't know the order the policy will request them, but since
            // the policy evaluates deterministically, the order matches.
            for (_remote_id, data) in &self.resolved_remotes {
                let data_val = value_to_regorus(data);
                queue.push_back(data_val);
            }
            responses.insert(identifier_key, queue);
            vm.set_host_await_responses(responses);
        }

        // Run in run-to-completion mode for resolved_members
        vm.set_execution_mode(ExecutionMode::RunToCompletion);
        let members = match vm.execute_entry_point_by_name("data.arbiter.resolved_members") {
            Ok(result) => value_from_regorus_single(&result),
            Err(e) => return Err(format!("Failed to query resolved members: {e:?}")),
        };

        // Also query missing_spaces (may not exist for custom policies)
        vm.set_execution_mode(ExecutionMode::RunToCompletion);
        let missing = match vm.execute_entry_point_by_name("data.arbiter.missing_spaces") {
            Ok(val) => value_from_regorus_single(&val),
            Err(_) => serde_json::Value::Array(vec![]),
        };

        Ok(serde_json::json!({
            "members": members,
            "missingSpaces": missing,
        }))
    }

    /// Cancel a pending evaluation (remove the VM).
    pub fn cancel(&mut self, job_id: JobId) {
        self.pending.remove(&job_id);
        self.response_caches.remove(&job_id);
    }

    /// Check if a job is still pending.
    pub fn contains(&self, job_id: JobId) -> bool {
        self.pending.contains_key(&job_id)
    }

    /// Number of pending evaluations.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    // -----------------------------------------------------------------------
    // Internal: cache helpers
    // -----------------------------------------------------------------------

    /// Extract the remote_id from the current suspension of a pending VM.
    fn extract_remote_id_from_suspension(&self, job_id: JobId) -> Option<String> {
        let entry = self.pending.get(&job_id)?;
        let reason = match entry.vm.suspend_reason()? {
            SuspendReason::HostAwait { argument, identifier, .. } => {
                let _id_str = identifier.as_string().ok()?;
                argument.as_string().ok()?.to_string()
            }
            _ => return None,
        };
        Some(reason)
    }

    /// Check the response cache for a given suspension reason.
    /// Returns Some(cached_value) if found.
    fn check_cache(&self, job_id: JobId, reason: &SuspendReason) -> Option<serde_json::Value> {
        let remote_id = match reason {
            SuspendReason::HostAwait { argument, .. } => {
                argument.as_string().ok()?.to_string()
            }
            _ => return None,
        };

        self.response_caches
            .get(&job_id)
            .and_then(|cache| cache.get(&remote_id).cloned())
    }

    // -----------------------------------------------------------------------
    // Internal: VM lifecycle
    // -----------------------------------------------------------------------

    fn build_vm(
        policy_source: &str,
        extensions: &str,
        input: &serde_json::Value,
        spaces: &im::HashMap<SpaceKey, Space>,
        entry_points: &[&str],
    ) -> Result<RegoVM, String> {
        // Combine policy + extensions
        let full_source = {
            let mut s = String::from(policy_source);
            s.push_str("\n\n");
            s.push_str(extensions);
            s
        };

        // Build data from the frozen arbiter snapshot
        let data = build_data_from_spaces(spaces);

        // Compile policy with data
        let compiled_policy = regorus::compile_policy_with_entrypoint(
            value_to_regorus(&data),
            &[PolicyModule {
                id: "".into(),
                content: full_source.into(),
            }],
            "data.arbiter.allow".into(),
        )
        .map_err(|e| format!("Failed to compile policy: {e}"))?;

        // Compile to RVM program with multiple entry points
        let program = Compiler::compile_from_policy(&compiled_policy, entry_points)
            .map_err(|e| format!("Failed to compile to RVM: {e}"))?;

        // Create VM and load program
        let mut vm = RegoVM::new();
        // Setting program before set_data so check_rule_data_conflicts can work
        vm.load_program(program.clone());
        vm.set_data(value_to_regorus(&data))
            .map_err(|e| format!("Failed to set VM data: {e}"))?;
        vm.set_input(value_to_regorus(input));
        vm.set_execution_mode(ExecutionMode::Suspendable);

        Ok(vm)
    }

    fn run_or_suspend(
        &mut self,
        mut vm: RegoVM,
        context: Option<JobContext>,
    ) -> VmResult {
        let _ = vm.execute();  // execute returns Ok even when suspended

        match vm.execution_state() {
            ExecutionState::Completed { result } => {
                VmResult::Completed(result.clone(), context)
            }
            ExecutionState::Suspended { reason, .. } => {
                let job_id = self.next_job_id;
                self.next_job_id += 1;

                let request = match host_request_from_suspend(reason) {
                    Some(r) => r,
                    None => return VmResult::Error("Unexpected suspension reason".into()),
                };

                self.pending.insert(
                    job_id,
                    PendingVm {
                        vm,
                        context,
                    },
                );

                VmResult::Suspended { job_id, request }
            }
            ExecutionState::Error { error } => {
                VmResult::Error(format!("{error:?}"))
            }
            state => {
                VmResult::Error(format!("Unexpected VM state after execute: {state:?}"))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SuspendReason → HostRequest conversion
// ---------------------------------------------------------------------------

fn host_request_from_suspend(reason: &SuspendReason) -> Option<HostRequest> {
    match reason {
        SuspendReason::HostAwait {
            argument,
            identifier,
            ..
        } => {
            let identifier_str = identifier.as_string().ok()?.to_string();
            let argument_str = argument.as_string().ok()?;

            match identifier_str.as_str() {
                "arbiter.resolve_remote" => {
                    // Argument is "arbiterDid|spaceKey"
                    let parts: Vec<&str> = argument_str.splitn(2, '|').collect();
                    if parts.len() == 2 {
                        Some(HostRequest::ResolveRemote {
                            remote_arbiter_did: parts[0].to_string(),
                            space_key: parts[1].to_string(),
                        })
                    } else {
                        None
                    }
                }
                _ => {
                    None
                }
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Data building from the frozen arbiter snapshot
// ---------------------------------------------------------------------------

fn build_data_from_spaces(spaces: &im::HashMap<SpaceKey, Space>) -> serde_json::Value {
    let mut spaces_obj = serde_json::Map::new();

    for (key, space) in spaces.iter() {
        let members: Vec<serde_json::Value> = space
            .members
            .iter()
            .map(|(member, access)| {
                serde_json::json!({
                    "member": member,
                    "access": access,
                })
            })
            .collect();

        spaces_obj.insert(
            key.clone(),
            serde_json::json!({
                "spaceType": space.space_type,
                "config": space.config,
                "members": members,
            }),
        );
    }

    serde_json::json!({
        "arbiter": {
            "spaces": spaces_obj,
        }
    })
}

// ---------------------------------------------------------------------------
// Value conversion helpers
// ---------------------------------------------------------------------------

/// Convert a regorus::Value to a serde_json::Value (single value).
fn value_from_regorus_single(val: &Value) -> serde_json::Value {
    let json_str = serde_json::to_string(val).unwrap_or_default();
    serde_json::from_str(&json_str).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Default extensions for the policy
// ---------------------------------------------------------------------------

/// Rego extension functions that the default policy uses.
///
/// These are appended to the policy source and define wrapper functions
/// around `__builtin_host_await` for all host-provided operations.
pub const POLICY_EXTENSIONS: &str = r#"
# Resolve members of a remote space.
# Suspends the VM — the host fetches the member list from the remote arbiter.
resolve_remote(arbiter_did, space_key) := members if {
    remote_id := concat("|", [arbiter_did, space_key])
    members := __builtin_host_await(remote_id, "arbiter.resolve_remote")
}
"#;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Member, ADMIN_SPACE_KEY};
    use crate::policy::lexicon;

    fn make_test_spaces() -> im::HashMap<SpaceKey, Space> {
        let mut spaces = im::HashMap::new();

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

        spaces
    }

    fn make_input(requester: &str, action: &str, space_key: &str) -> serde_json::Value {
        serde_json::json!({
            "requester": requester,
            "action": action,
            "resource": {
                "arbiterDid": "",
                "spaceKey": space_key,
            },
        })
    }

    #[test]
    fn test_vm_creation() {
        let mut pool = PolicyVmPool::new();
        let input = make_input("did:plc:alice", "deleteArbiter", "$admin");
        let spaces = make_test_spaces();

        let result = pool.start_evaluation(DEFAULT_POLICY, POLICY_EXTENSIONS, &input, &spaces, None);

        match &result {
            VmResult::Completed(value, _ctx) => {
                let allowed = value.as_bool().copied().unwrap_or(false);
                assert!(allowed, "Expected allow=true, got {value:?}");
            }
            VmResult::Error(e) => panic!("Unexpected error: {e}"),
            VmResult::Suspended { job_id, request } => {
                panic!("Unexpected suspension: job={job_id:?}, request={request:?}")
            }
        }
    }

    #[test]
    fn test_sync_evaluation_completes() {
        let mut pool = PolicyVmPool::new();
        let input = make_input("did:plc:alice", "resolveSpaceMembers", "$admin");
        let spaces = make_test_spaces();

        let result = pool.start_evaluation(DEFAULT_POLICY, POLICY_EXTENSIONS, &input, &spaces, None);

        match &result {
            VmResult::Completed(value, _ctx) => {
                let allowed = value.as_bool().copied().unwrap_or(false);
                assert!(allowed, "Owner should be able to resolve members");
            }
            VmResult::Error(e) => panic!("Unexpected error: {e}"),
            VmResult::Suspended { job_id, request } => {
                panic!("Unexpected suspension: job={job_id:?}, request={request:?}")
            }
        }
    }
}
