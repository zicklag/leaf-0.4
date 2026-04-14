
module model {
  import types.*

  /// The list of arbiters.
  ///
  /// It is assumed that each arbiter could be hosted on a separate server, so we assume that it's
  /// impossible to make updates atomically across multiple arbiters.
  var arbiters: ArbiterDid -> Arbiter

  /// Whether the last arbiter write operation failed with an error.
  var writeError: bool
  
  /// Initial state
  action initArbiter = all {
    arbiters' = Map(),
    writeError' = false
  }

  //
  // ACTIONS
  //
  
  /// Create a new arbiter.
  ///
  /// The user the authenticated user carrying out the action.
  action create(userDid: UserDid, arbiterDid: ArbiterDid): bool = 
    def valid = 
      // Arbiter doesn't already exist
      not(arbiters.has(arbiterDid))

    if (valid) all {
      // Add arbiter
      arbiters' = arbiters.put(arbiterDid, {
        did: arbiterDid,
        spaces: Map(
          // Create the special $admin space
          SpaceKey::ADMIN -> {
            config: SpaceConfig::default,
            members: Map(
              // Add the creator of the arbiter as an owner
              MemberUser(userDid) -> Owner
            )
          }
        )
      }),
      writeError' = false
    } else all {
      arbiters' = arbiters,
      writeError' = true
    }

  /// Create a new space
  action createSpace(userDid: UserDid, arbiterDid: ArbiterDid, spaceKey: SpaceKey): bool =
    def arbiter = arbiters.get(arbiterDid)
    def valid =
      // Arbiter exists
      arbiters.has(arbiterDid) and
      // Space does not already exist
      not(arbiter.spaces.has(spaceKey))

    if (valid) all {
      arbiters' = arbiters,
      writeError' = false,
    } else all {
      arbiters' = arbiters,
      writeError' = true
    }

  //
  // DEFINITIONS
  //

  /// Calculate the member list synchronously without resolving remote spaces.
  ///
  /// If the returned list contains remote spaces they will need to be resolved to complete the
  /// effective member list.
  pure def members(arbiter: Arbiter, spaceKey: SpaceKey): UnresolvedMemberList =
    // Define the initial fold state
    def initState: {
      memberList: UnresolvedMemberList,
      visited: Set[{ spaceKey: SpaceKey, access: Access }],
      queue: Set[{ spaceKey: SpaceKey, access: Access }]
    } = {
      // We start with the admin list for the members
      memberList: Map(),
      // We keep track of the spaces that we have visited to break cycles
      visited: Set(),
      // We keep track of a queue of spaces that we need to visit to add to the members list.
      queue: Set(
        // As well as any members from the admin space
        { spaceKey: SpaceKey::ADMIN, access: Owner },
        // We need to add the members from the space itself
        { spaceKey: spaceKey, access: RemoveMembers }
      ),
    }

    // Since we must avoid recursion we fold over the number of spaces that there are, essentially
    // acting as the max "recursion" depth. We have to additionally multiply by the number of access
    // types we have since each space might be visited up to once per access state.
    def spaceCount = arbiter.spaces.keys().size()
    def accessCount = Accesses.size()
    def recursionDepth = spaceCount * accessCount
    def outputState = 0.to(recursionDepth).fold(initState, (state, i) =>
      // Then we visit every space in the queue
      state.queue.fold(state, (state, queuedSpace) =>
        def spaceKey = queuedSpace.spaceKey
        def spaceAccess = queuedSpace.access
        def space = arbiter.spaces.get(spaceKey)

        // If this space does not exist
        if (not(spaceKey.in(arbiter.spaces.keys())))
          // ignore it and remove it from the queue
          state
            .with("visited", state.visited.union(Set(queuedSpace)))
            .with("queue", state.queue.exclude(Set(queuedSpace)))
        else 

        // Get the list of local users that have been directly added to this space
        def localUsers = space.members.keys().localUsersFromMembers()

        // Get the list of local child spaces
        def localSpaces = space.members.keys().localSpacesFromMembers().map(k =>
          def childSpaceAccess = space.members.get(MemberLocalSpace(k))
          {
            spaceKey: k,
            // Important: we can't give the child space any more access than we have in this queued
            // space. We must choose whichever access is weaker out of the access that this space
            // has been given, and the access that this space gives the child space.
            access: minAccess(childSpaceAccess, spaceAccess)
          }
        )

        // Get the list of remote spaces that have been added as a child to this space
        def remoteSpaces = space.members.keys().remoteSpacesFromMembers().map(spaceId =>
          def childSpaceAccess = space.members.get(MemberRemoteSpace(spaceId))
          (
            spaceId,
            // Important: we have to limit the access of the child space according to the access of
            // the current space.
            minAccess(childSpaceAccess, spaceAccess)
          )
        ).setToMap()

        // Add the current space to the list of visited spaces
        def visited = state.visited.union(Set(queuedSpace))

        // Add the local spaces to the queue
        def queue = state.queue.union(localSpaces).filter(q =>
          // Exclude any space that we have already visited, if we visited it with an access level
          // greater than or equal to the queued access level, since in that case, visiting it again
          // would not grant any more permissions.
          not(visited.exists(v => v.spaceKey == q.spaceKey and accessLevel(v.access) >= accessLevel(q.access))) 
        )

        // Add users in this space to the member list
        def memberListWithUsers = localUsers.fold(state.memberList, (memberList, user) =>
          def userAccessInSpace = space.members.get(MemberUser(user))

          // Get the access of the user, limited by the access of the space. i.e. even if the user
          // has write access in the space, if the space is only granted read access, then the user
          // can only have read access.
          def spaceLimitedAccess = minAccess(spaceAccess, userAccessInSpace)
          
          // If this user is already in the user list
          if (memberList.has(UMLUser(user)))
            // Then we assign them the greater of the access that they already had or that they were
            // granted here.
            memberList.put(UMLUser(user), maxAccess(memberList.get(UMLUser(user)), spaceLimitedAccess))

          // If the user is not already in the list
          else
            // Add them to the list with the access from this space
            memberList.put(UMLUser(user), spaceLimitedAccess)
        )

        // Add the remote spaces to the member list
        def memberList = remoteSpaces.keys().fold(memberListWithUsers, (memberList, spaceId) =>
          def access = remoteSpaces.get(spaceId) 
          def alreadyGrantedAccess = memberList.get(UMLSpace(spaceId))

          // If the space is already in the member list
          if (memberList.has(UMLSpace(spaceId)))
            // Add whatever access is greater out of the access they already had and the access
            // granted here
            memberList.put(UMLSpace(spaceId), maxAccess(alreadyGrantedAccess, access))
          // If the space hasn't been added to the member list
          else
            // Add it
            memberList.put(UMLSpace(spaceId), access)
        )

        // Update the state
        state
          .with("queue", queue)
          // Update the visited list
          .with("visited", visited)
          // Update the member list
          .with("memberList", memberList)
      )
    )

    // Return the flat user list
    outputState.memberList


  /// Takes a members list as returned by `members()`, and a map of the resolved member lists for
  /// external spaces and returns the final, fully-resolved member list.
  ///
  /// Note that if not all of the remote space's member list are returned, then the member list
  /// may be incomplete and the missing spaces will be returned as well.
  pure def joinResolvedSpaces(
    unresolved: UnresolvedMemberList,
    resolvedSpaces: SpaceId -> ResolvedMemberList
  ): ResolvedMemberList =
    def initState = { memberList: Map(), missingSpaces: Map() }

    // For every member in the unresolved list
    unresolved.keys().fold(initState, (state, listItem) =>
      // Get the member's access
      def memberAccess = unresolved.get(listItem)

      // Check whether the member is a user or a space
      match listItem {
        // If the member is a user
        | UMLUser(userDid) =>
          // If the user is already in output list
          if (state.memberList.has(userDid))
            // Add the member with the greatest access that they have been granted
            def existingAccess = state.memberList.get(userDid)
            state
              .with("memberList", state.memberList.put(userDid, maxAccess(memberAccess, existingAccess)))

          // If the user isn't already in the output list
          else
            // Add them with this access
            state.with("memberList", state.memberList.put(userDid, memberAccess))

        // If the member is a space
        | UMLSpace(spaceId) => 
          // If the space is in the list of resolved spaces
          if (resolvedSpaces.has(spaceId))
            def resolvedSpace = resolvedSpaces.get(spaceId)

            // Update the missing spaces list to include the spaces missing from this resolution
            def missingSpaces = resolvedSpace.missingSpaces.keys().fold(state.missingSpaces, (missingSpaces, spaceId) =>
              // Get the effective access of the missing space by limiting on the member's access
              def effectiveAccess = minAccess(resolvedSpace.missingSpaces.get(spaceId), memberAccess)

              // If the space is already missing
              if (missingSpaces.has(spaceId))
                // Update the space with the greatest missing access
                def existingAccess = missingSpaces.get(spaceId)
                missingSpaces.put(spaceId, maxAccess(existingAccess, effectiveAccess))
              // If the space wasn't missing before
              else
                // Add it to the missing list
                missingSpaces.put(spaceId, effectiveAccess)
            )

            // Update the member list with the members in the resolved space
            def memberList = resolvedSpace.memberList.keys().fold(state.memberList, (memberList, userDid) =>
              // Get the effective access of the member by limiting on the member's access
              def effectiveAccess = minAccess(memberAccess, resolvedSpace.memberList.get(userDid))

              // If the user was already in the member list 
              if (memberList.has(userDid))
                // Update member with highest access
                def existingAccess = memberList.get(userDid)
                memberList.put(userDid, maxAccess(existingAccess, effectiveAccess))
              else
                memberList.put(userDid, effectiveAccess)
            )

            // Return the updated state
            state
              .with("memberList", memberList)
              .with("missingSpaces", missingSpaces)

          // If the space isn't in the resolved spaces, then we need to add it to the missing list
          else
            // If the space is already in the missing list
            if (state.missingSpaces.has(spaceId))
              // Update the missing access with the greater of the missing accesses
              def existingAccess = state.missingSpaces.get(spaceId)
              state.with("missingSpaces", state.missingSpaces.put(spaceId, maxAccess(existingAccess, memberAccess)))

            // If the state isn't already missing, we can just add it
            else
              state.with("missingSpaces", state.missingSpaces.put(spaceId, memberAccess))
      }
    )

  //
  // INVARIANTS
  //

  /// Arbiter invariants
  val Inv = all {
    InvArbiterSpaceAlwaysExists
  }

  /// The special `$admin` space always exists in every arbiter
  val InvArbiterSpaceAlwaysExists =
    arbiters.keys().forall(arbiterDid =>
      SpaceKey::ADMIN.in(arbiters.get(arbiterDid).spaces.keys())
    )
}

module types {
  import basicSpells.* from "../lib/basicSpells"
  export basicSpells.*

  type ArbiterDid = str
  type UserDid = str
  type SpaceKey = str

  /// This is a reference to a space, qualified by the arbiter that owns it.
  type SpaceId = { arbiterDid: ArbiterDid, spaceKey: SpaceKey }

  /// The ID of the special "$admin" space that controls access to the arbiter itself.
  pure val SpaceKey::ADMIN = "$admin"

  /// The access a member may have to a permissioned space.
  /// All later permissions include the ones before.
  ///
  /// There are two "tiers" of permissions, space permissions, and arbiter permissions.
  ///
  /// The arbiter permissions only do anything if they end up inherited into the `$admin`
  /// space.
  ///
  /// Usually the arbiter permissions are added directly to users in the `$admin` space
  /// but other spaces can have users with arbiter permissions, too, and they will only
  /// come into affect if that space is delegated to by another `$admin` space.
  type Access =
    // SPACE PERMISSIONS ( work in any space )

    // Member can read records in the space.
    //
    // Because any user can always write records to their own permissioned repo in a space, write
    // access is enforced at the app, and not the arbiter. The arbiter just tracks space membership,
    // and the additional access levels needed to add / remove members from spaces.
    | AccessRecords
    // Member can add new members to the space
    | AddMembers
    // Member can remove members with equal or lower permission from the space
    | RemoveMembers
    //  Member can delete only the space that this access is granted to
    | RemoveSpace

    // ARBITER PERMISSIONS ( only work when applied in the `$admin` space )

    // The minimum arbiter admin permission: can create new spaces on the arbiter
    | CreateSpaces
    // The member can remove any space in the arbiter
    | RemoveAnySpace
    // The member can add new admins
    | AddAdmins
    // The member can remove other admins, or themselves, but not owners
    | RemoveAdmins
    // The member has no permissions limits and can remove other admins and owners
    | Owner

  /// Set of all accesses levels
  pure val Accesses = Set(
    AccessRecords,
    AddMembers,
    RemoveMembers,
    RemoveSpace,
    CreateSpaces,
    RemoveAnySpace,
    AddAdmins,
    RemoveAdmins,
    Owner
  )

  /// Get the numeric access level from `Access`.
  pure def accessLevel(a: Access): int =
    match a {
      | AccessRecords => 0
      | AddMembers => 1
      | RemoveMembers => 2
      | RemoveSpace => 3
      | CreateSpaces => 4
      | RemoveAnySpace => 5
      | AddAdmins => 6
      | RemoveAdmins => 7
      | Owner => 8
    }

  /// Get the smaller of the provided accesses
  pure def minAccess(a: Access, b: Access): Access =
    if (a.accessLevel() < b.accessLevel()) a else b
  
  /// Get the larger of the provided accesses
  pure def maxAccess(a: Access, b: Access): Access =
    if (a.accessLevel() > b.accessLevel()) a else b


  /// Filter out the remote spaces from a member list
  pure def remoteSpacesFromMembers(s: Set[Member]): Set[SpaceId] =
    s.fold(Set(), (spaces, m) => match m {
      | MemberLocalSpace(s) => spaces
      | MemberUser(u) => spaces
      | MemberRemoteSpace(s) => spaces.union(Set(s))
    })

  /// Filter out the users from a member list
  pure def localUsersFromMembers(s: Set[Member]): Set[UserDid] =
    s.fold(Set(), (users, m) => match m {
      | MemberLocalSpace(s) => users
      | MemberUser(u) => users.union(Set(u))
      | MemberRemoteSpace(s) => users
    })

  /// Filter out the local spaces from a member list
  pure def localSpacesFromMembers(s: Set[Member]): Set[SpaceKey] =
    s.fold(Set(), (spaces, m) => match m {
      | MemberLocalSpace(s) => spaces.union(Set(s))
      | MemberUser(u) => spaces
      | MemberRemoteSpace(s) => spaces
    })

  /// A member in a space may either be a user with a DID or another space, in which all the members
  /// of that space extend the members of this space.
  type Member = MemberUser(UserDid) | MemberLocalSpace(SpaceKey) | MemberRemoteSpace(SpaceId)

  /// An item returned by memberList: either a user, or a remote space that must be resolved
  type UnresolvedMemberListItem = UMLUser(UserDid) | UMLSpace(SpaceId)
  type UnresolvedMemberList = UnresolvedMemberListItem -> Access
  type ResolvedMemberList = { memberList: UserDid -> Access, missingSpaces: SpaceId -> Access }

  /// Extra configuration associated to a space
  type SpaceConfig = {
    // Whether or not records in the space can be read without being a member of the space.
    // In other words, this means the arbiter will give anyone who asks a space credential.
    publicRecords: bool,
    // Whether or not the space's member list can be queried without being a member of the space.
    publicMembers: bool,
  }
  pure def SpaceConfig::default: SpaceConfig = {
    publicRecords: false,
    publicMembers: false,
  }

  /// A permissioned space
  type Space = {
    // Space configuration
    config: SpaceConfig,
    // The access level for each member in the space
    members: Member -> Access
  }

  /// Data for an arbiter
  type Arbiter = {
    // The arbiter's DID
    did: ArbiterDid,
    // The spaces managed by the arbiter
    spaces: SpaceKey -> Space,
  }
}

module arbiter {
  import tests
  import types.*
  import model.*

  pure val userDids: Set[UserDid] = Set("Alice", "Bob", "Charlie", "Daffodil", "Eve")
  pure val arbiterDids: Set[UserDid] = Set("arb1", "arb2")
  pure val spaceKeys: Set[UserDid] = Set("space1", "space2")
  pure val roleIds: Set[SpaceKey] = Set("members", "team", "collaborators")


  action init = initArbiter

  action step = any {
    createArbiterAny,
  }

  action createArbiterAny = all {
    nondet userDid = userDids.oneOf()
    nondet arbiterDid = arbiterDids.oneOf()
    create(userDid, arbiterDid)
  }

}

module tests {
  import types.*
  import model.*

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
}

