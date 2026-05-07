//! Model-based testing for `arbiter-core` using the `quint-connect` framework.
//!
//! The Quint specification `arbiter_core_mbt.qnt` wraps the pure `arbiter_core`
//! state machine with `mbt::actionTaken` and `mbt::nondetPicks` variables, and
//! defines nondeterministic actions that exercise ALL job types, including:
//!   - Space creation/deletion/configuration
//!   - Member management (add/remove/set access)
//!   - Local and remote space delegation
//!   - Arbiter creation and deletion
//!   - Member list fetching
//!
//! The [`Driver`] implementation below replays each trace step by step, calling
//! the corresponding `ArbiterService` method. After every step the framework
//! compares the Quint-spec state (arbiters map + error + nextJobId) against the
//! Rust driver state; any mismatch fails the test.

use std::collections::BTreeMap;

use quint_connect::*;
use quint_connect::runner::{Config as RunnerConfig, RunConfig, run_test};
use serde::Deserialize;

use crate::{Arbiter, ArbiterDid, ArbiterService, JobArgs, Member, Access};

// ---------------------------------------------------------------------------
// State – matched against the Quint spec's state
// ---------------------------------------------------------------------------

/// The subset of the Quint `arbiter_core_mbt` state that the MBT framework
/// compares after every step.
///
/// Fields correspond 1:1 to the Quint `var` declarations (minus the
/// `mbt::*` variables which are consumed by the step framework).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ArbiterState {
    /// All known arbiters, keyed by DID.
    arbiters: BTreeMap<ArbiterDid, Arbiter>,
    /// Whether the last operation resulted in an error.
    error: bool,
    /// Monotonically increasing job-ID counter.
    #[serde(rename = "nextJobId")]
    next_job_id: i64,
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

/// The driver connects the Quint-generated traces to the Rust implementation.
#[derive(Clone, Debug, Default)]
pub struct ArbiterDriver {
    /// The Rust service implementation being tested.
    service: ArbiterService,
    /// Whether the last operation errored (mirrors the Quint `error` var).
    error: bool,
}

impl State<ArbiterDriver> for ArbiterState {
    fn from_driver(driver: &ArbiterDriver) -> Result<Self> {
        Ok(Self {
            arbiters: driver.service.arbiters.clone(),
            error: driver.error,
            next_job_id: driver.service.next_job_id,
        })
    }
}

impl Driver for ArbiterDriver {
    type State = ArbiterState;

    /// Execute one step from the Quint trace against the Rust implementation.
    fn step(&mut self, step: &Step) -> Result {
        /// Convenience macro: call a method on `self.service`, record whether
        /// it errored, and keep the updated state.
        macro_rules! call {
            ($f:ident, $($args:expr),*) => {
                let result = self.service.$f($($args),*);
                self.error = result.is_err();
            };
        }

        switch!(step {
            init => {
                self.service.clear();
                self.error = false;
            },

            // ---------------------------------------------------------------
            // Setup / state-priming
            // ---------------------------------------------------------------

            // --- ensureArbiterExists ---
            // Create arbiter only if it doesn't exist. Never errors.
            ensureArbiterExists(uid, aid) => {
                let uid: String = uid;
                let aid: String = aid;
                if !self.service.arbiters.contains_key(aid.as_str()) {
                    self.service.arbiters.insert(
                        aid.clone(),
                        Arbiter::new(aid.clone(), uid.clone()),
                    );
                }
                self.error = false;
            },

            // ---------------------------------------------------------------
            // Error-prone creation
            // ---------------------------------------------------------------

            // --- createArbiterAny ---
            // If arbiter exists: keep old, error=true, nextJobId unchanged
            // If arbiter doesn't exist: create new, error=false, nextJobId unchanged
            createArbiterAny(uid, aid) => {
                let uid: String = uid;
                let aid: String = aid;
                let existed = self.service.arbiters.contains_key(aid.as_str());
                if !existed {
                    self.service.arbiters.insert(
                        aid.clone(),
                        Arbiter::new(aid.clone(), uid.clone()),
                    );
                }
                self.error = existed;
                // next_job_id is NOT incremented for createArbiterAny
            },

            // ---------------------------------------------------------------
            // Admin member management
            // ---------------------------------------------------------------

            // --- addArbiterAdminAny ---
            addArbiterAdminAny(uid, aid, na) => {
                let uid: String = uid;
                let aid: String = aid;
                let na: String = na;
                if !self.service.arbiters.contains_key(aid.as_str()) {
                    self.service.arbiters.insert(
                        aid.clone(),
                        Arbiter::new(aid.clone(), uid.clone()),
                    );
                    self.service.alloc_job_id();
                    self.error = true;
                } else {
                    call!(run_admin_job, &aid, uid,
                        JobArgs::SetMemberAccess {
                            member: Member::User(na),
                            access: Access::Owner,
                        }
                    );
                }
            },

            // --- removeArbiterAdminAny ---
            removeArbiterAdminAny(uid, aid, rm) => {
                let uid: String = uid;
                let aid: String = aid;
                let rm: String = rm;
                if !self.service.arbiters.contains_key(aid.as_str()) {
                    self.service.arbiters.insert(
                        aid.clone(),
                        Arbiter::new(aid.clone(), uid.clone()),
                    );
                    self.service.alloc_job_id();
                    self.error = true;
                } else {
                    call!(run_admin_job, &aid, uid,
                        JobArgs::RemoveMember {
                            member: Member::User(rm),
                        }
                    );
                }
            },

            // --- setAdminMemberAccessAny ---
            // Set non-Owner access on a user in the $admin space.
            setAdminMemberAccessAny(uid, aid, na, acc) => {
                let uid: String = uid;
                let aid: String = aid;
                let na: String = na;
                let acc: Access = acc;
                if !self.service.arbiters.contains_key(aid.as_str()) {
                    self.service.arbiters.insert(
                        aid.clone(),
                        Arbiter::new(aid.clone(), uid.clone()),
                    );
                    self.service.alloc_job_id();
                    self.error = true;
                } else {
                    call!(run_admin_job, &aid, uid,
                        JobArgs::SetMemberAccess {
                            member: Member::User(na),
                            access: acc,
                        }
                    );
                }
            },

            // ---------------------------------------------------------------
            // Space lifecycle
            // ---------------------------------------------------------------

            // --- createSpaceAny ---
            // Uses the nondet-picked space key so creation can actually succeed.
            createSpaceAny(uid, aid, sk) => {
                let uid: String = uid;
                let aid: String = aid;
                let sk: String = sk;
                if !self.service.arbiters.contains_key(aid.as_str()) {
                    self.service.arbiters.insert(
                        aid.clone(),
                        Arbiter::new(aid.clone(), uid.clone()),
                    );
                    self.service.alloc_job_id();
                    self.error = true;
                } else {
                    call!(run_job, &aid, uid, sk.clone(), JobArgs::CreateSpace);
                }
            },

            // --- createAdminSpaceAlwaysErrors ---
            // Tries to create the $admin space (always errors).
            createAdminSpaceAlwaysErrors(uid, aid) => {
                let uid: String = uid;
                let aid: String = aid;
                if !self.service.arbiters.contains_key(aid.as_str()) {
                    self.service.arbiters.insert(
                        aid.clone(),
                        Arbiter::new(aid.clone(), uid.clone()),
                    );
                    self.service.alloc_job_id();
                    self.error = true;
                } else {
                    call!(run_admin_job, &aid, uid, JobArgs::CreateSpace);
                }
            },

            // --- deleteSpaceAny ---
            // Deletes a non-admin space.
            deleteSpaceAny(uid, aid, sk) => {
                let uid: String = uid;
                let aid: String = aid;
                let sk: String = sk;
                if !self.service.arbiters.contains_key(aid.as_str()) {
                    self.service.arbiters.insert(
                        aid.clone(),
                        Arbiter::new(aid.clone(), uid.clone()),
                    );
                    self.service.alloc_job_id();
                    self.error = true;
                } else {
                    call!(run_job, &aid, uid, sk, JobArgs::DeleteSpace);
                }
            },

            // --- configureSpaceAny ---
            // Configures a space's publicRecords / publicMembers flags.
            configureSpaceAny(uid, aid, sk, pubRec, pubMem) => {
                let uid: String = uid;
                let aid: String = aid;
                let _sk: String = sk;
                let pubRec: bool = pubRec;
                let pubMem: bool = pubMem;
                if !self.service.arbiters.contains_key(aid.as_str()) {
                    self.service.arbiters.insert(
                        aid.clone(),
                        Arbiter::new(aid.clone(), uid.clone()),
                    );
                    self.service.alloc_job_id();
                    self.error = true;
                } else {
                    call!(run_admin_job, &aid, uid,
                        JobArgs::ConfigureSpace(crate::SpaceConfig {
                            public_records: pubRec,
                            public_members: pubMem,
                        })
                    );
                }
            },

            // ---------------------------------------------------------------
            // Member management (any space)
            // ---------------------------------------------------------------

            // --- setSpaceMemberAccessAny ---
            // Uses the picked space key directly (not just $admin).
            // Nondet picks: uid, aid, sk, mbr, acc
            setSpaceMemberAccessAny(uid, aid, sk, mbr, acc) => {
                let uid: String = uid;
                let aid: String = aid;
                let sk: String = sk;
                let mbr: Member = mbr;
                let acc: Access = acc;
                if !self.service.arbiters.contains_key(aid.as_str()) {
                    self.service.arbiters.insert(
                        aid.clone(),
                        Arbiter::new(aid.clone(), uid.clone()),
                    );
                    self.service.alloc_job_id();
                    self.error = true;
                } else {
                    call!(set_space_member_access, uid, aid, sk, mbr, acc);
                }
            },

            // --- removeSpaceMemberAny ---
            // Uses the picked space key directly.
            // Nondet picks: uid, aid, sk, mbr
            removeSpaceMemberAny(uid, aid, sk, mbr) => {
                let uid: String = uid;
                let aid: String = aid;
                let sk: String = sk;
                let mbr: Member = mbr;
                if !self.service.arbiters.contains_key(aid.as_str()) {
                    self.service.arbiters.insert(
                        aid.clone(),
                        Arbiter::new(aid.clone(), uid.clone()),
                    );
                    self.service.alloc_job_id();
                    self.error = true;
                } else {
                    call!(remove_space_member, uid, aid, sk, mbr);
                }
            },

            // ---------------------------------------------------------------
            // Local and remote delegation
            // ---------------------------------------------------------------

            // --- addLocalSpaceDelegateAny ---
            // Adds a local space as a delegate member (MemberLocalSpace).
            // Nondet picks: uid, aid, sk, delegateSk, acc
            addLocalSpaceDelegateAny(uid, aid, sk, delegateSk, acc) => {
                let uid: String = uid;
                let aid: String = aid;
                let sk: String = sk;
                let delegateSk: String = delegateSk;
                let acc: Access = acc;
                if !self.service.arbiters.contains_key(aid.as_str()) {
                    self.service.arbiters.insert(
                        aid.clone(),
                        Arbiter::new(aid.clone(), uid.clone()),
                    );
                    self.service.alloc_job_id();
                    self.error = true;
                } else {
                    call!(set_space_member_access, uid, aid, sk,
                        Member::LocalSpace(delegateSk), acc
                    );
                }
            },

            // --- addRemoteSpaceDelegateAny ---
            // Adds a remote space as a delegate member (MemberRemoteSpace).
            // Nondet picks: uid, aid, sk, remoteAid, remoteSk, acc
            addRemoteSpaceDelegateAny(uid, aid, sk, remoteAid, remoteSk, acc) => {
                let uid: String = uid;
                let aid: String = aid;
                let sk: String = sk;
                let remoteAid: String = remoteAid;
                let remoteSk: String = remoteSk;
                let acc: Access = acc;
                if !self.service.arbiters.contains_key(aid.as_str()) {
                    self.service.arbiters.insert(
                        aid.clone(),
                        Arbiter::new(aid.clone(), uid.clone()),
                    );
                    self.service.alloc_job_id();
                    self.error = true;
                } else {
                    call!(set_space_member_access, uid, aid, sk,
                        Member::RemoteSpace(crate::SpaceId {
                            arbiter_did: remoteAid,
                            space_key: remoteSk,
                        }),
                        acc
                    );
                }
            },

            // ---------------------------------------------------------------
            // Fetch members (any space)
            // ---------------------------------------------------------------

            // --- fetchMembersAny ---
            // Fetches members of the picked space (not just $admin).
            // Nondet picks: uid, aid, sk
            fetchMembersAny(uid, aid, sk) => {
                let uid: String = uid;
                let aid: String = aid;
                let sk: String = sk;
                if !self.service.arbiters.contains_key(aid.as_str()) {
                    self.service.arbiters.insert(
                        aid.clone(),
                        Arbiter::new(aid.clone(), uid.clone()),
                    );
                    self.service.alloc_job_id();
                    self.error = true;
                } else {
                    call!(fetch_members, uid, aid, sk);
                }
            },

            // ---------------------------------------------------------------
            // Arbiter deletion
            // ---------------------------------------------------------------

            // --- deleteArbiterAny ---
            // Deletes an arbiter (only works if user is sole owner).
            // Nondet picks: uid, aid
            deleteArbiterAny(uid, aid) => {
                let uid: String = uid;
                let aid: String = aid;
                if !self.service.arbiters.contains_key(aid.as_str()) {
                    self.service.arbiters.insert(
                        aid.clone(),
                        Arbiter::new(aid.clone(), uid.clone()),
                    );
                    self.service.alloc_job_id();
                    self.error = true;
                } else {
                    call!(run_admin_job, &aid, uid, JobArgs::DeleteArbiter);
                }
            },
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get the absolute path to the MBT Quint specification.
fn spec_path() -> String {
    // CARGO_MANIFEST_DIR = <workspace>/crates/arbiter-core
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // Go up from <workspace>/crates/arbiter-core to <workspace>
    let workspace_root = manifest_dir
        .parent()
        .expect("CARGO_MANIFEST_DIR/crates should have parent")
        .parent()
        .expect("CARGO_MANIFEST_DIR should have parent");
    workspace_root
        .join("spec")
        .join("arbiter")
        .join("arbiter_core_mbt.qnt")
        .to_string_lossy()
        .to_string()
}

/// Run simulation with detailed error reporting.
fn run_simulation_test(driver: ArbiterDriver, max_samples: usize, max_steps: usize) {
    let config = RunnerConfig {
        test_name: "simulation".to_string(),
        gen_config: RunConfig {
            spec: spec_path(),
            main: Some("arbiter_core_mbt".to_string()),
            init: None,
            step: None,
            max_samples: Some(max_samples),
            max_steps: Some(max_steps),
            seed: quint_connect::runner::gen_random_seed().to_string(),
        },
    };

    match run_test(driver, config) {
        Ok(()) => {}
        Err(err) => {
            eprintln!("\n=== Full error chain ===");
            for (i, e) in err.chain().enumerate() {
                eprintln!("  [{}] {}", i, e);
            }
            eprintln!("========================\n");
            panic!("Test failed: {:#}", err);
        }
    }
}

/// Run a specific quint test with detailed error reporting.
fn run_quint_test(driver: ArbiterDriver, test_name: &str) {
    let config = RunnerConfig {
        test_name: test_name.to_string(),
        gen_config: quint_connect::runner::TestConfig {
            spec: spec_path(),
            main: Some("arbiter_core_mbt".to_string()),
            test: test_name.to_string(),
            max_samples: Some(1),
            seed: quint_connect::runner::gen_random_seed().to_string(),
        },
    };

    match run_test(driver, config) {
        Ok(()) => {}
        Err(err) => {
            eprintln!("\n=== Full error chain ===");
            for (i, e) in err.chain().enumerate() {
                eprintln!("  [{}] {}", i, e);
            }
            eprintln!("========================\n");
            panic!("Test '{}' failed: {:#}", test_name, err);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn simulation() {
    // Increased max_steps and max_samples for better coverage of all 14+ actions
    run_simulation_test(ArbiterDriver::default(), 50, 700);
}

// NOTE: The `quint_test` tests below require `quint test` to support the `--mbt`
// flag for producing structured nondet picks.  The current version of quint does
// not support `--mbt` on the `test` subcommand, so these will fail with:
//   "Expected nondet picks to be a `Value::Record`"
//
// Once quint adds `--mbt` support to `quint test`, uncomment these tests.
// Until then, the `simulation` test (which uses `quint run --mbt`) provides
// thorough MBT coverage via random trace generation.

// #[test]
// fn test_create_then_add_admin() {
//     run_quint_test(ArbiterDriver::default(), "createThenAddAdminTest");
// }
//
// #[test]
// fn test_create_duplicate_arbiter() {
//     run_quint_test(ArbiterDriver::default(), "createDuplicateArbiterTest");
// }
//
// #[test]
// fn test_add_then_remove_admin() {
//     run_quint_test(ArbiterDriver::default(), "addThenRemoveAdminTest");
// }
//
// #[test]
// fn test_create_space() {
//     run_quint_test(ArbiterDriver::default(), "createSpaceTest");
// }
//
// #[test]
// fn test_create_then_delete_space() {
//     run_quint_test(ArbiterDriver::default(), "createThenDeleteSpaceTest");
// }
//
// #[test]
// fn test_permission_denied() {
//     run_quint_test(ArbiterDriver::default(), "permissionDeniedAddAdminTest");
// }
//
// #[test]
// fn test_fetch_members_nonexistent() {
//     run_quint_test(
//         ArbiterDriver::default(),
//         "fetchMembersNonexistentArbiterTest",
//     );
// }
//
// #[test]
// fn test_configure_space_public() {
//     run_quint_test(ArbiterDriver::default(), "configureSpacePublicTest");
// }
//
// #[test]
// fn test_configure_non_existent_space() {
//     run_quint_test(ArbiterDriver::default(), "configureNonExistentSpaceTest");
// }
//
// #[test]
// fn test_delete_arbiter_as_sole_owner() {
//     run_quint_test(ArbiterDriver::default(), "deleteArbiterAsSoleOwnerTest");
// }
//
// #[test]
// fn test_delete_arbiter_with_two_owners_fails() {
//     run_quint_test(ArbiterDriver::default(), "deleteArbiterWithTwoOwnersFailsTest");
// }
//
// #[test]
// fn test_add_local_space_delegation() {
//     run_quint_test(ArbiterDriver::default(), "addLocalSpaceDelegationTest");
// }
//
// #[test]
// fn test_add_remote_space_delegation() {
//     run_quint_test(ArbiterDriver::default(), "addRemoteSpaceDelegationTest");
// }
//
// #[test]
// fn test_add_self_referencing_remote_space_fails() {
//     run_quint_test(ArbiterDriver::default(), "addSelfReferencingRemoteSpaceFailsTest");
// }
