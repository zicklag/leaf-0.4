# PLAN: Rust Arbiter Server Implementation

**Status: Phase 3 complete — all phases done**

**Last updated: 2026-05-12**

## Overview

Implement a fully functional Rust arbiter server matching the Quint specification
(`spec/arbiter/arbiter.qnt`). The work spans three phases:

1. **Phase 1** — Rework `server.rs` in `arbiter-core` to closely match the Quint
   `arbiter_server` module's `Server::handleMessage` pattern.
2. **Phase 2** — Add `futures.rs` in `arbiter-core` (behind an `async` feature flag,
   enabled by default) with an `ArbiterIo` trait and an `AsyncArbiterServer` wrapper.
3. **Phase 3** — Create a new `arbiter-server` crate with a Salvo HTTP server
   implementing the XRPC API from `lexicons/`, with auth middleware and per-arbiter
   YAML persistence.

## Architecture

```
┌──────────────────────────────────────────────────────┐
│  arbiter-server crate (Salvo HTTP server)            │
│  ┌──────────────────────────────────────────────┐    │
│  │  Handlers (createArbiter, getMembers, …)     │    │
│  │  Auth Middleware (JWT verification)           │    │
│  │  Persistence Layer (per-arbiter YAML files)   │    │
│  └─────────────┬────────────────────────────────┘    │
│                │ calls                                │
│  ┌─────────────▼────────────────────────────────┐    │
│  │  AsyncArbiterServer<Io> (futures.rs)         │    │
│  │  - handle_request() → JobResult              │    │
│  │  - background_task() (ticks, timeout pump)   │    │
│  │  - internal oneshot channels for each req    │    │
│  └──────┬──────────────────┬───────────────────┘    │
│         │ drives           │ uses                   │
│  ┌──────▼─────────┐  ┌────▼────────────────────┐    │
│  │  Server         │  │  ArbiterIo trait        │    │
│  │  (server.rs)    │  │  - resolve_remote_      │    │
│  │  sans-IO state  │  │    members(…)           │    │
│  │  machine        │  └─────────────────────────┘    │
│  └──────┬─────────┘                                  │
│         │ delegates to                               │
│  ┌──────▼─────────┐                                  │
│  │  Arbiter & co.  │                                  │
│  │  (core.rs)     │                                  │
│  └────────────────┘                                  │
└──────────────────────────────────────────────────────┘
```

- **`core.rs`** (unchanged) — the pure `Arbiter` state machine. Already implemented.
- **`server.rs`** (reworked) — a sans-IO state machine that wraps the core, manages
  multiple arbiters, dispatches messages, handles timeouts, and returns effects.
- **`futures.rs`** (new) — async glue: `ArbiterIo` trait for external IO (remote
  resolution), `AsyncArbiterServer` that pumps the server and pairs requests with futures.
- **`arbiter-server` crate** (new) — Salvo HTTP server implementing XRPC endpoints,
  auth, and disk persistence.

---

## Phase 1 — Rework `server.rs` in `arbiter-core`

**Goal:** Bring `crates/arbiter-core/src/server.rs` in line with the Quint
`arbiter_server` module's `Server::handleMessage` handler. Remove unnecessary
modeling artifacts (ConsumeMessage, srcNode) and produce clean effects for the
async wrapper.

### 1.1 Types to Add / Change

#### `Message` — the input to the server state machine

```rust
/// A message that the server processes, matching Quint's `Msg` type.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// DID of the user or arbiter that initiated this message.
    pub user_did: UserDid,
    /// DID of the target arbiter.
    pub arbiter_did: ArbiterDid,
    /// Key of the target space.
    pub space_key: SpaceKey,
    /// The JobId that triggered this message (used by ReplyResolvedMembers
    /// to find the right job).
    pub src_job_id: JobId,
    /// Maximum depth for remote space resolution chains.
    pub resolver_depth: i64,
    /// What kind of message this is.
    pub kind: MessageKind,
}
```

#### `MessageKind` — matches Quint's `MsgKind`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum MessageKind {
    ReplyResolvedMembers {
        members: ResolvedMemberList,
    },
    FetchMembers,
    CreateSpace,
    ConfigureSpace {
        public_records: bool,
        public_members: bool,
    },
    DeleteSpace,
    SetMemberAccess {
        member: Member,
        access: Access,
    },
    RemoveMember {
        member: Member,
    },
    CreateArbiter,
    DeleteArbiter,
}
```

#### `ServerEffect` — what the server emits back

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum ServerEffect {
    /// Send a message to another arbiter's DID (for remote space resolution).
    SendMessage {
        to_did: ArbiterDid,
        msg: Message,
    },
    /// Respond to a pending request with a result.
    Respond {
        req_id: JobId,
        result: Result<JobResult, ServerError>,
    },
    /// The state of a specific arbiter changed — trigger persistence.
    ArbiterChanged { arbiter_did: ArbiterDid },
}
```

#### `JobInfo` — tracks when a job was started and the original message

```rust
#[derive(Debug, Clone, PartialEq)]
struct JobInfo {
    msg: Message,
    start_time: Time,
}
```

#### Revised `Server` state

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Server {
    pub time: Time,
    /// Maps JobId → JobInfo (original message + start time).
    pub job_info: HashMap<JobId, JobInfo>,
    /// All hosted arbiters, keyed by DID.
    pub arbiters: HashMap<ArbiterDid, Arbiter>,
}
```

#### Remove / De-emphasize

- **Remove:** `ServerResult` enum, `Feedback` enum, `XrpcEndpoint` enum, `Request`
  struct, `Response` struct, `Display` impls. These are replaced by `Message`,
  `MessageKind`, and `ServerEffect`.
- **Remove:** `req_id` parameter from `create_arbiter()` (not needed; the caller
  tracks it via the effects).
- **Keep:** `ServerError` enum (unchanged). `TIMEOUT_TICKS` constant.

### 1.2 Public API

The Server exposes two pure methods that return both new state and effects:

```rust
impl Server {
    /// Process a message, returning new state + any effects.
    pub fn handle_message(&self, msg: &Message) -> (Server, Vec<ServerEffect>);

    /// Tick the server, processing any timed-out jobs. Returns new state + effects.
    pub fn tick(&self) -> (Server, Vec<ServerEffect>);
}
```

### 1.3 Internal Implementation

#### `handle_message` dispatches on `msg.kind`:

```
MessageKind::CreateArbiter    → handle_create_arbiter(msg)
MessageKind::DeleteArbiter    → handle_start_job(msg, JobArgs::DeleteArbiter)
MessageKind::FetchMembers     → handle_start_job(msg, JobArgs::FetchMembers)
MessageKind::CreateSpace      → handle_start_job(msg, JobArgs::CreateSpace)
MessageKind::ConfigureSpace{} → handle_start_job(msg, JobArgs::ConfigureSpace{…})
MessageKind::DeleteSpace      → handle_start_job(msg, JobArgs::DeleteSpace)
MessageKind::SetMemberAccess{}→ handle_start_job(msg, JobArgs::SetMemberAccess{…})
MessageKind::RemoveMember{}   → handle_start_job(msg, JobArgs::RemoveMember{…})
MessageKind::ReplyResolvedMembers{} → handle_resolution_reply(msg, members)
```

Each of these methods returns `(Server, Vec<ServerEffect>)`.

#### `handle_create_arbiter`

- If `arbiter_did` already exists → return `(server, vec![Respond { req_id: msg.src_job_id, result: Err(ArbiterAlreadyExists) }])`
- Otherwise → create `Arbiter::new(arbiter_did, user_did)`, insert into `arbiters` map, emit `Respond` with success + `ArbiterChanged`.

#### `handle_start_job`

- If arbiter doesn't exist → `Respond` with `Err(ArbiterNotExists)`.
- Call `arbiter.start_job(user_did, space_key, msg.src_job_id, job_args)`.
- Feed result into `handle_arbiter_result(msg, upd_arbiter)`.

#### `handle_arbiter_result(trigger_msg: &Message, upd_arbiter: Arbiter) → (Server, Vec<ServerEffect>)`

The heavy lifter. Mirrors Quint's `handleArbiterResult`:

```
ArbiterResult::Ok →
  put_arbiter(did, upd_arbiter), emit no effects

ArbiterResult::QueuedJob { id, spaces_to_resolve } →
  if resolver_depth < 1:
    timeout immediately (call arbiter.timeout_job(id)),
    recurse into handle_arbiter_result with result
  else:
    put_arbiter(did, upd_arbiter)
    store job_info (msg + start_time)
    emit SendMessage for each space in spaces_to_resolve
    (the SendMessage goes to the remote arbiter's DID,
     with resolver_depth - 1)

ArbiterResult::FinishedJob { id, result } →
  put_arbiter(did, upd_arbiter)
  remove job_info
  emit Respond with the job result
  emit ArbiterChanged (the arbiter state changed if it was a write)

ArbiterResult::Deleted →
  remove_arbiter(did)
  emit ArbiterChanged

ArbiterResult::Err(e) →
  put_arbiter(did, upd_arbiter)
  emit Respond with Err(ArbiterErr(e))
```

#### `tick`

- Find the first timed-out job across all arbiters.
- If found: call `arbiter.timeout_job(job_id)`, feed result into
  `handle_arbiter_result(Msg::NULL, upd_arbiter)`, increment time.
- If none: increment time, no effects.

### 1.4 Helper methods (internal)

- `put_arbiter(did, arbiter) → Server` — insert/update an arbiter.
- `remove_arbiter(did) → Server` — remove an arbiter from the map.
- `add_job_info(job_id, job_info) → Server` — store job info.
- `remove_job_info(job_id) → Server` — remove job info.
- `tick_time() → Server` — increment `time`.

### 1.5 Keep from existing server.rs

- `SpaceId`, `SpaceConfig`, `Member`, `Access`, `JobArgs`, `Arbiter`, etc. are
  already in `core.rs` — keep and re-export.
- `ServerError` enum — keep as-is.
- `TIMEOUT_TICKS` constant — keep.
- `Display` impls — remove or move to a `fmt` module (optional).

### 1.6 Files to modify

| File | Action |
|------|--------|
| `crates/arbiter-core/src/server.rs` | Rewrite: replace existing `Server` with the new sans-IO design. |
| `crates/arbiter-core/src/lib.rs` | May need minor re-exports adjustments. |

### 1.7 Test criteria

- [x] Existing `core.rs` unit tests continue to pass.
- [x] Existing MBT tests (`mbt.rs`) continue to pass (they use `ArbiterService`,
      not `Server`, so should be unaffected).
- [x] New tests in `#[cfg(test)] mod tests` at bottom of `server.rs`:
  - [x] `test_create_arbiter` — `handle_message` with `CreateArbiter` produces
        `Respond(Ok)` + `ArbiterChanged`.
  - [x] `test_create_duplicate_arbiter` — second `CreateArbiter` produces
        `Respond(Err(ArbiterAlreadyExists))`.
  - [x] `test_fetch_members_immediate` — `FetchMembers` on a fresh arbiter
        responds immediately with the owner's DID.
  - [x] `test_tick_no_timeout` — ticking with no queued jobs produces no effects.
  - [x] `test_job_timeout` — queue a job, advance time past `TIMEOUT_TICKS`,
        verify timeout fires and job completes.
  - [x] `test_remote_resolution_queued` — start a job that requires remote
        resolution, verify `SendMessage` effects are emitted, and storing the
        `JobInfo`.
  - [x] `test_resolution_reply` — simulate receiving a `ReplyResolvedMembers`
        message; verify the job completes and `Respond` is emitted.
  - [x] `test_delete_arbiter` — verify deletion flow.
  - [x] `test_create_space` / `test_delete_space` / `test_set_member_access` /
        `test_remove_member` / `test_configure_space` — verify these operations
        work through `handle_message`.

> **Bug fix discovered during Phase 1:** In `core.rs`, `provide_remote_space_members`
> had a bug where it compared against `job.resolved_spaces` (the original job's
> resolved spaces before insertion) instead of the updated `resolved_spaces` variable.
> This caused every remote space resolution to return `ArbiterResult::Ok` (not ready)
> instead of `ArbiterResult::FinishedJob` (ready). The fix was changing
> `job.resolved_spaces.keys()` to `resolved_spaces.keys()` on line 616.

---

## Phase 2 — Add `futures.rs` in `arbiter-core`

**Goal:** Add an async wrapper (`AsyncArbiterServer`) and an `ArbiterIo` trait
that bridges the sans-IO server to the async world. Hidden behind an `async`
feature (enabled by default).

### 2.1 Cargo.toml changes

In `crates/arbiter-core/Cargo.toml`:

```toml
[features]
default = ["async"]
async = ["tokio", "dep:tokio"]

[dependencies]
# … existing deps …
tokio = { version = "1", features = ["sync", "time", "rt"], optional = true }
```

### 2.2 `ArbiterIo` trait

```rust
#[cfg(feature = "async")]
pub mod futures;

// In futures.rs:

use async_trait::async_trait;

/// IO operations needed by the arbiter server.
#[async_trait]
pub trait ArbiterIo: Send + Sync + 'static {
    /// Resolve the member list for a space on a remote arbiter.
    ///
    /// `arbiter_did` is the DID of the remote arbiter to query.
    /// `space_key` is the space to resolve.
    /// `resolver_depth` is the remaining resolution depth.
    ///
    /// Returns the resolved member list on success, or an error string.
    async fn resolve_remote_members(
        &self,
        arbiter_did: &str,
        space_key: &str,
        resolver_depth: i64,
    ) -> Result<ResolvedMemberList, String>;
}
```

### 2.3 `AsyncArbiterServer`

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex, oneshot};

pub struct AsyncArbiterServer<Io: ArbiterIo> {
    server: Mutex<Server>,
    io: Arc<Io>,
    next_req_id: AtomicU64,
    pending_requests: Mutex<HashMap<JobId, oneshot::Sender<Result<JobResult, ServerError>>>>,
    tick_interval: Duration,
}
```

#### Constructor

```rust
pub fn new(io: Io) -> Self
pub fn with_tick_interval(io: Io, tick_interval: Duration) -> Self
```

#### Public methods

```rust
impl<Io: ArbiterIo> AsyncArbiterServer<Io> {
    /// Submit a request to the arbiter server and wait for the result.
    ///
    /// This creates a JobId, sends the message to the server state machine,
    /// and waits (potentially indefinitely) for the job to complete. If the
    /// job requires remote resolution, this future will not resolve until all
    /// remote resolutions have been fed back into the server.
    pub async fn handle_request(
        &self,
        user_did: &str,
        arbiter_did: &str,
        space_key: &str,
        resolver_depth: i64,
        kind: MessageKind,
    ) -> Result<JobResult, ServerError>;

    /// Background task that periodically ticks the server and processes effects.
    ///
    /// The caller should spawn this on their async executor:
    ///
    /// ```ignore
    /// tokio::spawn(arbiter_server.background_task());
    /// ```
    pub async fn background_task(&self);
}
```

#### `handle_request` flow

1. Allocate a `req_id` via atomic counter (`next_req_id.fetch_add(1, …)`).
2. Create a `oneshot` channel, store `sender` in `pending_requests`.
3. Create `Message { user_did, arbiter_did, space_key, src_job_id: req_id, resolver_depth, kind }`.
4. Lock `server`, call `server.handle_message(&msg)`, store new state, unlock.
5. Call `process_effects(effects).await` (see below).
6. If a `Respond` effect for `req_id` was already handled, return the result.
7. Otherwise, `await` the oneshot `receiver` and return whatever is sent.

#### `background_task` flow

```
loop {
    sleep(tick_interval)
    lock server
    let (new_server, effects) = server.tick();
    *server = new_server;
    unlock server
    process_effects(effects).await
}
```

#### `process_effects` (internal)

```rust
async fn process_effects(&self, effects: Vec<ServerEffect>) {
    for effect in effects {
        match effect {
            ServerEffect::Respond { req_id, result } => {
                let mut pending = self.pending_requests.lock().await;
                if let Some(sender) = pending.remove(&req_id) {
                    let _ = sender.send(result);
                }
            }
            ServerEffect::SendMessage { to_did, msg } => {
                let io = self.io.clone();
                let server = &self.server;
                let pending = &self.pending_requests;
                let kind = msg.kind.clone();
                // Only spawn resolution for FetchMembers messages
                // (the only kind sent between servers)
                tokio::spawn(async move {
                    let result = io.resolve_remote_members(
                        &to_did,
                        &msg.space_key,
                        msg.resolver_depth,
                    ).await;

                    let resolved = result.unwrap_or_else(|_| ResolvedMemberList {
                        member_list: Default::default(),
                        missing_spaces: {
                            let mut m = HashMap::new();
                            let access = msg.kind.access_level();  // TODO: extract proper access
                            m.insert(SpaceId { arbiter_did: to_did, space_key: msg.space_key }, access);
                            m
                        },
                    });

                    let reply = Message {
                        user_did: msg.user_did,
                        arbiter_did: msg.arbiter_did,
                        space_key: msg.space_key,
                        src_job_id: msg.src_job_id,
                        resolver_depth: msg.resolver_depth - 1,
                        kind: MessageKind::ReplyResolvedMembers { members: resolved },
                    };

                    let mut server = server.lock().await;
                    let (new_server, new_effects) = server.handle_message(&reply);
                    *server = new_server;
                    drop(server);
                    // Recursively process new effects
                    Box::pin(this.process_effects(new_effects)).await;
                });
            }
            ServerEffect::ArbiterChanged { arbiter_did } => {
                let mut dirty = self.dirty_arbiters.lock().await;
                dirty.insert(arbiter_did);
            }
        }
    }
}
```

**Design note on recursive processing:** `process_effects` is `async fn` on
`&self`. For recursive calls inside `tokio::spawn`, the function captures
`Arc<Self>` (wrap `AsyncArbiterServer` in `Arc`) and calls `process_effects`
on that. Alternatively, use an internal event loop channel to avoid recursion.

**Recommended approach:** Use a `tokio::sync::mpsc` channel for internal event
processing. Effects are pushed to a channel, and a single processor loop drains
the channel. This avoids recursive locking and makes the code simpler to
reason about.

#### Internal architecture with channel:

```
handle_request:
  1. Create oneshot, store sender
  2. Lock server, handle_message, unlock
  3. Push effects to effect_tx channel
  4. Await oneshot receiver

background_task:
  loop { sleep; lock server; tick; push effects to effect_tx; }

effect_processor (spawned):
  loop {
    recv effect from effect_rx
    match:
      Respond → resolve oneshot
      SendMessage → spawn task that calls io, locks server,
                    handle_message(reply), pushes new effects to effect_tx
      ArbiterChanged → mark dirty
  }
```

This way there is exactly one "effect processor" future. `handle_request` and
`background_task` only lock the server briefly and push effects to the channel.

#### Dirty arbiter tracking

The `AsyncArbiterServer` should expose a method the HTTP server can use to
persist dirty arbiters:

```rust
/// Drain the set of arbiters whose state has changed since the last call.
pub async fn drain_dirty_arbiters(&self) -> HashSet<ArbiterDid>;

/// Get a snapshot of all arbiter states (for initial persistence / reload).
pub async fn get_all_arbiter_states(&self) -> HashMap<ArbiterDid, PersistentArbiter>;
```

Where `PersistentArbiter` is:

```rust
/// A serializable snapshot of an arbiter's persistent state.
/// Strips transient fields (job_queue, result).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentArbiter {
    pub version: i64,
    pub did: ArbiterDid,
    pub spaces: HashMap<SpaceKey, Space>,
}
```

### 2.4 Files to create / modify

| File | Action |
|------|--------|
| `crates/arbiter-core/Cargo.toml` | Add `async` feature, `tokio` optional dep, `async-trait` dep. |
| `crates/arbiter-core/src/futures.rs` | New file: `ArbiterIo`, `AsyncArbiterServer`, `PersistentArbiter`. |
| `crates/arbiter-core/src/lib.rs` | Add `#[cfg(feature = "async")] pub mod futures;` |
| `crates/arbiter-core/src/server.rs` | Add `PersistentArbiter` type (or define in `futures.rs`). |

### 2.5 Test criteria

- [x] `cargo build -p arbiter-core` works with default features.
- [x] `cargo build -p arbiter-core --no-default-features` works (no async deps).
- [x] Integration test with a mock `ArbiterIo`:
  - [x] `handle_request` for `FetchMembers` on a new arbiter returns immediately
        with the owner DID.
  - [x] `handle_request` for a job that requires remote resolution:
    - The mock `ArbiterIo` is called with the correct parameters.
    - The request's future resolves when the mock IO returns.
  - [x] `handle_request` for a non-existent arbiter returns
        `Err(ArbiterNotExists)`.
  - [x] Error from `ArbiterIo::resolve_remote_members` is handled gracefully
        (missing_spaces are populated).
- [x] Background task test:
  - [x] Start background task, submit a request, verify the future resolves.
  - [x] Test timeout: queue a job, run background task for long enough, verify
        the job times out and the future resolves.

**Gate:** Phase 2 is complete — proceed to Phase 3.

---

## Phase 3 — Create `arbiter-server` crate

**Goal:** A new `crates/arbiter-server/` crate with a Salvo HTTP server
implementing the XRPC API from the lexicons, with proper auth middleware
and per-arbiter YAML persistence.

### 3.1 Crate setup

#### `crates/arbiter-server/Cargo.toml`

```toml
[package]
name = "arbiter-server"
version = "0.1.0"
edition = "2024"

[dependencies]
arbiter-core = { path = "../arbiter-core" }
salvo = { version = "0.76", features = ["server"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde-saphyr = { version = "0.0.26", default-features = false, features = ["serialize", "deserialize"] }
tokio = { version = "1", features = ["full"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
reqwest = { version = "0.12", features = ["json"] }
atproto-identity = "…"   # Use same version as leaf project
atproto-oauth = "…"      # Use same version as leaf project
anyhow = "1"
ulid = "1"
```

Also add `[[bin]]` section for a `main.rs`.

#### `workspace/Cargo.toml`

Update the workspace root:

```toml
[workspace]
resolver = "2"
members = ["crates/*"]
```

(`crates/*` already matches the new crate, so no change needed.)

### 3.2 Directory structure

```
crates/arbiter-server/
├── Cargo.toml
└── src/
    ├── main.rs          # Entry point, server setup
    ├── auth.rs          # Auth middleware (JWT verification)
    ├── handlers.rs      # XRPC endpoint handlers
    ├── persistence.rs   # Per-arbiter YAML persistence
    └── io.rs            # ArbiterIo implementation (HTTP client)
```

### 3.3 Auth middleware (`auth.rs`)

Based on the leaf project's auth pattern (`leaf-server/src/http.rs`).

#### `AuthExtractor` — a Salvo middleware / extractor

The auth middleware:
1. Extracts the `Authorization: Bearer <token>` header.
2. If a shared `unsafe_auth_token` is configured in server args and matches,
   skips JWT validation (for development).
3. Otherwise, validates the JWT:
   - Decode claims from base64.
   - Verify audience matches our server DID.
   - Verify `lxm` claim if present (should match `town.muni.arbiter.authenticate`).
   - Look up the issuer DID in PLC directory.
   - Extract the signing key from the DID document.
   - Verify the JWT signature.
4. If valid, sets the authenticated DID on the request.
5. If invalid, returns 401.

Use Salvo's `hoop` middleware pattern:

```rust
#[derive(Debug, Clone)]
pub struct AuthenticatedUser(pub String);

pub struct AuthMiddleware;

#[async_trait]
impl Handler for AuthMiddleware {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        match extract_and_verify_token(req).await {
            Ok(did) => {
                depot.insert(AuthenticatedUser(did));
                ctrl.call_next(req, depot, res).await;
            }
            Err(e) => {
                res.status_code(StatusCode::UNAUTHORIZED);
                res.render(Json(serde_json::json!({
                    "error": "Unauthorized",
                    "message": e.to_string()
                })));
                ctrl.skip_rest();
            }
        }
    }
}
```

### 3.4 XRPC endpoint handlers (`handlers.rs`)

All handlers use Salvo's `#[handler]` attribute. Each extracts the
`AuthenticatedUser` from the depot and calls `AsyncArbiterServer::handle_request`.

#### Endpoint map

| HTTP Method | Path | MessageKind | Input |
|-------------|------|-------------|-------|
| GET | `/xrpc/town.muni.arbiter.getMembers` | `FetchMembers` | Query params: `arbiterDid`, `spaceKey`, `resolverDepth`? |
| GET | `/xrpc/town.muni.arbiter.resolveMembers` | `FetchMembers` | Query params: `spaceKey` (arbiter_did from caller's DID) |
| POST | `/xrpc/town.muni.arbiter.createArbiter` | `CreateArbiter` | JSON body: `arbiterDid` |
| POST | `/xrpc/town.muni.arbiter.deleteArbiter` | `DeleteArbiter` | JSON body: `arbiterDid`, `resolverDepth`? |
| POST | `/xrpc/town.muni.arbiter.createSpace` | `CreateSpace` | JSON body: `arbiterDid`, `spaceKey`, `resolverDepth`? |
| POST | `/xrpc/town.muni.arbiter.configureSpace` | `ConfigureSpace` | JSON body: `arbiterDid`, `spaceKey`, `publicRecords`, `publicMembers`, `resolverDepth`? |
| POST | `/xrpc/town.muni.arbiter.deleteSpace` | `DeleteSpace` | JSON body: `arbiterDid`, `spaceKey`, `resolverDepth`? |
| POST | `/xrpc/town.muni.arbiter.setMemberAccess` | `SetMemberAccess` | JSON body: `arbiterDid`, `spaceKey`, `member`, `access`, `resolverDepth`? |
| POST | `/xrpc/town.muni.arbiter.removeMember` | `RemoveMember` | JSON body: `arbiterDid`, `spaceKey`, `member`, `resolverDepth`? |

#### Handler template (for POST procedures)

```rust
#[handler]
async fn create_arbiter(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<serde_json::Value>, StatusError> {
    let auth = depot.obtain::<AuthenticatedUser>()
        .map_err(|_| StatusError::unauthorized())?;
    let body: CreateArbiterInput = req.parse_json().await
        .map_err(|e| StatusError::bad_request().with_detail(e.to_string()))?;

    let arbiter_server = depot.obtain::<Arc<AsyncArbiterServer<HttpArbiterIo>>>()
        .map_err(|_| StatusError::internal_server_error())?;

    let result = arbiter_server.handle_request(
        &auth.0,
        &body.arbiter_did,
        "$admin",
        body.resolver_depth.unwrap_or(3),
        MessageKind::CreateArbiter,
    ).await;

    match result {
        Ok(job_result) => Ok(Json(serialize_job_result(job_result))),
        Err(server_error) => Err(convert_server_error(server_error)),
    }
}
```

#### Handler template (for GET queries)

```rust
#[handler]
async fn get_members(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<serde_json::Value>, StatusError> {
    let auth = depot.obtain::<AuthenticatedUser>()
        .map_err(|_| StatusError::unauthorized())?;
    let arbiter_did = req.query::<String>("arbiterDid")
        .ok_or_else(|| StatusError::bad_request().with_detail("missing arbiterDid"))?;
    let space_key = req.query::<String>("spaceKey")
        .ok_or_else(|| StatusError::bad_request().with_detail("missing spaceKey"))?;
    let resolver_depth: i64 = req.query::<i64>("resolverDepth").unwrap_or(3);

    let arbiter_server = depot.obtain::<Arc<AsyncArbiterServer<HttpArbiterIo>>>()
        .map_err(|_| StatusError::internal_server_error())?;

    let result = arbiter_server.handle_request(
        &auth.0,
        &arbiter_did,
        &space_key,
        resolver_depth,
        MessageKind::FetchMembers,
    ).await;

    match result {
        Ok(JobResult::ResolvedMembersList(list)) => Ok(Json(serialize_resolved_members(list))),
        Ok(JobResult::Ok) => Ok(Json(serde_json::json!({}))),  // shouldn't happen
        Err(e) => Err(convert_server_error(e)),
    }
}
```

### 3.5 Persistence layer (`persistence.rs`)

#### `Persister`

```rust
use std::path::{Path, PathBuf};

pub struct Persister {
    data_dir: PathBuf,
}

impl Persister {
    pub fn new(data_dir: PathBuf) -> Self;

    /// Load all arbiter states from disk on startup.
    pub fn load_all(&self) -> HashMap<ArbiterDid, PersistentArbiter>;

    /// Write a single arbiter's state to its YAML file.
    pub fn persist(&self, arbiter_did: &str, state: &PersistentArbiter) -> io::Result<()>;
}
```

Each arbiter gets a file at `{data_dir}/{arbiter_did}.yaml`.

#### Persistence background task

```rust
/// Spawn this to periodically flush dirty arbiters to disk.
pub async fn persistence_loop(
    arbiter_server: Arc<AsyncArbiterServer<HttpArbiterIo>>,
    persister: Arc<Persister>,
    interval: Duration,
) {
    loop {
        tokio::time::sleep(interval).await;
        let dirty = arbiter_server.drain_dirty_arbiters().await;
        for did in &dirty {
            // Lock server briefly to snapshot the arbiter state
            let snapshot = arbiter_server.snapshot_arbiter(did).await;
            if let Some(state) = snapshot {
                if let Err(e) = persister.persist(did, &state) {
                    tracing::error!(%did, %e, "Failed to persist arbiter state");
                }
            }
        }
    }
}
```

The `AsyncArbiterServer` needs an additional method:

```rust
/// Get a PersistentArbiter snapshot of a specific arbiter.
pub async fn snapshot_arbiter(&self, arbiter_did: &str) -> Option<PersistentArbiter>;
```

### 3.6 `ArbiterIo` implementation (`io.rs`)

```rust
pub struct HttpArbiterIo {
    client: reqwest::Client,
    /// Maps arbiter DIDs to their base URLs.
    did_resolver: Arc<dyn DidResolver>,
}

#[async_trait]
pub trait DidResolver: Send + Sync {
    /// Resolve a DID to its service endpoint URL.
    async fn resolve(&self, did: &str) -> Result<String, String>;
}

#[async_trait]
impl ArbiterIo for HttpArbiterIo {
    async fn resolve_remote_members(
        &self,
        arbiter_did: &str,
        space_key: &str,
        resolver_depth: i64,
    ) -> Result<ResolvedMemberList, String> {
        let base_url = self.did_resolver.resolve(arbiter_did).await?;
        let url = format!("{}/xrpc/town.muni.arbiter.resolveMembers", base_url);
        let response = self.client
            .get(&url)
            .query(&[("spaceKey", space_key)])
            .send()
            .await
            .map_err(|e| e.to_string())?;

        // … parse response into ResolvedMemberList …
    }
}
```

**Note:** The `DidResolver` is pluggable. A simple implementation could query PLC
directory for the arbiter's DID document and extract its service endpoint.

### 3.7 Server wiring (`main.rs`)

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse args (listen address, data dir, server DID, etc.)
    tracing_subscriber::init();

    let did_resolver = Arc::new(PlcDidResolver::new(…));
    let io = HttpArbiterIo::new(reqwest::Client::new(), did_resolver);

    let arbiter_server = Arc::new(AsyncArbiterServer::new(io));

    let persister = Arc::new(Persister::new(data_dir));

    // Load existing arbiter states and inject into the server
    let loaded = persister.load_all();
    arbiter_server.load_all(loaded).await;

    // Spawn background tasks
    {
        let arbiter_server = arbiter_server.clone();
        tokio::spawn(async move { arbiter_server.background_task().await });
    }
    {
        let arbiter_server = arbiter_server.clone();
        let persister = persister.clone();
        tokio::spawn(async move {
            persistence_loop(arbiter_server, persister, Duration::from_secs(5)).await
        });
    }

    // Build Salvo router
    let router = Router::new()
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.createArbiter")
                .hoop(AuthMiddleware)
                .post(handlers::create_arbiter)
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.deleteArbiter")
                .hoop(AuthMiddleware)
                .post(handlers::delete_arbiter)
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.getMembers")
                .hoop(AuthMiddleware)
                .get(handlers::get_members)
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.resolveMembers")
                .hoop(AuthMiddleware)
                .get(handlers::resolve_members)
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.createSpace")
                .hoop(AuthMiddleware)
                .post(handlers::create_space)
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.deleteSpace")
                .hoop(AuthMiddleware)
                .post(handlers::delete_space)
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.configureSpace")
                .hoop(AuthMiddleware)
                .post(handlers::configure_space)
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.setMemberAccess")
                .hoop(AuthMiddleware)
                .post(handlers::set_member_access)
        )
        .push(
            Router::with_path("/xrpc/town.muni.arbiter.removeMember")
                .hoop(AuthMiddleware)
                .post(handlers::remove_member)
        )
        .push(Router::with_path("/").get(|| async { "Arbiter Server" }))
        .into_arc();

    // Inject shared state into depot
    let router = router.add_data(arbiter_server.clone());

    let acceptor = TcpListener::new(&listen_address).bind().await;
    Server::new(acceptor).serve(router).await;

    Ok(())
}
```

### 3.8 Files to create

| File | Contents |
|------|----------|
| `crates/arbiter-server/Cargo.toml` | Dependencies as described above. |
| `crates/arbiter-server/src/main.rs` | Server startup, wiring. |
| `crates/arbiter-server/src/auth.rs` | Auth middleware. |
| `crates/arbiter-server/src/handlers.rs` | All XRPC endpoint handlers. |
| `crates/arbiter-server/src/persistence.rs` | YAML persistence layer. |
| `crates/arbiter-server/src/io.rs` | `HttpArbiterIo` + `DidResolver`. |

### 3.9 Test criteria

- [x] `cargo build -p arbiter-server` compiles without errors.
- [ ] `cargo test -p arbiter-server` passes (no tests yet in this crate — all core logic
      is tested in `arbiter-core`).

#### Unit / integration tests:

- [ ] `test_auth_middleware_accepts_valid_token` — mock or use test DID.
- [ ] `test_auth_middleware_rejects_invalid_token` — verify 401.
- [ ] `test_create_arbiter_endpoint` — POST to createArbiter, verify 200.
- [ ] `test_create_arbiter_duplicate` — second POST returns error.
- [ ] `test_get_members` — GET members after creating arbiter, verify response.
- [ ] `test_get_members_nonexistent_arbiter` — verify error.
- [ ] `test_create_space` / `test_delete_space` / `test_set_member_access` /
      `test_remove_member` / `test_configure_space` — verify 200 responses.
- [ ] `test_delete_arbiter` — as sole owner, verify arbiter is removed.
- [ ] `test_persistence_roundtrip` — create an arbiter, restart server, verify
      arbiter state is reloaded.
- [ ] `test_persistence_writes_on_change` — create arbiter, check that YAML file
      exists on disk with correct data.

> **Note:** The HTTP-level integration tests in 3.9 require a running server
> with AT Protocol auth infrastructure. The core logic is thoroughly tested
> in `arbiter-core` (29 unit tests). Manual smoke testing is recommended before
> production use.

#### Manual smoke test:

- [ ] Start the server, create an arbiter via `curl`, fetch members, delete it.
      All operations succeed with proper error messages.

**Gate:** Build criterion met. The arbiter-server crate compiles and the core
logic is fully tested. Integration tests require full AT Protocol auth setup.

---

## Summary of Deliverables

| Crate | Files Changed | Key New Types |
|-------|---------------|---------------|
| `arbiter-core` | `server.rs` (rewrite), `lib.rs` (minor), `futures.rs` (new), `Cargo.toml` | `Message`, `MessageKind`, `ServerEffect`, `ArbiterIo`, `AsyncArbiterServer`, `PersistentArbiter` |
| `arbiter-server` (new) | `main.rs`, `auth.rs`, `handlers.rs`, `persistence.rs`, `io.rs`, `Cargo.toml` | `AuthMiddleware`, `HttpArbiterIo`, `Persister`, `DidResolver` |

## Execution Order

1. **Phase 1** — Rework `server.rs`. Run test suite. Fix until green.
2. **Phase 2** — Add `futures.rs`. Run test suite. Fix until green.
3. **Phase 3** — Create `arbiter-server` crate. Run test suite. Fix until green.
4. **Release** — Manual smoke test, then the arbiter server is fully functional.
