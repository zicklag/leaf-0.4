/// The state machine of the arbiter core. This handles the role computation and resolution logic
/// and is meant to be wrapped by the arbiter server that will drive it.
module arbiter_core {
  import utils.* from "../lib/utils"

  //
  // TYPES
  //

  type ArbiterDid = str
  type UserDid = str
  type SpaceKey = str

  /// The ID of the special "$admin" space that controls access to the arbiter itself.
  pure val SpaceKey::ADMIN: SpaceKey = "$admin"

  /// The core state of an arbiter
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
    // The result of the last operation on the arbiter
    result: ArbiterResult,
  }

  /// A result for the arbiter state machine
  type ArbiterResult =
    | ArbiterOk
    // The job was queued and the contained list of spaces needs their members resolved.
    | ArbiterQueuedJob({
        id: JobId,
        spacesToResolve: Set[SpaceId]
      })
    | ArbiterFinishedJob({
      id: JobId,
      result: JobResult
    })
    | ArbiterDeleted
    | ArbiterErr(ArbiterError)

  /// An error result for the arbiter state machine
  type ArbiterError =
    | ErrJobIdsExhausted
    | ErrJobNotExists
    | ErrSpaceNotNeeded
    | ErrSpaceAlreadyResolved
    | ErrSpaceAlreadyExists
    | ErrSpaceNotExists
    | ErrPermissionDenied
    | ErrCannotDeleteAdminSpace
    | ErrMemberNotExist
    | ErrPermissionChanged
    | ErrArbiterDeletionMustSpecifyAdminSpace
    | ErrWriteOperationAlreadyInProgress
    | ErrRemoteSpaceReferencesLocalArbiter
    | ErrOnlyLastOwnerCanDeleteArbiter
    | ErrJobsTimedOut(Set[JobId])

  /// This is a reference to a space, qualified by the arbiter that owns it.
  type SpaceId = { arbiterDid: ArbiterDid, spaceKey: SpaceKey }

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
    // This member can read the member list, but isn't granted any other access and won't be
    // included in the permissioned space's member list.
    | ReadMemberList
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
    // Member can change the space's configuration **and** can write records to the space's
    // permissioned repo under the arbiter's account.
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
    ReadMemberList,
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
      | ReadMemberList => 0
      | IsMember => 1
      | AddMembers => 2
      | RemoveMembers => 3
      | ConfigureSpace => 4
      | CreateSpaces => 5
      | RemoveSpace => 6
      | Owner => 7
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

  /// Get the final member list for the permissioned space, based on a resolved member list.
  pure def permissionedSpaceMembers(resolved: ResolvedMemberList): Set[UserDid] =
    resolved.memberList.keys()
      // Filter out users that don't have at least IsMember
      .filter(userDid => 
        def access = resolved.memberList.get(userDid)
        access.includesAccess(IsMember)
      )

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

  type JobId = int

  /// This represents a call to perform some action on the arbiter.
  type JobArgs = 
    | JobFetchMembers
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
    | JobDeleteArbiter

  /// The result of a finished job
  type JobResult =
    | JobOk
    | JobOkResolvedMembersList(ResolvedMemberList)
  
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

  pure val VERSION_BITS = 32
  pure val VERSION_MAX = 2^VERSION_BITS - 1

  /// Increment the arbiter version number with a wrapping add.
  pure def nextVersion(a: Arbiter): Arbiter = a.with("version", (a.version + 1) % (2 ^ VERSION_BITS))

  /// Calculate the number of updates to the arbiter version before we end up colliding with the
  /// given job's version. We need to make sure we timeout the job before this happens to make sure
  /// that the job doesn't end up having the same version as the arbiter even though the arbiter
  /// has changed.
  pure def versionDiff(a: Arbiter, job: Job): int = 
    // If the arbiter's version is less than the job's version, then it means that the arbiter has
    // hit the max version and started to wrap around
    if (a.version < job.arbiterVersion) job.arbiterVersion - a.version else

    // If the arbiter version is greater than the job version, then that means it hasn't wrapped
    // yet, and it can go up to the wrapping point, and then all the way back around up to the
    // arbiter version.
    if (a.version > job.arbiterVersion) VERSION_MAX - a.version + job.arbiterVersion else

    // If the versions are equal, we assume that they are the same, and we have the entire version
    // size before we start to wrap around.
    VERSION_MAX

  /// Remove a job from the job queue
  pure def removeJob(arbiter: Arbiter, jobId: JobId): Arbiter =
    arbiter.with("jobQueue", arbiter.jobQueue.mapRemove(jobId))

  //
  // ACTIONS
  //

  // Create a new arbiter with the given arbiter DID and initial owner.
  pure def Arbiter::new(arbiterDid: ArbiterDid, userDid: UserDid): Arbiter = 
    {
      version: 0,
      did: arbiterDid,
      jobQueue: Map(),
      spaces: Map(
        // Create the special $admin space
        SpaceKey::ADMIN -> {
          config: SpaceConfig::default,
          members: Map(
            // Add the creator of the arbiter as an owner
            MemberUser(userDid) -> Owner
          ),
        }
      ),
      result: ArbiterOk
    }

  /// Process a new job.
  ///
  /// NOTE: the `userDid` is required to be authenticated by the wrapper around the arbiter state
  /// machine as having actually come from the did.
  pure def Arbiter::startJob(arbiter: Arbiter, userDid: UserDid, spaceKey: SpaceKey, args: JobArgs): Arbiter =
    // Get a new job ID if available
    // In a real implementation this can be any globally unique ID.
    def newId = 1.to(100).exclude(arbiter.jobQueue.keys()).fold(None, (j, a) => Some(j.orElse(a)))

    // Return an error if we are out of IDs
    match (newId) { | None => arbiter.throw(ErrJobIdsExhausted) | Some(jobId) =>

    // Make sure we don't have any jobs we need to force a timeout on to avoid a compare-and-swap
    // version collision
    def collidingJobs = arbiter.jobQueue.keys().filter(jobId =>
      def job = arbiter.jobQueue.get(jobId) 
      versionDiff(arbiter, job) == 1
    )
    if (collidingJobs.size() > 0)
        arbiter.with("jobQueue",
          // Remove all of the jobs that have colliding versions
          arbiter.jobQueue.keys().exclude(collidingJobs)
            .map(jobId => (jobId, arbiter.jobQueue.get(jobId))).setToMap()
        )
        // Return the list of colliding versions in an error. The caller will have to re-try the job
        // if they want to progress the original job.
        .with("result", ArbiterErr(ErrJobsTimedOut(collidingJobs)))
    else

    // Do some validation on the job
    match (jobValidationError(arbiter, spaceKey, args)) { | Some(err) => arbiter.throw(err) | None =>

    // Get the local member list
    def unresolvedMembers = arbiter.members(spaceKey)

    // Try to resolve it, which will succeed if there's no access delegated to remote spaces
    def attemptedResolve = unresolvedMembers.resolveSync()

    match (attemptedResolve) {
      // If we successfully resolved the member list, execute the job immediately
      | Some(resolved) => executeJob(arbiter, resolved, {
          id: jobId,
          userDid: userDid,
          spaceKey: spaceKey,
          args: args,
          unresolvedMembers: unresolvedMembers,
          resolvedSpaces: Map(),
          arbiterVersion: arbiter.version,
        })

      // If we cannot resolve synchronously, then we need to queue the job
      | None => 
        // Check whether there is an existing write job queued for this user
        def userHasExistingWriteJob = arbiter.jobQueue.keys().exists(jobId =>
          def job = arbiter.jobQueue.get(jobId)
          job.userDid == userDid and
          match (job.args) {
            // We don't care about reads
            | JobFetchMembers => false
            // All get all the other write jobs
            | _ => true
          } 
        )

        // We don't allow a user to submit concurrent write jobs because they will just end up
        // stepping on each-other at the compare-and-swap stage and stopping it early saves memory
        // and helps prevent DoS attacks.
        //
        // NOTE: it's still possible to try to DoS the service with read requests which should be
        // allowed concurrently and preventing that is the job of the wrapper around the arbiter
        // core, since it is equipped to track things like timing, source IP addresses, etc.
        if (userHasExistingWriteJob) arbiter.throw(ErrWriteOperationAlreadyInProgress) else

        // Queue the job
        arbiter
          // FIXME: this is a bug. We should not increment the arbiter version for new jobs. This
          // will prevent all mutations from applying properly, if they require a remote space
          // resolution.
          //
          // I want to leave this here until we have a test that fails with the bug in it.
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
          .with("result", ArbiterQueuedJob({
            id: jobId,
            spacesToResolve: unresolvedMembers.spacesToResolve()
          }))
    }}
  }

  /// Provide the resolved space member list for a job that was waiting on a member list to be
  /// resolved.
  ///
  /// NOTE: This action is assumed to be called by a trusted wrapper around the arbiter core. The
  /// resolved member list / must be validated using mechanisms like HTTPs to ensure it actually came
  /// from the remote arbiter with the given DID.
  ///
  /// NOTE: providing a completely enormous resolved member list is potentially a DoS vector. This
  /// should be handled by the wrapper.
  pure def Arbiter::provideRemoteSpaceMembers(arbiter: Arbiter, jobId: JobId, spaceId: SpaceId, members: ResolvedMemberList): Arbiter =
    def job = arbiter.jobQueue.get(jobId)

    // Check job exists
    if (not(arbiter.jobQueue.has(jobId))) arbiter.throw(ErrJobNotExists) else
    // Check that this job actually needs the space to be resolved
    if (not(job.unresolvedMembers.has(UMLSpace(spaceId)))) arbiter.throw(ErrSpaceNotNeeded) else
    // Check that we haven't already resolved the space
    if (job.resolvedSpaces.has(spaceId)) arbiter.throw(ErrSpaceAlreadyResolved) else

    // Create an updated list of the resolved spaces
    def resolvedSpaces = job.resolvedSpaces.put(spaceId, members)

    // Check whether the job has all of it's spaces resolved and is ready to be executed
    def jobIsReady = job
      .unresolvedMembers
      .spacesToResolve()
      .exclude(resolvedSpaces.keys()).size() == 0

    // If the job is ready to be executed
    if (jobIsReady)
      // Execute it immediately
      def resolved = joinResolvedSpaces(job.unresolvedMembers, resolvedSpaces)
      executeJob(arbiter, resolved, job)

    // If the job is not ready
    else
      // Just update the queued job's resolved spaces
      arbiter
        .nextVersion() // FIXME: this is a bug
        .with("jobQueue",
          arbiter.jobQueue.put(jobId,
            job.with("resolvedSpaces", resolvedSpaces) 
          )
        ).with("result", ArbiterOk)

  /// Timeout a job that is in the queue and waiting for space member lists to be resolved.
  ///
  /// This will execute the job without waiting for the remote member lists to finish resolving.
  pure def Arbiter::timeoutJob(arbiter: Arbiter, jobId: JobId): Arbiter =
    def job = arbiter.jobQueue.get(jobId)

    // Check job exists
    if (not(arbiter.jobQueue.has(jobId))) arbiter.throw(ErrJobNotExists) else

    // Resolve the member list with whatever spaces it has right now
    def resolved = joinResolvedSpaces(job.unresolvedMembers, job.resolvedSpaces)

    // Execute the job
    executeJob(arbiter, resolved, job)


  //
  // HELPER DEFINITIONS ( NOT CALLED FROM OUTSIDE )
  //

  /// Throw an error
  pure def throw(arbiter: Arbiter, e: ArbiterError): Arbiter =
    arbiter.with("result", ArbiterErr(e))

  /// Throw an error and remove a job from the job queue
  pure def throwJobErr(arbiter: Arbiter, e: ArbiterError, jobId: JobId): Arbiter =
    arbiter.removeJob(jobId).with("result", ArbiterErr(e))
  
  /// Return any validation errors for the job if there are issues
  pure def jobValidationError(arbiter: Arbiter, spaceKey: SpaceKey, args: JobArgs): Option[ArbiterError] =
    match (args) {
      | JobFetchMembers =>
        // Make sure the space exists
        if (not(arbiter.spaces.has(spaceKey))) Some(ErrSpaceNotExists)
        else None
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
        if (not(arbiter.spaces.has(spaceKey))) Some(ErrSpaceNotExists) else
        // Make sure that we don't add a remote space with the DID of the current space ( it should
        // be a local space, then )
        if (match (x.member) { | MemberRemoteSpace(id) => id.arbiterDid == arbiter.did | _ => false })
          Some(ErrRemoteSpaceReferencesLocalArbiter)
        else None
      | JobRemoveMember(x) => 
        def space = arbiter.spaces.get(spaceKey)
        // Make sure the space exists
        if (not(arbiter.spaces.has(spaceKey))) Some(ErrSpaceNotExists)
        // Make sure the member is in the space
        else if (not(space.members.has(x.member))) Some(ErrMemberNotExist)
        else None
      | JobDeleteArbiter(x) =>
        def space = arbiter.spaces.get(spaceKey)
        // Require the space key to be $admin just to be explicit.
        // NOTE: this is a little weird but allows us to keep the jobs homogenously including the space key.
        if (not(spaceKey == SpaceKey::ADMIN)) Some(ErrArbiterDeletionMustSpecifyAdminSpace) else
        // Make sure there is only a single delegation in the $admin space.
        // We want you to remove all other admins before you can delete the arbiter.
        if (space.members.keys().size() > 1) Some(ErrOnlyLastOwnerCanDeleteArbiter)
        else None
    }

  /// Finishes the execution of a job, applying the changes, or erroring if they are invalid.
  pure def executeJob(
    arbiter: Arbiter, 
    resolved: ResolvedMemberList,
    job: Job,
  ): Arbiter =
    // Check arbiter exists
    match (job.args) {
      | JobFetchMembers => fetchMembers(arbiter, resolved, job)
      | JobCreateSpace => createSpace(arbiter, resolved, job)
      | JobConfigureSpace(config) => configureSpace(arbiter, resolved, job, config)
      | JobDeleteSpace => deleteSpace(arbiter, resolved, job)
      | JobSetMemberAccess(m) => setMemberAccess(arbiter, resolved, job, m.member, m.access)
      | JobRemoveMember(args) => removeMember(arbiter, resolved, job, args.member)
      | JobDeleteArbiter(args) => deleteArbiter(arbiter, resolved, job)
    }

  /// Execute Job: Fetch a space's member list
  pure def fetchMembers(
    arbiter: Arbiter,
    resolved: ResolvedMemberList,
    job: Job,
  ): Arbiter = 
    def space = arbiter.spaces.get(job.spaceKey)
    def memberAccess = resolved.memberList.get(job.userDid)

    // Check space exists
    if (not(arbiter.spaces.has(job.spaceKey))) arbiter.throwJobErr(ErrSpaceNotExists, job.id) else

    if (
      // If the members list for this space is not public
      not(space.config.publicMembers) and
      // Make sure that the user is a member of the list and has ReadMemberList access.
      // NOTE: the access check is somewhat redundant at the time of writing because it is the lowest
      // access you can have.
      not(resolved.memberList.has(job.userDid) and memberAccess.includesAccess(ReadMemberList))
    )
      arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    // Return the member list with the job result
    arbiter
      .removeJob(job.id)
      .with("result", ArbiterFinishedJob({
        id: job.id,
        result: JobOkResolvedMembersList(resolved)
      }))

  /// Execute Job: Create a space
  pure def createSpace(
    arbiter: Arbiter, 
    resolved: ResolvedMemberList,
    job: Job,
  ): Arbiter = 
    def memberAccess = resolved.memberList.get(job.userDid)

    // Check that the user is in the member list
    if (not(resolved.memberList.has(job.userDid))) arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    // Check that arbiter has not been modified since the job was started ( compare-and-swap style )
    if (arbiter.version != job.arbiterVersion) arbiter.throwJobErr(ErrPermissionChanged, job.id) else

    // Check space doesn't exist
    if (arbiter.spaces.has(job.spaceKey)) arbiter.throwJobErr(ErrSpaceAlreadyExists, job.id) else

    // Check that the user has access to create spaces
    // NOTE: The space did not exist at the time the job was started, so the member list will only
    // include those in the admin role, which is appropriate for us to check who can create a space.
    if (not(memberAccess.includesAccess(CreateSpaces))) arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    // Create the new space
    arbiter
      .nextVersion()
      .removeJob(job.id)
      .with("spaces", arbiter.spaces.put(job.spaceKey, {
        config: SpaceConfig::default,
        members: Map(),
      }))
      .with("result", ArbiterFinishedJob({
        id: job.id,
        result: JobOk
      }))

  /// Execute Job: Update space configuration
  pure def configureSpace(
    arbiter: Arbiter, 
    resolved: ResolvedMemberList,
    job: Job,
    config: SpaceConfig
  ): Arbiter =
    def userAccess = resolved.memberList.get(job.userDid)
    def space = arbiter.spaces.get(job.spaceKey)

    // Check that the user is in the member list
    if (not(resolved.memberList.has(job.userDid))) arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    // Check that arbiter has not been modified since the job was started ( compare-and-swap style )
    if (arbiter.version != job.arbiterVersion) arbiter.throwJobErr(ErrPermissionChanged, job.id) else

    // Check space exists
    if (not(arbiter.spaces.has(job.spaceKey))) arbiter.throwJobErr(ErrSpaceNotExists, job.id) else

    // Check that the user has access to configure the space
    if (not(userAccess.includesAccess(ConfigureSpace))) arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    // Configure the space
    arbiter
      .nextVersion()
      .removeJob(job.id)
      .with("spaces", arbiter.spaces.put(job.spaceKey, space.with("config", config)))
      .with("result", ArbiterFinishedJob({
        id: job.id,
        result: JobOk
      }))

  /// Execute Job: Delete a space
  pure def deleteSpace(
    arbiter: Arbiter, 
    resolved: ResolvedMemberList,
    job: Job,
  ): Arbiter =
    def userAccess = resolved.memberList.get(job.userDid)
    def space = arbiter.spaces.get(job.spaceKey)

    // Check that the user is in the member list
    if (not(resolved.memberList.has(job.userDid))) arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    // Check that arbiter has not been modified since the job was started ( compare-and-swap style )
    if (arbiter.version != job.arbiterVersion) arbiter.throwJobErr(ErrPermissionChanged, job.id) else

    // Check space exists
    if (not(arbiter.spaces.has(job.spaceKey))) arbiter.throwJobErr(ErrSpaceNotExists, job.id) else

    // Make sure we don't delete the $admin space
    // ( we already checked this earlier in jobValidationError but just to be sure... )
    if (job.spaceKey == SpaceKey::ADMIN) arbiter.throwJobErr(ErrCannotDeleteAdminSpace, job.id) else

    // Check that the user has access to delete the space
    if (not(userAccess.includesAccess(RemoveSpace))) arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    // Remove the space from the arbiter
    arbiter
      .nextVersion()
      .removeJob(job.id)
      .with("spaces", arbiter.spaces.mapRemove(job.spaceKey))
      .with("result", ArbiterFinishedJob({
        id: job.id,
        result: JobOk
      }))

  /// Execute Job: Set a member's access in a space
  pure def setMemberAccess(
    arbiter: Arbiter, 
    resolved: ResolvedMemberList,
    job: Job,
    updatedMember: Member,
    updatedMemberAccess: Access
  ): Arbiter =
    def userAccess = resolved.memberList.get(job.userDid)
    def space = arbiter.spaces.get(job.spaceKey)

    // Check that the user is in the member list
    if (not(resolved.memberList.has(job.userDid))) arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    // Check that arbiter has not been modified since the job was started ( compare-and-swap style )
    if (arbiter.version != job.arbiterVersion) arbiter.throwJobErr(ErrPermissionChanged, job.id) else

    // Check space exists
    if (not(arbiter.spaces.has(job.spaceKey))) arbiter.throwJobErr(ErrSpaceNotExists, job.id) else

    // Check that the user has access to add members
    if (not(userAccess.includesAccess(AddMembers))) arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    // Check that the access the user is trying to grant the member is less than or equal to their own
    if (not(userAccess.includesAccess(updatedMemberAccess))) arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    def updatedMemberExists = space.members.has(updatedMember)
    def updatedMemberExistingAccess = space.members.get(updatedMember)

    // Check that the user isn't modifying the access of a member that already exists, if they don't have RemoveMember permission
    if (updatedMemberExists and not(userAccess.includesAccess(RemoveMembers))) arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    // Check that the user isn't modifying the access of a member that already exists, if the updated member has more access than the user
    if (updatedMemberExists and updatedMemberExistingAccess.accessLevel() > userAccess.accessLevel()) arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    // Update the member's access
    arbiter
      .nextVersion()
      .removeJob(job.id)
      .with("spaces", arbiter.spaces.put(job.spaceKey, space
        .with("members", space.members.put(updatedMember, updatedMemberAccess))
      ))
      .with("result", ArbiterFinishedJob({
        id: job.id,
        result: JobOk
      }))

  /// Execute Job: Remove a member from a space
  pure def removeMember(
    arbiter: Arbiter, 
    resolved: ResolvedMemberList,
    job: Job,
    removedMember: Member,
  ): Arbiter =
    def userAccess = resolved.memberList.get(job.userDid)
    def space = arbiter.spaces.get(job.spaceKey)
    def removedMemberAccess = space.members.get(removedMember)

    // Check that the user is in the member list
    if (not(resolved.memberList.has(job.userDid))) arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    // Check that arbiter has not been modified since the job was started ( compare-and-swap style )
    if (arbiter.version != job.arbiterVersion) arbiter.throwJobErr(ErrPermissionChanged, job.id) else

    // Check space exists
    if (not(arbiter.spaces.has(job.spaceKey))) arbiter.throwJobErr(ErrSpaceNotExists, job.id) else

    // Check that the user has access to remove members
    if (not(userAccess.includesAccess(RemoveMembers))) arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    // Check that the user isn't removing a member that has more access than they do
    if (removedMemberAccess.accessLevel() > userAccess.accessLevel()) arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    // Remove the member
    arbiter
      .nextVersion()
      .removeJob(job.id)
      .with("spaces", arbiter.spaces.put(job.spaceKey, space
        .with("members", space.members.mapRemove(removedMember))
      ))
      .with("result", ArbiterFinishedJob({
        id: job.id,
        result: JobOk
      }))

  /// Execute Job: Delete an arbiter
  pure def deleteArbiter(
    arbiter: Arbiter, 
    resolved: ResolvedMemberList,
    job: Job,
  ): Arbiter =
    def userAccess = resolved.memberList.get(job.userDid)
    def space = arbiter.spaces.get(job.spaceKey)

    // Check that the user is in the member list
    if (not(resolved.memberList.has(job.userDid))) arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    // Check that arbiter has not been modified since the job was started ( compare-and-swap style )
    if (arbiter.version != job.arbiterVersion) arbiter.throwJobErr(ErrPermissionChanged, job.id) else

    // Check that the space is the admin one
    if (not(job.spaceKey == SpaceKey::ADMIN)) arbiter.throwJobErr(ErrArbiterDeletionMustSpecifyAdminSpace, job.id) else

    // Check that the user has `Owner` access
    if (not(userAccess.includesAccess(Owner))) arbiter.throwJobErr(ErrPermissionDenied, job.id) else

    // Check that space only has one member.
    // We don't want to allow you to delete an arbiter unless you're the last admin delegation.
    if (space.members.keys().size() > 1) arbiter.throwJobErr(ErrOnlyLastOwnerCanDeleteArbiter, job.id) else

    // NOTE: It is the job of the wrapper to actually delete the arbiter. We can't do that because
    // we _are_ the arbiter.
    arbiter.with("result", ArbiterDeleted)

  //
  // READ OPERATIONS
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

  /// Core arbiter invariants
  pure def Arbiter::invariants(arbiter: Arbiter): bool = and {
    InvArbiterSpaceAlwaysExists(arbiter),
    InvArbiterHasAtLeastOneOwner(arbiter),
    InvRemoteSpaceMemberDoesNotReferenceLocalArbiter(arbiter),
  }

  /// The special `$admin` space always exists
  pure def InvArbiterSpaceAlwaysExists(arbiter: Arbiter): bool =
    arbiter.spaces.has(SpaceKey::ADMIN)

  /// There is at least one owner
  pure def InvArbiterHasAtLeastOneOwner(arbiter: Arbiter): bool =
    def space = arbiter.spaces.get(SpaceKey::ADMIN)
    space.members.keys().exists(member =>
      def access = space.members.get(member)
      access == Owner
    )
  
  /// There isn't any remote space members that reference the local arbiter. A local local space
  /// member should be used instead in that case.
  pure def InvRemoteSpaceMemberDoesNotReferenceLocalArbiter(arbiter: Arbiter): bool =
    arbiter.spaces.keys().forall(spaceKey =>
      def space = arbiter.spaces.get(spaceKey)
      not(space.members.keys().exists(member => match (member) {
        | MemberRemoteSpace(id) => id.arbiterDid == arbiter.did
        | _ => false
      }))
    )
}

/// The server wraps the core, presents the XRPC API, and manages multiple arbiters.
module arbiter_server {
  import utils.* from "../lib/utils"

  import arbiter_core as core
  import arbiter_core.Arbiter
  import arbiter_core.Arbiter::new
  import arbiter_core.Arbiter::startJob
  import arbiter_core.Arbiter::timeoutJob
  import arbiter_core.Arbiter::provideRemoteSpaceMembers
  import arbiter_core.ArbiterDid
  import arbiter_core.UserDid
  import arbiter_core.Member
  import arbiter_core.Access
  import arbiter_core.SpaceKey
  import arbiter_core.JobId
  import arbiter_core.JobArgs
  import arbiter_core.JobOk
  import arbiter_core.JobResult
  import arbiter_core.SpaceId
  import arbiter_core.SpaceKey::ADMIN
  import arbiter_core.ResolvedMemberList
  import arbiter_core.ArbiterError
  import arbiter_core.JobFetchMembers
  import arbiter_core.JobCreateSpace  
  import arbiter_core.JobDeleteSpace
  import arbiter_core.JobConfigureSpace
  import arbiter_core.JobSetMemberAccess
  import arbiter_core.JobRemoveMember
  import arbiter_core.JobDeleteArbiter

  /// Number of ticks before timing out a job
  pure val TIMEOUT_TICKS = 8

  /// A request ID
  type ReqId = int
  type Time = int

  /// An server state
  type Server = {
    time: Time,
    jobStartTimes: ReqId -> Time,
    arbiters: ArbiterDid -> Arbiter,
    result: ServerResult
  }

  /// Feedback given by the server to outside
  type Feedback =
    | Respond(Response)
    | ResolveRemoteList({
        reqId: ReqId,
        space: SpaceId,
        resolverDepth: int,
      })

  /// An XRPC request
  type Request = { id: ReqId, endpoint: XrpcEndpoint }

  /// An XRPC endpoint with it's arguments
  type XrpcEndpoint =
    | XFetchMembers({
        userDid: UserDid,
        arbiterDid: ArbiterDid,
        spaceKey: SpaceKey,
        resolverDepth: int,
      })
    | XCreateSpace({
        userDid: UserDid,
        arbiterDid: ArbiterDid,
        spaceKey: SpaceKey,
        resolverDepth: int,
      })
    | XConfigureSpace({
        userDid: UserDid,
        arbiterDid: ArbiterDid,
        spaceKey: SpaceKey,
        publicRecords: bool,
        publicMembers: bool,
        resolverDepth: int,
      })
    | XDeleteSpace({
        userDid: UserDid,
        arbiterDid: ArbiterDid,
        spaceKey: SpaceKey,
        resolverDepth: int,
      })
    | XSetMemberAccess({
        userDid: UserDid,
        arbiterDid: ArbiterDid,
        spaceKey: SpaceKey,
        member: Member,
        access: Access,
        resolverDepth: int,
      })
    | XRemoveMember({
        userDid: UserDid,
        arbiterDid: ArbiterDid,
        spaceKey: SpaceKey,
        member: Member,
        resolverDepth: int,
      })
    | XCreateArbiter({
        userDid: UserDid,
        arbiterDid: ArbiterDid,
        resolverDepth: int,
      })
    | XDeleteArbiter({
        userDid: UserDid,
        arbiterDid: ArbiterDid,
        resolverDepth: int,
      })

  /// A response
  type Response = { id: ReqId, data: JobResult }

  /// The result of an action on the server
  type ServerResult =
    // The operation was successful and there is a set of feedback to be processed by the wrapper.
    | ServerOk(Set[Feedback])
    // The operation errored
    | ServerErr(ServerError)
    // This is reserved for things that should never happen. If we run into one of these it is a
    // bug in the specification.
    | Panic

  /// The server errors we can run into
  type ServerError =
    | ErrArbiterAlreadyExists
    | ErrArbiterNotExists
    | ErrArbiterErr(ArbiterError)
    | ErrDuplicateReqId

  //
  // TRIVIAL SERVER HELPERS
  //

  pure def throw(server: Server, e: ServerError): Server = server.with("result", ServerErr(e))
  pure def todo(server: Server): Server = server
  pure def panic(server: Server): Server = server.with("result", Panic)
  pure def ok(server: Server, feedback: Set[Feedback]): Server = server.with("result", ServerOk(feedback))
  pure def tick(server: Server): Server = server.with("time", server.time + 1)
  pure def putArbiter(server: Server, arbiterDid: ArbiterDid, arbiter: Arbiter): Server = server.with("arbiters", server.arbiters.put(arbiterDid, arbiter))
  pure def removeArbiter(server: Server, arbiterDid: ArbiterDid): Server = server.with("arbiters", server.arbiters.mapRemove(arbiterDid))
  pure def addJobStartTime(server: Server, reqId: ReqId): Server = server.with("jobStartTimes", server.jobStartTimes.put(reqId, server.time))
  pure def removeJobStartTime(server: Server, reqId: ReqId): Server = server.with("jobStartTimes", server.jobStartTimes.mapRemove(reqId))

  //
  // ACTIONS
  //

  /// Create a new server instance
  pure def Server::default: Server = {
    time: 0,
    jobStartTimes: Map(),
    arbiters: Map(),
    result: ServerOk(Set())
  }

  /// Process an XRPC request
  pure def Server::request(server: Server, req: Request): Server =
    // Reject requests with an in-use request ID
    if (server.arbiters.keys().exists(arbiterDid =>
      def arbiter = server.arbiters.get(arbiterDid)
      arbiter.jobQueue.has(req.id)
    )) server.throw(ErrDuplicateReqId) else

    // Handle the request
    match (req.endpoint) {
      | XCreateArbiter(args) => server.createArbiter(req.id, args.userDid, args.arbiterDid)
      | XDeleteArbiter(args) => startJob(server, req.id, args.resolverDepth, args.userDid, args.arbiterDid, SpaceKey::ADMIN, JobDeleteArbiter)
      | XFetchMembers(args) => server.startJob(req.id, args.resolverDepth, args.userDid, args.arbiterDid, args.spaceKey, JobFetchMembers)
      | XCreateSpace(args) => server.startJob(req.id, args.resolverDepth, args.userDid, args.arbiterDid, args.spaceKey, JobCreateSpace)
      | XConfigureSpace(args) => server.startJob(req.id, args.resolverDepth, args.userDid, args.arbiterDid, args.spaceKey, JobConfigureSpace({
          publicRecords: args.publicRecords,
          publicMembers: args.publicMembers,
        }))
      | XDeleteSpace(args) => server.startJob(req.id, args.resolverDepth, args.userDid, args.arbiterDid, args.spaceKey, JobDeleteSpace)
      | XSetMemberAccess(args) => server.startJob(req.id, args.resolverDepth, args.userDid, args.arbiterDid, args.spaceKey, JobSetMemberAccess({
          member: args.member,
          access: args.access,
        }))
      | XRemoveMember(args) => server.startJob(req.id, args.resolverDepth, args.userDid, args.arbiterDid, args.spaceKey, JobRemoveMember({
          member: args.member,
        }))
    }

  /// Tick the server to let it process timeouts and resolution steps
  pure def Server::tick(server: Server): Server =
    // Find a timed out job, if it exists
    def timedOutJob = server.arbiters.keys().fold(None, (s, arbiterDid) => s.someOrOther(
      def arbiter = server.arbiters.get(arbiterDid)
      
      // Look at every job
      arbiter.jobQueue.keys().fold(s, (s, jobId) => s.someOrOther(
        def jobStart = server.jobStartTimes.get(jobId)
        // If the job is timed out
        if (server.time - jobStart >= TIMEOUT_TICKS)
          // Return it
          Some({ arbiterDid: arbiterDid, jobId: jobId })
        else None
      ))
    ))

    match (timedOutJob) {
      // If there is a timed out job
      | Some(timedOut) =>
        def arbiterDid = timedOut.arbiterDid
        def arbiter = server.arbiters.get(arbiterDid)

        // Timeout the job on the arbiter
        def updArbiter = Arbiter::timeoutJob(arbiter, timedOut.jobId)

        server
          // Advance time
          .tick()
          // Update the arbiter
          .handleArbiterResult(updArbiter, 0)

      // Just advance time if there are no timed out jobs
      | None => server.tick()
    }

  //
  // HELPERS
  //

  pure def createArbiter(server: Server, reqId: ReqId, userDid: UserDid, arbiterDid: ArbiterDid): Server =
    // Make sure the arbiter doesn't already exist
    if (server.arbiters.has(arbiterDid)) server.throw(ErrArbiterAlreadyExists) else
    // Create a new arbiter
    def newArbiter = Arbiter::new(arbiterDid, userDid)
    server
      // Add the arbiter to the server
      .putArbiter(arbiterDid, newArbiter)
      // Result was OK
      .ok(Set())

  pure def startJob(server: Server, reqId: ReqId, resolverDepth: int, userDid: UserDid, arbiterDid: ArbiterDid, spaceKey: SpaceKey, jobArgs: JobArgs): Server =
    def arbiter = server.arbiters.get(arbiterDid)
    // Make sure the arbiter exists
    if (not(server.arbiters.has(arbiterDid))) server.throw(ErrArbiterNotExists) else
    // Start the job
    def updArbiter = Arbiter::startJob(arbiter, userDid, spaceKey, jobArgs)
    // Handle the result
    server.handleArbiterResult(updArbiter, resolverDepth)

  /// Provide the result of a remote server member list resolution
  pure def Server::resolvedMemberList(server: Server, arbiterDid: ArbiterDid, reqId: ReqId, spaceId: SpaceId, members: ResolvedMemberList): Server =
    def arbiter = server.arbiters.get(arbiterDid)
    // Make sure the arbiter exists
    if (not(server.arbiters.has(arbiterDid))) server.throw(ErrArbiterNotExists) else
    // Update the arbiter
    def updArbiter = Arbiter::provideRemoteSpaceMembers(arbiter, reqId, spaceId, members)
    // Handle the result
    server.handleArbiterResult(updArbiter, 0)

  /// Given an arbiter that has just been updated, handle the result
  pure def handleArbiterResult(server: Server, updArbiter: Arbiter, resolverDepth: int): Server =
    def arbiterDid = updArbiter.did

    match (updArbiter.result) {
      // If the arbiter is OK
      | ArbiterOk =>
        server
          // Update the arbiter
          .putArbiter(arbiterDid, updArbiter)
          .ok(Set())

      // If we need to queue remote space resolution
      | ArbiterQueuedJob(job) =>
        // If the resolver depth is less than one, we can't actually solve the queued spaces.
        if (resolverDepth < 1)
          // We need to immediately timeout the job
          def timedOutArbiter = Arbiter::timeoutJob(updArbiter, job.id)

          // And handle the result
          match (timedOutArbiter.result) {
            // A delete request finishes
            | ArbiterDeleted =>
              server
                // Remove the arbiter
                .removeArbiter(arbiterDid)
                .ok(Set())

            // The job finishes successfully
            | ArbiterFinishedJob(job) =>
              server
                // Update the arbiter
                .putArbiter(arbiterDid, timedOutArbiter)
                // Remove the recorded job start time
                .removeJobStartTime(job.id)
                // Respond with the result
                .ok(Set(Respond({ id: job.id, data: job.result })))

            // If there is an error
            | ArbiterErr(e) =>
              server
                // Update the arbiter
                .putArbiter(arbiterDid, timedOutArbiter)
                // Return the error
                .throw(ErrArbiterErr(e))

            // No other result should be possible
            | _ => server.panic()
          }

        // If we are OK to run resolution
        else
          server
            // Update the arbiter
            .putArbiter(arbiterDid, updArbiter)
            // Record the job start time
            .addJobStartTime(job.id)
            // We need to resolve the members for the given space list
            .ok(job.spacesToResolve.map(x => ResolveRemoteList({
              space: x,
              reqId: job.id,
              // Make sure we decrement the resolver depth so that we don't go in
              // infinite cycles trying to resolve spaces.
              resolverDepth: resolverDepth - 1
            })))

      // If the arbiter finished a job
      | ArbiterFinishedJob(job) =>
        server
          // Update the arbiter
          .putArbiter(arbiterDid, updArbiter)
          // Remove the recorded job start time
          .removeJobStartTime(job.id)
          // Respond with the result
          .ok(Set(Respond({ id: job.id, data: job.result })))

      // If the arbiter is deleted
      | ArbiterDeleted =>
        server
          // Remove the arbiter
          .removeArbiter(arbiterDid)
          .ok(Set())

      // If there is an error
      | ArbiterErr(e) =>
        server
          // Update the arbiter
          .putArbiter(arbiterDid, updArbiter)
          // Return the error
          .throw(ErrArbiterErr(e))
    }
}

/// Module wrapping up the whole arbiter system for trace generation
module arbiter {
  import utils.* from "../lib/utils"

  import arbiter_server.*
  import arbiter_core.Arbiter
  import arbiter_core.ArbiterDid
  import arbiter_core.UserDid
  import arbiter_core.SpaceId
  import arbiter_core.ResolvedMemberList
  import arbiter_core.UnresolvedMemberList
  import arbiter_core.spacesToResolve
  import arbiter_core.SpaceKey
  import arbiter_core.SpaceKey::ADMIN
  import arbiter_core.ArbiterOk
  import arbiter_core.Member
  import arbiter_core.MemberUser
  import arbiter_core.MemberLocalSpace
  import arbiter_core.MemberRemoteSpace
  import arbiter_core.SpaceConfig
  import arbiter_core.Owner
  import arbiter_core.IsMember
  import arbiter_core.AddMembers
  import arbiter_core.RemoveMembers
import arbiter_core.localUsersFromMembers

  // --------------------------------------------------------------------------
  // Types
  // --------------------------------------------------------------------------

  type NetId = str

  /// This ID represents requests from outside the server cluster, like user requests
  pure val NetId::OUTSIDE = "outside"

  pure val userDids = Set("alice", "bob", "eve", "frank", "charlie", "dave", "grace", "henry", "ivan")

  type Msg = {
    src: NetId,
    dst: NetId,
    kind: MsgKind,
  }

  pure def isRequest(m: Msg): bool =
    match (m.kind) {
      | MsgRequest(_) => true
      | MsgReply(_) => false
    }

  pure def isReply(m: Msg): bool =
    match (m.kind) {
      | MsgRequest(_) => false
      | MsgReply(_) => true
    }

  type MsgKind =
    | MsgRequest(Request)
    | MsgReply(Reply)

  type Reply = {
    id: ReqId,
    kind: ReplyKind,
  }

  type ReplyKind = 
    | ReplyOk
    | ReplyErr
    | ReplyMemberList({
        reqId: ReqId,
        spaceId: SpaceId,
        members: ResolvedMemberList,
      })

  /// Tracks pending resolution requests so we know which original job to complete
  /// when a resolution reply comes back.
  type PendingInfo = {
    originalReqId: ReqId,
    arbiterDid: ArbiterDid,
    spaceId: SpaceId,
  }

  // --------------------------------------------------------------------------
  // State variables
  // --------------------------------------------------------------------------

  var msgs: Set[Msg]
  var servers: NetId -> Server
  var pendingResolutions: ReqId -> PendingInfo

  // --------------------------------------------------------------------------
  // Helper functions
  // --------------------------------------------------------------------------

  /// Find which server hosts a given arbiter DID.
  pure def serverForArbiter(servers: NetId -> Server, arbiterDid: ArbiterDid): Option[NetId] =
    servers.keys().fold(None, (result, serverId) =>
      def server = servers.get(serverId)
      if (server.arbiters.has(arbiterDid))
        someOrOther(Some(serverId), result)
      else
        someOrOther(None, result)
    )

  /// Get all request IDs that are free (not used by any in-flight message,
  /// pending resolution, or job in any arbiter's queue).
  pure def freeReqId(
    msgs: Set[Msg],
    servers: NetId -> Server,
    pendingResolutions: ReqId -> PendingInfo
  ): Set[int] =
    1.to(100)
      .exclude(msgs.map(msg =>
        match (msg.kind) {
          | MsgRequest(req) => req.id
          | MsgReply(reply) => reply.id
        }
      ))
      .exclude(pendingResolutions.keys())
      .exclude(servers.keys().map(serverId =>
        def server = servers.get(serverId)
        server.arbiters.keys().map(arbiterDid =>
          def arbiter = server.arbiters.get(arbiterDid)
          arbiter.jobQueue.keys()
        ).flatten()
      ).flatten())

  /// Convert ServerOk feedback (Respond items) into reply messages.
  /// ResolveRemoteList items are NOT handled here — they are dispatched
  /// separately by dispatchResolutionRequest.
  pure def respondFeedbackToMessages(
    msgSrc: NetId, msgDst: NetId, req: Request,
    feedback: Set[Feedback]
  ): Set[Msg] =
    feedback.fold(Set(), (acc, fb) =>
      match (fb) {
        | Respond(resp) =>
          match (resp.data) {
            | JobOkResolvedMembersList(members) =>
              match (req.endpoint) {
                | XFetchMembers(args) =>
                  acc.union(Set({
                    src: msgDst,
                    dst: msgSrc,
                    kind: MsgReply({
                      id: resp.id,
                      kind: ReplyMemberList({
                        reqId: resp.id,
                        spaceId: { arbiterDid: args.arbiterDid, spaceKey: args.spaceKey },
                        members: members,
                      }),
                    }),
                  }))
                | _ =>
                  acc.union(Set({
                    src: msgDst,
                    dst: msgSrc,
                    kind: MsgReply({ id: resp.id, kind: ReplyOk }),
                  }))
              }
            | JobOk =>
              acc.union(Set({
                src: msgDst,
                dst: msgSrc,
                kind: MsgReply({ id: resp.id, kind: ReplyOk }),
              }))
          }
        | _ => acc
      }
    )

  // --------------------------------------------------------------------------
  // Actions
  // --------------------------------------------------------------------------

  /// Initialise the system with two servers, each hosting one arbiter.
  /// server1 hosts the "acme" arbiter (which has a remote space delegation
  /// to games:#gaming).  server2 hosts the "games" arbiter.
  action init = all {
    servers' = Map(
      "server1" -> Server::default,
      "server2" -> Server::default,
    ),
    msgs' = Set(),
    pendingResolutions' = Map(),
  }

  /// Top-level step — pick one action to execute.
  action step = any {
    tickServerAny,
    handleServerRequest,
    dispatchResolutionRequest,
    handleServerReply,
    sendOutsideRequest,
    receiveReplyToOutsideAny,
  }

  /// Pick a server and advance its clock.  This may trigger job timeouts.
  action tickServerAny =
    nondet serverId = servers.keys().oneOf()
    def server = servers.get(serverId)
    all {
      servers' = servers.put(serverId, Server::tick(server)),
      msgs' = msgs,
      pendingResolutions' = pendingResolutions,
    }

  /// Discard replies destined for outside — we don't simulate the external client.
  action receiveReplyToOutsideAny =
    nondet reply = msgs.fold(Set(), (out, m) =>
      if (m.src == NetId::OUTSIDE)
        match (m.kind) {
          | MsgRequest(_) => out
          | MsgReply(_) => out.union(Set(m))
        }
      else out
    ).oneOf()
    all {
      servers' = servers,
      msgs' = msgs.exclude(Set(reply)),
      pendingResolutions' = pendingResolutions,
    }

  /// Process a request message at the destination server.
  ///
  /// 1. Calls Server::request to submit the job to the arbiter.
  /// 2. If the job completes immediately (Respond feedback), converts the
  ///    response into a reply message and sends it back to the requester.
  /// 3. If the job needs resolution (ResolveRemoteList feedback), the server
  ///    state is updated with the queued job and the resolution dispatch is
  ///    left to the dispatchResolutionRequest action in a later step.
  action handleServerRequest =
    nondet msg = msgs.fold(Set(), (out, m) =>
      match (m.kind) {
        | MsgRequest(_) => out.union(Set(m))
        | MsgReply(_) => out
      }
    ).oneOf()
    match (msg.kind) {
      | MsgRequest(req) =>
        def server = servers.get(msg.dst)
        def updServer = Server::request(server, req)

        // Convert Respond feedback into reply messages
        def newMsgs = match (updServer.result) {
          | ServerOk(feedback) =>
            respondFeedbackToMessages(msg.src, msg.dst, req, feedback)
          | _ => Set()
        }

        all {
          servers' = servers.put(msg.dst, updServer),
          msgs' = msgs.exclude(Set(msg)).union(newMsgs),
          pendingResolutions' = pendingResolutions,
        }

      // Unreachable: the fold above only selects MsgRequest values
      | MsgReply(_) =>
        all {
          servers' = servers,
          msgs' = msgs,
          pendingResolutions' = pendingResolutions,
        }
    }

  /// Find an arbiter with a queued job that is waiting on a remote space
  /// resolution, pick one unresolved space, and send a fetch-members request
  /// to the server that hosts that space.
  ///
  /// A fresh request ID is used (avoiding the job-ID-reuse problem) and the
  /// mapping from this new ID to the original job context is stored in
  /// pendingResolutions.
  action dispatchResolutionRequest =
    // Only consider servers that have at least one arbiter with a job
    // that has unresolved remote spaces.
    def serversWithNeedyJobs = servers.keys().filter(serverId =>
      def s = servers.get(serverId)
      s.arbiters.keys().exists(did =>
        def a = s.arbiters.get(did)
        a.jobQueue.keys().exists(jobId =>
          def j = a.jobQueue.get(jobId)
          j.unresolvedMembers.spacesToResolve()
            .exclude(j.resolvedSpaces.keys()).size() > 0
        )
      )
    )

    nondet serverId = serversWithNeedyJobs.oneOf()
    def server = servers.get(serverId)

    // Find arbiters on this server that have needy jobs
    def arbitersWithNeedyJobs = server.arbiters.keys().filter(did =>
      def arbiter = server.arbiters.get(did)
      arbiter.jobQueue.keys().exists(jobId =>
        def job = arbiter.jobQueue.get(jobId)
        job.unresolvedMembers.spacesToResolve()
          .exclude(job.resolvedSpaces.keys()).size() > 0
      )
    )

    nondet arbiterDid = arbitersWithNeedyJobs.oneOf()
    def arbiter = server.arbiters.get(arbiterDid)

    // Pick a specific job that needs resolution
    def needyJobIds = arbiter.jobQueue.keys().filter(jobId =>
      def job = arbiter.jobQueue.get(jobId)
      job.unresolvedMembers.spacesToResolve()
        .exclude(job.resolvedSpaces.keys()).size() > 0
    )

    nondet jobId = needyJobIds.oneOf()
    def job = arbiter.jobQueue.get(jobId)

    // Pick one unresolved space from that job
    def unresolvedSpaces = job.unresolvedMembers.spacesToResolve()
      .exclude(job.resolvedSpaces.keys())

    nondet spaceId = unresolvedSpaces.oneOf()

    // Find the server that hosts the remote arbiter
    match (serverForArbiter(servers, spaceId.arbiterDid)) {
      | Some(host) =>
        // Use a user from the target arbiter's $admin space for the resolution
        // request — these users have Owner access and can read any space.
        def targetServer = servers.get(host)
        def targetArbiter = targetServer.arbiters.get(spaceId.arbiterDid)
        def adminSpace = targetArbiter.spaces.get(SpaceKey::ADMIN)
        def adminUsers = adminSpace.members.keys().localUsersFromMembers()
        nondet resolutionUserDid = adminUsers.oneOf()

        // Pick a fresh request ID for the resolution request
        def availableIds = freeReqId(msgs, servers, pendingResolutions)
        nondet resolutionReqId = availableIds.oneOf()

        all {
          servers' = servers,
          msgs' = msgs.union(Set({
            src: serverId,
            dst: host,
            kind: MsgRequest({
              id: resolutionReqId,
              endpoint: XFetchMembers({
                userDid: resolutionUserDid,
                arbiterDid: spaceId.arbiterDid,
                spaceKey: spaceId.spaceKey,
                resolverDepth: 1,
              }),
            }),
          })),
          pendingResolutions' = pendingResolutions.put(resolutionReqId, {
            originalReqId: jobId,
            arbiterDid: arbiterDid,
            spaceId: spaceId,
          }),
        }

      | None =>
        // Remote arbiter not found on any server — message is lost and
        // the job will eventually timeout.
        all {
          servers' = servers,
          msgs' = msgs,
          pendingResolutions' = pendingResolutions,
        }
    }

  /// Process a reply message, feeding resolved member lists back into the
  /// arbiter that was waiting for them.
  ///
  /// 1. Looks up the reply's ID in pendingResolutions to find the original job.
  /// 2. Calls Server::resolvedMemberList with the original job info.
  /// 3. If the job completes as a result, any Respond feedback is converted
  ///    into a reply to outside (which receiveReplyToOutsideAny will consume).
  action handleServerReply =
    nondet msg = msgs.fold(Set(), (out, m) =>
      match (m.kind) {
        | MsgRequest(_) => out
        | MsgReply(_) => out.union(Set(m))
      }
    ).oneOf()
    match (msg.kind) {
      | MsgReply(reply) => match (reply.kind) {
          | ReplyMemberList(r) =>
            if (pendingResolutions.has(reply.id))
              def info = pendingResolutions.get(reply.id)
              def server = servers.get(msg.dst)
              def updServer = Server::resolvedMemberList(
                server, info.arbiterDid, info.originalReqId,
                r.spaceId, r.members
              )

              // If the job completed, send the response to outside
              def newMsgs = match (updServer.result) {
                | ServerOk(feedback) =>
                  respondFeedbackToMessages(
                    msg.dst, NetId::OUTSIDE,
                    // Build a minimal Request for the helper to use
                    { id: info.originalReqId, endpoint: XFetchMembers({
                      userDid: "",
                      arbiterDid: info.spaceId.arbiterDid,
                      spaceKey: info.spaceId.spaceKey,
                      resolverDepth: 0,
                    })},
                    feedback
                  )
                | _ => Set()
              }

              all {
                servers' = servers.put(msg.dst, updServer),
                msgs' = msgs.exclude(Set(msg)).union(newMsgs),
                pendingResolutions' = pendingResolutions.mapRemove(reply.id),
              }
            else
              // No pending resolution — discard the message
              all {
                servers' = servers,
                msgs' = msgs.exclude(Set(msg)),
                pendingResolutions' = pendingResolutions,
              }

          | _ =>
            // ReplyOk or ReplyErr — just discard
            all {
              servers' = servers,
              msgs' = msgs.exclude(Set(msg)),
              pendingResolutions' = pendingResolutions,
            }
        }

      // Unreachable: the fold above only selects MsgReply values
      | MsgRequest(_) =>
        all {
          servers' = servers,
          msgs' = msgs,
          pendingResolutions' = pendingResolutions,
        }
    }

  /// Send a request from "outside" to a server.
  /// Picks an existing arbiter, an existing space on that arbiter, a user
  /// DID, and a resolver depth, then creates a FetchMembers request.
  action sendOutsideRequest =
    nondet serverId = servers.keys().oneOf()
    def server = servers.get(serverId)
    nondet arbiterDid = server.arbiters.keys().oneOf()
    def arbiter = server.arbiters.get(arbiterDid)
    nondet spaceKey = arbiter.spaces.keys().oneOf()
    nondet userDid = userDids.oneOf()
    nondet resolverDepth = Set(0, 1, 2).oneOf()

    def availableIds = freeReqId(msgs, servers, pendingResolutions)
    nondet reqId = availableIds.oneOf()

    all {
      msgs' = msgs.union(Set({
        src: NetId::OUTSIDE,
        dst: serverId,
        kind: MsgRequest({
          id: reqId,
          endpoint: XFetchMembers({
            userDid: userDid,
            arbiterDid: arbiterDid,
            spaceKey: spaceKey,
            resolverDepth: resolverDepth,
          }),
        }),
      })),
      servers' = servers,
      pendingResolutions' = pendingResolutions,
    }

  // --------------------------------------------------------------------------
  // Test helpers — used by the run block below
  // --------------------------------------------------------------------------

  /// A witness that is true when any pending resolution exists.
  /// Use with: quint run --witnesses=witnessHasPendingResolution
  val witnessHasPendingResolution = pendingResolutions.keys().size() > 0

  /// Send a specific fetch-members request for acme.#dev from outside.
  /// This space has a remote delegation to games:#gaming, so it will
  /// trigger cross-server resolution when processed on server1.
  action sendDevFetchRequest =
    nondet userDid = userDids.oneOf()
    nondet resolverDepth = Set(1, 2).oneOf()
    def availableIds = freeReqId(msgs, servers, pendingResolutions)
    nondet reqId = availableIds.oneOf()
    all {
      msgs' = msgs.union(Set({
        src: NetId::OUTSIDE,
        dst: "server1",
        kind: MsgRequest({
          id: reqId,
          endpoint: XFetchMembers({
            userDid: userDid,
            arbiterDid: "acme",
            spaceKey: "#dev",
            resolverDepth: resolverDepth,
          }),
        }),
      })),
      servers' = servers,
      pendingResolutions' = pendingResolutions,
    }

  action debugPrintVars = all {
    msgs' = q::debug(msgs),
    servers' = q::debug(servers),
    pendingResolutions' = q::debug(pendingResolutions),
  }

  /// Intentionally exercise the cross-server resolution path.
  ///
  /// 1. init:     server1 hosts acme, server2 hosts games
  /// 2. sendDev:  outside requests fetch-members for acme.#dev
  /// 3. handle:   server1 queues a job on acme (needs games:#gaming)
  /// 5. dispatch: server1 sends a resolution fetch to server2
  /// 5. handle:   server2 processes the fetch for games:#gaming (sync resolve)
  /// 6. handle:   server1 receives the resolution reply, job completes
  ///
  /// After step 6, pendingResolutions should be back to empty and the
  /// acme arbiter's job queue should be empty (the job finished).
  run crossServerResolve1Test =
    initScenario1
      .then(sendDevFetchRequest)
      .then(handleServerRequest)
      .then(debugPrintVars)
      .then(dispatchResolutionRequest)
      .then(handleServerRequest)
      .then(handleServerReply)
      .expect(pendingResolutions == Map())

  // --------------------------------------------------------------------------
  // Static arbiter fixtures
  // --------------------------------------------------------------------------

  action initScenario1 = all {
    servers' = Map(
      "server1" -> Server::default.putArbiter(acmeArbiter.did, acmeArbiter),
      "server2" -> Server::default.putArbiter(gamesArbiter.did, gamesArbiter),
    ),
    msgs' = Set(),
    pendingResolutions' = Map(),
  }

  /// "acme" arbiter on server1 — its #dev space has a remote delegation
  /// to games:#gaming, which triggers cross-server resolution.
  pure val acmeArbiter: Arbiter = {
    version: 0,
    did: "acme",
    jobQueue: Map(),
    result: ArbiterOk,
    spaces: Map(
      SpaceKey::ADMIN -> {
        config: { publicRecords: false, publicMembers: true },
        members: Map(MemberUser("alice") -> Owner, MemberUser("bob") -> Owner),
      },
      "moderators" -> {
        config: { publicRecords: false, publicMembers: true },
        members: Map(MemberUser("charlie") -> RemoveMembers, MemberUser("dave") -> AddMembers),
      },
      "#general" -> {
        config: { publicRecords: false, publicMembers: true },
        members: Map(MemberLocalSpace("moderators") -> RemoveMembers, MemberUser("eve") -> IsMember),
      },
      "#random" -> {
        config: { publicRecords: false, publicMembers: true },
        members: Map(MemberLocalSpace("moderators") -> RemoveMembers, MemberUser("frank") -> IsMember),
      },
      "#dev" -> {
        config: { publicRecords: false, publicMembers: true },
        members: Map(
          MemberLocalSpace("moderators") -> RemoveMembers,
          MemberRemoteSpace({ arbiterDid: "games", spaceKey: "#gaming" }) -> IsMember,
        ),
      },
    ),
  }

  /// "games" arbiter on server2 — its #gaming space resolves synchronously
  /// (all members are local), providing a clean resolution target.
  pure val gamesArbiter: Arbiter = {
    version: 0,
    did: "games",
    jobQueue: Map(),
    result: ArbiterOk,
    spaces: Map(
      SpaceKey::ADMIN -> {
        config: { publicRecords: false, publicMembers: true },
        members: Map(MemberUser("grace") -> Owner),
      },
      "#lobby" -> {
        config: { publicRecords: false, publicMembers: true },
        members: Map(MemberUser("grace") -> Owner, MemberUser("henry") -> IsMember),
      },
      "#gaming" -> {
        config: { publicRecords: false, publicMembers: true },
        members: Map(
          MemberLocalSpace("#lobby") -> RemoveMembers,
          MemberUser("ivan") -> AddMembers,
        ),
      },
    ),
  }
}