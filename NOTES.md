# Implementation Notes

## Phase 1 — Rework `server.rs` ✅ COMPLETE

### Summary
Rewrote `crates/arbiter-core/src/server.rs` to be a sans-IO state machine matching the
Quint `arbiter_server` pattern. 20 unit tests pass.

### Key changes:
- **Removed**: `XrpcEndpoint`, `Request`, `Response`, `Feedback`, `ServerResult`, `Display` impls
- **Added**: `Message`, `MessageKind`, `ServerEffect`, `JobInfo`, `PersistentArbiter`
- **Added methods**: `Server::handle_message()` and `Server::tick()` returning `(Server, Vec<ServerEffect>)`
- **Added persistence**: `persistent_arbiter()`, `all_persistent_arbiters()`, `load_persistent_arbiter()`
- **Error types**: Used `thiserror` for `ServerError` and `ArbiterError`
- **lib.rs**: Updated re-exports

### Bug fix discovered:
**`core.rs` line 616**: `provide_remote_space_members` compared against `job.resolved_spaces` (before insertion) instead of `resolved_spaces` (after insertion). Fixed changing `job.resolved_spaces.keys()` → `resolved_spaces.keys()`.

### Files changed:
- `crates/arbiter-core/src/server.rs` — rewritten
- `crates/arbiter-core/src/core.rs` — bug fix + thiserror
- `crates/arbiter-core/Cargo.toml` — added `thiserror`

---

## Phase 2 — Add `futures.rs` ✅ COMPLETE

### Summary
Added `async` feature with `ArbiterIo` trait and `AsyncArbiterServer`. 9 async tests pass.

### Key additions:
- `ArbiterIo` trait with `resolve_remote_members()`
- `AsyncArbiterServer<Io>` with effect processor using `mpsc` channel
- `drain_dirty_arbiters()`, `snapshot_arbiter()`, `load_all()` for persistence
- Built with `cargo build --no-default-features` and with default features

### Files changed:
- `crates/arbiter-core/Cargo.toml` — added `async` feature, `tokio`, `async-trait`
- `crates/arbiter-core/src/futures.rs` — new file
- `crates/arbiter-core/src/lib.rs` — feature-gated module

---

## Phase 3 — Create `arbiter-server` crate ✅ COMPLETE

### Summary
Created a new `crates/arbiter-server/` crate with Salvo HTTP server implementing
the XRPC API from the lexicons, with auth middleware and per-arbiter YAML persistence.

### Files created:
| File | Contents |
|------|----------|
| `Cargo.toml` | Dependencies: arbiter-core, salvo 0.76, serde, serde_yaml, reqwest, etc. |
| `src/main.rs` | Server startup, wiring, persistence loop, background task |
| `src/auth.rs` | Auth middleware (dev token bypass + JWT stub) |
| `src/handlers.rs` | All 9 XRPC endpoint handlers |
| `src/persistence.rs` | Per-arbiter YAML file persistence |
| `src/io.rs` | `HttpArbiterIo` + `PlcDidResolver` |

### XRPC Endpoints:
| Method | Path | Handler |
|--------|------|---------|
| POST | `/xrpc/town.muni.arbiter.createArbiter` | `create_arbiter` |
| POST | `/xrpc/town.muni.arbiter.deleteArbiter` | `delete_arbiter` |
| GET | `/xrpc/town.muni.arbiter.getMembers` | `get_members` |
| GET | `/xrpc/town.muni.arbiter.resolveMembers` | `resolve_members` (internal) |
| POST | `/xrpc/town.muni.arbiter.createSpace` | `create_space` |
| POST | `/xrpc/town.muni.arbiter.deleteSpace` | `delete_space` |
| POST | `/xrpc/town.muni.arbiter.configureSpace` | `configure_space` |
| POST | `/xrpc/town.muni.arbiter.setMemberAccess` | `set_member_access` |
| POST | `/xrpc/town.muni.arbiter.removeMember` | `remove_member` |

### Architecture:
```
main.rs
  ├── Load persisted states from disk (YAML)
  ├── Spawn background tick task (periodic Server::tick)
  ├── Spawn persistence loop (flush dirty arbiters every 5s)
  └── Start Salvo HTTP server
        ├── AuthMiddleware (JWT / unsafe token)
        ├── ServerDataMiddleware (injects Arc<AsyncArbiterServer>)
        └── 9 XRPC endpoint handlers
```

### Configuration (environment variables):
- `SERVER_DID` — (required) DID of this server instance
- `LISTEN` — address to bind (default: `0.0.0.0:8080`)
- `DATA_DIR` — directory for YAML state files (default: `./data/arbiters`)
- `PLC_URL` — PLC directory base URL (default: `https://plc.directory`)
- `UNSAFE_AUTH_TOKEN` — dev mode token to bypass JWT (optional)
- `PERSIST_INTERVAL` — seconds between persistence flushes (default: 5)
- `TICK_INTERVAL_MS` — milliseconds between server ticks (default: 100)

### Salvo 0.76 API notes:
- `Depot::get::<V>(key)` returns `Result<&V, Option<&Box<dyn Any>>>`, NOT `Option<&V>`
- `StatusError` uses `.detail()` not `.with_detail()`
- `res.render(Json(...))` uses `salvo::writing::Json`, not `salvo::Json`
- `Router` uses `.hoop(middleware)` for middleware, `.add_data()` doesn't exist
- Server state is shared via a middleware that inserts `Arc<AsyncArbiterServer>` into the depot
