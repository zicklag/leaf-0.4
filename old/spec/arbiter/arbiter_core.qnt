module arbiter_core_types {
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

  type ArbiterError = {
      kind: ArbiterErrorKind,
      // The job that errored, if any
      jobId: Option[JobId],
    }

  /// An error result for the arbiter state machine
  type ArbiterErrorKind =
    | ErrJobIdExists
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
    | JobOkResolvedMemberList(ResolvedMemberList)
  
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
}

/// The state machine of the arbiter core. This handles the role computation and resolution logic
/// and is meant to be wrapped by the arbiter server that will drive it.
module arbiter_core {
  import arbiter_core_types.*

  import utils.* from "../lib/utils"

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

  // TODO: we need to model the jobs being randomly dropped in case the server crashes.

  // Create a new arbiter with the given arbiter DID and initial owner.
  pure def new(arbiterDid: ArbiterDid, userDid: UserDid): Arbiter = 
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
  pure def startJob(arbiter: Arbiter, userDid: UserDid, spaceKey: SpaceKey, jobId: JobId, args: JobArgs): Arbiter =
    // Make sure the job ID doesn't already exist
    if (arbiter.jobQueue.has(jobId)) arbiter.throw(ErrJobIdExists) else

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
        .throw(ErrJobsTimedOut(collidingJobs))
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
    }
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
  pure def provideRemoteSpaceMembers(arbiter: Arbiter, jobId: JobId, spaceId: SpaceId, members: ResolvedMemberList): Arbiter =
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
        .with("jobQueue",
          arbiter.jobQueue.put(jobId,
            job.with("resolvedSpaces", resolvedSpaces) 
          )
        ).with("result", ArbiterOk)

  /// Timeout a job that is in the queue and waiting for space member lists to be resolved.
  ///
  /// This will execute the job without waiting for the remote member lists to finish resolving.
  pure def timeoutJob(arbiter: Arbiter, jobId: JobId): Arbiter =
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
  pure def throw(arbiter: Arbiter, e: ArbiterErrorKind): Arbiter =
    arbiter.with("result", ArbiterErr({ kind: e, jobId: None }))

  /// Throw an error and remove a job from the job queue
  pure def throwJobErr(arbiter: Arbiter, e: ArbiterErrorKind, jobId: JobId): Arbiter =
    arbiter.removeJob(jobId).with("result", ArbiterErr({ kind: e, jobId: Some(jobId) }))
  
  /// Return any validation errors for the job if there are issues
  pure def jobValidationError(arbiter: Arbiter, spaceKey: SpaceKey, args: JobArgs): Option[ArbiterErrorKind] =
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
        result: JobOkResolvedMemberList(resolved)
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