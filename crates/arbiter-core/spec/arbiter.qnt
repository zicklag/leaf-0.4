module arbiter {
  import types.*

  import model.*

  pure val userDids: Set[UserDid] = Set("Alice", "Bob", "Charlie", "Daffodil", "Eve")
  pure val arbiterDids: Set[UserDid] = Set("arb1", "arb2")
  pure val spaceKeys: Set[UserDid] = Set("space1", "space2")
  pure val roleIds: Set[RoleId] = Set("members", "team", "collaborators")

  action init = initArbiter

  action step = any {
    createArbiterAny,
    addArbiterAdminAny,
    removeArbiterAdminAny,
    createSpaceAny,
    setSpaceMemberAccessAny,
    createRoleAny,
    addRoleMemberAny,
    removeRoleMemberAny,
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

  action setSpaceMemberAccessAny = all {
    nondet arbiterDid = arbiterDids.oneOf()
    nondet userDid = userDids.oneOf()
    nondet spaceKey = spaceKeys.oneOf()
    nondet member = userDids.map(x => User(x)).union(roleIds.map(x => Role(x))).oneOf()
    nondet access = Set(Read, Write).oneOf()
    setSpaceMemberAccess(userDid, arbiterDid, spaceKey, member, access)
  }

  action removeSpaceMemberAny = all {
    nondet arbiterDid = arbiterDids.oneOf()
    nondet userDid = userDids.oneOf()
    nondet spaceKey = spaceKeys.oneOf()
    nondet member = userDids.map(x => User(x)).union(roleIds.map(x => Role(x))).oneOf()
    removeSpaceMember(userDid, arbiterDid, spaceKey, member)
  }

  action createRoleAny = all {
    nondet arbiterDid = userDids.oneOf()
    nondet userDid = userDids.oneOf()
    nondet roleId = roleIds.oneOf()
    createRole(userDid, arbiterDid, roleId)
  }

  action addRoleMemberAny = all {
    nondet arbiterDid = arbiterDids.oneOf()
    nondet userDid = userDids.oneOf()
    nondet roleId = roleIds.oneOf()
    nondet member = userDids.oneOf()
    addRoleMember(userDid, arbiterDid, roleId, member)
  }

  action removeRoleMemberAny = all {
    nondet arbiterDid = userDids.oneOf()
    nondet userDid = userDids.oneOf()
    nondet roleId = roleIds.oneOf()
    nondet member = userDids.oneOf()
    removeRoleMember(userDid, arbiterDid, roleId, member)
  }
}

module model {
  import types.*

  /// Counter for creating unique IDs
  var arbiters: ArbiterDid -> Arbiter
  var error: bool
  
  action initArbiter = all {
    arbiters' = Map(),
    error' = false
  }

  /// Arbiter invariants
  val Inv = all {
    InvAtLeastOneAdmin,
    InvRoleDoesNotSquashIndividualWrite,
    InvAllAdminsCanWrite,
  }

  /// There is always at least one admin for any given arbiter
  val InvAtLeastOneAdmin =
    arbiters.keys().forall(arbiterDid =>
      def arbiter = arbiters.get(arbiterDid)
      arbiter.admins.size() >= 1
    )

  /// Invariant: all admins can write to every space managed by the arbiter they are an admin of.
  val InvAllAdminsCanWrite =
    arbiters.keys().forall(arbiterDid =>
      def arbiter = arbiters.get(arbiterDid)
      arbiter.spaces.keys().forall(spaceKey =>
        def space = arbiter.spaces.get(spaceKey) 
        match getSpaceMemberList(arbiterDid, spaceKey) {
          | Some(flatUserAccessList) =>
            arbiter.admins.forall(adminDid =>
              flatUserAccessList.get(adminDid) == Write 
            )
          | None => false
        }
      )
    )
  
  /// Invariant: If you have direct write access, then that means the flat access list should have reported
  /// your access as Write, not allowing the read access inherited from a role to squash your write
  /// access.
  val InvRoleDoesNotSquashIndividualWrite =
    arbiters.keys().forall(arbiterDid =>
      def arbiter = arbiters.get(arbiterDid)
      arbiter.spaces.keys().forall(spaceKey =>
        def space = arbiter.spaces.get(spaceKey) 
        match getSpaceMemberList(arbiterDid, spaceKey) {
          | Some(flatUserAccessList) =>
            space.members.keys()
            .exclude(arbiter.admins.map(x => User(x))).forall(member =>
              match member {
                | User(userDid) =>
                  if (space.members.keys().contains(User(userDid)))
                    def flatAccess = flatUserAccessList.get(userDid)
                    def directAccess = space.members.get(User(userDid))
                    (directAccess == Write).implies(flatAccess == Write)
                  else true
                | Role(roleId) => true
              } 
            )
          | None => false
        }
      )
    )

  /// Get the flattened member list for a space, evaluating roles
  def getSpaceMemberList(arbiterDid: ArbiterDid, spaceKey: SpaceKey): Option[UserDid -> Access] =
    // Get the arbiter
    if (not(arbiterDid.in(arbiters.keys()))) None
    else def arbiter = arbiters.get(arbiterDid)

    // Get the space
    if (not(spaceKey.in(arbiter.spaces.keys()))) None
    else def space = arbiter.spaces.get(spaceKey)

    // Calculate the member access list, resolving roles as necessary
    def admins = arbiter.admins.fold(Map(), (members, adminDid) => members.put(adminDid, Write))
    def members = space.members.keys().fold(
      admins,
      (members, member) =>
        def access = space.members.get(member)
        match member {
          | User(userDid) => 
            if (members.keys().contains(userDid) and members.get(userDid) == Write)
              members // If the member already has write access, just leave it
            else 
              members.put(userDid, access)
          | Role(roleId) =>
            def userDids = arbiter.roles.get(roleId)
            userDids.fold(members, (m, userDid) =>
              if (m.keys().contains(userDid) and m.get(userDid) == Write)
                m // If the member already has write access, just leave it
              else
                m.put(userDid, access)
            )
        }
    )
    Some(members)

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
        roles: Map(),
        spaces: Map()
      }),
      error' = false
    } else all {
      arbiters' = arbiters,
      error' = true
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
      error' = false
    } else all {
      arbiters' = arbiters,
      error' = true
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
      error' = false
    } else all {
      arbiters' = arbiters,
      error' = true
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
        arbiter.with("spaces", arbiter.spaces.put(spaceKey, { members: Map() }))
      ),
      error' = false
    } else all {
      arbiters' = arbiters,
      error' = true,
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
      spaceKey.in(arbiter.spaces.keys()) and

      // Role exists, if member is a role
      match member {
        | Role(r) => r.in(arbiter.roles.keys())
        | User(u) => true
      }

    if (valid) all {
      arbiters' = arbiters.set(arbiterDid,
        arbiter.with("spaces", arbiter.spaces.set(spaceKey,
          space.with("members", space.members.put(member, access)) 
        ))
      ),
      error' = false
    } else all {
      arbiters' = arbiters,
      error' = true,
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
      error' = false
    } else all {
      arbiters' = arbiters,
      error' = true,
    }

  /// Add a new role to the arbiter
  action createRole(
    userDid: UserDid,
    arbiterDid: ArbiterDid,
    roleId: RoleId,
  ): bool =
    def arbiter = arbiters.get(arbiterDid)
    def valid = 
      // Arbiter exists
      arbiterDid.in(arbiters.keys()) and 
      // The user is an admin
      userDid.in(arbiter.admins) and
      // Role does not already exist
      not(roleId.in(arbiter.roles.keys()))

    if (valid) all {
      // Update roles list
      arbiters' = arbiters.set(
        arbiterDid,
        arbiter.with("roles", arbiter.roles.put(roleId, Set()))
      ),
      error' = false
    } else all {
      arbiters' = arbiters,
      error' = true,
    }

  /// Add a new member to a role
  action addRoleMember(
    userDid: UserDid,
    arbiterDid: ArbiterDid,
    roleId: RoleId,
    member: UserDid
  ): bool =
    def arbiter = arbiters.get(arbiterDid)
    def valid = 
      // Arbiter exists
      arbiterDid.in(arbiters.keys()) and
      // The user is an admin
      userDid.in(arbiter.admins) and
      // Role already exist
      roleId.in(arbiter.roles.keys()) and
      // Member not already in role
      not(member.in(arbiter.roles.get(roleId)))

    if (valid) all {
      // Update roles list
      arbiters' = arbiters.set(
        arbiterDid,
        arbiter.with("roles", arbiter.roles.setBy(roleId, role => role.union(Set(member))))
      ),
      error' = false
    } else all {
      arbiters' = arbiters,
      error' = true,
    }

  /// Remove a member from a role
  action removeRoleMember(
    userDid: UserDid,
    arbiterDid: ArbiterDid,
    roleId: RoleId,
    member: UserDid
  ): bool =
    def arbiter = arbiters.get(arbiterDid)
    def valid =
      // Arbiter exists
      arbiterDid.in(arbiters.keys()) and
      // The user is an admin
      userDid.in(arbiter.admins) and
      // Role already exist
      roleId.in(arbiter.roles.keys()) and
      // Member is in role
      member.in(arbiter.roles.get(roleId))

    if (valid) all {
      arbiters' = arbiters.set(
        arbiterDid,
        arbiter.with("roles", arbiter.roles.setBy(roleId, role => role.exclude(Set(member))))
      ),
      error' = false
    } else all {
      arbiters' = arbiters,
      error' = true,
    }
  }

module types {
  import basicSpells.* from "./basicSpells"
  export basicSpells.*

  //
  // PDS
  //

  type ArbiterDid = str
  type UserDid = str
  type SpaceKey = str
  type Record = str
  type RoleId = str

  /// The PDS data associated to an individual user
  type UserPds = {
    // The DID of the user that this PDS data is for
    did: UserDid,
    // The permissioned repos that the user has on the PDS
    permRepos: SpaceKey -> PermRepo
  }

  /// A permissioned repo
  type PermRepo = {
    arbiterDid: ArbiterDid,
    // The space that the permissioned repo is a part of
    spaceKey: SpaceKey,
    // The records in the repo
    records: Set[Record]
  }

  //
  // Arbiter
  //

  /// The access a member may have to a permissioned space
  type Access = Read | Write

  type Member = Role(RoleId) | User(UserDid)

  /// A permissioned space
  type PermSpace = {
    // The access level for each member in the space
    members: Member -> Access
  }

  /// Data for an arbiter
  type Arbiter = {
    did: ArbiterDid,
    admins: Set[UserDid],
    roles: RoleId -> Set[UserDid],
    spaces: SpaceKey -> PermSpace,
  }
}