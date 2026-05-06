module arbiter_test {
  import utils.* from "../lib/utils"
  import arbiter_core_types.* from "./arbiter_core"

  val nodes = Set("a", "b")
  import arbiter_server(
    NODES = nodes,
    ARBITERS = Set("arb1", "arb2"),
    USERS = Set("alice", "bob", "charlie"),
  ).* from "./arbiter"
  import choreo(processes = nodes) as choreo from "../lib/choreo"

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
          subcomArbiter.did -> subcomArbiter,
        )
      },
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
    userDid: UserDid,
    dst: Node,
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

  /// Resolve a space with several layers of delegation including remote delegation
  run resolveAcmeDevChannelArbiterTest =
    // Setup scenario 1
    initScenario1
      // Send message from user to resolve member list of #dev channel
      .then(sendMsgFromUser("alice", "a", "acme", "#dev", XFetchMembers, 2))
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

  /// Resolve a space with delegates spaces and a remote delegation, but force it not to resolve the
  /// remote delegation by setting the resolver depth to 0.
  run resolveAcmeDevChannelWithNoResolveArbiterTest =
    // Setup scenario 1
    initScenario1
      // Send message from user to resolve member list of #dev channel, with a resolver depth of 0
      .then(sendMsgFromUser("alice", "a", "acme", "#dev", XFetchMembers, 0))
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
  
  /// Try to resolve a space with multiple nested delegations and a remote delegation, where the
  /// remote delegation times out.
  run resolveAcmeDevChannelWithTimeoutArbiterTest =
    // Setup scenario 1
    initScenario1
      // Send message from user to resolve member list of #dev channel
      .then(sendMsgFromUser("alice", "a", "acme", "#dev", XFetchMembers, 2))
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

  /// Have an owner add a new owner
  run addOwnerToAcmeArbiterTest =
    initScenario1
      // Have bob, an owner, send a message to add subcomAdmin as an owner to the $admin space
      .then(sendMsgFromUser("bob", "a", "acme", "$admin", XSetMemberAccess({ member: MemberUser("newOwner"), access: Owner }), 2))
      // Have server a handle the message
      .then(handleMsg("a"))
      // It should complete immediately with subcomAdmin as a new member.
      .expect(choreo::s.system.get("a").arbiters.get("acme").spaces.get("$admin").members.get(MemberUser("newOwner")) == Owner)

  /// Have a remotely-delegated owner add a new local owner.
  run addOwnerToSubcomAsAcmeOwnerArbiterTest =
    initScenario1
      // Have bob, who is an owner of acme, which has been remotely delegated as an owner of subcom,
      // send a message to add a new owner.
      .then(sendMsgFromUser("bob", "b", "subcom", "$admin", XSetMemberAccess({ member: MemberUser("newOwner"), access: Owner }), 2))
      // Server B will handle the message, and realize that it must resolve the remote member list of acme to complete the action.
      .then(handleMsg("b"))
      // Server A will receive server B's request to resolve the $admin member list
      .then(handleMsg("a"))
      // Then server B will receive the reply and finish the action
      .then(handleMsg("b"))
      // And we will see that the new owner has been added successfully
      .expect(choreo::s.system.get("b").arbiters.get("subcom").spaces.get("$admin").members.get(MemberUser("newOwner")) == Owner)

  /// Try to add an owner as a remotely delegated owner, with a remote timeout, which will fail to authorize
  run failToAddOwnerToSubcomAsAcmeOwnerWithTimeoutArbiterTest =
    initScenario1
      // Have bob, who is an owner of acme, which has been remotely delegated as an owner of subcom,
      // send a message to add a new owner.
      .then(sendMsgFromUser("bob", "b", "subcom", "$admin", XSetMemberAccess({ member: MemberUser("newOwner"), access: Owner }), 2))
      // Server B will handle the message, and realize that it must resolve the remote member list of acme to complete the action.
      .then(handleMsg("b"))
      // Tick the server enough times to force a timeout of the remote resolution
      .then(9.reps(i => tickServer("b")))
      // And we will see that the arbiter errored with permission denied because it couldn't validate that the member was actually
      // in the remote arbiter's list.
      .expect(choreo::s.extensions.serverError == Some({ node: "b", error: ErrArbiterErr(ErrPermissionDenied) }))

  /// Make sure that adding a member to a space that has a remote delegation can be forced to
  /// resolve instantly by setting resolver depth to 0, and that local admins will still be able to add users in that case.
  run addOwnerToSubcomAsLocalOwnerArbiterTest =
    initScenario1
      // Have subcomAdmin, who is a local owner of subcom, try to add a new owner
      .then(sendMsgFromUser("subcomAdmin", "b", "subcom", "$admin", XSetMemberAccess({ member: MemberUser("newOwner"), access: Owner }), 0))
      // Server B will handle the message, and realize that subcomAdmin is a local owner so access doesn't require a remote lookup
      .then(handleMsg("b"))
      // And we will see that the new owner has been added successfully
      .expect(choreo::s.system.get("b").arbiters.get("subcom").spaces.get("$admin").members.get(MemberUser("newOwner")) == Owner)

  /// Make sure that adding a member to a space that has a remote delegation works if you are a local owner, even if
  /// the remote resolution times out.
  run addOwnerToSubcomAsLocalOwnerWithRemoteResolveTimeoutArbiterTest =
    initScenario1
      // Have subcomAdmin, who is a local owner of subcom, try to add a new owner
      .then(sendMsgFromUser("subcomAdmin", "b", "subcom", "$admin", XSetMemberAccess({ member: MemberUser("newOwner"), access: Owner }), 2))
      // Server B will handle the message, and realize that subcomAdmin is a local owner so access doesn't require a remote lookup
      .then(handleMsg("b"))
      // Tick the server enough times to force a timeout of the remote resolution
      .then(9.reps(i => tickServer("b")))
      // And we will see that the new owner has been added successfully
      .expect(choreo::s.system.get("b").arbiters.get("subcom").spaces.get("$admin").members.get(MemberUser("newOwner")) == Owner)

  /// Try to add an owner with an account that shouldn't be allowed to
  run failToAddOwnerToSubcomAsAcmeOwnerArbiterTest =
    initScenario1
      // Have rando, who has no permissions in the space try to add themselves as an owner
      .then(sendMsgFromUser("rando", "b", "subcom", "$admin", XSetMemberAccess({ member: MemberUser("rando"), access: Owner }), 2))
      // Server B will handle the message, and realize that it must resolve the remote member list of acme to complete the action.
      .then(handleMsg("b"))
      // Server A will receive server B's request to resolve the $admin member list
      .then(handleMsg("a"))
      // Then server B will receive the reply and finish the action
      .then(handleMsg("b"))
      // And we will see that server throws an error
      .expect(choreo::s.extensions.serverError == Some({ node: "b", error: ErrArbiterErr(ErrPermissionDenied) }))

  /// Try to have both alice and bob try to add a member to a space with a remote delegation at the
  /// same time, and show that only one of the requests is accepted.
  ///
  /// TODO: this test makes me realize we aren't telling you when a particular job errors so the
  /// runtime isn't cleaning up the errored jobs.
  run twoOwnersAddAMemberToSpaceWithRemoteDelegationSimultaneouslyArbiterTest =
    initScenario1
      // Alice sends a request to add a new member
      .then(sendMsgFromUser("alice", "b", "subcom", "$admin", XSetMemberAccess({ member: MemberUser("newMember1"), access: AddMembers }), 2))
      // Server B handles it, but will start a job that needs to resolve the member list from server a
      .then(handleMsg("b"))
      // And Bob sends one immediately after for a different user
      .then(sendMsgFromUser("bob", "b", "subcom", "$admin", XSetMemberAccess({ member: MemberUser("newMember2"), access: RemoveMembers }), 2))
      // Server B handles it, but will start another job to handle the member list from server a
      .then(handleMsg("b"))
      // Server A handles both of the resolution requests and will send replies to server B. It's
      // nondeterministic which one it handles first.
      .then(handleMsg("a"))
      .then(handleMsg("a"))
      // Server B will handle one of the resolutions, which should succeed
      .then(handleMsg("b"))
      // At this point the arbiter should have successfully completed the 
      .expect(match(choreo::s.system.get("b").arbiters.get("subcom").result) {
          | ArbiterFinishedJob(job) => job.result == JobOk
          | _ => false
        })
      // Now server B handles the next reply, which will attempt to complete the second job.
      .then(handleMsg("b"))
      // But this will error because the state has been changed since the request was submitted by
      // the earlier request.
      .expect(choreo::s.system.get("b").arbiters.get("subcom").result == ArbiterErr(ErrPermissionChanged))
      // One of the new members should have been added to the arbiter
      .expect(
        def member1 = MemberUser("newMember1")
        def member2 = MemberUser("newMember2")
        def m = choreo::s.system.get("b").arbiters.get("subcom").spaces.get("$admin").members {
          (m.has(member1) and m.get(member1) == AddMembers)
            or
          (m.has(member2) and m.get(member2) == RemoveMembers)
        }
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

  /// Subcom: A sub-community of acme.
  ///
  /// The owners of acme are set as owners of subcom.
  pure val subcomArbiter: Arbiter = {
    version: 0,
    did: "subcom",
    jobQueue: Map(),
    result: ArbiterOk,
    spaces: Map(
      SpaceKey::ADMIN -> {
        config: { publicRecords: false, publicMembers: false },
        members: Map(
          MemberRemoteSpace({ arbiterDid: "acme", spaceKey: "$admin" }) -> Owner,
          MemberUser("subcomAdmin") -> Owner,
        )
      }
    )
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

