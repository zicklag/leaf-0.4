# Implementation Notes

## Phase 1 — Rework `server.rs`

### Status: IN PROGRESS

### Key insights from codebase:
- `core.rs` contains the pure `Arbiter` state machine — already implemented, solid, well-tested via MBT
- `server.rs` currently has an older design with `XrpcEndpoint`, `Request`, `Response`, `Feedback`, `ServerResult` — needs rewrite to match Quint `arbiter_server` pattern
- `service.rs` contains `ArbiterService` — used by MBT tests, not affected by server rewrite
- `lib.rs` re-exports everything from `core`, `server`, `service` via `pub use *`
- MBT tests use `ArbiterService` directly (not `Server`), so they should be unaffected by changes

### Design decisions:
1. **sans-IO design**: `Server::handle_message(&self, msg) -> (Server, Vec<ServerEffect>)` and `Server::tick(&self) -> (Server, Vec<ServerEffect>)` — both return new state + effects
2. **No DuplicateReqId check**: The Quint spec checks for duplicate ReqIds, but the PLAN removes this as a modeling artifact. In practice, the async wrapper allocates unique IDs via atomic counter.
3. **Simplified Respond**: The Quint spec sends `ReplyResolvedMembers` messages back to the source node. The PLAN simplifies to always emit `Respond` with the `JobResult`. The async wrapper's `process_effects` resolves the pending oneshot.
4. **createArbiter emits Respond**: The Quint spec only emits `UpdatePlc`, but the PLAN says emit `Respond` with success + `ArbiterChanged` — this is because in the sans-IO model we need the caller to know creation succeeded.
5. **FinishedJob always emits Respond**: In the Quint spec, `FinishedJob` with `JobOk` does NOT emit a response (it's just ok()). But the PLAN says emit `Respond` for all finished jobs. This is a design choice for the sans-IO model to always complete the future.

### Things to watch out for:
- `handleArbiterResult` in the spec uses `server.jobInfo.has(job.id)` to decide which message to reply to. If job info exists, reply to original message; otherwise reply to the trigger message. In the PLAN, this is simplified — we always reply to the trigger message (which IS the original message for queued jobs).
- The spec uses a choreography model with nodes and PLC directory — this is NOT being implemented in the sans-IO server; instead, `ServerEffect::SendMessage` is emitted and the async wrapper handles resolution.
- `Server::handleResolutionReply` in the spec looks up the job info, then gets the arbiter DID from the job info's original message. This means for a reply, the `arbiterDid` comes from job info, not the message itself.

### Implementation steps (Phase 1):
- [x] Understand existing codebase
- [ ] Rewrite `server.rs` with new types: `Message`, `MessageKind`, `ServerEffect`, `JobInfo`
- [ ] Update `lib.rs` re-exports (remove old types that no longer exist)
- [ ] Build with `cargo build -p arbiter-core`
- [ ] Run existing tests (`cargo test -p arbiter-core` — skip long-running MBT simulation)
- [ ] Write comprehensive unit tests for the new server
- [ ] Verify all tests pass
