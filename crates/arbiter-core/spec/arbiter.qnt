module arbiter {
  import test

  import types.*

  import model.*

  pure val userDids: Set[UserDid] = Set("Alice", "Bob", "Charlie", "Daffodil", "Eve")
  pure val arbiterDids: Set[UserDid] = Set("arb1", "arb2")
  pure val spaceKeys: Set[UserDid] = Set("space1", "space2")
  pure val roleIds: Set[SpaceKey] = Set("members", "team", "collaborators")


  action init = initArbiter

  action step = any {
    createArbiterAny,
    addArbiterAdminAny,
    removeArbiterAdminAny,
    createSpaceAny,
    configureSpaceAny,
    // setSpaceMemberAccessAny,
  }

  action createArbiterAny = all {
    nondet userDid = userDids.oneOf()
    nondet arbiterDid = arbiterDids.oneOf()
    create(userDid, arbiterDid)
  }

  action addArbiterAdminAny = all {
    nondet arbiterDid = arbiterDids.oneOf()
    nondet userDid = userDids.oneOf()
    nondet newAdminDid = userDids.oneOf()
    addAdmin(userDid, arbiterDid, newAdminDid)
  }

  action removeArbiterAdminAny = all {
    nondet arbiterDid = arbiterDids.oneOf()
    nondet userDid = userDids.oneOf()
    nondet removedAdmin = userDids.oneOf()
    removeAdmin(userDid, arbiterDid, removedAdmin)
  }

  action createSpaceAny = all {
    nondet arbiterDid = arbiterDids.oneOf()
    nondet userDid = userDids.oneOf()
    nondet spaceKey = spaceKeys.oneOf()
    createSpace(userDid, arbiterDid, spaceKey)
  }

  action configureSpaceAny = all {
    nondet arbiterDid = arbiterDids.oneOf()
    nondet userDid = userDids.oneOf()
    nondet spaceKey = spaceKeys.oneOf()
    nondet publicRecords = Set(true, false).oneOf()
    nondet publicMembers = Set(true, false).oneOf()
    configureSpace(userDid, arbiterDid, spaceKey, { publicRecords: publicRecords, publicMembers: publicMembers })
  }

  // action setSpaceMemberAccessAny = all {
  //   nondet arbiterDid = arbiterDids.oneOf()
  //   nondet userDid = userDids.oneOf()
  //   nondet spaceKey = spaceKeys.oneOf()
  //   def possibleSpaces = arbiterDids.map(a => roleIds.map(r => { arbiterDid: a, spaceKey: r })).flatten()
  //   nondet member = userDids.map(x => MemberUser(x)).union(possibleSpaces.map(x => MemberSpace(x))).oneOf()
  //   nondet access = Accesses.oneOf()
  //   setSpaceMemberAccess(userDid, arbiterDid, spaceKey, member, access)
  // }

  // action removeSpaceMemberAny = all {
  //   nondet arbiterDid = arbiterDids.oneOf()
  //   nondet userDid = userDids.oneOf()
  //   nondet spaceKey = spaceKeys.oneOf()
  //   def possibleSpaces = arbiterDids.map(a => roleIds.map(r => { arbiterDid: a, spaceKey: r })).flatten()
  //   nondet member = userDids.map(x => MemberUser(x)).union(possibleSpaces.map(x => MemberSpace(x))).oneOf()
  //   removeSpaceMember(userDid, arbiterDid, spaceKey, member)
  // }
}

module model {
  import types.*

  /// Counter for creating unique IDs
  var arbiters: ArbiterDid -> Arbiter
  /// Whether the last write operation failed with an error
  var writeError: bool
  
  action initArbiter = all {
    arbiters' = Map(),
    writeError' = false
  }

  /// Arbiter invariants
  val Inv = all {
    InvAtLeastOneAdmin,
    InvAllAdminsCanRemoveMembers,
  }

  /// There is always at least one admin for any given arbiter
  val InvAtLeastOneAdmin =
    arbiters.keys().forall(arbiterDid =>
      def arbiter = arbiters.get(arbiterDid)
      arbiter.admins.size() >= 1
    )

  /// Admins are always given manage space permissions for each space
  val InvAllAdminsCanRemoveMembers =
    arbiters.keys().forall(arbiterDid =>
      def arbiter = arbiters.get(arbiterDid)
      arbiter.spaces.keys().forall(spaceKey =>
        def spaceMembers = flatMemberList(arbiter, spaceKey)
        arbiter.admins.forall(adminDid =>
          spaceMembers.get(MLUser(adminDid)) == RemoveMembers
        )
      )
    )

  /// Create a new arbiter
  action create(userDid: UserDid, arbiterDid: ArbiterDid): bool = 
    def valid = not(arbiterDid.in(arbiters.keys()))

    if (valid) all {
      // Arbiter doesn't already exist
      // Add arbiter
      arbiters' = arbiters.put(arbiterDid, {
        did: arbiterDid,
        // Make the creator an admin
        admins: Set(userDid),
        spaces: Map()
      }),
      writeError' = false
    } else all {
      arbiters' = arbiters,
      writeError' = true
    }

  /// Add an admin to the arbiter
  action addAdmin(userDid: UserDid, arbiterDid: ArbiterDid, newAdmin: UserDid): bool =
    def arbiter = arbiters.get(arbiterDid)
    def valid = 
      // Arbiter exists
      arbiterDid.in(arbiters.keys()) and
      // user is admin
      userDid.in(arbiter.admins)

    if (valid) all {
      // Add new admin
      def updAdmins = arbiter.admins.union(Set(newAdmin))
      def updArbiter = arbiter.with("admins", updAdmins)
      arbiters' = arbiters.set(
        arbiterDid,
        updArbiter
      ),
      writeError' = false
    } else all {
      arbiters' = arbiters,
      writeError' = true
    }

  /// Remove an admin from the arbiter
  action removeAdmin(userDid: UserDid, arbiterDid: ArbiterDid, removedAdminDid: UserDid): bool =
    def arbiter = arbiters.get(arbiterDid)
    def valid =
      // Arbiter exists
      arbiterDid.in(arbiters.keys()) and
      // There is more than one admin ( we can't remove the last admin )
      arbiter.admins.size() > 1 and
      // user is admin
      userDid.in(arbiter.admins) and
      // The user to remove is an admin
      removedAdminDid.in(arbiter.admins)

    if (valid) all {
      // Remove the admin
      def admins = arbiter.admins.exclude(Set(removedAdminDid))
      def updated = arbiter.with("admins", admins)
      arbiters' = arbiters.set(
        arbiterDid,
        updated
      ),
      writeError' = false
    } else all {
      arbiters' = arbiters,
      writeError' = true
    }

  /// Create a new space, managed by an arbiter
  action createSpace(userDid: UserDid, arbiterDid: ArbiterDid, spaceKey: SpaceKey): bool =
    def arbiter = arbiters.get(arbiterDid)
    def valid = 
      // Arbiter exists
      arbiterDid.in(arbiters.keys()) and
      // The user is an admin
      userDid.in(arbiter.admins) and 
      // Space key does not exist
      not(spaceKey.in(arbiter.spaces.keys()))

    if (valid) all {
      // Add space key
      arbiters' = arbiters.set(
        arbiterDid,
        arbiter.with("spaces", arbiter.spaces.put(spaceKey, {
          config: SpaceConfig::default,
          members: Map()
        }))
      ),
      writeError' = false
    } else all {
      arbiters' = arbiters,
      writeError' = true,
    }

  action configureSpace(userDid: UserDid, arbiterDid: ArbiterDid, spaceKey: SpaceKey, spaceConfig: SpaceConfig): bool = 
    def arbiter = arbiters.get(arbiterDid)
    def space = arbiter.spaces.get(spaceKey)
    def valid = 
      // Arbiter exists
      arbiterDid.in(arbiters.keys()) and
      // Space exists
      spaceKey.in(arbiter.spaces.keys()) and 
      // User can manage space
      false

    if (valid) all {
      arbiters' = arbiters.set(arbiterDid,
        arbiter.with("spaces", arbiter.spaces.set(spaceKey,
          // Update the space's config
          space.with("config", spaceConfig) 
        ))
      ),
      writeError' = false
    } else all {
      arbiters' = arbiters,
      writeError' = true,
    }

  /// Set the access that a member has to a space
  action setSpaceMemberAccess(
    userDid: UserDid,
    arbiterDid: ArbiterDid,
    spaceKey: SpaceKey,
    member: Member,
    access: Access
  ): bool = 
    def arbiter = arbiters.get(arbiterDid)
    def space = arbiter.spaces.get(spaceKey)
    def valid = 
      // Arbiter exists
      arbiterDid.in(arbiters.keys()) and
      // The user is an admin
      userDid.in(arbiter.admins) and
      // Space exists
      spaceKey.in(arbiter.spaces.keys())

    if (valid) all {
      arbiters' = arbiters.set(arbiterDid,
        arbiter.with("spaces", arbiter.spaces.set(spaceKey,
          space.with("members", space.members.put(member, access)) 
        ))
      ),
      writeError' = false
    } else all {
      arbiters' = arbiters,
      writeError' = true,
    }

  /// Remove a member's access to a space
  action removeSpaceMember(
    userDid: UserDid,
    arbiterDid: ArbiterDid,
    spaceKey: SpaceKey,
    member: Member,
  ): bool =
    def arbiter = arbiters.get(arbiterDid)
    def space = arbiter.spaces.get(spaceKey)
    def valid = 
      // Arbiter exists
      arbiterDid.in(arbiters.keys()) and
      // The user is an admin
      userDid.in(arbiter.admins) and
      // Space exists
      spaceKey.in(arbiter.spaces.keys())

    if (valid) all {
      arbiters' = arbiters.set(
        arbiterDid,
        arbiter
          .with("spaces", arbiter.spaces.set(spaceKey,
            space.with("members", space.members.mapRemove(member))  
          )
        )
      ),
      writeError' = false
    } else all {
      arbiters' = arbiters,
      writeError' = true,
    }

  /// Calculate the local member list synchronously.
  ///
  /// This is distinct from asynchronously calculating the total member list, which may
  /// require making remote network calls to other arbiters.
  pure def flatMemberList(arbiter: Arbiter, spaceKey: SpaceKey): MemberListItem -> Access =
    // Admins are always in the member list
    def admins = arbiter.admins.map(x => (MLUser(x), RemoveMembers)).setToMap()

    // Define the initial fold state
    def initState: {
      memberList: MemberListItem -> Access,
      visited: Set[{ spaceKey: SpaceKey, access: Access }],
      queue: Set[{ spaceKey: SpaceKey, access: Access }]
    } = {
      // We start with the admin list for the members
      memberList: admins,
      // We keep track of the spaces that we have visited to break cycles
      visited: Set(),
      // We keep track of a queue of spaces that we need to visit to add to the members list.
      queue: Set({ spaceKey: spaceKey, access: RemoveMembers }),
    }

    // Since we must avoid recursion we fold over the number of spaces that there are essentially
    // acting as the max "recursion" depth.
    //
    // NOTE: I think it's possible in remote circumstances that you could fail to fully expand the
    // membership list in cases where we end up needing to visit a space multiple times because it
    // was added with multiple different accesses throughout the tree.
    //
    // This might be acceptable.
    def outputState = 0.to(arbiter.spaces.keys().size()).fold(initState, (state, i) =>
      // Then we visit every space in the queue
      state.queue.fold(state, (state, queuedSpace) =>
        def spaceKey = queuedSpace.spaceKey
        def spaceAccess = queuedSpace.access
        def space = arbiter.spaces.get(spaceKey)

        // If this role does not exist
        if (not(spaceKey.in(arbiter.spaces.keys())))
          // ignore it remove it from the queue
          state
            .with("visited", state.visited.union(Set(queuedSpace)))
            .with("queue", state.queue.exclude(Set(queuedSpace)))
        else 

        // Get the list of local users that have been directly a
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

        // Get the list or remote spaces that have been added as a child to this space
        def remoteSpaces = space.members.keys().remoteSpacesFromMembers().map(spaceId =>
          def childSpaceAccess = space.members.get(MemberRemoteSpace(spaceId))
          (
            spaceId,
            // Important: we have to limit the access of the child space according to the access of
            // the current space.
            //
            // We additionally limit remote spaces so they can only have up to WriteRecords. We
            // don't want to depend on responses from remote spaces to calculate who can change
            // ACLs.
            minAccess(minAccess(childSpaceAccess, spaceAccess), WriteRecords)
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
          def spaceLimitedAccess = minAccess(queuedSpace.access, userAccessInSpace)
          
          // If this user is already in the user list
          if (MLUser(user).in(memberList.keys()))
            // Then we assign them the greater of the access that they already had or that they were
            // granted here.
            memberList.put(MLUser(user), maxAccess(memberList.get(MLUser(user)), spaceLimitedAccess))

          // If the user is not already in the list
          else
            // Add them to the list with the access from this space
            memberList.put(MLUser(user), spaceLimitedAccess)
        )

        // Add the remote spaces to the member list
        def memberList = remoteSpaces.keys().fold(memberListWithUsers, (memberList, spaceId) =>
          def access = remoteSpaces.get(spaceId) 
          def alreadyGrantedAccess = memberList.get(MLSpace(spaceId))

          // If the space is already in the member list
          if (MLSpace(spaceId).in(memberList.keys()))
            // Add whatever access is greater out of the access they already had and the access
            // granted here
            memberList.put(MLSpace(spaceId), maxAccess(alreadyGrantedAccess, access))
          // If the space hasn't been added to the member list
          else
            // Add it
            memberList.put(MLSpace(spaceId), access)
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
}

module types {
  import basicSpells.* from "./basicSpells"
  export basicSpells.*

  type ArbiterDid = str
  type UserDid = str
  type SpaceKey = str

  /// This is a reference to a space, qualified by the arbiter that owns it.
  type SpaceId = { arbiterDid: ArbiterDid, spaceKey: SpaceKey }

  /// The access a member may have to a permissioned space.
  /// All later permissions include the ones before
  type Access =
    // Member can read records in the space
    | ReadRecords
    // Member can write records in the space
    | WriteRecords
    // Member can add new readers to the space
    | AddReaders 
    // Member can add new writers to the space
    | AddWriters
    // Member can and and remove members with any access to/from the space
    | RemoveMembers

  pure val Accesses = Set(ReadRecords, WriteRecords, AddReaders, AddWriters, RemoveMembers)

  /// Get the numeric access level from `Access`.
  pure def accessLevel(a: Access): int =
    match a {
      | ReadRecords => 0
      | WriteRecords => 1
      | AddReaders => 2
      | AddWriters => 3
      | RemoveMembers => 4
    }

  /// Get the smaller of the provided accesses
  pure def minAccess(a: Access, b: Access): Access =
    if (a.accessLevel() < b.accessLevel()) a else b
  
  /// Get the larger of the provided accesses
  pure def maxAccess(a: Access, b: Access): Access =
    if (a.accessLevel() > b.accessLevel()) a else b

  /// A member in a space may either be a user with a DID or another space, in which all the members
  /// of that space extend the members of this space.
  type Member = MemberUser(UserDid) | MemberLocalSpace(SpaceKey) | MemberRemoteSpace(SpaceId)

  type MemberListItem = MLUser(UserDid) | MLSpace(SpaceId)

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

  /// Extra configuration associated to a space
  type SpaceConfig = {
    // Whether or not records in the space can be read without being a member of the space.
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
    config: SpaceConfig,
    // The access level for each member in the space
    members: Member -> Access
  }

  /// Data for an arbiter
  type Arbiter = {
    did: ArbiterDid,
    admins: Set[UserDid],
    spaces: SpaceKey -> Space,
  }
}

module test {
  import types.*
  import model.*

  pure val arbiter1 = {
    did: "arbiter1",
    admins: Set("zicklag", "erlend", "meri"),
    spaces: Map(
      "members" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberUser("charlie") -> RemoveMembers,
          MemberUser("eve") -> WriteRecords,
        )
      },
      "team" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberUser("zicklag") -> RemoveMembers,
          MemberUser("erlend") -> RemoveMembers,
          MemberUser("meri") -> RemoveMembers,
          MemberUser("alice") -> RemoveMembers,
          MemberLocalSpace("designers") -> WriteRecords,
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
          MemberLocalSpace("members") -> WriteRecords,
          MemberUser("bob") -> ReadRecords,
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
          MemberLocalSpace("members") -> ReadRecords,
        )
      }
    )
  }

  /// Run some test cases to validate the flat local member list output
  run memberListArbiter1Test = all {
    all {
      assert(
        flatMemberList(arbiter1, "#general") ==
        Map(
          // Can manage because she's in the team
          MLUser("alice") -> RemoveMembers,
          // Can read because he's directly added to general channel
          MLUser("bob") -> ReadRecords,
          // Can write because he's a member
          MLUser("charlie") -> WriteRecords,
          // Can write because she's a member
          MLUser("eve") -> WriteRecords,
          // Can write because he's in designers which is in the team
          MLUser("fish") -> WriteRecords,
          // Can manage because she's an admin
          MLUser("meri") -> RemoveMembers,
          // Can manage because he's an admin
          MLUser("erlend") -> RemoveMembers,
          // Can manage because he's an admin
          MLUser("zicklag") -> RemoveMembers,
        )
      ),
      assert(
        flatMemberList(arbiter1, "#internal") ==
        Map(
          // Can manage because she's in the team
          MLUser("alice") -> RemoveMembers,
          // Can write because he's in designers which is in the team
          MLUser("fish") -> WriteRecords,
          // Can manage because she's an admin
          MLUser("meri") -> RemoveMembers,
          // Can manage because he's an admin
          MLUser("erlend") -> RemoveMembers,
          // Can manage because he's an admin
          MLUser("zicklag") -> RemoveMembers,
        )
      ),
      assert(
        flatMemberList(arbiter1, "#design") ==
        Map(
          // Can manage because she's in the team
          MLUser("alice") -> RemoveMembers,
          // Charlie can read 'cause he's in members
          MLUser("charlie") -> ReadRecords,
          // Eve can read 'cause she's in members
          MLUser("eve") -> ReadRecords,
          // Can manage 'cause he's in designers
          MLUser("fish") -> RemoveMembers,
          // Can manage because she's an admin
          MLUser("meri") -> RemoveMembers,
          // Can manage because he's an admin
          MLUser("erlend") -> RemoveMembers,
          // Can manage because he's an admin
          MLUser("zicklag") -> RemoveMembers,
        )
      )
    }
  }

  pure val arbiter2 = {
    did: "arbiter2",
    admins: Set("zicklag"),
    spaces: Map(
      "managers" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberUser("meri") -> RemoveMembers,
        )
      },
      "team" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberUser("erlend") -> WriteRecords,
        )
      },
      "strange" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberLocalSpace("managers") -> RemoveMembers,
        )
      },
      "#test" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberLocalSpace("managers") -> ReadRecords,
          MemberLocalSpace("team") -> RemoveMembers,
        )
      },
      "#off-topic" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberLocalSpace("managers") -> ReadRecords,
          MemberLocalSpace("strange") -> RemoveMembers,
        )
      },
      "#on-topic" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberLocalSpace("managers") -> ReadRecords,
        )
      },
    )
  }

  /// Run some test cases to validate the flat local member list output
  run memberListArbiter2Test = all {
    all {
      assert(
        flatMemberList(arbiter2, "#test") ==
        Map(
          MLUser("meri") -> ReadRecords,
          MLUser("erlend") -> WriteRecords,
          MLUser("zicklag") -> RemoveMembers,
        )
      ),
      assert(
        flatMemberList(arbiter2, "#off-topic") ==
        Map(
          MLUser("meri") -> RemoveMembers,
          MLUser("zicklag") -> RemoveMembers,
        )
      ),
      assert(
        flatMemberList(arbiter2, "#on-topic") ==
        Map(
          MLUser("meri") -> ReadRecords,
          MLUser("zicklag") -> RemoveMembers,
        )
      ),
    }
  }

  pure val arbiter3 = {
    did: "arbiter2",
    admins: Set("admin"),
    spaces: Map(
      "role1" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberUser("user1") -> RemoveMembers,
          MemberLocalSpace("role2") -> ReadRecords,
          MemberRemoteSpace({ arbiterDid: "arbiter4", spaceKey: "example" }) -> AddWriters,
        )
      },
      "role2" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberUser("user2") -> WriteRecords,
          MemberLocalSpace("role3") -> WriteRecords,
        )
      },
      "role3" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberUser("user3") -> RemoveMembers,
          MemberUser("user4") -> ReadRecords,
          MemberLocalSpace("role1") -> RemoveMembers,
        )
      },
      "#channel1" -> {
        config: SpaceConfig::default,
        members: Map(
          MemberLocalSpace("role2") -> RemoveMembers,
          // This should be safely ignored
          MemberLocalSpace("doesn't exist") -> RemoveMembers,
        )
      },
    )
  }

   /// Run some test cases to validate the flat local member list output
  run memberListArbiter3Test = all {
    all {
      assert(
        flatMemberList(arbiter3, "#channel1") ==
        Map(
          MLUser("admin") -> RemoveMembers,
          MLUser("user2") -> WriteRecords,
          MLUser("user3") -> WriteRecords,
          MLUser("user4") -> ReadRecords,
          // User one has write because they were granted manage in role1,
          // which was granted manage in role 3,
          // which was granted write in role 2,
          // which was granted manage in channel 1
          MLUser("user1") -> WriteRecords,
          // Note how the remote space's access is limited to write because we don't allow members
          // from remote spaces to manage members.
          MLSpace({ arbiterDid: "arbiter4", spaceKey: "example" }) -> WriteRecords
        )
      ),
    }
  }

}