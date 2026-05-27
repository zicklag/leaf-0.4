/// The server wraps the core, presents the XRPC API, and manages multiple arbiters.
module arbiter_server {
  import utils.* from "../lib/utils"

  import choreo(processes = NODES) as choreo from "../lib/choreo"
  pure def CustomEffect = choreo::CustomEffect

  import arbiter_core_types.* from "./arbiter_core"
  import arbiter_core as core from "./arbiter_core"

  pure val TIMEOUT_TICKS = 8

  const NODES: Set[Node]
  const ARBITERS: Set[ArbiterDid]
  const USERS: Set[UserDid]

  type Node = str
  type Time = int

  /// The message data.
  ///
  /// Note: For reply message kinds, the fields other than `kind` are preserved.
  type Msg = {
    // The node that this message came from. Preserved in replies
    srcNode: Node,
    // the user / arbiter that sent this message, except in the case of a reply, where it is the
    // user / arbiter that sent the message that is being replied to
    userDid: str,
    // The arbiter that this message is targeting
    arbiterDid: ArbiterDid,
    // The space key this message is targeting
    spaceKey: SpaceKey,
    // The JobID that triggered the original message
    srcJobId: JobId,
    // The maximum resolution chain depth
    resolverDepth: int,
    // The kind of message
    kind: MsgKind,
  }

  pure val Msg::NULL = {
    srcNode: "",
    userDid: "",
    arbiterDid: "",
    spaceKey: "",
    srcJobId: 0,
    resolverDepth: 0,
    kind: XFetchMembers,
  }

  /// The server message type.
  type MsgKind =
    | ReplyResolvedMembers(ReplyResolvedMembersArgs)
    | XFetchMembers
    | XCreateSpace
    | XConfigureSpace(XConfigureSpaceArgs)
    | XDeleteSpace
    | XSetMemberAccess(XSetMemberAccessArgs)
    | XRemoveMember(XRemoveMemberArgs)
    | XCreateArbiter
    | XDeleteArbiter

  type ReplyResolvedMembersArgs = {
    members: ResolvedMemberList,
  }
  type XConfigureSpaceArgs = {
    publicRecords: bool,
    publicMembers: bool,
  }
  type XSetMemberAccessArgs = {
    member: Member,
    access: Access,
  }
  type XRemoveMemberArgs = {
    member: Member,
  }

  /// The server errors we can run into
  type ServerError =
    | ErrArbiterAlreadyExists
    | ErrArbiterNotExists
    | ErrArbiterErr(ArbiterError)
    | ErrDuplicateReqId

  /// A server's state
  type Server = {
    time: Time,
    jobInfo: JobId -> JobInfo,
    arbiters: ArbiterDid -> Arbiter,
  }

  type JobInfo = {
    // The message that started this job
    msg: Msg,
    // The time the job was started
    startTime: Time,
  }

  type CustomEffects =
    | UpdatePlc({ arbiterDid: ArbiterDid, node: Node })
    | ConsumeMessage({
        node: Node,
        msg: Msg,
      })
    | Error({
        node: Node,
        error: ServerError,
      })
    | Panic(Node)
  type Event = ()
  type Extensions = {
    // "DID PLC" mapping of arbiter DIDs to the node they are hosted on.
    didPlc: ArbiterDid -> Node,
    panickedServer: Option[Node],
    serverError: Option[{
      node: Node,
      error: ServerError
    }]
  }

  /* Boilerplate */
  type LocalState = choreo::LocalState[Node, Server]
  type LocalContext = choreo::LocalContext[Node, Server, Msg, Event, Extensions]
  type Transition = choreo::Transition[Node, Server, Msg, Event, CustomEffects]
  type GlobalContext = choreo::GlobalContext[
    Node,
    Server,
    Msg,
    Event,
    Extensions
  ]
  /* End of boilerplate */

  //
  // ACTIONS
  // 

  // System init state
  action init = choreo::init({
    system: NODES.mapBy(n => nodeInit(n)),
    messages: NODES.union(USERS).mapBy(n => Set()),
    events: NODES.mapBy(n => Set()),
    extensions: {
      didPlc: Map(),
      panickedServer: None,
      serverError: None
    }
  })

  // Initial node state
  pure def nodeInit(n: Node): LocalState = {
    process_id: n,
    time: 0,
    jobInfo: Map(),
    arbiters: Map(),
  }

  // Step action
  action step = choreo::step(
    transitions,
    applyCustomEffects
  )

  val NoPanics = choreo::s.extensions.panickedServer == None
  val AllArbiterInvariants = choreo::s.system.keys().forall(nodeId =>
    def node = choreo::s.system.get(nodeId)
    node.arbiters.keys().forall(arbiterDid =>
      def arbiter = node.arbiters.get(arbiterDid)
      core::Arbiter::invariants(arbiter)
    )
  )
  val Invariant = and {
    NoPanics,
    AllArbiterInvariants,
  }

  // Possible state transitions
  pure def transitions(ctx: LocalContext): Set[Transition] = Set(
    // Tick the server
    Server::tick(ctx),
    // Handle one of the messages
    handleMessage(ctx),
    // Simulate users sending requests to the system
    sendUserMessage(ctx),
  ).flatten()

  pure def handleMessage(ctx: LocalContext): Set[Transition] =
    ctx.messages.map(msg => Server::handleMessage(ctx, msg))

  // Applies global effects
  pure def applyCustomEffects(ctx: GlobalContext, effect: CustomEffects): GlobalContext =
    match (effect) {
      // Update the PLC resolution record
      | UpdatePlc(record) => {
          ...ctx,
          extensions: {
            ...ctx.extensions,
            didPlc: ctx.extensions.didPlc.put(record.arbiterDid, record.node)
          }
        }
      // Consume a message so it isn't handled again by the node
      | ConsumeMessage(args) => {
        ...ctx,
        messages: ctx.messages.put(args.node, ctx.messages.get(args.node).exclude(Set(args.msg)))
      }
      | Error(e) => { ...ctx, extensions: { ...ctx.extensions, serverError: Some(e) } }
      | Panic(node) => { ...ctx, extensions: { ...ctx.extensions, panickedServer: Some(node) } }
    }

  //
  // TRIVIAL SERVER HELPERS
  //

  pure def throw(server: LocalState, serverErr: ServerError): Transition =
    // If this is an arbiter error with a job ID we want to remove the job info from the server,
    // since the job is done now.
    def updServer = match (serverErr) {
      | ErrArbiterErr(e) => match (e.jobId) {
          | Some(jobId) => server.removeJobInfo(jobId)
          | None => server
        }
      | _ => server
    }
    // Return the error effect with the possibly-updated server
    { post_state: updServer, effects: Set(CustomEffect(Error({ node: server.process_id, error: serverErr }))) }

  pure def panic(server: LocalState): Transition = { post_state: server, effects: Set(CustomEffect(Panic(server.process_id))) }
  pure def ok(server: LocalState): Transition = { post_state: server, effects: Set() }
  pure def updatePlc(server: LocalState, arbiterDid: ArbiterDid, node: Node): Transition = { post_state: server, effects: Set(CustomEffect(UpdatePlc({ arbiterDid: arbiterDid, node: node })))}
  pure def send(server: LocalState, msgs: Set[{toNode: Node, msg: Msg}]): Transition = { post_state: server, effects: msgs.map(x => choreo::Send({to: x.toNode, message: x.msg})) }

  pure def tick(server: LocalState): LocalState = server.with("time", server.time + 1)
  pure def putArbiter(server: LocalState, arbiterDid: ArbiterDid, arbiter: Arbiter): LocalState = server.with("arbiters", server.arbiters.put(arbiterDid, arbiter))
  pure def removeArbiter(server: LocalState, arbiterDid: ArbiterDid): LocalState = server.with("arbiters", server.arbiters.mapRemove(arbiterDid))
  pure def addJobInfo(server: LocalState, jobId: JobId, jobInfo: JobInfo): LocalState = server.with("jobInfo", server.jobInfo.put(jobId, jobInfo))
  pure def removeJobInfo(server: LocalState, jobId: JobId): LocalState = server.with("jobInfo", server.jobInfo.mapRemove(jobId))

  //
  // TRANSITION IMPLEMENTATIONS
  //

  /// Process a message
  pure def Server::handleMessage(ctx: LocalContext, msg: Msg): Transition =
    def server = ctx.state

    // Handle the message
    def transition = match (msg.kind) {
      | XCreateArbiter => ctx.createArbiter(msg)
      | XDeleteArbiter(args) => ctx.startJob(msg, JobDeleteArbiter)
      | ReplyResolvedMembers(args) => ctx.handleResolutionReply(msg, args.members)
      | XFetchMembers => ctx.startJob(msg, JobFetchMembers)
      | XCreateSpace => ctx.startJob(msg, JobCreateSpace)
      | XConfigureSpace(args) => ctx.startJob(msg, JobConfigureSpace({
          publicRecords: args.publicRecords,
          publicMembers: args.publicMembers,
        }))
      | XDeleteSpace => ctx.startJob(msg, JobDeleteSpace)
      | XSetMemberAccess(args) => ctx.startJob(msg, JobSetMemberAccess({
          member: args.member,
          access: args.access,
        }))
      | XRemoveMember(args) => ctx.startJob(msg, JobRemoveMember({
          member: args.member,
        }))
    }

    // Make sure the message is consumed so we don't handle it a second time, by adding
    // a ConsumeMessage effect.
    transition.with("effects",
      transition.effects
        .union(Set(CustomEffect(ConsumeMessage({ node: server.process_id, msg: msg }))))
    )

  /// Tick the server to let it process timeouts and resolution steps
  pure def Server::tick(ctx: LocalContext): Set[Transition] =
    def server = ctx.state

    // Find a timed out job, if it exists
    def timedOutJob = server.arbiters.keys().fold(None, (s, arbiterDid) => s.someOrOther(
      def arbiter = server.arbiters.get(arbiterDid)
      
      // Look at every job
      arbiter.jobQueue.keys().fold(s, (s, jobId) => s.someOrOther(
        def jobInfo = server.jobInfo.get(jobId)
        // If the job is timed out
        if (server.time - jobInfo.startTime >= TIMEOUT_TICKS)
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
        def updArbiter = core::timeoutJob(arbiter, timedOut.jobId)

        Set(ctx
          .with("state", server.tick())
          // Update the arbiter. Pass a null message because the handleArbiterResult wants to know
          // what message triggered the result, in case it needs to create a new job and store who
          // to respond to when that job completes.
          //
          // In this case of timing out an existing job, there should be no case in which this message
          // actually gets used so a null message is fine.
          .handleArbiterResult(updArbiter, Msg::NULL))

      // Just advance time if there are no timed out jobs
      | None => Set(server.tick().ok())
    }

  //
  // HELPERS
  //

  pure def handleResolutionReply(ctx: LocalContext, m: Msg, members: ResolvedMemberList): Transition =
    def server = ctx.state
    def jobInfo = server.jobInfo.get(m.srcJobId)
    def arbiter = server.arbiters.get(jobInfo.msg.arbiterDid)

    // Make sure the arbiter exists
    if (not(server.arbiters.has(jobInfo.msg.arbiterDid))) server.throw(ErrArbiterNotExists) else
    // Update the arbiter
    def updArbiter = core::provideRemoteSpaceMembers(arbiter, m.srcJobId, { arbiterDid: m.arbiterDid, spaceKey: m.spaceKey }, members)
    // Handle the result
    ctx.handleArbiterResult(updArbiter, m)

  pure def createArbiter(ctx: LocalContext, m: Msg): Transition =
    def server = ctx.state

    // Make sure the arbiter doesn't already exist
    if (server.arbiters.has(m.arbiterDid)) server.throw(ErrArbiterAlreadyExists) else
    // Create a new arbiter
    def newArbiter = core::new(m.arbiterDid, m.userDid)
    server
      // Add the arbiter to the server
      .putArbiter(m.arbiterDid, newArbiter)
      // Update PLC directory to resolve the did to this node, too
      .updatePlc(m.arbiterDid, server.process_id)

  pure def startJob(ctx: LocalContext, msg: Msg, jobArgs: JobArgs): Transition =
    def server = ctx.state
    def arbiter = server.arbiters.get(msg.arbiterDid)

    // Generate a unique job ID
    def newId = 1.to(100).exclude(server.jobInfo.keys()).fold(None, (j, a) => Some(j.orElse(a)))

    // Panic if we are out of IDs. The implementation must make sure this never happens
    match (newId) { | None => server.panic() | Some(jobId) =>

    // Make sure the arbiter exists
    if (not(server.arbiters.has(msg.arbiterDid))) server.throw(ErrArbiterNotExists) else
    // Start the job
    def updArbiter = core::startJob(arbiter, msg.userDid, msg.spaceKey, jobId, jobArgs)
    // Handle the result
    ctx.handleArbiterResult(updArbiter, msg)

    }

  /// Given an arbiter that has just been updated, handle the result
  pure def handleArbiterResult(ctx: LocalContext, updArbiter: Arbiter, msg: Msg): Transition =
    def server = ctx.state
    def arbiterDid = updArbiter.did

    match (updArbiter.result) {
      // If the arbiter is OK
      | ArbiterOk =>
        server
          // Update the arbiter
          .putArbiter(arbiterDid, updArbiter)
          .ok()

      // If we need to queue remote space resolution
      | ArbiterQueuedJob(job) =>
        // If the resolver depth is less than one, we can't actually solve the queued spaces.
        if (msg.resolverDepth < 1)
          // We need to immediately timeout the job
          def timedOutArbiter = core::timeoutJob(updArbiter, job.id)

          // And handle the result
          match (timedOutArbiter.result) {
            // A delete request finishes
            | ArbiterDeleted =>
              server
                // Remove the arbiter
                .removeArbiter(arbiterDid)
                .ok()

            // The job finishes successfully
            | ArbiterFinishedJob(job) =>
              def jobMsg =
                // If we have info for this job it means it was previously queued.
                if (server.jobInfo.has(job.id))
                  // We want to reply to the original message that created the job.
                  server.jobInfo.get(job.id).msg
                // If we don't have info for this job, it means that the job was completed immediately and
                // not queued, so we want to reply to the message that triggered this result.
                else msg
                
              match (job.result) {
                | JobOkResolvedMemberList(members) =>
                  server
                    // Update the arbiter
                    .putArbiter(arbiterDid, timedOutArbiter)
                    // Cleanup job info
                    .removeJobInfo(job.id)
                    // Respond with the result
                    .send(Set({
                      toNode: jobMsg.srcNode,
                      msg: {
                        ...jobMsg,
                        kind: ReplyResolvedMembers({
                          members: members,
                        }),
                      }
                    }))

                | JobOk => server
                  // Update the arbiter
                  .putArbiter(arbiterDid, timedOutArbiter)
                  // Cleanup job info
                  .removeJobInfo(job.id)
                  .ok()
              }

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
            // Record the job info
            .addJobInfo(job.id, {
              msg: msg,
              startTime: server.time,
            })
            // We need to resolve the members for the given space list
            .send(job.spacesToResolve.map(spaceId => {
              // FIXME: this resolve could fail
              toNode: ctx.extensions.didPlc.get(spaceId.arbiterDid),
              msg: {
                srcNode: server.process_id,
                userDid: arbiterDid,
                arbiterDid: spaceId.arbiterDid,
                spaceKey: spaceId.spaceKey,
                srcJobId: job.id,
                // Make sure we decrement the resolver depth so that we don't go in
                // infinite cycles trying to resolve spaces.
                resolverDepth: msg.resolverDepth - 1,
                kind: XFetchMembers,
              }
            }))

      // If the arbiter finished a job
      | ArbiterFinishedJob(job) =>
        def jobMsg =
          // If we have info for this job it means it was previously queued.
          if (server.jobInfo.has(job.id))
            // We want to reply to the original message that created the job.
            server.jobInfo.get(job.id).msg
          // If we don't have info for this job, it means that the job was completed immediately and
          // not queued, so we want to reply to the message that triggered this result.
          else msg

        match (job.result) {
          | JobOk => server
              // Update the arbiter
              .putArbiter(arbiterDid, updArbiter)
              // Remove the job info
              .removeJobInfo(job.id)
              // We're good
              .ok()
          | JobOkResolvedMemberList(members) => server
              // Update the arbiter
              .putArbiter(arbiterDid, updArbiter)
              // Remove the job info
              .removeJobInfo(job.id)
              // Respond with the result
              .send(Set({
                  toNode: jobMsg.srcNode,
                  msg: {
                    ...jobMsg,
                    kind: ReplyResolvedMembers({
                      members: members,
                    }),
                  }
                }))
        }

      // If the arbiter is deleted
      | ArbiterDeleted =>
        server
          // Remove the arbiter
          .removeArbiter(arbiterDid)
          .ok()

      // If there is an error
      | ArbiterErr(e) =>
        server
          // Update the arbiter
          .putArbiter(arbiterDid, updArbiter)
          // Return the error
          .throw(ErrArbiterErr(e))
    }

  pure def sendMessage(ctx: LocalContext, msg: Msg): Transition =
    { post_state: ctx.state, effects: Set(choreo::Send({ to: ctx.state.process_id, message: msg })) }

  // Simulate a user sending a random message to the server
  pure def sendUserMessage(ctx: LocalContext): Set[Transition] =
    ARBITERS.map(arbiterDid =>
      USERS.map(userDid => Set(
        // Create arbiter
        sendMessage(ctx, {
          srcNode: userDid,
          userDid: userDid,
          arbiterDid: arbiterDid,
          spaceKey: SpaceKey::ADMIN,
          srcJobId: 0,
          resolverDepth: 3,
          kind: XCreateArbiter,
        }),
        // 
        sendMessage(ctx, {
          srcNode: userDid,
          userDid: userDid,
          arbiterDid: arbiterDid,
          spaceKey: SpaceKey::ADMIN,
          srcJobId: 0,
          resolverDepth: 3,
          kind: XFetchMembers,
        })
      )).flatten()
    ).flatten()
}

module arbiter {
  import arbiter_server(
    NODES = Set("a", "b"),
    ARBITERS = Set("arb1", "arb2"),
    USERS = Set("alice", "bob", "charlie"),
  ).*
}

