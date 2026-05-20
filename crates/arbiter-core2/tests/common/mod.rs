//! Integration test harness for arbiter-core2.
//!
//! Simulates a multi-arbiter environment with auto-resolution of remote
//! delegations, mirroring the browser simulator's orchestrator.

use arbiter_core2::core::*;
use serde_json::json;

/// All-purpose test harness wrapping a multi-arbiter server.
pub struct Harness {
    pub state: ServerState,
    next_job_id: JobId,
}

impl Harness {
    pub fn new() -> Self {
        Self {
            state: ServerState::default(),
            next_job_id: 1,
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
        let mut resolved_remotes = json!({});

        for _depth in 0..10 {
            let mut arbiter = self.get_arbiter(arbiter_did);
            arbiter.process_operation(user_did, space_key, args.clone(), &resolved_remotes);

            match &arbiter.result {
                ArbiterResult::NeedsResolution { spaces, .. } => {
                    // Resolve each remote space
                    for entry in spaces {
                        let remote_key =
                            format!("{}|{}", entry.remote_arbiter_did, entry.space_key);
                        if resolved_remotes.get(&remote_key).is_some() {
                            continue;
                        }
                        let members = self.resolve_remote_members(
                            &entry.remote_arbiter_did,
                            arbiter_did,
                            &entry.space_key,
                        );
                        let arr: serde_json::Value =
                            serde_json::to_value(members).unwrap_or(json!([]));
                        resolved_remotes[&remote_key] = arr;
                    }
                    // Apply the arbiter state (job was queued)
                    self.set_arbiter(arbiter_did, arbiter);
                }
                _ => {
                    let result = ProcessResult {
                        result: arbiter.result.clone(),
                        state: self.state.clone(),
                    };
                    // If deleted, remove from state
                    match &arbiter.result {
                        ArbiterResult::Deleted => {
                            self.state.arbiters = self.state.arbiters.without(arbiter_did);
                        }
                        _ => {
                            self.set_arbiter(arbiter_did, arbiter);
                        }
                    }
                    return result;
                }
            }
        }

        panic!("Remote resolution depth limit exceeded for {arbiter_did}/{space_key}")
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
            ArbiterResult::Deleted => {} // also OK for delete operations
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

    /// Assert an operation fails with a specific error.
    pub fn assert_err(
        &mut self,
        arbiter_did: &str,
        user_did: &str,
        space_key: &str,
        args: JobArgs,
        expected: ArbiterErrorKind,
    ) {
        let r = self.process(arbiter_did, user_did, space_key, args);
        match &r.result {
            ArbiterResult::Err(e) => {
                assert!(
                    std::mem::discriminant(&e.kind) == std::mem::discriminant(&expected),
                    "Expected {expected:?}, got {e:?}"
                );
            }
            other => panic!("Expected error for {user_did}@{arbiter_did}/{space_key}, got {other:?}"),
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

    /// Assert the resolved members for a space contain specific DIDs with specific access.
    pub fn assert_member(
        &mut self,
        arbiter_did: &str,
        user_did: &str,
        space_key: &str,
        expected: &[(&str, &str)], // (did, access_level)
    ) {
        let members = self.resolved_members(arbiter_did, user_did, space_key);
        for (expected_did, expected_level) in expected {
            let found = members.iter().find(|m| {
                m.get("did").and_then(|v| v.as_str()) == Some(*expected_did)
            });
            assert!(
                found.is_some(),
                "Member {expected_did} not found in resolved members for {arbiter_did}/{space_key}"
            );
            let level = found
                .and_then(|m| m.get("access"))
                .and_then(|a| a.get("level"))
                .and_then(|v| v.as_str());
            assert_eq!(
                level,
                Some(*expected_level),
                "Member {expected_did} in {arbiter_did}/{space_key}: expected access {expected_level}, got {level:?}"
            );
        }
    }

    /// Assert that a DID is NOT in the resolved members.
    pub fn assert_no_member(&mut self, arbiter_did: &str, user_did: &str, space_key: &str, absent_did: &str) {
        let members = self.resolved_members(arbiter_did, user_did, space_key);
        let found = members.iter().any(|m| {
            m.get("did").and_then(|v| v.as_str()) == Some(absent_did)
        });
        assert!(
            !found,
            "Member {absent_did} should NOT be in resolved members for {arbiter_did}/{space_key}"
        );
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

    fn set_arbiter(&mut self, did: &str, arbiter: Arbiter) {
        self.state.arbiters = self.state.arbiters.update(did.to_string(), arbiter);
    }

    fn resolve_remote_members(
        &mut self,
        arbiter_did: &str,
        as_arbiter: &str,
        space_key: &str,
    ) -> Vec<serde_json::Value> {
        // Resolve as the calling arbiter — the remote arbiter checks permissions.
        let r = self.process(
            arbiter_did,
            as_arbiter,
            space_key,
            JobArgs::ResolveMembers,
        );
        match r.result {
            ArbiterResult::Finished(JobResult::ResolvedMembersList(response)) => {
                response.get("members")
                    .and_then(|m| m.as_array())
                    .cloned()
                    .unwrap_or_default()
            }
            _ => vec![],
        }
    }

    /// Get the version of an arbiter (for concurrency testing).
    pub fn version(&self, arbiter_did: &str) -> ArbiterVersion {
        self.get_arbiter(arbiter_did).version
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
    pub state: ServerState,
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
