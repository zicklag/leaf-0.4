//! Integration test harness for arbiter-core2.
//!
//! Simulates a multi-arbiter environment with auto-resolution of remote
//! delegations using the suspendable RegoVM.

use arbiter_core2::core::*;
use arbiter_core2::policy_vm::PolicyVmPool;
use serde_json::json;

/// All-purpose test harness wrapping a multi-arbiter server.
pub struct Harness {
    pub state: ServerState,
    pub vm_pool: PolicyVmPool,
    #[allow(dead_code)]
    /// Map from job_id to the arbiter DID that owns it.
    job_owners: std::collections::HashMap<JobId, String>,
}

impl Harness {
    pub fn new() -> Self {
        Self {
            state: ServerState::default(),
            vm_pool: PolicyVmPool::new(),
            job_owners: std::collections::HashMap::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Arbiter creation
    // -----------------------------------------------------------------------

    /// Create an arbiter with a custom policy.
    pub fn create_arbiter(&mut self, did: &str, owner: &str, policy: &str) {
        let config = json!({
            "$type": "town.muni.arbiter.config.regoPolicy",
            "policy": policy,
        });
        let arbiter =
            Arbiter::new(did.to_string(), owner.to_string(), config).expect("create arbiter");
        self.state.arbiters = self.state.arbiters.update(did.to_string(), arbiter);
    }

    /// Create an arbiter with the built-in default access-levels policy.
    pub fn create_default_arbiter(&mut self, did: &str, owner: &str) {
        self.create_arbiter(did, owner, DEFAULT_POLICY);
    }

    // -----------------------------------------------------------------------
    // Operations with auto-resolution
    // -----------------------------------------------------------------------

    /// Process an operation, auto-resolving remote spaces.
    /// Returns the final result after all resolutions complete.
    pub fn process(
        &mut self,
        arbiter_did: &str,
        user_did: &str,
        space_key: &str,
        args: JobArgs,
    ) -> ProcessResult {
        let current_arbiter = self.get_arbiter(arbiter_did);

        // Start the operation
        let (result, should_delete) = self.process_inner(current_arbiter, user_did, space_key, args);

        if should_delete {
            self.state.arbiters = self.state.arbiters.without(arbiter_did);
        } else {
            // result carries the final arbiter state
        }

        ProcessResult {
            result,
        }
    }

    /// Inner loop: process an operation, auto-resolving remote spaces.
    /// Returns (final_result, should_delete).
    fn process_inner(
        &mut self,
        mut current_arbiter: Arbiter,
        user_did: &str,
        space_key: &str,
        args: JobArgs,
    ) -> (ArbiterResult, bool) {
        // Capture the calling arbiter's DID before it potentially gets moved
        let calling_arbiter_did = current_arbiter.did.clone();

        current_arbiter.process_operation(user_did, space_key, args, &mut self.vm_pool);

        loop {
            let next = current_arbiter.result.clone();
            match next {
                ArbiterResult::Suspended { job_id, request } => {
                    let resolved = match request.kind {
                        SuspensionKind::ResolveRemote => {
                            let remote_did = request.remote_arbiter_did
                                .expect("ResolveRemote missing remote_arbiter_did");
                            let remote_key = request.space_key
                                .expect("ResolveRemote missing space_key");
                            // Resolve as the calling arbiter's DID (server-to-server)
                            self.resolve_remote_members(&remote_did, &calling_arbiter_did, &remote_key)
                        }
                    };

                    current_arbiter.resume_operation(job_id, &resolved, &mut self.vm_pool);
                }
                ArbiterResult::Deleted => {
                    return (current_arbiter.result, true);
                }
                _ => {
                    self.state.arbiters = self.state.arbiters.update(
                        current_arbiter.did.clone(),
                        current_arbiter,
                    );
                    return (next, false);
                }
            }
        }
    }

    /// Assert an operation succeeds.
    pub fn assert_ok(
        &mut self,
        arbiter_did: &str,
        user_did: &str,
        space_key: &str,
        args: JobArgs,
    ) {
        let r = self.process(arbiter_did, user_did, space_key, args);
        match &r.result {
            ArbiterResult::Finished(JobResult::Ok)
            | ArbiterResult::Ok
            | ArbiterResult::Finished(JobResult::ResolvedMembersList(_)) => {}
            ArbiterResult::Deleted => {}
            other => panic!(
                "Expected success for {user_did}@{arbiter_did}/{space_key}, got {other:?}"
            ),
        }
    }

    /// Assert an operation is denied with PermissionDenied.
    pub fn assert_denied(
        &mut self,
        arbiter_did: &str,
        user_did: &str,
        space_key: &str,
        args: JobArgs,
    ) {
        let r = self.process(arbiter_did, user_did, space_key, args);
        match &r.result {
            ArbiterResult::Err(e) => {
                assert!(
                    matches!(e.kind, ArbiterErrorKind::PermissionDenied),
                    "Expected PermissionDenied, got {e:?}"
                );
            }
            other => panic!(
                "Expected PermissionDenied for {user_did}@{arbiter_did}/{space_key}, got {other:?}"
            ),
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn get_arbiter(&self, did: &str) -> Arbiter {
        self.state
            .arbiters
            .get(did)
            .cloned()
            .unwrap_or_else(|| panic!("Arbiter {did} not found"))
    }

    fn resolve_remote_members(
        &mut self,
        arbiter_did: &str,
        as_arbiter: &str,
        space_key: &str,
    ) -> serde_json::Value {
        // Resolve as the calling arbiter — the remote arbiter checks permissions.
        let r = self.process(
            arbiter_did,
            as_arbiter,
            space_key,
            JobArgs::ResolveMembers,
        );
        // Return JUST the members array (the policy expects to iterate over this)
        match r.result {
            ArbiterResult::Finished(JobResult::ResolvedMembersList(response)) => {
                response.get("members")
                    .cloned()
                    .unwrap_or(json!([]))
            }
            _ => json!([]),
        }
    }

    /// Get the resolved members for a space (with auto-resolution).
    pub fn resolved_members(
        &mut self,
        arbiter_did: &str,
        user_did: &str,
        space_key: &str,
    ) -> Vec<serde_json::Value> {
        let r = self.process(
            arbiter_did,
            user_did,
            space_key,
            JobArgs::ResolveMembers,
        );
        match &r.result {
            ArbiterResult::Finished(JobResult::ResolvedMembersList(response)) => {
                response.get("members")
                    .and_then(|m| m.as_array())
                    .cloned()
                    .unwrap_or_default()
            }
            other => panic!(
                "Expected ResolvedMembersList for {user_did}@{arbiter_did}/{space_key}, got {other:?}"
            ),
        }
    }

}

impl Default for Harness {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a processed operation (after all auto-resolutions).
#[derive(Debug)]
pub struct ProcessResult {
    pub result: ArbiterResult,
}

// -----------------------------------------------------------------------
// Helper shortcuts for common JobArgs
// -----------------------------------------------------------------------

pub fn create_space(_key: &str) -> JobArgs {
    JobArgs::CreateSpace {
        space_type: "town.muni.arbiter.config.space".into(),
        config: json!({
            "$type": "town.muni.arbiter.config.space",
            "publicRecords": false,
            "publicMembers": false,
        }),
    }
}

pub fn add_member(member: Member, level: &str) -> JobArgs {
    JobArgs::SetSpaceMemberAccess {
        member,
        access: json!({
            "$type": "town.muni.arbiter.config.accessLevel",
            "level": level,
        }),
    }
}

pub fn remove_member(member: Member) -> JobArgs {
    JobArgs::RemoveSpaceMember { member }
}

pub fn delete_arbiter() -> JobArgs {
    JobArgs::DeleteArbiter
}

pub fn delete_space() -> JobArgs {
    JobArgs::DeleteSpace
}

pub fn member_did(did: &str) -> Member {
    Member::MemberDid(did.to_string())
}

pub fn member_local(key: &str) -> Member {
    Member::MemberLocalSpace(key.to_string())
}

pub fn member_remote(arbiter: &str, key: &str) -> Member {
    Member::MemberRemoteSpace(SpaceId {
        arbiter_did: arbiter.to_string(),
        space_key: key.to_string(),
    })
}
