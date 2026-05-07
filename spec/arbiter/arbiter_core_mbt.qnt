// -*- mode: Bluespec; -*-
/**
 * Model-based testing wrapper around `arbiter_core`.
 *
 * Each action is structured with nondet picks OUTSIDE the all block so that
 * the nondet bindings are in scope for every sub-action inside the all block.
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
  // Run a job on the $admin space, returning the new arbiter.
  //
  // On success (FinishedJob, Deleted, Ok, QueuedJob) → return updated arbiter.
  // On error → return the OLD arbiter unchanged.
  // -------------------------------------------------------------------------

  pure def runAdminJobGetArb(arb: Arbiter, uid: UserDid, jid: JobId, a: JobArgs): Arbiter = {
    val upd = core::startJob(arb, uid, SpaceKey::ADMIN, jid, a)
    match (upd.result) {
      | ArbiterFinishedJob(_) => upd
      | ArbiterDeleted => upd
      | ArbiterOk => upd
      | ArbiterQueuedJob(_) => upd
      | ArbiterErr(_) => arb
    }
  }

  // -------------------------------------------------------------------------
  // Actions – nondet picks are OUTSIDE the all block to keep them in scope.
  // -------------------------------------------------------------------------

  action init = all {
    arbiters' = Map(),
    error' = false,
    nextJobId' = 1,
    mbt::actionTaken' = "init",
    mbt::nondetPicks' = "",
  }

  // --- createArbiterAny ---
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

  // --- addArbiterAdminAny ---
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
          core::new(aid, "")
      ),
      error' = if (aid.in(arbiters.keys()))
        isErrResult(core::startJob(arbiters.get(aid), uid, SpaceKey::ADMIN, nextJobId,
          JobSetMemberAccess({ member: MemberUser(na), access: Owner })).result)
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "addArbiterAdminAny",
      mbt::nondetPicks' = na,
    }

  // --- removeArbiterAdminAny ---
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
          core::new(aid, "")
      ),
      error' = if (aid.in(arbiters.keys()))
        isErrResult(core::startJob(arbiters.get(aid), uid, SpaceKey::ADMIN, nextJobId,
          JobRemoveMember({ member: MemberUser(rm) })).result)
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "removeArbiterAdminAny",
      mbt::nondetPicks' = rm,
    }

  // --- createSpaceAny ---
  action createSpaceAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    nondet sk = oneOf(spaceKeys)
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          runAdminJobGetArb(arbiters.get(aid), uid, nextJobId, JobCreateSpace)
        else
          core::new(aid, "")
      ),
      error' = if (aid.in(arbiters.keys()))
        isErrResult(core::startJob(arbiters.get(aid), uid, SpaceKey::ADMIN, nextJobId, JobCreateSpace).result)
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "createSpaceAny",
      mbt::nondetPicks' = sk,
    }

  // --- setSpaceMemberAccessAny ---
  action setSpaceMemberAccessAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    nondet sk = oneOf(allSpaceKeys)
    nondet mbr = oneOf(memberUsers)
    nondet acc = oneOf(accesses)
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          runAdminJobGetArb(arbiters.get(aid), uid, nextJobId,
            JobSetMemberAccess({ member: mbr, access: acc }))
        else
          core::new(aid, "")
      ),
      error' = if (aid.in(arbiters.keys()))
        isErrResult(core::startJob(arbiters.get(aid), uid, SpaceKey::ADMIN, nextJobId,
          JobSetMemberAccess({ member: mbr, access: acc })).result)
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "setSpaceMemberAccessAny",
      mbt::nondetPicks' = sk,
    }

  // --- removeSpaceMemberAny ---
  action removeSpaceMemberAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    nondet sk = oneOf(allSpaceKeys)
    nondet mbr = oneOf(memberUsers)
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          runAdminJobGetArb(arbiters.get(aid), uid, nextJobId,
            JobRemoveMember({ member: mbr }))
        else
          core::new(aid, "")
      ),
      error' = if (aid.in(arbiters.keys()))
        isErrResult(core::startJob(arbiters.get(aid), uid, SpaceKey::ADMIN, nextJobId,
          JobRemoveMember({ member: mbr })).result)
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "removeSpaceMemberAny",
      mbt::nondetPicks' = sk,
    }

  // --- fetchMembersAny ---
  action fetchMembersAny =
    nondet uid = oneOf(USER_DIDS)
    nondet aid = oneOf(ARBITER_DIDS)
    all {
      arbiters' = arbiters.put(aid,
        if (aid.in(arbiters.keys()))
          runAdminJobGetArb(arbiters.get(aid), uid, nextJobId, JobFetchMembers)
        else
          core::new(aid, "")
      ),
      error' = if (aid.in(arbiters.keys()))
        isErrResult(core::startJob(arbiters.get(aid), uid, SpaceKey::ADMIN, nextJobId, JobFetchMembers).result)
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "fetchMembersAny",
      mbt::nondetPicks' = uid,
    }

  // -------------------------------------------------------------------------
  // Simulation step
  // -------------------------------------------------------------------------

  action step = any {
    createArbiterAny,
    addArbiterAdminAny,
    removeArbiterAdminAny,
    createSpaceAny,
    setSpaceMemberAccessAny,
    removeSpaceMemberAny,
    fetchMembersAny,
  }

  // -------------------------------------------------------------------------
  // Named tests
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
      error' = isErrResult(core::startJob(arbiters.get("a1"), "alice", SpaceKey::ADMIN, nextJobId,
        JobSetMemberAccess({ member: MemberUser("bob"), access: Owner })).result),
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
      // Try to create the same arbiter again
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
    // Step 1: alice adds bob as Owner
    .then(all {
      arbiters' = arbiters.put("a1",
        runAdminJobGetArb(arbiters.get("a1"), "alice", nextJobId,
          JobSetMemberAccess({ member: MemberUser("bob"), access: Owner }))
      ),
      error' = isErrResult(core::startJob(arbiters.get("a1"), "alice", SpaceKey::ADMIN, nextJobId,
        JobSetMemberAccess({ member: MemberUser("bob"), access: Owner })).result),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "addArbiterAdminAny",
      mbt::nondetPicks' = "bob",
    })
    // Step 2: alice removes bob
    .then(all {
      arbiters' = arbiters.put("a1",
        runAdminJobGetArb(arbiters.get("a1"), "alice", nextJobId,
          JobRemoveMember({ member: MemberUser("bob") }))
      ),
      error' = isErrResult(core::startJob(arbiters.get("a1"), "alice", SpaceKey::ADMIN, nextJobId,
        JobRemoveMember({ member: MemberUser("bob") })).result),
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

  /// Create an arbiter, create a space, then verify the space exists.
  run createSpaceTest =
    all {
      arbiters' = Map("a1" -> core::new("a1", "alice")),
      error' = false,
      nextJobId' = 1,
      mbt::actionTaken' = "init",
      mbt::nondetPicks' = "",
    }
    .then(all {
      arbiters' = arbiters.put("a1",
        runAdminJobGetArb(arbiters.get("a1"), "alice", nextJobId, JobCreateSpace)
      ),
      error' = isErrResult(core::startJob(arbiters.get("a1"), "alice", SpaceKey::ADMIN, nextJobId, JobCreateSpace).result),
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "createSpaceAny",
      mbt::nondetPicks' = "space1",
    })
    .expect({
      val ar = arbiters.get("a1")
      ar.spaces.has("space1") and ar.spaces.has(SpaceKey::ADMIN)
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
      error' = isErrResult(core::startJob(arbiters.get("a1"), "bob", SpaceKey::ADMIN, nextJobId,
        JobSetMemberAccess({ member: MemberUser("charlie"), access: Owner })).result),
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
      // "bob" can't fetch members from a non-existent arbiter "nonexistent"
      arbiters' = arbiters.put("nonexistent",
        if ("nonexistent".in(arbiters.keys()))
          runAdminJobGetArb(arbiters.get("nonexistent"), "bob", nextJobId, JobFetchMembers)
        else
          core::new("nonexistent", "")
      ),
      error' = if ("nonexistent".in(arbiters.keys()))
        isErrResult(core::startJob(arbiters.get("nonexistent"), "bob", SpaceKey::ADMIN, nextJobId, JobFetchMembers).result)
        else true,
      nextJobId' = nextJobId + 1,
      mbt::actionTaken' = "fetchMembersAny",
      mbt::nondetPicks' = "bob",
    })
    .expect(error == true)
}
