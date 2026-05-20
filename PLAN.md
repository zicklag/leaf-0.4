# PLAN: Arbiter v2 — OPA-Powered Policy-Plugin Architecture

**Status:** In Progress
**Last updated:** 2026-05-20

## Overview

Build the next iteration of the Muni Town Arbiter — a sans-IO core state machine that
uses [Open Policy Agent](https://www.openpolicyagent.org/) (via the [`regorus`](https://crates.io/crates/regorus)
embedded Rego interpreter) to make authorization fully pluggable via Rego policies.

This decouples **"who is who"** (the hierarchical role/membership tree) from
**"what you can do"** (the authorization policy), enabling:
- A standardized interoperable API for group membership (the `town.muni.arbiter.*` XRPC lexicons)
- Custom policies for different governance models (democratic election, weighted voting, etc.)
- Interop with Habitat and other group-management systems at the membership layer

### The Two Deliverables

1. **`crates/arbiter-core2/`** — New sans-IO core state machine with embedded Regorus
   policy evaluation, matching the new lexicon spec.
2. **`arbiter-simulator/` (update)** — Update the existing Svelte-based browser simulator
   to use the new `arbiter-wasm2` (compiled from `arbiter-core2`) instead of the old
   hard-coded access-level engine.

A server crate (`arbiter-server2`) is deferred until after the core + simulator are solid.

## Architecture Decisions

### 1. Sans-IO Core Pattern (Same as v1)

The new core follows the same proven pattern as `arbiter-core`:
- **Pure state machine** — all mutations return new state + effects
- **No async, no IO** — remote member resolution is an effect emitted by the core,
  handled by the IO layer (simulator or async server)
- **WASM-compilable** — no tokio, no stdio, no filesystem deps

### 2. regorus for Rego Evaluation

`regorus` (pure Rust Rego interpreter, MIT/Apache 2.0, supports WASM) is embedded
directly in the core. No external OPA sidecar needed. The Rego policy is stored
in the arbiter config and evaluated on each authorization decision.

Key regorus features we use:
- `Regorus::set_input()` — provide the evaluation context (user, action, resource, member list)
- `Regorus::set_policy()` — load the policy from the arbiter config
- `Regorus::eval_bool("data.arbiter.allow")` — get the allow/deny decision
- Custom builtins via `Regorus::add_builtin()` — for remote member resolution helpers

### 3. Data Model (Spaces + Members)

We keep the "spaces" terminology from the lexicon (not renaming to "roles" — that's
just one semantic use of spaces).

```rust
struct Arbiter {
    did: String,
    version: u64,                    // Compare-and-swap concurrency control
    config: serde_json::Value,       // OpenUnion — contains $type + rego policy
    spaces: HashMap<String, Space>,
    job_queue: HashMap<JobId, Job>,
}

struct Space {
    space_type: String,              // NSID for the space config lexicon
    config: serde_json::Value,       // OpenUnion — publicRecords, publicMembers
    members: HashMap<Member, serde_json::Value>,  // Member → access config
}

struct Job {
    id: JobId,
    user_did: String,
    space_key: String,
    args: JobArgs,
    unresolved_members: UnresolvedMemberList,
    resolved_spaces: HashMap<SpaceId, ResolvedMemberList>,
    arbiter_version: u64,
}
```

### 4. Authorization via Rego Policy

The core asks the policy: "does user X have permission to perform action Y
on resource Z given the current member list?" The policy returns `allow: true/false`.

**Policy input shape:**
```json
{
  "requester": "did:plc:abc123",
  "action": "setMemberAccess",
  "resource": {
    "arbiterDid": "did:plc:xyz789",
    "spaceKey": "my-space"
  },
  "memberList": [
    {"did": "did:plc:abc123", "access": {"$type": "town.muni.arbiter.config.accessLevel", "level": "Owner"}},
    {"did": "did:plc:def456", "access": {"$type": "town.muni.arbiter.config.accessLevel", "level": "IsMember"}}
  ],
  "params": {
    // Action-specific parameters (e.g., target member, target access)
  }
}
```

**Actions evaluated by the policy:**
| Action | Description |
|--------|-------------|
| `fetchMembers` | Read the resolved member list |
| `createSpace` | Create a new space |
| `configureSpace` | Modify space config |
| `deleteSpace` | Delete a space |
| `addMember` | Add/set a member's access |
| `removeMember` | Remove a member |
| `deleteArbiter` | Delete the arbiter |
| `getMembers` | Get raw (unresolved) members |
| `getArbiterConfig` | Read arbiter config |
| `setArbiterConfig` | Modify arbiter config |
| `getSpaceConfig` | Read space config |

### 5. Config Lexicons

| NSID | Location | Contents |
|------|----------|----------|
| `town.muni.arbiter.config.regoPolicy` | Arbiter config | `{ "policy": "package arbiter\n\n...rego..." }` |
| `town.muni.arbiter.config.space` | Space config | `{ "publicRecords": bool, "publicMembers": bool }` |
| `town.muni.arbiter.config.accessLevel` | Member access | `{ "level": "Owner" }` (same string values as Quint spec) |

### 6. Concurrency Control

The Rust core handles compare-and-swap version checks (same as v1). The Rego
policy is **not** involved in concurrency — it only answers "is this action
allowed given the current state?"

### 7. OPA Default Policy

A Rego policy file at `policies/access-levels.rego` replicates the current
Quint access-level authorization:
- 8 access levels: ReadMemberList < IsMember < AddMembers < RemoveMembers <
  ConfigureSpace < CreateSpaces < RemoveSpace < Owner
- Can't grant higher access than you have
- Need RemoveMembers to modify existing members
- Need Owner to delete arbiter
- Can't modify members with higher access than yours
- Space creation requires CreateSpaces in resolved member list
- Read access checks (publicRecords, publicMembers)
- Only last owner can delete arbiter

### 8. Remote Member Resolution

Same pattern as v1:
- The core resolves members locally first
- If remote spaces need resolution, the job is queued with a `ResolveMembers` effect
- The IO layer (simulator/server) resolves remote spaces and feeds results back
- The Rego policy doesn't need custom builtins for remote resolution (option B
  from the discussion — resolution happens at the Rust/IO layer)

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│  arbiter-core2 (sans-IO state machine)                          │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  Core (pure state machine)                              │   │
│  │  - Stores: arbiters, spaces, members, jobs               │   │
│  │  - On auth-needed operations:                            │   │
│  │    1. Build input: user, action, resource, memberList   │   │
│  │    2. Call regorus::eval_bool("data.arbiter.allow")     │   │
│  │    3. If denied → ErrPermissionDenied                    │   │
│  │    4. If allowed → perform mutation, return effects      │   │
│  │  - Emits effects (ResolveMembers, JobComplete, etc.)     │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              ↕                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  regorus (embedded Rego)                                │   │
│  │  - Policy loaded from arbiter config at creation time   │   │
│  │  - Evaluated per authorization decision                 │   │
│  │  - May have custom builtins for arbiter-specific ops    │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                             ↕
┌─────────────────────────────┬──────────────────────────────────┐
│  arbiter-simulator          │  arbiter-server2 (future)        │
│  (WASM, browser)            │  (Salvo HTTP, async)             │
│                             │                                  │
│  - Svelte 5 SPA             │  - Salvo HTTP server             │
│  - WASM-compiled core2      │  - reqwest for remote resolution │
│  - Auto-resolve effects     │  - YAML persistence              │
│    in JS orchestrator       │  - JWT auth middleware           │
└─────────────────────────────┴──────────────────────────────────┘
```

## File Structure Plan

```
leaf-0.4/
├── Cargo.toml                          # Add arbiter-core2 workspace member
├── PLAN.md                             # This file
│
├── crates/
│   ├── arbiter-core/                   # Existing v1 (kept as reference)
│   ├── arbiter-server/                 # Existing v1 (kept as reference)
│   ├── arbiter-wasm/                   # Existing v1 (kept as reference)
│   │
│   ├── arbiter-core2/                  # NEW: Sans-IO core with OPA
│   │   ├── Cargo.toml
│   │   ├── build.rs                    # NEW: Compile default policy into binary
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── core.rs                 # Arbiter state machine (policy-aware)
│   │       ├── server.rs               # Multi-arbiter server state machine
│   │       ├── service.rs              # Service layer
│   │       ├── futures.rs              # Async wrapper (for future server)
│   │       └── policy.rs               # NEW: Rego policy evaluation wrapper
│   │
│   └── arbiter-wasm2/                  # NEW: WASM bindings for core2
│       ├── Cargo.toml
│       └── src/
│           └── lib.rs                  # ArbiterEngine (wasm-bindgen API)
│
├── policies/
│   └── access-levels.rego              # NEW: Default OPA policy
│
├── arbiter-simulator/                  # UPDATE existing simulator
│   ├── (existing files with updates to use arbiter-wasm2)
│   └── src/
│       ├── lib/
│       │   ├── simulator.ts            # UPDATE: Use new wasm engine
│       │   ├── types.ts                # UPDATE: New type definitions
│       │   └── simulation-store.ts     # UPDATE: New state shape
│       └── (components updated as needed)
│
└── typelex/
    └── main.tsp                         # Source for lexicons (unmodified)
```

## Implementation Phases

### Phase 1: Default OPA Policy
- Write `policies/access-levels.rego` — replicates Quint access-level authorization
- Rego policy should be self-contained and testable with `opa eval`

### Phase 2: arbiter-core2 — Sans-IO Core
- Create crate with regorus dependency
- Implement data model (Arbiter, Space, Member, Job)
- Implement policy evaluation wrapper (`policy.rs`)
- Implement core operations with regorus-driven authorization
- Sans-IO server state machine (message dispatch, effects, timeout)

### Phase 3: arbiter-wasm2 — WASM Bindings
- Create crate with core2 + wasm-bindgen
- Expose ArbiterEngine similar to v1 pattern
- Test WASM compilation with wasm-pack

### Phase 4: Simulator Update
- Update arbiter-simulator to use new wasm engine
- Update types, state, components
- Test interactive exploration

### Phase 5: arbiter-server2 (deferred)
- HTTP server wrapping core2 with Salvo
- Persistence, auth, etc.

## TODO Checklist

### Phase 1 ✅ Default OPA Policy
- [x] Write `policies/access-levels.rego` — replicates Quint access-level authorization
- [x] Validate policy syntax with `opa check`
- [x] Smoke-test with OPA eval (5 test cases passing)

### Phase 2 ✅ arbiter-core2 — Sans-IO Core
- [x] Create crate with workspace Cargo.toml
- [x] Add regorus + serde_json + im + anyhow dependencies
- [x] Define data model types (Arbiter, Space, Member, Job, Config)
- [x] Implement policy evaluation wrapper (`policy.rs`) with custom builtins
- [x] Rego policy uses lazy queries via `arbiter.get_space_members()` custom builtin
- [x] Policy evaluates `needs_resolution` separately from `allow`
- [x] Snapshot-based: PolicyEngine captures `Arc<im::HashMap>` at construction time
- [x] Write unit tests (12 passing)
- [x] Core state machine operations:
  - [x] Arbiter::new() with policy validation
  - [x] process_operation() — full policy check + execute flow
  - [x] provide_resolved_remotes() — re-evaluate after async resolution
  - [x] timeout_job() — handle unresolved remotes
  - [x] validate_operation() — structural pre-checks
  - [x] All JobArgs operations (CreateSpace, DeleteSpace, SetSpaceMemberAccess, etc.)

### Phase 3 ✅ arbiter-wasm2 — WASM Bindings
- [x] Create crate with core2 + wasm-bindgen
- [x] SimulationEngine wraps ServerState + Arbiter operations
- [x] All complex types cross boundary as JSON strings
- [x] Compiles for `wasm32-unknown-unknown` target
- [x] Supports: create_arbiter, process_operation, provide_resolved_remotes, get_state

### Phase 3 ⬜ arbiter-wasm2 — WASM Bindings
- [ ] Create crate with core2 + wasm-bindgen
- [ ] Expose ArbiterEngine API
- [ ] Test WASM compilation with wasm-pack

### Phase 4 ✅ Simulator Update
- [x] Update package.json to depend on arbiter-wasm2
- [x] Rewrite types.ts with local type definitions (no wasm imports)
- [x] Rewrite simulator.ts with direct process_operation API + auto-resolution loop
- [x] Rewrite simulation-store.svelte.ts with new state model
- [x] Update utils.ts for new member/access format
- [x] Update CreateArbiterBar, ActionPanel, DetailPanel, ArbiterActions
- [x] Update SpaceNode, Canvas, AccessLegend for new data format
- [x] Add type declaration file for arbiter-wasm2
- [x] svelte-check passes with 0 errors
- [x] Add Policy Editor tab: switch between Visual (graph) and Policy (code editor)
- [x] Policy editor with live validation via regorus (800ms debounce)
- [x] "Apply to All" button updates all arbiters' configs
- [x] Reset button restores compiled-in default policy
- [x] New arbiters automatically use current edited policy

### Phase 5 ⬜ arbiter-server2 (deferred)
- [ ] HTTP server wrapping core2
- [ ] YAML/JSON persistence
- [ ] Auth middleware
