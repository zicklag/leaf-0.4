module arbiter_test {
  import types.* from "./arbiter"
  import model.* from "./arbiter"

  run joinResolvedSpacesTest =
    def collectiveSpace = { arbiterDid: "arb1", spaceKey: "collective" }
    def citySpace = { arbiterDid: "arb2", spaceKey: "city" }
    all {
      // Simple test of resolving a space to a member list
      assert(
        joinResolvedSpaces(
          Map(
            UMLUser("zicklag") -> Owner,
            UMLSpace(collectiveSpace) -> AccessRecords
          ),
          Map(
            collectiveSpace -> {
              memberList: Map(
                "meri" -> RemoveAdmins
              ),
              missingSpaces: Map()
            }
          )
        )
          ==
        {
          memberList: Map(
            "zicklag" -> Owner,
            "meri" -> AccessRecords,
          ),
          missingSpaces: Map(
          )
        }
      ),
      // Test missing lists inside of resolved spaces
      assert(
        joinResolvedSpaces(
          Map(
            UMLUser("zicklag") -> Owner,
            UMLSpace(collectiveSpace) -> AccessRecords
          ),
          Map(
            collectiveSpace -> {
              memberList: Map(
                "meri" -> RemoveAdmins
              ),
              missingSpaces: Map(
                citySpace -> RemoveMembers
              )
            }
          )
        )
          ==
        {
          memberList: Map(
            "zicklag" -> Owner,
            "meri" -> AccessRecords,
          ),
          missingSpaces: Map(
            citySpace -> AccessRecords
          )
        }
      ),
      // Test twice missing lists inside of resolved spaces
      assert(
        joinResolvedSpaces(
          Map(
            UMLUser("zicklag") -> Owner,
            UMLSpace(collectiveSpace) -> AccessRecords,
            UMLSpace(citySpace) -> AddMembers,
          ),
          Map(
            collectiveSpace -> {
              memberList: Map(
                "meri" -> RemoveAdmins
              ),
              missingSpaces: Map(
                citySpace -> RemoveMembers
              )
            }
          )
        )
          ==
        {
          memberList: Map(
            "zicklag" -> Owner,
            "meri" -> AccessRecords,
          ),
          missingSpaces: Map(
            citySpace -> AddMembers
          )
        }
      ),
      // Make sure that a missing space is registered properly
      assert(
        joinResolvedSpaces(
          Map(
            UMLUser("zicklag") -> Owner,
            UMLSpace(collectiveSpace) -> AccessRecords
          ),
          Map()
        )
          ==
        {
          memberList: Map(
            "zicklag" -> Owner,
          ),
          missingSpaces: Map(
            collectiveSpace -> AccessRecords
          )
        }
      )
    }

  pure val arbiter1: Arbiter = {
    did: "arbiter1",
    spaces: Map(
      SpaceKey::ADMIN -> {
        config: SpaceConfig::default,
        members: Map(
          MemberUser("erlend") -> Owner,
          MemberUser("zicklag") -> Owner,
          MemberUser("meri") -> Owner,
        )
      },
      "members" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberUser("charlie") -> RemoveMembers,
          MemberUser("eve") -> AccessRecords,
        )
      },
      "team" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberUser("erlend") -> RemoveMembers,
          MemberUser("zicklag") -> RemoveMembers,
          MemberUser("meri") -> RemoveMembers,
          MemberUser("alice") -> RemoveMembers,
          MemberLocalSpace("designers") -> AccessRecords,
        )
      },
      "designers" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberUser("fish") -> RemoveMembers,
        )
      },
      "#general" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberLocalSpace("team") -> RemoveMembers,
          MemberLocalSpace("members") -> AccessRecords,
          MemberUser("bob") -> AccessRecords,
        )
      },
      "#internal" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberLocalSpace("team") -> RemoveMembers,
        )
      },
      "#design" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberLocalSpace("team") -> RemoveMembers,
          MemberLocalSpace("designers") -> RemoveMembers,
          MemberLocalSpace("members") -> AccessRecords,
        )
      }
    )
  }

  /// Run some test cases to validate the flat local member list output
  run memberListArbiter1Test = all {
    all {
      assert(
        arbiter1.members("#general") ==
        Map(
          // Can manage because she's in the team
         UMLUser("alice") -> RemoveMembers,
          // Can read because he's directly added to general channel
         UMLUser("bob") -> AccessRecords,
          // Can write because he's a member
         UMLUser("charlie") -> AccessRecords,
          // Can write because she's a member
         UMLUser("eve") -> AccessRecords,
          // Can write because he's in designers which is in the team
         UMLUser("fish") -> AccessRecords,
          // Can manage because she's an admin
         UMLUser("meri") -> Owner,
          // Can manage because he's an admin
         UMLUser("erlend") -> Owner,
          // Can manage because he's an admin
         UMLUser("zicklag") -> Owner,
        )
      ),
      assert(
        arbiter1.members("#internal") ==
        Map(
          // Can manage because she's in the team
         UMLUser("alice") -> RemoveMembers,
          // Can write because he's in designers which is in the team
         UMLUser("fish") -> AccessRecords,
          // Can manage because she's an admin
         UMLUser("meri") -> Owner,
          // Can manage because he's an admin
         UMLUser("erlend") -> Owner,
          // Can manage because he's an admin
         UMLUser("zicklag") -> Owner,
        )
      ),
      assert(
        arbiter1.members("#design") ==
        Map(
          // Can manage because she's in the team
         UMLUser("alice") -> RemoveMembers,
          // Charlie can read 'cause he's in members
         UMLUser("charlie") -> AccessRecords,
          // Eve can read 'cause she's in members
         UMLUser("eve") -> AccessRecords,
          // Can manage 'cause he's in designers
         UMLUser("fish") -> RemoveMembers,
          // Can manage because she's an admin
         UMLUser("meri") -> Owner,
          // Can manage because he's an admin
         UMLUser("erlend") -> Owner,
          // Can manage because he's an admin
         UMLUser("zicklag") -> Owner,
        )
      )
    }
  }

  // pure val arbiter2: Arbiter = {
  //   did: "arbiter2",
  //   owners: Set("zicklag"),
  //   admins: Map(),
  //   spaces: Map(
  //     "managers" -> {
  //       config: SpaceConfig::default,
  //       members: Map(
  //         MemberUser("meri") -> RemoveMembers,
  //       )
  //     },
  //     "team" -> {
  //       config: SpaceConfig::default,
  //       members: Map(
  //         MemberUser("erlend") -> WriteRecords,
  //       )
  //     },
  //     "strange" -> {
  //       config: SpaceConfig::default,
  //       members: Map(
  //         MemberLocalSpace("managers") -> RemoveMembers,
  //       )
  //     },
  //     "#test" -> {
  //       config: SpaceConfig::default,
  //       members: Map(
  //         MemberLocalSpace("managers") -> ReadRecords,
  //         MemberLocalSpace("team") -> RemoveMembers,
  //       )
  //     },
  //     "#off-topic" -> {
  //       config: SpaceConfig::default,
  //       members: Map(
  //         MemberLocalSpace("managers") -> ReadRecords,
  //         MemberLocalSpace("strange") -> RemoveMembers,
  //       )
  //     },
  //     "#on-topic" -> {
  //       config: SpaceConfig::default,
  //       members: Map(
  //         MemberLocalSpace("managers") -> ReadRecords,
  //       )
  //     },
  //   )
  // }

  // /// Run some test cases to validate the flat local member list output
  // run memberListArbiter2Test = all {
  //   all {
  //     assert(
  //       flatMemberList(arbiter2, "#test") ==
  //       Map(
  //        UMLUser("meri") -> ReadRecords,
  //        UMLUser("erlend") -> WriteRecords,
  //        UMLUser("zicklag") -> RemoveMembers,
  //       )
  //     ),
  //     assert(
  //       flatMemberList(arbiter2, "#off-topic") ==
  //       Map(
  //        UMLUser("meri") -> RemoveMembers,
  //        UMLUser("zicklag") -> RemoveMembers,
  //       )
  //     ),
  //     assert(
  //       flatMemberList(arbiter2, "#on-topic") ==
  //       Map(
  //        UMLUser("meri") -> ReadRecords,
  //        UMLUser("zicklag") -> RemoveMembers,
  //       )
  //     ),
  //   }
  // }

  // pure val arbiter3: Arbiter = {
  //   did: "arbiter2",
  //   owners: Set("owner"),
  //   admins: Map(),
  //   spaces: Map(
  //     "role1" -> {
  //       config: SpaceConfig::default,
  //       members: Map(
  //         MemberUser("user1") -> RemoveMembers,
  //         MemberLocalSpace("role2") -> ReadRecords,
  //         MemberRemoteSpace({ arbiterDid: "arbiter4", spaceKey: "example" }) -> AddWriters,
  //       )
  //     },
  //     "role2" -> {
  //       config: SpaceConfig::default,
  //       members: Map(
  //         MemberUser("user2") -> WriteRecords,
  //         MemberLocalSpace("role3") -> WriteRecords,
  //       )
  //     },
  //     "role3" -> {
  //       config: SpaceConfig::default,
  //       members: Map(
  //         MemberUser("user3") -> RemoveMembers,
  //         MemberUser("user4") -> ReadRecords,
  //         MemberLocalSpace("role1") -> RemoveMembers,
  //       )
  //     },
  //     "#channel1" -> {
  //       config: SpaceConfig::default,
  //       members: Map(
  //         MemberLocalSpace("role2") -> RemoveMembers,
  //         // This should be safely ignored
  //         MemberLocalSpace("doesn't exist") -> RemoveMembers,
  //       )
  //     },
  //   )
  // }

  //  /// Run some test cases to validate the flat local member list output
  // run memberListArbiter3Test = all {
  //   all {
  //     assert(
  //       flatMemberList(arbiter3, "#channel1") ==
  //       Map(
  //        UMLUser("admin") -> RemoveMembers,
  //        UMLUser("user2") -> WriteRecords,
  //        UMLUser("user3") -> WriteRecords,
  //        UMLUser("user4") -> ReadRecords,
  //         // User one has write because they were granted manage in role1,
  //         // which was granted manage in role 3,
  //         // which was granted write in role 2,
  //         // which was granted manage in channel 1
  //        UMLUser("user1") -> WriteRecords,
  //         // Note how the remote space's access is limited to write because we don't allow members
  //         // from remote spaces to manage members.
  //         MLSpace({ arbiterDid: "arbiter4", spaceKey: "example" }) -> WriteRecords
  //       )
  //     ),
  //   }
  // }

  //
  // PROPERTY-BASED INVARIANT TESTS
  //

  // -- Helper defs --

  pure def allDirectUsersInArbiter(arbiter: Arbiter): Set[UserDid] =
    arbiter.spaces.values().map(space =>
      space.members.keys().map(m => match m {
        | MemberUser(u) => Set(u)
        | _ => Set()
      }).flatten()
    ).flatten()

  pure def allRemoteSpacesInArbiter(arbiter: Arbiter): Set[SpaceId] =
    arbiter.spaces.values().map(space =>
      space.members.keys().map(m => match m {
        | MemberRemoteSpace(sid) => Set(sid)
        | _ => Set()
      }).flatten()
    ).flatten()

  pure def allGrantsForUser(arbiter: Arbiter, userDid: UserDid): Set[Access] =
    arbiter.spaces.values().map(space =>
      space.members.keys().map(m => match m {
        | MemberUser(u) => if (u == userDid) Set(space.members.get(m)) else Set()
        | _ => Set()
      }).flatten()
    ).flatten()

  pure def adminUsersInArbiter(arbiter: Arbiter): Set[UserDid] =
    if (arbiter.spaces.has(SpaceKey::ADMIN))
      arbiter.spaces.get(SpaceKey::ADMIN).members.keys().map(m => match m {
        | MemberUser(u) => Set(u)
        | _ => Set()
      }).flatten()
    else
      Set()

  pure def directUsersInUnresolved(unresolved: UnresolvedMemberList): Set[UserDid] =
    unresolved.keys().map(k => match k {
      | UMLUser(u) => Set(u)
      | _ => Set()
    }).flatten()

  pure def spaceIdsInUnresolved(unresolved: UnresolvedMemberList): Set[SpaceId] =
    unresolved.keys().map(k => match k {
      | UMLSpace(sid) => Set(sid)
      | _ => Set()
    }).flatten()

  pure def directAccessForUser(unresolved: UnresolvedMemberList, userDid: UserDid): Set[Access] =
    unresolved.keys().filter(k => match k {
      | UMLUser(u) => u == userDid
      | _ => false
    }).map(k => unresolved.get(k))

  pure def directAccessForSpace(unresolved: UnresolvedMemberList, sid: SpaceId): Set[Access] =
    unresolved.keys().filter(k => match k {
      | UMLSpace(s) => s == sid
      | _ => false
    }).map(k => unresolved.get(k))

  pure def resolvedUserDids(resolvedSpaces: SpaceId -> ResolvedMemberList): Set[UserDid] =
    resolvedSpaces.values().map(r => r.memberList.keys()).flatten()

  pure def transitivelyMissingSpaces(resolvedSpaces: SpaceId -> ResolvedMemberList): Set[SpaceId] =
    resolvedSpaces.values().map(r => r.missingSpaces.keys()).flatten()

  pure def maxAccessInSet(accesses: Set[Access]): Access =
    accesses.fold(AccessRecords, (a, b) => maxAccess(a, b))

  // -- Well-formed test arbiters of varying structure --

  pure val invArbiterMinimal: Arbiter = {
    did: "inv_arb1",
    spaces: Map(
      SpaceKey::ADMIN -> {
        config: SpaceConfig::default,
        members: Map(MemberUser("alice") -> Owner)
      }
    )
  }

  pure val invArbiterBasic: Arbiter = {
    did: "inv_arb2",
    spaces: Map(
      SpaceKey::ADMIN -> {
        config: SpaceConfig::default,
        members: Map(MemberUser("alice") -> Owner, MemberUser("bob") -> AddAdmins)
      },
      "team" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberUser("charlie") -> RemoveMembers,
          MemberUser("dave") -> AccessRecords,
        )
      }
    )
  }

  pure val invArbiterNested: Arbiter = {
    did: "inv_arb3",
    spaces: Map(
      SpaceKey::ADMIN -> {
        config: SpaceConfig::default,
        members: Map(MemberUser("alice") -> Owner)
      },
      "team" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberUser("bob") -> RemoveMembers,
          MemberLocalSpace("subteam") -> AddMembers,
        )
      },
      "subteam" -> {
        config: SpaceConfig::default,
        members: Map(MemberUser("carol") -> RemoveMembers)
      },
      "project" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberLocalSpace("team") -> RemoveMembers,
          MemberUser("dave") -> AccessRecords,
        )
      }
    )
  }

  pure val invArbiterWithRemote: Arbiter = {
    did: "inv_arb4",
    spaces: Map(
      SpaceKey::ADMIN -> {
        config: SpaceConfig::default,
        members: Map(MemberUser("alice") -> Owner)
      },
      "shared" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberUser("bob") -> RemoveMembers,
          MemberRemoteSpace({ arbiterDid: "other_arb", spaceKey: "ext_space" }) -> AddMembers,
        )
      }
    )
  }

  pure val invArbiterCycle: Arbiter = {
    did: "inv_arb5",
    spaces: Map(
      SpaceKey::ADMIN -> {
        config: SpaceConfig::default,
        members: Map(MemberUser("alice") -> Owner)
      },
      "a" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberUser("bob") -> RemoveMembers,
          MemberLocalSpace("b") -> AddMembers,
        )
      },
      "b" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberUser("carol") -> RemoveMembers,
          MemberLocalSpace("a") -> AddMembers,
        )
      }
    )
  }

  pure val invArbiterEmptyMembers: Arbiter = {
    did: "inv_arb6",
    spaces: Map(
      SpaceKey::ADMIN -> {
        config: SpaceConfig::default,
        members: Map(MemberUser("alice") -> Owner)
      },
      "empty" -> {
        config: SpaceConfig::default,
        members: Map()
      }
    )
  }

  pure val invArbiterNonExistentRef: Arbiter = {
    did: "inv_arb7",
    spaces: Map(
      SpaceKey::ADMIN -> {
        config: SpaceConfig::default,
        members: Map(MemberUser("alice") -> Owner)
      },
      "channel" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberLocalSpace("does_not_exist") -> RemoveMembers,
          MemberUser("bob") -> AccessRecords,
        )
      }
    )
  }

  pure val testArbiters: Set[Arbiter] = Set(
    invArbiterMinimal,
    invArbiterBasic,
    invArbiterNested,
    invArbiterWithRemote,
    invArbiterCycle,
    invArbiterEmptyMembers,
    invArbiterNonExistentRef,
    arbiter1,
  )

  // -- Small universes for joinResolvedSpaces() tests --

  pure val invSid1: SpaceId = { arbiterDid: "arb_a", spaceKey: "space_x" }
  pure val invSid2: SpaceId = { arbiterDid: "arb_b", spaceKey: "space_y" }
  pure val invUmlItems: Set[UnresolvedMemberListItem] = Set(
    UMLUser("u1"), UMLUser("u2"), UMLSpace(invSid1)
  )
  pure val invAccesses3: Set[Access] = Set(AccessRecords, RemoveMembers, Owner)
  pure val invItemSubsets: Set[Set[UnresolvedMemberListItem]] = invUmlItems.powerset()
  pure val allSmallUnresolved: Set[UnresolvedMemberList] =
    invItemSubsets.map(subset => subset.setOfMaps(invAccesses3)).flatten()

  //
  // members() invariants
  //

  /// Every UMLUser in the output is a direct member of some space in the arbiter
  run membersUsersAreReachableTest = all {
    nondet arb = testArbiters.oneOf()
    nondet spaceKey = arb.spaces.keys().oneOf()
    def result = arb.members(spaceKey)
    def allDirectUsers = allDirectUsersInArbiter(arb)
    assert(
      result.keys().forall(k => match k {
        | UMLUser(u) => u.in(allDirectUsers)
        | UMLSpace(_) => true
      })
    )
  }

  /// Every user's output access never exceeds the max of all accesses they
  /// were directly granted across spaces.
  run membersInvUserAccessCappedTest = all {
    nondet arb = testArbiters.oneOf()
    nondet spaceKey = arb.spaces.keys().oneOf()
    def result = arb.members(spaceKey)
    assert(
      result.keys().forall(k => match k {
        | UMLUser(userDid) =>
          def outputAccess = result.get(k)
          def grants = allGrantsForUser(arb, userDid)
          (grants.size() == 0)
            or accessLevel(outputAccess) <= accessLevel(maxAccessInSet(grants))
        | UMLSpace(_) => true
      })
    )
  }

  /// Every UMLSpace in the output corresponds to a MemberRemoteSpace in some space
  run membersRemoteSpacesAreReachableTest = all {
    nondet arb = testArbiters.oneOf()
    nondet spaceKey = arb.spaces.keys().oneOf()
    def result = arb.members(spaceKey)
    def allRemote = allRemoteSpacesInArbiter(arb)
    assert(
      result.keys().forall(k => match k {
        | UMLUser(_) => true
        | UMLSpace(spaceId) => spaceId.in(allRemote)
      })
    )
  }

  /// Admin space members always appear in the output
  run membersAdminsAlwaysPresentTest = all {
    nondet arb = testArbiters.oneOf()
    nondet spaceKey = arb.spaces.keys().oneOf()
    def result = arb.members(spaceKey)
    def adminUsers = adminUsersInArbiter(arb)
    assert(
      adminUsers.forall(u => result.has(UMLUser(u)))
    )
  }

  //
  // joinResolvedSpaces() invariants
  //

  /// Every user in the output memberList is either a direct UMLUser in the input
  /// or appears in a resolved space's memberList
  run joinResolvedUsersAreFromInputTest = all {
    nondet unresolved = allSmallUnresolved.oneOf()
    nondet resolvedUser = Set("u1", "u2").oneOf()
    nondet resolvedAccess = invAccesses3.oneOf()
    def resolvedSpaces = Map(invSid1 -> {
      memberList: Map(resolvedUser -> resolvedAccess),
      missingSpaces: Map()
    })
    def result = joinResolvedSpaces(unresolved, resolvedSpaces)
    def directUsers = directUsersInUnresolved(unresolved)
    def resolvedUsers = resolvedUserDids(resolvedSpaces)
    assert(
      result.memberList.keys().forall(u =>
        u.in(directUsers.union(resolvedUsers))
      )
    )
  }

  /// No user's output access exceeds the max of their direct input access and
  /// the access from any resolved space.
  run joinResolvedInvUserAccessCappedTest = all {
    nondet unresolved = allSmallUnresolved.oneOf()
    nondet resolvedAccess = invAccesses3.oneOf()
    def resolvedSpaces = Map(invSid1 -> {
      memberList: Map("u1" -> resolvedAccess),
      missingSpaces: Map()
    })
    def result = joinResolvedSpaces(unresolved, resolvedSpaces)
    assert(
      result.memberList.keys().forall(userDid =>
        def outputAccess = result.memberList.get(userDid)
        def directAccess = directAccessForUser(unresolved, userDid)
        def fromResolved = resolvedSpaces.has(invSid1) and resolvedSpaces.get(invSid1).memberList.has(userDid)
        def maxSource = if (directAccess.size() > 0 and fromResolved)
          maxAccess(
            maxAccessInSet(directAccess),
            resolvedSpaces.get(invSid1).memberList.get(userDid)
          )
        else if (directAccess.size() > 0)
          maxAccessInSet(directAccess)
        else if (fromResolved)
          resolvedSpaces.get(invSid1).memberList.get(userDid)
        else
          outputAccess
        accessLevel(outputAccess) <= accessLevel(maxSource)
      )
    )
  }

  /// Every space in missingSpaces is either an unresolved UMLSpace or was
  /// transitively missing from a resolved space
  run joinResolvedMissingSpacesAreUnresolvedTest = all {
    nondet unresolved = allSmallUnresolved.oneOf()
    nondet transitiveAccess = invAccesses3.oneOf()
    def resolvedSpaces = Map(invSid1 -> {
      memberList: Map("u1" -> transitiveAccess),
      missingSpaces: Map(invSid2 -> transitiveAccess)
    })
    def result = joinResolvedSpaces(unresolved, resolvedSpaces)
    def unresolvedIds = spaceIdsInUnresolved(unresolved)
    def transitiveMissing = transitivelyMissingSpaces(resolvedSpaces)
    assert(
      result.missingSpaces.keys().forall(sid =>
        sid.in(unresolvedIds.union(transitiveMissing))
      )
    )
  }

  /// No missing space's access exceeds its access in the input unresolved list
  run joinResolvedMissingAccessCappedTest = all {
    nondet unresolved = allSmallUnresolved.oneOf()
    def result = joinResolvedSpaces(unresolved, Map())
    assert(
      result.missingSpaces.keys().forall(sid =>
        def outputAccess = result.missingSpaces.get(sid)
        def directAccess = directAccessForSpace(unresolved, sid)
        (directAccess.size() == 0)
          or directAccess.forall(a => accessLevel(outputAccess) <= accessLevel(a))
      )
    )
  }
}