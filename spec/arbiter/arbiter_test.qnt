module arbiter_test {
  import utils.* from "../lib/utils"

  import arbiter_server(
    NODES = Set("a", "b"),
    ARBITERS = Set("arb1", "arb2"),
    USERS = Set("alice", "bob", "charlie"),
  ).* from "./arbiter"
  import arbiter_core_types.* from "./arbiter_core"

  import fixtures.*

  action initScenario1 = choreo::init({
    system: Map(
      "a" -> {
        process_id: "a",
        time: 0,
        jobInfo: Map(),
        arbiters: Map(
          acmeArbiter.did -> acmeArbiter,
        )
      },
      "b" -> {
        process_id: "b",
        time: 0,
        jobInfo: Map(),
        arbiters: Map(
          gamesArbiter.did -> gamesArbiter,
        )
      }
    ),
    messages: NODES.union(USERS).mapBy(n => Set()),
    events: NODES.mapBy(n => Set()),
    extensions: {
      didPlc: Map(
        "acme" -> "a",
        "games" -> "b",
      ),
      panickedServer: None,
      serverError: None,
    }
  })

  action sendMsgFromUser(
    dst: Node,
    userDid: UserDid,
    arbiterDid: ArbiterDid,
    spaceKey: SpaceKey,
    kind: MsgKind,
    resolverDepth: int
  ): bool =
    choreo::s' = choreo::s.with("messages",
      def msgs = choreo::s.messages.get(dst)
      choreo::s.messages.put(dst, msgs.union(Set({
        srcNode: userDid,
        userDid: userDid,
        arbiterDid: arbiterDid,
        spaceKey: spaceKey,
        srcJobId: 0,
        resolverDepth: resolverDepth,
        kind: kind,
      }))))
  action handleMsg(node: Node): bool = choreo::step_with(node, handleMessage, applyCustomEffects)
  action tickServer(node: Node): bool = choreo::step_with(node, Server::tick, applyCustomEffects)

  run resolveAcmeDevChannelArbiterTest =
    // Setup scenario 1
    initScenario1
      // Send message from user to resolve member list of #dev channel
      .then(sendMsgFromUser("a", "alice", "acme", "#dev", XFetchMembers, 2))
      // Have server A handle the message
      .then(handleMsg("a"))
      // That will trigger a message to resolve a member list from server B, which handles its message
      .then(handleMsg("b"))
      // Which will send a reply to server A, which handles that message
      .then(handleMsg("a"))
      // And finally alice will have a response in her messages with the member list
      .expect(
        choreo::s.messages.get("alice").exists(msg =>
          match (msg.kind) {
            | ReplyResolvedMembers(result) =>
              result.members.memberList == Map(
                // Admins of acme server
                "alice" -> Owner,
                "bob" -> Owner,
                // Moderators of acme server
                "charlie" -> RemoveMembers,
                "dave" -> AddMembers,
                // Admin of remote #gaming space
                "georgia" -> IsMember,
                // Members of remote #lobby space
                "henry" -> IsMember,
                "grace" -> IsMember,
                // Member of remote #gaming space
                "ivan" -> IsMember,
              )
            | _ => false
          }
        )
      )

   run resolveAcmeDevChannelWithNoResolveArbiterTest =
    // Setup scenario 1
    initScenario1
      // Send message from user to resolve member list of #dev channel, with a resolver depth of 0
      .then(sendMsgFromUser("a", "alice", "acme", "#dev", XFetchMembers, 0))
      // Have server A handle the message
      .then(handleMsg("a"))
      // Alice should get the response, without sending a resolution to the remote arbiter
      .expect(
        choreo::s.messages.get("alice").exists(msg =>
          match (msg.kind) {
            | ReplyResolvedMembers(result) =>
              result.members.memberList == Map(
                // Admins of acme server
                "alice" -> Owner,
                "bob" -> Owner,
                // Moderators of acme server
                "charlie" -> RemoveMembers,
                "dave" -> AddMembers,
              )
                and
              result.members.missingSpaces == Map({ arbiterDid: "games", spaceKey: "#gaming" } -> IsMember)
            | _ => false
          }
        )
      )
  
  run resolveAcmeDevChannelWithTimeoutArbiterTest =
    // Setup scenario 1
    initScenario1
      // Send message from user to resolve member list of #dev channel
      .then(sendMsgFromUser("a", "alice", "acme", "#dev", XFetchMembers, 2))
      // Have server A handle the message, which will require resolving the remote member list from server b
      .then(handleMsg("a"))
      // Tick server a enough times that it will timeout the remote request 
      .then(9.reps(i => tickServer("a")))
      // Alice should get the response, without the members from the remote arbiter
      .expect(
        choreo::s.messages.get("alice").exists(msg =>
          match (msg.kind) {
            | ReplyResolvedMembers(result) =>
              result.members.memberList == Map(
                // Admins of acme server
                "alice" -> Owner,
                "bob" -> Owner,
                // Moderators of acme server
                "charlie" -> RemoveMembers,
                "dave" -> AddMembers,
              )
                and
              result.members.missingSpaces == Map({ arbiterDid: "games", spaceKey: "#gaming" } -> IsMember)
            | _ => false
          }
        )
      )
}

module fixtures {
  import arbiter_core_types.* from "./arbiter_core"

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
        members: Map(MemberUser("georgia") -> Owner),
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

