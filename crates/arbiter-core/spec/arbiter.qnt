module arbiter {
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
    setSpaceMemberAccessAny,
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

  action setSpaceMemberAccessAny = all {
    nondet arbiterDid = arbiterDids.oneOf()
    nondet userDid = userDids.oneOf()
    nondet spaceKey = spaceKeys.oneOf()
    def possibleSpaces = arbiterDids.map(a => roleIds.map(r => { arbiterDid: a, spaceKey: r })).flatten()
    nondet member = userDids.map(x => MemberUser(x)).union(possibleSpaces.map(x => MemberSpace(x))).oneOf()
    nondet access = Accesses.oneOf()
    setSpaceMemberAccess(userDid, arbiterDid, spaceKey, member, access)
  }

  action removeSpaceMemberAny = all {
    nondet arbiterDid = arbiterDids.oneOf()
    nondet userDid = userDids.oneOf()
    nondet spaceKey = spaceKeys.oneOf()
    def possibleSpaces = arbiterDids.map(a => roleIds.map(r => { arbiterDid: a, spaceKey: r })).flatten()
    nondet member = userDids.map(x => MemberUser(x)).union(possibleSpaces.map(x => MemberSpace(x))).oneOf()
    removeSpaceMember(userDid, arbiterDid, spaceKey, member)
  }
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
    // InvRoleDoesNotSquashIndividualWrite,
    // InvAllAdminsCanWrite,
  }

  /// There is always at least one admin for any given arbiter
  val InvAtLeastOneAdmin =
    arbiters.keys().forall(arbiterDid =>
      def arbiter = arbiters.get(arbiterDid)
      arbiter.admins.size() >= 1
    )

  // /// Invariant: all admins can write to every space managed by the arbiter they are an admin of.
  // val InvAllAdminsCanWrite =
  //   arbiters.keys().forall(arbiterDid =>
  //     def arbiter = arbiters.get(arbiterDid)
  //     arbiter.spaces.keys().forall(spaceKey =>
  //       def space = arbiter.spaces.get(spaceKey) 
  //       match getSpaceMemberList(arbiterDid, spaceKey) {
  //         | Some(flatUserAccessList) =>
  //           arbiter.admins.forall(adminDid =>
  //             flatUserAccessList.get(adminDid) == Write 
  //           )
  //         | None => false
  //       }
  //     )
  //   )
  
  // /// Invariant: If you have direct write access, then that means the flat access list should have reported
  // /// your access as Write, not allowing the read access inherited from a role to squash your write
  // /// access.
  // val InvRoleDoesNotSquashIndividualWrite =
  //   arbiters.keys().forall(arbiterDid =>
  //     def arbiter = arbiters.get(arbiterDid)
  //     arbiter.spaces.keys().forall(spaceKey =>
  //       def space = arbiter.spaces.get(spaceKey) 
  //       match getSpaceMemberList(arbiterDid, spaceKey) {
  //         | Some(flatUserAccessList) =>
  //           space.members.keys()
  //           .exclude(arbiter.admins.map(x => User(x))).forall(member =>
  //             match member {
  //               | User(userDid) =>
  //                 if (space.members.keys().contains(User(userDid)))
  //                   def flatAccess = flatUserAccessList.get(userDid)
  //                   def directAccess = space.members.get(User(userDid))
  //                   (directAccess == Write).implies(flatAccess == Write)
  //                 else true
  //               | Role(roleId) => true
  //             } 
  //           )
  //         | None => false
  //       }
  //     )
  //   )

  // /// Get the flattened member list for a space, evaluating roles
  // def getSpaceMemberList(arbiterDid: ArbiterDid, spaceKey: SpaceKey): Option[UserDid -> Access] =
  //   // Get the arbiter
  //   if (not(arbiterDid.in(arbiters.keys()))) None
  //   else def arbiter = arbiters.get(arbiterDid)

  //   // Get the space
  //   if (not(spaceKey.in(arbiter.spaces.keys()))) None
  //   else def space = arbiter.spaces.get(spaceKey)

  //   // Calculate the member access list, resolving roles as necessary
  //   def admins = arbiter.admins.fold(Map(), (members, adminDid) => members.put(adminDid, Write))
  //   def members = space.members.keys().fold(
  //     admins,
  //     (members, member) =>
  //       def access = space.members.get(member)
  //       match member {
  //         | User(userDid) => 
  //           if (members.keys().contains(userDid) and members.get(userDid) == Write)
  //             members // If the member already has write access, just leave it
  //           else 
  //             members.put(userDid, access)
  //         | Role(roleId) =>
  //           def userDids = arbiter.roles.get(roleId)
  //           userDids.fold(members, (m, userDid) =>
  //             if (m.keys().contains(userDid) and m.get(userDid) == Write)
  //               m // If the member already has write access, just leave it
  //             else
  //               m.put(userDid, access)
  //           )
  //       }
  //   )
  //   Some(members)

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
      // The user is an admin
      userDid.in(arbiter.admins) and
      // Space exists
      spaceKey.in(arbiter.spaces.keys())

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
  /// All higher later permissions include the ones before
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
    | ManageSpace

  pure val Accesses = Set(ReadRecords, WriteRecords, AddReaders, AddWriters, ManageSpace)

  /// Get the numeric access level from `Access`.
  pure def accessLevel(a: Access): int =
    match a {
      | ReadRecords => 0
      | WriteRecords => 1
      | AddReaders => 2
      | AddWriters => 3
      | ManageSpace => 4
    }

  /// Check whether the access `a` is greater ( more power ) than the access `b`
  pure def accessGreaterThan(a: Access, b: Access): bool =
    accessLevel(a) > accessLevel(b)

  /// A member in a space may either be a user with a DID or another space, in which all the members
  /// of that space extend the members of this space.
  type Member = MemberSpace(SpaceId) | MemberUser(UserDid)

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