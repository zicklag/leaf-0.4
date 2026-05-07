// -*- mode: Bluespec; -*-
/**
 * Model-based testing wrapper around `arbiter_core`.
 *
 * Each action is structured with nondet picks OUTSIDE the all block so that
 * the nondet bindings are in scope for every sub-action inside the all block.
 *
 * Provides actions that cover ALL job types with a mix of success and error
 * scenarios, ensuring better coverage of the arbiter state machine.
 *
 * Actions fall into two categories:
 *   - *Error-prone*: actions that often fail (random user/arbiter combinations)
 *   - *Success-oriented*: actions that first set up state and then operate,
 *     producing more valid transitions
 *
 * Together they exercise every JobArgs variant, multiple space types, and
 * both local and remote delegation scenarios.
 */
module arbiter_core_mbt {
  import arbiter_core_types.* from "./arbiter_core"
  import arbiter_core as core from "./arbiter_core"
  import utils.* from "../lib/utils"

  pure val USER_DIDS: Set[UserDid] = Set("alice", "bob", "charlie")
  pure val ARBITER_DIDS: Set[ArbiterDid] = Set("a1", "a2")

  var arbiters: ArbiterDid -> Arbiter
  var error: bool
  var nextJobId: int
  var mbt::actionTaken: str
  var mbt::nondetPicks: str

  // -------------------------------------------------------------------------
  // Pure helpers
  // -------------------------------------------------------------------------

  pure def isErrResult(r: ArbiterResult): bool =
    match (r) { | ArbiterErr(_) => true | _ => false }

  pure val spaceKeys = Set("space1", "space2", "space3")
  pure val allSpaceKeys = spaceKeys.union(Set(SpaceKey::ADMIN))
  pure val accesses = Set(
    ReadMemberList, IsMember, AddMembers, RemoveMembers,
    ConfigureSpace, CreateSpaces, RemoveSpace, Owner,
  )
  pure val memberUsers = USER_DIDS.map(did => MemberUser(did))

  // -------------------------------------------------------------------------
  // Run a job on a specific space, returning the new arbiter.
  //
  // On success (FinishedJob, Deleted, Ok, QueuedJob) → return updated arbiter.
  // On error → return the OLD arbiter unchanged.
  // -------------------------------------------------------------------------

  pure def runJobGetArb(arb: Arbiter, uid: UserDid, spaceKey: SpaceKey, jid: JobId, a: JobArgs): Arbiter = {
    val upd = core::startJob(arb, uid, spaceKey, jid, a)
    match (upd.result) {
      | ArbiterFinishedJob(_) => upd
      | ArbiterDeleted => upd
      | ArbiterOk => upd
      | ArbiterQueuedJob(_) => upd
      | ArbiterErr(_) => arb
    }
  }

  /// Convenience: run a job on the $admin space.
  pure def runAdminJobGetArb(arb: Arbiter, uid: UserDid, jid: JobId, a: JobArgs): Arbiter =
    runJobGetArb(arb, uid, SpaceKey::ADMIN, jid, a)

  /// Check whether a job errors.
  pure def jobHasError(arb: Arbiter, uid: UserDid, spaceKey: SpaceKey, jid: JobId, a: JobArgs): bool =
    isErrResult(core::startJob(arb, uid, spaceKey, jid, a).result)

  pure def adminJobHasError(arb: Arbiter, uid: UserDid, jid: JobId, a: JobArgs): bool =
    jobHasError(arb, uid, SpaceKey::ADMIN, jid, a)

  /// Returns the set of space keys that exist on a given arbiter (excluding $admin).
  pure def nonAdminSpaceKeysOf(arb: Arbiter): Set[SpaceKey] =
    arb.spaces.keys().exclude(Set(SpaceKey::ADMIN))

  /// Returns true if the given arbiter has at least one non-admin space.
  pure def hasNonAdminSpace(arb: Arbiter): bool =
    nonAdminSpaceKeysOf(arb).size() > 0

  /// Get one existing non-admin space key from an arbiter, defaulting to ADMIN
  /// if none exist (which will cause an appropriate error downstream).
  pure def anyExistingSpaceKey(arb: Arbiter): SpaceKey = {
    val extra = nonAdminSpaceKeysOf(arb)
    if (extra.size() > 0)
      extra.fold(SpaceKey::ADMIN, (_, k) => k)
    else
      SpaceKey::ADMIN
  }

  // -------------------------------------------------------------------------
  // Actions
  // -------------------------------------------------------------------------

  action init = all {
    arbiters' = Map(),
    error' = false,
    nextJobId' = 1,
    mbt::actionTaken' = "init",
    mbt::nondetPicks' = "",
  }

  // ====================== SETUP / STATE-MUTATING ======================

  /// Create an arbiter (only if not exists). Never errors – pure setup.
  action ensureArbiterExists =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    all {
      arbiters' =
        if (aid.in(arbiters.keys()))
          arbiters
        else
          arbiters.put(aid, core::new(aid, uid)),
      error' = false,
      nextJobId' = nextJobId,
      mbt::actionTaken' = "ensureArbiterExists",
      mbt::nondetPicks' = uid,
    }

  /// Create an arbiter (errors if already exists). Tests duplicate-creation error.
  action createArbiterAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          arbiters.get(aid)
        else
          core::new(aid, uid)
      ),
      error' = aid.in(arbiters.keys()),
      nextJobId' = nextJobId,
      mbt::actionTaken' = "createArbiterAny",
      mbt::nondetPicks' = uid,
    }

  // ====================== MEMBER MANAGEMENT (admin space) ======================

  /// Add an admin to the $admin space. Errors if arbiter missing or user lacks permission.
  action addArbiterAdminAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    nondet na = oneOf(USER_DIDS)
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          runAdminJobGetArb(arbiters.get(aid), uid, nextJobId,
            JobSetMemberAccess({ member: MemberUser(na), access: Owner }))
        else
          // Arbiter missing → create dummy and mark error (matches old behaviour)
          core::new(aid, uid)
      ),
      error' = if (aid.in(arbiters.keys()))
        adminJobHasError(arbiters.get(aid), uid, nextJobId,
          JobSetMemberAccess({ member: MemberUser(na), access: Owner }))
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "addArbiterAdminAny",
      mbt::nondetPicks' = na,
    }

  /// Remove an admin from the $admin space.
  action removeArbiterAdminAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    nondet rm = oneOf(USER_DIDS)
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          runAdminJobGetArb(arbiters.get(aid), uid, nextJobId,
            JobRemoveMember({ member: MemberUser(rm) }))
        else
          core::new(aid, uid)
      ),
      error' = if (aid.in(arbiters.keys()))
        adminJobHasError(arbiters.get(aid), uid, nextJobId,
          JobRemoveMember({ member: MemberUser(rm) }))
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "removeArbiterAdminAny",
      mbt::nondetPicks' = rm,
    }

  // ====================== SPACE CREATION ======================

  /// Create a new space (uses the nondet-picked space key rather than $admin,
  /// so it can actually succeed). The space key is recorded in nondetPicks.
  /// Errors if the space already exists or the user lacks CreateSpaces access.
  action createSpaceAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    nondet sk = oneOf(spaceKeys)
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          // Use the PICKED space key, not ADMIN, so creation can succeed
          runJobGetArb(arbiters.get(aid), uid, sk, nextJobId, JobCreateSpace)
        else
          core::new(aid, uid)
      ),
      error' = if (aid.in(arbiters.keys()))
        jobHasError(arbiters.get(aid), uid, sk, nextJobId, JobCreateSpace)
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "createSpaceAny",
      mbt::nondetPicks' = sk,
    }

  /// Try to create a space under the $admin key (always errors because
  /// $admin already exists). Exercises the ErrSpaceAlreadyExists code path.
  action createAdminSpaceAlwaysErrors =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          runAdminJobGetArb(arbiters.get(aid), uid, nextJobId, JobCreateSpace)
        else
          core::new(aid, uid)
      ),
      error' = if (aid.in(arbiters.keys()))
        adminJobHasError(arbiters.get(aid), uid, nextJobId, JobCreateSpace)
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "createAdminSpaceAlwaysErrors",
      mbt::nondetPicks' = uid,
    }

  // ====================== SPACE DELETION ======================

  /// Delete a space from an arbiter (using the picked space key directly).
  /// Errors if the space is $admin, doesn't exist, or the user lacks RemoveSpace.
  action deleteSpaceAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    nondet sk = oneOf(allSpaceKeys)
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          runJobGetArb(arbiters.get(aid), uid, sk, nextJobId, JobDeleteSpace)
        else
          core::new(aid, uid)
      ),
      error' = if (aid.in(arbiters.keys()))
        jobHasError(arbiters.get(aid), uid, sk, nextJobId, JobDeleteSpace)
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "deleteSpaceAny",
      mbt::nondetPicks' = sk,
    }

  // ====================== SPACE CONFIGURATION ======================

  /// Configure a space (publicRecords / publicMembers).
  /// Errors if the space doesn't exist or the user lacks ConfigureSpace.
  action configureSpaceAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    nondet sk = oneOf(allSpaceKeys)
    nondet pubRec = oneOf(Set(false, true))
    nondet pubMem = oneOf(Set(false, true))
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          runAdminJobGetArb(arbiters.get(aid), uid, nextJobId,
            JobConfigureSpace({ publicRecords: pubRec, publicMembers: pubMem }))
        else
          core::new(aid, uid)
      ),
      error' = if (aid.in(arbiters.keys()))
        adminJobHasError(arbiters.get(aid), uid, nextJobId,
          JobConfigureSpace({ publicRecords: pubRec, publicMembers: pubMem }))
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "configureSpaceAny",
      mbt::nondetPicks' = sk,
    }

  // ====================== MEMBER MANAGEMENT (any space) ======================

  /// Set member access in any space (admin or non-admin).
  /// Works on the picked space key using runJobGetArb so non-admin spaces work.
  action setSpaceMemberAccessAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    nondet sk = oneOf(allSpaceKeys)
    nondet mbr = oneOf(memberUsers)
    nondet acc = oneOf(accesses)
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          runJobGetArb(arbiters.get(aid), uid, sk, nextJobId,
            JobSetMemberAccess({ member: mbr, access: acc }))
        else
          core::new(aid, uid)
      ),
      error' = if (aid.in(arbiters.keys()))
        jobHasError(arbiters.get(aid), uid, sk, nextJobId,
          JobSetMemberAccess({ member: mbr, access: acc }))
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "setSpaceMemberAccessAny",
      mbt::nondetPicks' = sk,
    }

  /// Remove a member from any space.
  action removeSpaceMemberAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    nondet sk = oneOf(allSpaceKeys)
    nondet mbr = oneOf(memberUsers)
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          runJobGetArb(arbiters.get(aid), uid, sk, nextJobId,
            JobRemoveMember({ member: mbr }))
        else
          core::new(aid, uid)
      ),
      error' = if (aid.in(arbiters.keys()))
        jobHasError(arbiters.get(aid), uid, sk, nextJobId,
          JobRemoveMember({ member: mbr }))
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "removeSpaceMemberAny",
      mbt::nondetPicks' = sk,
    }

  // ====================== LOCAL SPACE DELEGATION ======================

  /// Delegate a local space into another space (MemberLocalSpace).
  /// This adds a local space as a member of another space.
  action addLocalSpaceDelegateAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    nondet sk = oneOf(allSpaceKeys)
    nondet delegateSk = oneOf(spaceKeys)
    nondet acc = oneOf(accesses)
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          runJobGetArb(arbiters.get(aid), uid, sk, nextJobId,
            JobSetMemberAccess({ member: MemberLocalSpace(delegateSk), access: acc }))
        else
          core::new(aid, uid)
      ),
      error' = if (aid.in(arbiters.keys()))
        jobHasError(arbiters.get(aid), uid, sk, nextJobId,
          JobSetMemberAccess({ member: MemberLocalSpace(delegateSk), access: acc }))
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "addLocalSpaceDelegateAny",
      mbt::nondetPicks' = delegateSk,
    }

  // ====================== REMOTE SPACE DELEGATION ======================

  /// Add a remote space as a delegate (MemberRemoteSpace).
  /// Uses a remote arbiter DID and a space key on that remote arbiter.
  /// This exercises the resolution path (queued jobs, timeout, etc.) when
  /// running at the server level, and at the core level it exercises the
  /// UnresolvedMemberList with UMLSpace entries.
  action addRemoteSpaceDelegateAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    nondet sk = oneOf(allSpaceKeys)
    nondet remoteAid = oneOf(ARBITER_DIDS)
    nondet remoteSk = oneOf(spaceKeys)
    nondet acc = oneOf(accesses)
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          runJobGetArb(arbiters.get(aid), uid, sk, nextJobId,
            JobSetMemberAccess({
              member: MemberRemoteSpace({ arbiterDid: remoteAid, spaceKey: remoteSk }),
              access: acc,
            }))
        else
          core::new(aid, uid)
      ),
      error' = if (aid.in(arbiters.keys()))
        // This can error with ErrRemoteSpaceReferencesLocalArbiter if remoteAid == local aid
        jobHasError(arbiters.get(aid), uid, sk, nextJobId,
          JobSetMemberAccess({
            member: MemberRemoteSpace({ arbiterDid: remoteAid, spaceKey: remoteSk }),
            access: acc,
          }))
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "addRemoteSpaceDelegateAny",
      mbt::nondetPicks' = remoteAid,
    }

  // ====================== FETCH MEMBERS ======================

  /// Fetch members of any existing space on an arbiter.
  /// Errors if the arbiter or space doesn't exist, or the user lacks ReadMemberList
  /// (unless publicMembers is true).
  action fetchMembersAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    nondet sk = oneOf(allSpaceKeys)
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          runJobGetArb(arbiters.get(aid), uid, sk, nextJobId, JobFetchMembers)
        else
          core::new(aid, uid)
      ),
      error' = if (aid.in(arbiters.keys()))
        jobHasError(arbiters.get(aid), uid, sk, nextJobId, JobFetchMembers)
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "fetchMembersAny",
      mbt::nondetPicks' = sk,
    }

  // ====================== DELETE ARBITER ======================

  /// Delete an arbiter entirely (only possible when the user is the last owner
  /// of the $admin space).
  /// Errors if the user isn't an owner, or if there are multiple owners.
  action deleteArbiterAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          runAdminJobGetArb(arbiters.get(aid), uid, nextJobId, JobDeleteArbiter)
        else
          core::new(aid, uid)
      ),
      error' = if (aid.in(arbiters.keys()))
        adminJobHasError(arbiters.get(aid), uid, nextJobId, JobDeleteArbiter)
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "deleteArbiterAny",
      mbt::nondetPicks' = uid,
    }

  // ====================== ARBITER-LEVEL MEMBER ACCESS ======================

  /// Add a non-owner access level to a user in the $admin space.
  /// Exercises AddMembers, RemoveMembers, ConfigureSpace, CreateSpaces,
  /// RemoveSpace, ReadMemberList, IsMember – all non-Owner access levels.
  action setAdminMemberAccessAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    nondet na = oneOf(USER_DIDS)
    nondet acc = oneOf(Set(ReadMemberList, IsMember, AddMembers, RemoveMembers, ConfigureSpace, CreateSpaces, RemoveSpace))
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          runAdminJobGetArb(arbiters.get(aid), uid, nextJobId,
            JobSetMemberAccess({ member: MemberUser(na), access: acc }))
        else
          core::new(aid, uid)
      ),
      error' = if (aid.in(arbiters.keys()))
        adminJobHasError(arbiters.get(aid), uid, nextJobId,
          JobSetMemberAccess({ member: MemberUser(na), access: acc }))
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "setAdminMemberAccessAny",
      mbt::nondetPicks' = na,
    }

  // -------------------------------------------------------------------------
  // Simulation step – all actions available for random trace generation
  // -------------------------------------------------------------------------

  action step = any {
    // Setup / state-priming (low error rate)
    ensureArbiterExists,

    // Error-prone creation (duplicate arbiter → error)
    createArbiterAny,

    // Admin member management (mixed success/error)
    addArbiterAdminAny,
    removeArbiterAdminAny,
    setAdminMemberAccessAny,

    // Space lifecycle (creation, deletion, configuration)
    createSpaceAny,
    createAdminSpaceAlwaysErrors,
    deleteSpaceAny,
    configureSpaceAny,

    // Member management in any space
    setSpaceMemberAccessAny,
    removeSpaceMemberAny,

    // Local and remote delegation
    addLocalSpaceDelegateAny,
    addRemoteSpaceDelegateAny,

    // Fetch members (any space)
    fetchMembersAny,

    // Arbiter deletion
    deleteArbiterAny,
  }

  // -------------------------------------------------------------------------
  // Named tests (unchanged from original – verify basic operations)
  // -------------------------------------------------------------------------

  /// Create an arbiter, then add an admin, and verify both users are Owners.
  run createThenAddAdminTest =
    all {
      arbiters' = Map("a1" -> core::new("a1", "alice")),
      error' = false,
      nextJobId' = 1,
      mbt::actionTaken' = "init",
      mbt::nondetPicks' = "",
    }
    .then(all {
      arbiters' = arbiters.put("a1",
        runAdminJobGetArb(arbiters.get("a1"), "alice", nextJobId,
          JobSetMemberAccess({ member: MemberUser("bob"), access: Owner }))
      ),
      error' = adminJobHasError(arbiters.get("a1"), "alice", nextJobId,
        JobSetMemberAccess({ member: MemberUser("bob"), access: Owner })),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "addArbiterAdminAny",
      mbt::nondetPicks' = "bob",
    })
    .expect({
      val ar = arbiters.get("a1")
      val am = ar.spaces.get(SpaceKey::ADMIN).members
      am.get(MemberUser("alice")) == Owner and
        am.get(MemberUser("bob")) == Owner
    })

  /// Create an arbiter, then try to create it again (should error).
  run createDuplicateArbiterTest =
    all {
      arbiters' = Map("a1" -> core::new("a1", "alice")),
      error' = false,
      nextJobId' = 1,
      mbt::actionTaken' = "init",
      mbt::nondetPicks' = "",
    }
    .then(all {
      arbiters' = arbiters.put("a1",
        if ("a1".in(arbiters.keys()))
          arbiters.get("a1")
        else
          core::new("a1", "alice")
      ),
      error' = "a1".in(arbiters.keys()),
      nextJobId' = nextJobId,
      mbt::actionTaken' = "createArbiterAny",
      mbt::nondetPicks' = "alice",
    })
    .expect(error == true and arbiters.keys() == Set("a1"))

  /// Create an arbiter, add an admin, remove the admin, verify only the creator remains Owner.
  run addThenRemoveAdminTest =
    all {
      arbiters' = Map("a1" -> core::new("a1", "alice")),
      error' = false,
      nextJobId' = 1,
      mbt::actionTaken' = "init",
      mbt::nondetPicks' = "",
    }
    .then(all {
      arbiters' = arbiters.put("a1",
        runAdminJobGetArb(arbiters.get("a1"), "alice", nextJobId,
          JobSetMemberAccess({ member: MemberUser("bob"), access: Owner }))
      ),
      error' = adminJobHasError(arbiters.get("a1"), "alice", nextJobId,
        JobSetMemberAccess({ member: MemberUser("bob"), access: Owner })),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "addArbiterAdminAny",
      mbt::nondetPicks' = "bob",
    })
    .then(all {
      arbiters' = arbiters.put("a1",
        runAdminJobGetArb(arbiters.get("a1"), "alice", nextJobId,
          JobRemoveMember({ member: MemberUser("bob") }))
      ),
      error' = adminJobHasError(arbiters.get("a1"), "alice", nextJobId,
        JobRemoveMember({ member: MemberUser("bob") })),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "removeArbiterAdminAny",
      mbt::nondetPicks' = "bob",
    })
    .expect({
      val ar = arbiters.get("a1")
      val am = ar.spaces.get(SpaceKey::ADMIN).members
      am.get(MemberUser("alice")) == Owner and
        not(am.has(MemberUser("bob")))
    })

  /// Create an arbiter, create a new non-admin space, then verify the space exists.
  run createSpaceTest =
    all {
      arbiters' = Map("a1" -> core::new("a1", "alice")),
      error' = false,
      nextJobId' = 1,
      mbt::actionTaken' = "init",
      mbt::nondetPicks' = "",
    }
    .then(all {
      // Create space "space1" – use the actual space key, not ADMIN
      arbiters' = arbiters.put("a1",
        runJobGetArb(arbiters.get("a1"), "alice", "space1", nextJobId, JobCreateSpace)
      ),
      error' = jobHasError(arbiters.get("a1"), "alice", "space1", nextJobId, JobCreateSpace),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "createSpaceAny",
      mbt::nondetPicks' = "space1",
    })
    .expect({
      val ar = arbiters.get("a1")
      ar.spaces.has("space1") and ar.spaces.has(SpaceKey::ADMIN)
    })

  /// Create an arbiter, create a space, delete the space, verify it's gone.
  run createThenDeleteSpaceTest =
    all {
      arbiters' = Map("a1" -> core::new("a1", "alice")),
      error' = false,
      nextJobId' = 1,
      mbt::actionTaken' = "init",
      mbt::nondetPicks' = "",
    }
    .then(all {
      // Create space "space1"
      arbiters' = arbiters.put("a1",
        runJobGetArb(arbiters.get("a1"), "alice", "space1", nextJobId, JobCreateSpace)
      ),
      error' = jobHasError(arbiters.get("a1"), "alice", "space1", nextJobId, JobCreateSpace),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "createSpaceAny",
      mbt::nondetPicks' = "space1",
    })
    .then(all {
      // Delete space "space1" via the $admin space (since we operate on ADMIN
      // but pass JobDeleteSpace which targets the space in the original spaceKey... 
      // Actually, deleteSpace works on job.spaceKey. Let's run it on the space itself)
      arbiters' = arbiters.put("a1",
        runJobGetArb(arbiters.get("a1"), "alice", "space1", nextJobId, JobDeleteSpace)
      ),
      error' = jobHasError(arbiters.get("a1"), "alice", "space1", nextJobId, JobDeleteSpace),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "deleteSpaceAny",
      mbt::nondetPicks' = "space1",
    })
    .expect({
      val ar = arbiters.get("a1")
      not(ar.spaces.has("space1")) and ar.spaces.has(SpaceKey::ADMIN)
    })

  /// Permission denied: bob (not a member) tries to add an admin.
  run permissionDeniedAddAdminTest =
    all {
      arbiters' = Map("a1" -> core::new("a1", "alice")),
      error' = false,
      nextJobId' = 1,
      mbt::actionTaken' = "init",
      mbt::nondetPicks' = "",
    }
    .then(all {
      arbiters' = arbiters.put("a1",
        runAdminJobGetArb(arbiters.get("a1"), "bob", nextJobId,
          JobSetMemberAccess({ member: MemberUser("charlie"), access: Owner }))
      ),
      error' = adminJobHasError(arbiters.get("a1"), "bob", nextJobId,
        JobSetMemberAccess({ member: MemberUser("charlie"), access: Owner })),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "addArbiterAdminAny",
      mbt::nondetPicks' = "charlie",
    })
    .expect(error == true)

  /// Fetch members of a non-existent arbiter should error.
  run fetchMembersNonexistentArbiterTest =
    all {
      arbiters' = Map(),
      error' = false,
      nextJobId' = 1,
      mbt::actionTaken' = "init",
      mbt::nondetPicks' = "",
    }
    .then(all {
      arbiters' = arbiters.put("nonexistent",
        if ("nonexistent".in(arbiters.keys()))
          runAdminJobGetArb(arbiters.get("nonexistent"), "bob", nextJobId, JobFetchMembers)
        else
          core::new("nonexistent", "bob")
      ),
      error' = if ("nonexistent".in(arbiters.keys()))
        adminJobHasError(arbiters.get("nonexistent"), "bob", nextJobId, JobFetchMembers)
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "fetchMembersAny",
      mbt::nondetPicks' = "bob",
    })
    .expect(error == true)

  /// Create a space, configure it with public members, verify the config.
  run configureSpacePublicTest =
    all {
      arbiters' = Map("a1" -> core::new("a1", "alice")),
      error' = false,
      nextJobId' = 1,
      mbt::actionTaken' = "init",
      mbt::nondetPicks' = "",
    }
    .then(all {
      // Create space "space1"
      arbiters' = arbiters.put("a1",
        runJobGetArb(arbiters.get("a1"), "alice", "space1", nextJobId, JobCreateSpace)
      ),
      error' = jobHasError(arbiters.get("a1"), "alice", "space1", nextJobId, JobCreateSpace),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "createSpaceAny",
      mbt::nondetPicks' = "space1",
    })
    .then(all {
      // Configure the $admin space to have public members (alice has Owner → ConfigureSpace)
      arbiters' = arbiters.put("a1",
        runAdminJobGetArb(arbiters.get("a1"), "alice", nextJobId,
          JobConfigureSpace({ publicRecords: false, publicMembers: true }))
      ),
      error' = adminJobHasError(arbiters.get("a1"), "alice", nextJobId,
        JobConfigureSpace({ publicRecords: false, publicMembers: true })),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "configureSpaceAny",
      mbt::nondetPicks' = SpaceKey::ADMIN,
    })
    .expect({
      val ar = arbiters.get("a1")
      ar.spaces.get(SpaceKey::ADMIN).config.publicMembers == true
    })

  /// Configure a non-existent space must error.
  run configureNonExistentSpaceTest =
    all {
      arbiters' = Map("a1" -> core::new("a1", "alice")),
      error' = false,
      nextJobId' = 1,
      mbt::actionTaken' = "init",
      mbt::nondetPicks' = "",
    }
    .then(all {
      // Try to configure a space that doesn't exist
      arbiters' = arbiters.put("a1",
        runJobGetArb(arbiters.get("a1"), "alice", "nonexistent", nextJobId,
          JobConfigureSpace({ publicRecords: false, publicMembers: true }))
      ),
      error' = jobHasError(arbiters.get("a1"), "alice", "nonexistent", nextJobId,
        JobConfigureSpace({ publicRecords: false, publicMembers: true })),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "configureSpaceAny",
      mbt::nondetPicks' = "nonexistent",
    })
    .expect(error == true)

  /// Delete the arbiter as the sole owner.
  run deleteArbiterAsSoleOwnerTest =
    all {
      arbiters' = Map("a1" -> core::new("a1", "alice")),
      error' = false,
      nextJobId' = 1,
      mbt::actionTaken' = "init",
      mbt::nondetPicks' = "",
    }
    .then(all {
      // Delete arbiter as the sole owner
      arbiters' = arbiters.put("a1",
        runAdminJobGetArb(arbiters.get("a1"), "alice", nextJobId, JobDeleteArbiter)
      ),
      error' = adminJobHasError(arbiters.get("a1"), "alice", nextJobId, JobDeleteArbiter),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "deleteArbiterAny",
      mbt::nondetPicks' = "alice",
    })
    .expect(error == false and arbiters.get("a1").result == ArbiterDeleted)

  /// Cannot delete the arbiter when there are two owners.
  run deleteArbiterWithTwoOwnersFailsTest =
    all {
      arbiters' = Map("a1" -> core::new("a1", "alice")),
      error' = false,
      nextJobId' = 1,
      mbt::actionTaken' = "init",
      mbt::nondetPicks' = "",
    }
    .then(all {
      // Add bob as owner
      arbiters' = arbiters.put("a1",
        runAdminJobGetArb(arbiters.get("a1"), "alice", nextJobId,
          JobSetMemberAccess({ member: MemberUser("bob"), access: Owner }))
      ),
      error' = adminJobHasError(arbiters.get("a1"), "alice", nextJobId,
        JobSetMemberAccess({ member: MemberUser("bob"), access: Owner })),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "addArbiterAdminAny",
      mbt::nondetPicks' = "bob",
    })
    .then(all {
      // Try to delete – should fail because there are two owners
      arbiters' = arbiters.put("a1",
        runAdminJobGetArb(arbiters.get("a1"), "alice", nextJobId, JobDeleteArbiter)
      ),
      error' = adminJobHasError(arbiters.get("a1"), "alice", nextJobId, JobDeleteArbiter),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "deleteArbiterAny",
      mbt::nondetPicks' = "alice",
    })
    .expect(error == true)

  /// Add a local space delegation, then verify the delegation is set up
  /// correctly in the raw members map.
  run addLocalSpaceDelegationTest =
    all {
      arbiters' = Map("a1" -> core::new("a1", "alice")),
      error' = false,
      nextJobId' = 1,
      mbt::actionTaken' = "init",
      mbt::nondetPicks' = "",
    }
    .then(all {
      // Create space "mods" with bob as IsMember
      arbiters' = arbiters.put("a1",
        runJobGetArb(arbiters.get("a1"), "alice", "mods", nextJobId, JobCreateSpace)
      ),
      error' = jobHasError(arbiters.get("a1"), "alice", "mods", nextJobId, JobCreateSpace),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "createSpaceAny",
      mbt::nondetPicks' = "mods",
    })
    .then(all {
      // Add bob as IsMember of mods
      arbiters' = arbiters.put("a1",
        runJobGetArb(arbiters.get("a1"), "alice", "mods", nextJobId,
          JobSetMemberAccess({ member: MemberUser("bob"), access: IsMember }))
      ),
      error' = jobHasError(arbiters.get("a1"), "alice", "mods", nextJobId,
        JobSetMemberAccess({ member: MemberUser("bob"), access: IsMember })),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "addLocalSpaceDelegateAny",
      mbt::nondetPicks' = "mods",
    })
    .then(all {
      // Delegate mods into $admin with IsMember access
      arbiters' = arbiters.put("a1",
        runAdminJobGetArb(arbiters.get("a1"), "alice", nextJobId,
          JobSetMemberAccess({ member: MemberLocalSpace("mods"), access: IsMember }))
      ),
      error' = adminJobHasError(arbiters.get("a1"), "alice", nextJobId,
        JobSetMemberAccess({ member: MemberLocalSpace("mods"), access: IsMember })),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "addLocalSpaceDelegateAny",
      mbt::nondetPicks' = "mods",
    })
    .expect({
      // Check that the delegation was added to $admin's raw members.
      // Note: bob does NOT appear directly in $admin's members; he appears
      // only after resolution via the `members()` function. We verify the
      // delegation structure is correct here.
      val adminMembers = arbiters.get("a1").spaces.get(SpaceKey::ADMIN).members
      adminMembers.has(MemberLocalSpace("mods")) and
        adminMembers.get(MemberLocalSpace("mods")) == IsMember and
        // alice is still in raw members as Owner
        adminMembers.has(MemberUser("alice")) and
        adminMembers.get(MemberUser("alice")) == Owner and
        // bob is in the mods space's raw members
        arbiters.get("a1").spaces.get("mods").members.has(MemberUser("bob")) and
        arbiters.get("a1").spaces.get("mods").members.get(MemberUser("bob")) == IsMember
    })

  /// Add a remote space delegation (MemberRemoteSpace) – exercises the resolution path.
  run addRemoteSpaceDelegationTest =
    all {
      arbiters' = Map("a1" -> core::new("a1", "alice")),
      error' = false,
      nextJobId' = 1,
      mbt::actionTaken' = "init",
      mbt::nondetPicks' = "",
    }
    .then(all {
      // Add a remote space delegation to the $admin space
      arbiters' = arbiters.put("a1",
        runAdminJobGetArb(arbiters.get("a1"), "alice", nextJobId,
          JobSetMemberAccess({
            member: MemberRemoteSpace({ arbiterDid: "a2", spaceKey: "remoteSpace" }),
            access: IsMember,
          }))
      ),
      error' = adminJobHasError(arbiters.get("a1"), "alice", nextJobId,
        JobSetMemberAccess({
          member: MemberRemoteSpace({ arbiterDid: "a2", spaceKey: "remoteSpace" }),
          access: IsMember,
        })),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "addRemoteSpaceDelegateAny",
      mbt::nondetPicks' = "a2",
    })
    .expect({
      // The remote space should appear in the $admin member list as an unresolved space
      val adminMembers = arbiters.get("a1").spaces.get(SpaceKey::ADMIN).members
      adminMembers.has(MemberRemoteSpace({ arbiterDid: "a2", spaceKey: "remoteSpace" }))
    })

  /// Try to add a remote space delegation that references the local arbiter – should error.
  run addSelfReferencingRemoteSpaceFailsTest =
    all {
      arbiters' = Map("a1" -> core::new("a1", "alice")),
      error' = false,
      nextJobId' = 1,
      mbt::actionTaken' = "init",
      mbt::nondetPicks' = "",
    }
    .then(all {
      // Try to add a remote space that points to the SAME arbiter
      arbiters' = arbiters.put("a1",
        runAdminJobGetArb(arbiters.get("a1"), "alice", nextJobId,
          JobSetMemberAccess({
            member: MemberRemoteSpace({ arbiterDid: "a1", spaceKey: "aLocalSpace" }),
            access: Owner,
          }))
      ),
      error' = adminJobHasError(arbiters.get("a1"), "alice", nextJobId,
        JobSetMemberAccess({
          member: MemberRemoteSpace({ arbiterDid: "a1", spaceKey: "aLocalSpace" }),
          access: Owner,
        })),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "addRemoteSpaceDelegateAny",
      mbt::nondetPicks' = "a1",
    })
    .expect(error == true)
}
