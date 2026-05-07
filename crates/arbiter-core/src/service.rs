//! High-level wrapper around the arbiter core state machine for model-based testing.
//!
//! `ArbiterService` manages a pool of arbiters and exposes ergonomic methods
//! that correspond to the actions in the `arbiter_core_mbt` Quint specification.
//! Each method drives the pure `Arbiter::start_job` state machine and returns
//! `Ok(())` on success or `Err(ArbiterError)` on failure.

use std::collections::BTreeMap;

use crate::core::*;

/// A service that manages multiple arbiters and provides a high-level API
/// for the MBT test driver.
#[derive(Debug, Clone, Default)]
pub struct ArbiterService {
    /// All known arbiters, keyed by DID.
    pub arbiters: BTreeMap<ArbiterDid, Arbiter>,
    /// Next job-ID to assign (monotonically increasing).
    pub(crate) next_job_id: JobId,
}

impl ArbiterService {
    /// Create a new empty service.
    pub fn new() -> Self {
        Self {
            arbiters: BTreeMap::new(),
            next_job_id: 1,
        }
    }

    // -----------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------

    /// Allocate a fresh job ID and advance the counter.
    pub(crate) fn alloc_job_id(&mut self) -> JobId {
        let id = self.next_job_id;
        self.next_job_id += 1;
        id
    }

    /// Run a simple job on the `$admin` space (which resolves synchronously
    /// as long as there are no remote-space dependencies).
    ///
    /// **Precondition**: the arbiter with `arbiter_did` must already exist.
    ///
    /// On success the updated arbiter is written back to the map.
    /// On error the old arbiter is kept (matching Quint's `runAdminJobGetArb`).
    pub fn run_admin_job(
        &mut self,
        arbiter_did: &ArbiterDid,
        user_did: UserDid,
        args: JobArgs,
    ) -> Result<(), ArbiterError> {
        let arbiter = self.arbiters.get(arbiter_did).cloned().unwrap();
        let job_id = self.alloc_job_id();
        let upd = arbiter.start_job(user_did, ADMIN_SPACE_KEY.to_string(), job_id, args);
        match upd.result.clone() {
            ArbiterResult::FinishedJob { .. }
            | ArbiterResult::Deleted
            | ArbiterResult::Ok
            | ArbiterResult::QueuedJob { .. } => {
                self.arbiters.insert(arbiter_did.clone(), upd);
                Ok(())
            }
            ArbiterResult::Err(e) => {
                // Matches Quint's `runAdminJobGetArb`: on error, keep the old arbiter.
                Err(e)
            }
        }
    }

    // -----------------------------------------------------------------
    // Public API (mirrors the Quint MBT actions)
    // -----------------------------------------------------------------

    /// Create a new arbiter.
    ///
    /// Returns `Ok(())` if created, `Err` if one already exists.
    /// N.B.: Does NOT increment `next_job_id` – matches the Quint
    /// `createArbiterAny` action.
    pub fn create(
        &mut self,
        user_did: UserDid,
        arbiter_did: ArbiterDid,
    ) -> Result<(), ArbiterError> {
        if self.arbiters.contains_key(&arbiter_did) {
            return Err(ArbiterError::SpaceAlreadyExists);
        }
        let arbiter = Arbiter::new(arbiter_did.clone(), user_did);
        self.arbiters.insert(arbiter_did, arbiter);
        Ok(())
    }

    /// Add an admin (Owner) to the `$admin` space.
    ///
    /// **Precondition**: the arbiter must already exist (caller is responsible
    /// for handling the "missing arbiter" case).
    pub fn add_admin(
        &mut self,
        user_did: UserDid,
        arbiter_did: ArbiterDid,
        new_admin_did: UserDid,
    ) -> Result<(), ArbiterError> {
        self.run_admin_job(
            &arbiter_did,
            user_did,
            JobArgs::SetMemberAccess {
                member: Member::User(new_admin_did),
                access: Access::Owner,
            },
        )
    }

    /// Remove an admin from the `$admin` space.
    ///
    /// **Precondition**: the arbiter must already exist.
    pub fn remove_admin(
        &mut self,
        user_did: UserDid,
        arbiter_did: ArbiterDid,
        removed_admin: UserDid,
    ) -> Result<(), ArbiterError> {
        self.run_admin_job(
            &arbiter_did,
            user_did,
            JobArgs::RemoveMember {
                member: Member::User(removed_admin),
            },
        )
    }

    /// Create a space on the arbiter.
    ///
    /// **Precondition**: the arbiter must already exist.
    pub fn create_space(
        &mut self,
        user_did: UserDid,
        arbiter_did: ArbiterDid,
        space_key: SpaceKey,
    ) -> Result<(), ArbiterError> {
        let arbiter = self.arbiters.get(&arbiter_did).cloned().unwrap();
        let job_id = self.alloc_job_id();
        let upd = arbiter.start_job(user_did, space_key, job_id, JobArgs::CreateSpace);
        match upd.result.clone() {
            ArbiterResult::FinishedJob { .. }
            | ArbiterResult::Ok
            | ArbiterResult::QueuedJob { .. } => {
                self.arbiters.insert(arbiter_did, upd);
                Ok(())
            }
            ArbiterResult::Err(e) => {
                // Keep old arbiter (matches Quint runAdminJobGetArb semantics)
                Err(e)
            }
            ArbiterResult::Deleted => {
                self.arbiters.insert(arbiter_did, upd);
                Ok(())
            }
        }
    }

    /// Set a member's access level in a space.
    ///
    /// **Precondition**: the arbiter must already exist.
    pub fn set_space_member_access(
        &mut self,
        user_did: UserDid,
        arbiter_did: ArbiterDid,
        _space_key: SpaceKey,
        member: Member,
        access: Access,
    ) -> Result<(), ArbiterError> {
        self.run_admin_job(
            &arbiter_did,
            user_did,
            JobArgs::SetMemberAccess { member, access },
        )
    }

    /// Remove a member from a space.
    ///
    /// **Precondition**: the arbiter must already exist.
    pub fn remove_space_member(
        &mut self,
        user_did: UserDid,
        arbiter_did: ArbiterDid,
        _space_key: SpaceKey,
        member: Member,
    ) -> Result<(), ArbiterError> {
        self.run_admin_job(
            &arbiter_did,
            user_did,
            JobArgs::RemoveMember { member },
        )
    }

    /// Fetch the resolved member list of the `$admin` space.
    ///
    /// **Precondition**: the arbiter must already exist.
    pub fn fetch_members(
        &mut self,
        user_did: UserDid,
        arbiter_did: ArbiterDid,
    ) -> Result<(), ArbiterError> {
        self.run_admin_job(&arbiter_did, user_did, JobArgs::FetchMembers)
    }

    // -----------------------------------------------------------------
    // State helpers
    // -----------------------------------------------------------------

    /// Remove all state (for `init`).
    pub fn clear(&mut self) {
        self.arbiters.clear();
        self.next_job_id = 1;
    }
}
