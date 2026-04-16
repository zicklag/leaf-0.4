
module model {
  import types.*

  type Error =
    | ErrArbiterAlreadyExists
    | ErrArbiterNotExists
    | ErrJobIdAlreadyUsed
    | ErrJobNotExists
    | ErrSpaceNotNeeded
    | ErrSpaceAlreadyResolved
    | ErrSpaceAlreadyExists
    | ErrSpaceNotExists
    | ErrPermissionDenied
    | ErrOnlyLastOwnerCanDeleteArbiter
    | ErrCannotDeleteAdminSpace
    | ErrMemberNotExist
    | ErrPermissionChanged

  /// The list of arbiters.
  ///
  /// It is assumed that each arbiter could be hosted on a separate server, so we assume that it's
  /// impossible to make updates atomically across multiple arbiters.
  var arbiters: ArbiterDid -> Arbiter

  var error: Option[Error]
  
  /// Initial state
  action initArbiter = all {
    arbiters' = Map(),
  }

  //
  // ACTIONS
  //

  action throw(e: Error): bool = all {
    arbiters' = arbiters,
    error' = Some(e),
  }

  action ok: bool = all {
    arbiters' = arbiters,
    error' = None,
  }
  
  /// Create a new arbiter.
  ///
  /// The user the authenticated user carrying out the action.
  action create(userDid: UserDid, arbiterDid: ArbiterDid): bool = 
    // Check arbiter exists
    if (arbiters.has(arbiterDid)) throw(ErrArbiterAlreadyExists) else

    all {
      // Add arbiter
      arbiters' = arbiters.put(arbiterDid, {
        version: 0,
        did: arbiterDid,
        jobQueue: Map(),
        jobResults: Map(),
        spaces: Map(
          // Create the special $admin space
          SpaceKey::ADMIN -> {
            config: SpaceConfig::default,
            members: Map(
              // Add the creator of the arbiter as an owner
              MemberUser(userDid) -> Owner
            ),
          }
        )
      }),
      error' = None,
    }

  /// Receive a new job to process 
  action startJob(userDid: UserDid, arbiterDid: ArbiterDid, spaceKey: SpaceKey, jobId: JobId, args: JobArgs): bool =
    def arbiter = arbiters.get(arbiterDid)

    // Check arbiter exists
    if (not(arbiters.has(arbiterDid))) throw(ErrArbiterNotExists) else
    // Check Job ID not already used
    if (arbiter.jobQueue.has(jobId) or arbiter.jobResults.has(jobId)) throw(ErrJobIdAlreadyUsed) else

    // Do some validation on the job
    match (jobValidationError(arbiter, spaceKey, args)) { | Some(err) => throw(err) | None =>

    // Get the local member list
    def unresolvedMembers = arbiter.members(spaceKey)

    // Try to resolve it, which will succeed if there's no access delegated to remote spaces
    def attemptedResolve = unresolvedMembers.resolveSync()

    match (attemptedResolve) {
      // If we successfully resolved the member list, execute the job immediately
      | Some(resolved) => executeJob(arbiterDid, resolved, {
          id: jobId,
          userDid: userDid,
          spaceKey: spaceKey,
          args: args,
          unresolvedMembers: unresolvedMembers,
          resolvedSpaces: Map(),
          arbiterVersion: arbiter.version,
        })

      // If we cannot resolve synchronously, then we need to queue the job
      | None => all {
        arbiters' = arbiters.put(arbiterDid, arbiter
          .nextVersion()
          .with("jobQueue", arbiter.jobQueue.put(jobId, {
            id: jobId,
            userDid: userDid,
            spaceKey: spaceKey,
            args: args,
            unresolvedMembers: unresolvedMembers,
            resolvedSpaces: Map(),
            arbiterVersion: arbiter.version,
          }))
        ),
        error' = None,
      }
    }}

  /// Provide the resolved space member list for a job that was waiting on a member list to be resolved.
  action provideRemoteSpaceMembers(arbiterDid: ArbiterDid, jobId: JobId, spaceId: SpaceId, members: ResolvedMemberList): bool =
    def arbiter = arbiters.get(arbiterDid)
    def job = arbiter.jobQueue.get(jobId)

    // Check arbiter exists
    if (not(arbiters.has(arbiterDid))) throw(ErrArbiterNotExists) else
    // Check job exists
    if (not(arbiter.jobQueue.has(jobId))) throw(ErrJobNotExists) else
    // Check that this job actually needs the space to be resolved
    if (not(job.unresolvedMembers.has(UMLSpace(spaceId)))) throw(ErrSpaceNotNeeded) else
    // Check that we haven't already resolved the space
    if (job.resolvedSpaces.has(spaceId)) throw(ErrSpaceAlreadyResolved) else

    // Create an updated list of the resolved spaces
    def resolvedSpaces = job.resolvedSpaces.put(spaceId, members)

    // Check whether the job has all of it's spaces resolved and is ready to be executed
    def jobIsReady = job
      .unresolvedMembers
      .spacesToResolve()
      .exclude(job.resolvedSpaces.keys()).size() == 0

    // If the job is ready to be executed
    if (jobIsReady)
      // Execute it immediately
      def resolved = joinResolvedSpaces(job.unresolvedMembers, resolvedSpaces)
      executeJob(arbiterDid, resolved, job)

    // If the job is not ready
    else
      all {
        // Just update the queued job's resolved spaces
        arbiters' = arbiters.put(arbiterDid, arbiter
          .nextVersion()
          .with("jobQueue",
            arbiter.jobQueue.put(jobId,
              job.with("resolvedSpaces", resolvedSpaces) 
            )
          )
        ),
        error' = None,
      }

  /// Timeout a job that is in the queue and waiting for space member lists to be resolved.
  ///
  /// This will execute the job without waiting for the remote member lists to finish resolving.
  action timeoutJob(arbiterDid: ArbiterDid, jobId: JobId): bool =
    def arbiter = arbiters.get(arbiterDid)
    def job = arbiter.jobQueue.get(jobId)

    // Check arbiter exists
    if (not(arbiters.has(arbiterDid))) throw(ErrArbiterNotExists) else
    // Check job exists
    if (not(arbiter.jobQueue.has(jobId))) throw(ErrJobNotExists) else

    // Resolve the member list with whatever spaces it has right now
    def resolved = joinResolvedSpaces(job.unresolvedMembers, job.resolvedSpaces)

    // Execute the job
    executeJob(arbiterDid, resolved, job)


  //
  // HELPER ACTIONS
  //
  
  /// Return any validation errors for the job if there are issues
  pure def jobValidationError(arbiter: Arbiter, spaceKey: SpaceKey, args: JobArgs): Option[Error] =
    match (args) {
      | JobCreateSpace =>
        // Make sure the space doesn't already exist
        if (arbiter.spaces.has(spaceKey)) Some(ErrSpaceAlreadyExists)
        else None
      | JobConfigureSpace(x) =>
        // Make sure the space exists
        if (not(arbiter.spaces.has(spaceKey))) Some(ErrSpaceNotExists)
        else None
      | JobDeleteSpace =>
        // Make sure we don't delete the $admin space
        if (spaceKey == SpaceKey::ADMIN) Some(ErrCannotDeleteAdminSpace) else
        // Make sure the space exists
        if (not(arbiter.spaces.has(spaceKey))) Some(ErrSpaceNotExists)
        else None
      | JobSetMemberAccess(x) =>
        // Make sure the space exists
        if (not(arbiter.spaces.has(spaceKey))) Some(ErrSpaceNotExists)
        else None
      | JobRemoveMember(x) => 
        def space = arbiter.spaces.get(spaceKey)
        // Make sure the space exists
        if (not(arbiter.spaces.has(spaceKey))) Some(ErrSpaceNotExists)
        // Make sure the member is in the space
        else if (not(space.members.has(x.member))) Some(ErrMemberNotExist)
        else None
    }

  /// Finishes the execution of a job, applying the changes, or erroring if they are invalid.
  action executeJob(
    arbiterDid: ArbiterDid, 
    members: ResolvedMemberList,
    job: Job,
  ): bool =
    def arbiter = arbiters.get(arbiterDid)

    // Check arbiter exists
    if (not(arbiters.has(arbiterDid))) throw(ErrArbiterNotExists) else

    // Check that arbiter has not been modified since the job was started ( compare-and-swap style )
    if (arbiter.version != job.arbiterVersion) throw(ErrPermissionChanged) else

    match (job.args) {
      | JobCreateSpace => createSpace(arbiterDid, members, job)
      | JobConfigureSpace(config) => configureSpace(arbiterDid, members, job, config)
      | JobDeleteSpace => deleteSpace(arbiterDid, members, job)
      | JobSetMemberAccess(m) => setMemberAccess(arbiterDid, members, job, m.member, m.access)
      | JobRemoveMember(args) => removeMember(arbiterDid, members, job, args.member)
    }

  action createSpace(
    arbiterDid: ArbiterDid, 
    resolved: ResolvedMemberList,
    job: Job,
  ): bool = 
    def arbiter = arbiters.get(arbiterDid)
    def memberAccess = resolved.memberList.get(job.userDid)

    // Check space doesn't exist
    if (arbiter.spaces.has(job.spaceKey)) throw(ErrSpaceAlreadyExists) else

    // Check that the user has access to create spaces
    // NOTE: The space did not exist at the time the job was started, so the member list will only
    // include those in the admin role, which is appropriate for us to check who can create a space.
    if (not(resolved.memberList.has(job.userDid)) or not(memberAccess.includesAccess(CreateSpaces)))
      throw(ErrPermissionDenied)
    else

    all {
      // Create the new space
      arbiters' = arbiters.put(arbiterDid, arbiter
        .nextVersion()
        .with("spaces", arbiter.spaces.put(job.spaceKey, {
          config: SpaceConfig::default,
          members: Map(),
        }))),
      error' = None,
    }

  action configureSpace(
    arbiterDid: ArbiterDid, 
    resolved: ResolvedMemberList,
    job: Job,
    config: SpaceConfig
  ): bool =
    def arbiter = arbiters.get(arbiterDid)
    def userAccess = resolved.memberList.get(job.userDid)
    def space = arbiter.spaces.get(job.spaceKey)

    // Check space exists
    if (not(arbiter.spaces.has(job.spaceKey))) throw(ErrSpaceNotExists) else
    // Check that the user has access to configure the space
    if (not(resolved.memberList.has(job.userDid) and userAccess.includesAccess(ConfigureSpace)))
      throw(ErrPermissionDenied)
    else

    all {
      // Configure the space
      arbiters' = arbiters.put(arbiterDid, arbiter
        .nextVersion()
        .with("spaces",
          arbiter.spaces.put(job.spaceKey, space.with("config", config)))),
      error' = None,
    }

  action deleteSpace(
    arbiterDid: ArbiterDid, 
    resolved: ResolvedMemberList,
    job: Job,
  ): bool =
    def arbiter = arbiters.get(arbiterDid)
    def userAccess = resolved.memberList.get(job.userDid)
    def space = arbiter.spaces.get(job.spaceKey)

    // Check space exists
    if (not(arbiter.spaces.has(job.spaceKey))) throw(ErrSpaceNotExists) else
    // Make sure we don't delete the $admin space ( we already checked this earlier in jobValidationError but just to be sure )
    if (job.spaceKey == SpaceKey::ADMIN) throw(ErrCannotDeleteAdminSpace) else
    // Check that the user has access to delete the space
    if (not(resolved.memberList.has(job.userDid) and userAccess.includesAccess(RemoveSpace)))
      throw(ErrPermissionDenied)
    else

    all {
      // Remove the space from the arbiter
      arbiters' = arbiters.put(arbiterDid, arbiter
        .nextVersion()
        .with("spaces", arbiter.spaces.mapRemove(job.spaceKey))),
      error' = None,
    }

  action setMemberAccess(
    arbiterDid: ArbiterDid, 
    resolved: ResolvedMemberList,
    job: Job,
    updatedMember: Member,
    updatedMemberAccess: Access
  ): bool =
    def arbiter = arbiters.get(arbiterDid)
    def userAccess = resolved.memberList.get(job.userDid)
    def space = arbiter.spaces.get(job.spaceKey)

    // Check space exists
    if (not(arbiter.spaces.has(job.spaceKey))) throw(ErrSpaceNotExists) else
    // Check that the user has access to the space
    if (not(resolved.memberList.has(job.userDid))) throw(ErrPermissionDenied) else
    // Check that the user has access to add members
    if (not(userAccess.includesAccess(AddMembers))) throw(ErrPermissionDenied) else
    // Check that the access the user is trying to grant the member is less than or equal to their own
    if (not(userAccess.includesAccess(updatedMemberAccess))) throw(ErrPermissionDenied) else

    def updatedMemberExists = space.members.has(updatedMember)
    def updatedMemberExistingAccess = space.members.get(updatedMember)

    // Check that the user isn't modifying the access of a member that already exists, if they don't have RemoveMember permission
    if (updatedMemberExists and not(userAccess.includesAccess(RemoveMembers))) throw(ErrPermissionDenied) else
    // Check that the user isn't modifying the access of a member that already exists, if the updated member has more access than the user
    if (updatedMemberExists and updatedMemberExistingAccess.accessLevel() > userAccess.accessLevel()) throw(ErrPermissionDenied) else

    all {
      // Update the member's access
      arbiters' = arbiters.put(arbiterDid, arbiter
        .nextVersion()
        .with("spaces", arbiter.spaces.put(job.spaceKey, space
          .with("members", space.members.put(updatedMember, updatedMemberAccess))
        ))
      ),
      error' = None,
    }

  action removeMember(
    arbiterDid: ArbiterDid, 
    resolved: ResolvedMemberList,
    job: Job,
    removedMember: Member,
  ): bool =
    def arbiter = arbiters.get(arbiterDid)
    def userAccess = resolved.memberList.get(job.userDid)
    def space = arbiter.spaces.get(job.spaceKey)
    def removedMemberAccess = space.members.get(removedMember)

    // Check space exists
    if (not(arbiter.spaces.has(job.spaceKey))) throw(ErrSpaceNotExists) else
    // Check that the member is in the space
    if (not(space.members.has(removedMember))) throw(ErrMemberNotExist) else
    // Check that the user has access to remove members
    if (not(userAccess.includesAccess(RemoveMembers))) throw(ErrPermissionDenied) else

    // Check that the user isn't removing a member that has more access than they do
    if (removedMemberAccess.accessLevel() > userAccess.accessLevel()) throw(ErrPermissionDenied) else

    all {
      // Remove the member
      arbiters' = arbiters.put(arbiterDid, arbiter
        .nextVersion()
        .with("spaces", arbiter.spaces.put(job.spaceKey, space
          .with("members", space.members.mapRemove(removedMember))
        ))
      ),
      error' = None,
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
      // Note we allow up to the full owner access at the top-level.
      queue: Set(
        // As well as any members from the admin space
        { spaceKey: SpaceKey::ADMIN, access: Owner },
        // We need to add the members from the space itself
        { spaceKey: spaceKey, access: Owner }
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
    | IsMember
    // Member can add new members to the space, with permissions equal to or less than their own
    | AddMembers
    // Member can remove members with equal or lower permission from the space
    | RemoveMembers
    // Member can change the space's configuration
    | ConfigureSpace
    // Member can create new spaces. Note that this is not effective unless granted to a user or
    // space that is delegated to in the $admin space.
    | CreateSpaces
    // Member can delete a space. Note that if this access is granted to somebody in the $admin
    // space, then it means that they can delete any space.
    | RemoveSpace
    // The member has no permissions limits and can remove other admins and owners
    | Owner

  /// Set of all accesses levels
  pure val Accesses = Set(
    IsMember,
    AddMembers,
    RemoveMembers,
    ConfigureSpace,
    CreateSpaces,
    RemoveSpace,
    Owner
  )

  /// Get the numeric access level from `Access`.
  pure def accessLevel(a: Access): int =
    match a {
      | IsMember => 0
      | AddMembers => 1
      | RemoveMembers => 2
      | ConfigureSpace => 3
      | CreateSpaces => 4
      | RemoveSpace => 5
      | Owner => 6
    }

  /// Get the smaller of the provided accesses
  pure def minAccess(a: Access, b: Access): Access =
    if (a.accessLevel() < b.accessLevel()) a else b
  
  /// Get the larger of the provided accesses
  pure def maxAccess(a: Access, b: Access): Access =
    if (a.accessLevel() > b.accessLevel()) a else b

  /// Returns whether access `b` is included in access `a`
  pure def includesAccess(a: Access, b: Access): bool =
    accessLevel(a) >= accessLevel(b)

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

  /// Synchronously returns a resolved member list if it is possible to do so without making any 
  /// resolutions for remote spaces.
  pure def resolveSync(unresolved: UnresolvedMemberList): Option[ResolvedMemberList] =
    if (unresolved.keys().forall(member =>
      match member {
        | UMLUser(u) => true
        | UMLSpace(s) => false
      }
    ))
      Some({
        memberList: unresolved.keys().map(member => match member {
          | UMLUser(u) => Set((u, unresolved.get(member)))
          | UMLSpace(s) => Set()
        }).flatten().setToMap(),
        missingSpaces: Map(),
      })
    else
      None

  /// Get the list of spaces that need to be resolved
  pure def spacesToResolve(unresolved: UnresolvedMemberList): Set[SpaceId] =
    unresolved.keys().fold(Set(), (ids, id) =>
      match (id) {
        | UMLUser(u) => ids
        | UMLSpace(spaceId) => ids.union(Set(spaceId))
      }
    )

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

  /// This represents a call to perform some action on the arbiter.
  type JobArgs = 
    | JobCreateSpace
    | JobConfigureSpace(SpaceConfig)
    | JobDeleteSpace
    | JobSetMemberAccess({
        member: Member,
        access: Access,
      })
    | JobRemoveMember({
      member: Member
    })

  type JobId = int
  
  /// A job is a work-in-progress action such as adding or removing a member.
  type Job = {
    // Unique ID for the job, used to respond to set the job finish status
    id: JobId,
    // The user performing the actions
    userDid: UserDid,
    // The space that is being operated on
    spaceKey: SpaceKey,
    // The arguments to the job to perform
    args: JobArgs,
    // The unresolved member list calculated at the time the job was received
    unresolvedMembers: UnresolvedMemberList,
    // The list of external space member lists that have been resolved.
    //
    // Once all of the external spaces in the unresolvedList have been resolved
    // we can produce the final member list and then actually perform the operation.
    resolvedSpaces: SpaceId -> ResolvedMemberList, 
    // The version of the arbiter when the job was started
    arbiterVersion: int,
  }

  /// Data for an arbiter
  type Arbiter = {
    // Monotonic version of the arbiter config that is updated for every change to the arbiter and
    // that is used for compare-and-swap updates to the arbiter config.
    version: int,
    // The arbiter's DID
    did: ArbiterDid,
    // The spaces managed by the arbiter
    spaces: SpaceKey -> Space,
    // Space resolution jobs that are work-in-progress
    jobQueue: JobId -> Job,
    // The results from finished jobs. Will be true if the job succeeded
    jobResults: JobId -> bool
  }

  /// Increment the version number on the arbiter
  pure def nextVersion(a: Arbiter): Arbiter = a.with("version", a.version + 1)

  pure def isSome(opt: Option): bool =
    match (opt) { | Some => true | None => false }

  pure def unwrapOr(opt: Option[a], default: a): a =
    match (opt) { | Some(u) => u | None => default }
}

module arbiter_core {
  import types.*
  import model.*

  pure val userDids: Set[UserDid] = Set("Alice", "Bob", "Charlie", "Daffodil", "Eve")
  pure val arbiterDids: Set[ArbiterDid] = Set("arb1", "arb2")
  pure val spaceKeys: Set[SpaceKey] = Set("space1", "space2")
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


