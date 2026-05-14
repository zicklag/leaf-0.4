# PLAN: Arbiter Web Simulator

**Status:** Planning Phase
**Last updated:** 2026-05-13

## Overview

Build a full interactive web simulator for the arbiter authorization system. Users
will be able to create virtual user accounts, create arbiters and spaces, set up
delegation chains between spaces, and see the resulting resolved member lists and
access levels computed by the real arbiter state machine — all running in the browser
via WebAssembly, with all IO simulated inside the Svelte SPA.

The project has two deliverables:

1. **`crates/arbiter-wasm/`** — A Rust crate that compiles the sans-IO `Server` state
   machine from `arbiter-core` to WebAssembly, exposing a JSON-based API for the JS
   side to drive.
2. **`arbiter-simulator/`** — A Svelte 5 SPA (Vite + TypeScript) that imports the wasm
   module and provides the interactive UI: sidebar for user/arbiter management, a
   graph canvas showing the arbiter/space/delegation structure, and a detail panel
   showing computed member lists and access levels.

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│  arbiter-simulator/ (Svelte 5 SPA)                           │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐    │
│  │  App.svelte (layout: sidebar | canvas | detail panel) │    │
│  │                                                       │    │
│  │  ┌──────────┐  ┌──────────────┐  ┌───────────────┐  │    │
│  │  │ Sidebar  │  │  Canvas      │  │  Detail Panel │  │    │
│  │  │ - users  │  │  - arbiter   │  │  - member     │  │    │
│  │  │ - create │  │    nodes     │  │    list       │  │    │
│  │  │ - action │  │  - space     │  │  - access     │  │    │
│  │  │   panel  │  │    nodes     │  │    levels     │  │    │
│  │  │          │  │  - delegation│  │  - config     │  │    │
│  │  │          │  │    edges     │  │               │  │    │
│  │  └──────────┘  └──────────────┘  └───────────────┘  │    │
│  └──────────────────────────────────────────────────────┘    │
│                           ↕ calls                            │
│  ┌──────────────────────────────────────────────────────┐    │
│  │  simulator.ts — Simulation orchestrator              │    │
│  │  - manages the wasm engine                           │    │
│  │  - routes SendMessage effects back as                │    │
│  │    ReplyResolvedMembers (auto-resolve)               │    │
│  │  - manages job ID allocation                         │    │
│  │  - provides a clean async dispatch() API             │    │
│  └──────────────────────┬───────────────────────────────┘    │
│                         ↕ calls JSON                        │
│  ┌──────────────────────▼───────────────────────────────┐    │
│  │  arbiter_engine_bg.wasm                              │    │
│  │  (compiled from crates/arbiter-wasm/src/lib.rs)      │    │
│  │                                                      │    │
│  │  Exposes:                                            │    │
│  │  - handleMessage(msg_json) → effects_json            │    │
│  │  - tick() → effects_json                             │    │
│  │  - getState() → state_json                           │    │
│  │  - resolveMemberList(unresolved_json) → resolved_json│    │
│  └──────────────────────┬───────────────────────────────┘    │
│                         │ delegates to                      │
│  ┌──────────────────────▼───────────────────────────────┐    │
│  │  arbiter-core/src/server.rs (sans-IO Server)          │    │
│  │  arbiter-core/src/core.rs (Arbiter, Space, etc.)     │    │
│  └──────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────┘
```

## Data Flow (Remote Resolution)

When a user configures a space delegation and then fetches members:

```
1. JS calls simulator.dispatch({ kind: "FetchMembers", ... })
2. JS sends Message → wasm engine
3. Engine → Server::handle_message → QueuedJob + SendMessage effect
4. JS receives SendMessage effect (target: remote arbiter's space)
5. JS resolves the target space's current members directly from engine state
6. JS sends ReplyResolvedMembers back to engine
7. Engine → Server::handle_message → provide_remote_space_members → FinishedJob
8. JS receives Respond effect with the resolved member list
9. JS updates the UI with the result
```

All of this happens in a single `dispatch()` call from the JS side, with recursive
resolution handled automatically by the simulator orchestrator.

## Phase 1 — Wasm Bindings Crate (`crates/arbiter-wasm/`)

### 1.1 Crate setup

**File:** `crates/arbiter-wasm/Cargo.toml`

```toml
[package]
name = "arbiter-wasm"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
arbiter-core = { path = "../arbiter-core", default-features = false }
wasm-bindgen = "0.2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

Note: `arbiter-core` is used with `default-features = false` to exclude the `async`
feature (no tokio dependency), keeping the wasm binary small.

### 1.2 Exported API

**File:** `crates/arbiter-wasm/src/lib.rs`

All complex types cross the wasm boundary as JSON strings. The engine exposes a
clean, minimal API:

```rust
use wasm_bindgen::prelude::*;
use arbiter_core::server::*;
use arbiter_core::core::*;

/// The core arbiter engine running in the browser.
#[wasm_bindgen]
pub struct ArbiterEngine {
    server: Server,
    next_job_id: i64,
}

#[wasm_bindgen]
impl ArbiterEngine {
    /// Create a new engine with an empty server state.
    pub fn new() -> ArbiterEngine;

    /// Process a message (JSON-serialized `Message`), returning JSON-serialized
    /// effects (`Vec<ServerEffectView>`).
    ///
    /// The caller provides `src_job_id` in the message, or 0 for auto-assignment.
    /// If src_job_id is 0, the engine assigns one.
    pub fn handle_message(&mut self, msg_json: &str) -> String;

    /// Advance time by one tick. Returns JSON-serialized effects.
    pub fn tick(&mut self) -> String;

    /// Get a complete snapshot of the server state for rendering.
    /// Returns JSON-serialized `ServerStateView`.
    pub fn get_state(&self) -> String;

    /// Given a JSON-serialized `UnresolvedMemberList`, resolve any local-space
    /// members by looking them up in the current server state.
    ///
    /// Remote space members that can't be resolved are returned in `missing_spaces`.
    /// This is used by the JS simulator to auto-resolve delegations.
    pub fn resolve_member_list(&self, unresolved_json: &str) -> String;
}
```

### 1.3 JSON View Types

We define plain-data view structs for serialization, to avoid exposing complex
internal types (like `im::HashMap`) across the wasm boundary.

```rust
// --- View types for the JS side ---

#[derive(Serialize, Deserialize)]
pub struct ServerStateView {
    pub time: i64,
    pub arbiters: Vec<ArbiterView>,
    pub pending_jobs: Vec<PendingJobView>,
}

#[derive(Serialize, Deserialize)]
pub struct ArbiterView {
    pub did: String,
    pub version: i64,
    pub spaces: Vec<SpaceView>,
}

#[derive(Serialize, Deserialize)]
pub struct SpaceView {
    pub key: String,
    pub config: SpaceConfigView,
    pub members: Vec<MemberEntryView>,
}

#[derive(Serialize, Deserialize)]
pub struct MemberEntryView {
    pub member_type: String,  // "User", "LocalSpace", "RemoteSpace"
    pub value: String,        // The DID or space key or SpaceId JSON
    pub access: String,       // Access level name
}

#[derive(Serialize, Deserialize)]
pub struct SpaceConfigView {
    pub public_records: bool,
    pub public_members: bool,
}

#[derive(Serialize, Deserialize)]
pub struct PendingJobView {
    pub id: i64,
    pub user_did: String,
    pub space_key: String,
    pub args_type: String,
}

/// Effects returned to the JS side after handle_message / tick.
#[derive(Serialize, Deserialize)]
pub struct ServerEffectView {
    pub effect_type: String,  // "Respond", "SendMessage", "ArbiterChanged", "ArbiterDeleted"
    pub req_id: Option<i64>,
    pub result: Option<JobResultView>,
    pub error: Option<String>,
    pub to_did: Option<String>,
    pub message: Option<MessageView>,
    pub arbiter_did: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct JobResultView {
    pub result_type: String,  // "Ok", "ResolvedMembersList"
    pub member_list: Vec<MemberEntryView>,
    pub missing_spaces: Vec<MissingSpaceView>,
}

#[derive(Serialize, Deserialize)]
pub struct MissingSpaceView {
    pub arbiter_did: String,
    pub space_key: String,
    pub access: String,
}
```

### 1.4 Wasm-specific considerations

- **No stdio/filesystem** — The wasm target has no filesystem. All state lives in
  memory. Persistence is handled by the JS side (e.g., localStorage or IndexedDB).
- **Single-threaded** — The engine runs synchronously on the main thread (or can be
  moved to a Web Worker). All methods are `&mut self` — no async.
- **JSON boundary** — All complex data crosses as JSON strings. This is the simplest
  and most maintainable approach for an interactive explorer. The JSON overhead is
  negligible for the scale of data we're handling.
- **Size** — The wasm binary should be small (just `arbiter-core` + `serde` +
  `wasm-bindgen`). Target < 500KB gzipped.

### 1.5 Building

```bash
cd crates/arbiter-wasm
wasm-pack build --target web
```

This produces `pkg/` with:
- `arbiter_wasm_bg.wasm` — the actual wasm binary
- `arbiter_wasm.js` — JS glue (loads wasm, provides `ArbiterEngine` class)
- `arbiter_wasm.d.ts` — TypeScript declarations

## Phase 2 — Svelte 5 SPA (`arbiter-simulator/`)

### 2.1 Project setup

```bash
mkdir -p arbiter-simulator
cd arbiter-simulator
pnpm create vite@latest . --template svelte-ts
pnpm add -D @sveltejs/vite-plugin-svelte
```

**Build tooling:**
- **Vite** — bundler with wasm support via `vite-plugin-wasm` and `vite-plugin-top-level-await`
- **Svelte 5** — with runes ($state, $derived, $effect) for reactive state
- **TypeScript** — strict mode
- **D3.js** (optional) — for the graph/force layout if we want auto-layout of nodes
  - Alternative: custom Canvas/HTML layout — simpler for a small number of nodes

**Directory layout:**
```
arbiter-simulator/
├── package.json
├── vite.config.ts
├── svelte.config.js
├── tsconfig.json
├── index.html
├── public/
│   └── favicon.svg
├── src/
│   ├── main.ts
│   ├── App.svelte
│   ├── app.css
│   ├── lib/
│   │   ├── simulator.ts        # Simulation orchestrator (wasm wrapper)
│   │   ├── types.ts            # TypeScript types mirroring Rust types
│   │   ├── simulation-store.ts # Svelte stores for app state
│   │   └── utils.ts            # Formatting, helpers
│   ├── components/
│   │   ├── Toolbar.svelte      # Top bar: active user, reset, undo
│   │   ├── Sidebar.svelte      # Left panel: users, actions
│   │   ├── UserList.svelte     # User accounts list + create form
│   │   ├── ActionPanel.svelte  # Create arbiter, space, add member, etc.
│   │   ├── Canvas.svelte       # Center: graph visualization
│   │   ├── ArbiterNode.svelte  # A single arbiter with its spaces
│   │   ├── SpaceNode.svelte    # A single space within an arbiter
│   │   ├── DelegationEdge.svelte # Arrow between spaces
│   │   └── DetailPanel.svelte  # Right panel: selected item details
│   └── stores/
│       └── types.ts            # Store types (exported from components)
```

### 2.2 TypeScript types (`src/lib/types.ts`)

Mirror the Rust view types for a fully typed JS experience:

```typescript
export type MessageKind =
  | { type: 'CreateArbiter' }
  | { type: 'DeleteArbiter' }
  | { type: 'FetchMembers' }
  | { type: 'CreateSpace' }
  | { type: 'ConfigureSpace'; publicRecords: boolean; publicMembers: boolean }
  | { type: 'DeleteSpace' }
  | { type: 'SetMemberAccess'; member: Member; access: Access }
  | { type: 'RemoveMember'; member: Member }
  | { type: 'ReplyResolvedMembers'; members: ResolvedMemberList };

export interface Message {
  userDid: string;
  arbiterDid: string;
  spaceKey: string;
  srcJobId: number;
  resolverDepth: number;
  kind: MessageKind;
}

export type Access = 'ReadMemberList' | 'IsMember' | 'AddMembers'
  | 'RemoveMembers' | 'ConfigureSpace' | 'CreateSpaces'
  | 'RemoveSpace' | 'Owner';

export interface Member {
  tag: 'MemberUser' | 'MemberLocalSpace' | 'MemberRemoteSpace';
  value: string;
}

export interface SpaceId {
  arbiterDid: string;
  spaceKey: string;
}

// ... more types matching the Rust view types
```

### 2.3 Simulator orchestrator (`src/lib/simulator.ts`)

The core JS class that bridges the Svelte app to the wasm engine:

```typescript
import init, { ArbiterEngine } from 'arbiter-wasm';

export class Simulator {
  private engine!: ArbiterEngine;
  private initialized = false;
  private nextJobId = 1;

  async init(): Promise<void> {
    await init();
    this.engine = ArbiterEngine.new();
    this.initialized = true;
  }

  /// Dispatch a high-level action. Internally handles the full effect loop
  /// including recursive remote space resolution.
  async dispatch(msg: Message): Promise<SimulationResult> {
    // Assign job ID if not set
    if (msg.srcJobId === 0) {
      msg.srcJobId = this.nextJobId++;
    }
    
    // Send to engine
    const effectsJson = this.engine.handleMessage(JSON.stringify(msg));
    const effects: ServerEffectView[] = JSON.parse(effectsJson);
    
    // Process effects, handling SendMessage recursively
    const results = await this.processEffects(effects);
    
    return {
      effects: results,
      state: this.getState(),
    };
  }

  private async processEffects(
    effects: ServerEffectView[],
    depth = 0
  ): Promise<ServerEffectView[]> {
    if (depth > 10) return effects; // safety limit
    
    const all: ServerEffectView[] = [];
    
    for (const effect of effects) {
      if (effect.effectType === 'SendMessage' && effect.message) {
        // Auto-resolve: send FetchMembers, get result, reply
        const resolved = this.resolveRemoteSpace(effect.message);
        if (resolved) {
          const reply: Message = {
            ...effect.message,
            srcJobId: this.nextJobId++,
            kind: { type: 'ReplyResolvedMembers', members: resolved },
          };
          const subEffects = await this.dispatchRaw(reply);
          all.push(...subEffects);
        }
      } else {
        all.push(effect);
      }
    }
    
    return all;
  }

  private resolveRemoteSpace(msg: Message): ResolvedMemberList | null {
    // Get the unresolved members for the target space
    const membersJson = this.engine.getSpaceMembers(msg.arbiterDid, msg.spaceKey);
    if (!membersJson) return null;
    const members = JSON.parse(membersJson);
    // Resolve any local-space references
    const resolvedJson = this.engine.resolveMemberList(JSON.stringify(members));
    return JSON.parse(resolvedJson);
  }

  getState(): ServerStateView {
    return JSON.parse(this.engine.getState());
  }
}
```

### 2.4 Svelte App State (`src/lib/simulation-store.ts`)

Using Svelte 5 runes for reactive state:

```typescript
import { Simulator } from './simulator';

// Global simulator instance
export let simulator: Simulator;

// Application state (using Svelte 5 $state runes)
let currentUserId = $state<string | null>(null);
let users = $state<UserAccount[]>([]);
let state = $state<ServerStateView | null>(null);
let selectedArbiterDid = $state<string | null>(null);
let selectedSpaceKey = $state<string | null>(null);
let selectedSpaceMembers = $state<MemberEntryView[] | null>(null);
let resolutionChain = $state<ResolutionChainStep[]>([]);
let notifications = $state<Notification[]>([]);

// Derived state
let currentUser = $derived(users.find(u => u.did === currentUserId));
let selectedArbiter = $derived(
  state?.arbiters.find(a => a.did === selectedArbiterDid)
);
let selectedSpace = $derived(
  selectedArbiter?.spaces.find(s => s.key === selectedSpaceKey)
);
```

### 2.5 Component Design

#### App.svelte (Main Layout)

```
┌──────────────────────────────────────────────────────────┐
│  Toolbar  [User ▼]  [Reset]  [Export Config]            │
├──────────┬───────────────────────────┬───────────────────┤
│ Sidebar  │      Canvas               │  Detail Panel     │
│          │                           │                   │
│ Users:   │   ┌─────┐                 │  Selected Space:  │
│  ● alice │   │Arb 1│                 │  "admin"          │
│  ○ bob   │   │ ├─ $admin──┐         │                   │
│  ○ carol │   │ └─ team──┐ │         │  Members:         │
│          │   └─────────│─│──────────│  alice → Owner    │
│ Actions: │     ┌──────┐│ │          │  bob → ReadMember  │
│ [New Ar] │     │Arb 2 │└─│──────────│                   │
│ [New Sp] │     │ └─ $admin │         │  Delegation:      │
│ [Add Mb] │     │ └─ shared│         │  team → Arb2/     │
│          │     └──────────┘         │    shared         │
│          │                          │  (IsMember ∩      │
│          │                          │   Owner = IsMember)│
└──────────┴───────────────────────────┴───────────────────┘
```

#### Toolbar.svelte
- Current user dropdown (shows all created users)
- Reset button (clears all simulator state)
- Export/Import config buttons (JSON)

#### Sidebar.svelte
Contains two sections:

1. **UserList.svelte** — Virtual user accounts
   - List of created users with their DIDs
   - "Create User" form (enter DID or generate random)
   - Delete user button (removes from list only — doesn't affect arbiters)
   - Click to set as active user

2. **ActionPanel.svelte** — Context-sensitive actions
   - Based on current selection (nothing selected / arbiter selected / space selected)
   - "Create Arbiter" — always available (provide DID)
   - "Create Space" — when an arbiter is selected
   - "Add Member" — when a space is selected (choose user/local-space/remote-space + access level)
   - "Remove Member" — when a space member is selected in the detail panel
   - "Configure Space" — toggle public_records / public_members
   - "Delete Space" / "Delete Arbiter" — with confirmation

#### Canvas.svelte

The main visualization area. Layout options:

**Option A: CSS/HTML layout** (recommended for initial implementation)
- Arbiters are box containers with a header showing the DID
- Spaces are cards inside the arbiter container
- Delegation edges are SVG lines/arrows between space cards
- Selected item is highlighted
- Click to select, double-click to expand

**Option B: Canvas/WebGL** (future enhancement)
- Uses a force-directed graph layout for large number of nodes
- Could use D3.js or custom canvas rendering

The Canvas shows:
- **Arbiter nodes**: Large rounded rectangles labeled with the arbiter DID
- **Space nodes**: Smaller cards within arbiter nodes, labeled with space key
  - Color-coded by config (green = public members, blue = private)
  - Badge showing member count
- **Delegation edges**: Directed arrows from a space to a remote space
  - Labeled with the delegated access level
  - Hover shows the effective access after composition
- **$admin space**: Always present, visually distinct (crown icon or gold border)

#### DetailPanel.svelte

Shows when a space, arbiter, or member is selected:

- **Space selected**: 
  - Space key and config toggles
  - **Resolved Member List** — the main output! Shows each user DID and their
    computed effective access level
    - Color-coded by access level (green=ReadMemberList → red=Owner)
    - Shows the delegation chain for indirect members
  - **Member entries** — raw members added to this space
  - **Access level legend** — explanation of what each level means
  - **Delegation chain** — for indirect members, show the path through spaces

- **Arbiter selected**:
  - DID and version
  - List of spaces with member counts
  - Invariant checks (has owner, admin space exists, etc.)

- **Member selected** (in a space's member list):
  - The member's DID/SpaceId
  - The access level granted
  - For remote spaces: link to navigate to that space

### 2.6 User Interaction Flows

#### Flow: Create a new arbiter
1. User selects "alice" as active user in the toolbar
2. User clicks "Create Arbiter" in the action panel
3. User enters DID (e.g., "did:example:my-arbiter") or accepts generated one
4. JS calls `simulator.dispatch({ userDid: "alice", arbiterDid: "...", spaceKey: "$admin", kind: { type: "CreateArbiter" } })`
5. Engine processes, returns state with new arbiter
6. Canvas re-renders showing the new arbiter node with its $admin space containing alice as Owner

#### Flow: Add member delegation
1. User selects a space (e.g., "team" on arbiter "did:example:org")
2. User clicks "Add Member" 
3. User picks member type: "Remote Space" and fills in the SpaceId
4. User picks access level: "IsMember" (read access to member list)
5. JS calls `simulator.dispatch({ ..., kind: { type: "SetMemberAccess", member: { tag: "MemberRemoteSpace", value: {...} }, access: "IsMember" } })`
6. Engine adds the member
7. Detail panel shows the delegation edge
8. Canvas shows a new arrow from "team" to the remote space

#### Flow: Fetch resolved members
1. User clicks "Fetch Members" on a space (or this could auto-display when selecting a space)
2. JS calls `simulator.dispatch({ ..., kind: { type: "FetchMembers" } })`
3. Engine queues the job, emits SendMessage for remote spaces
4. JS auto-resolves by looking up target space members
5. Engine completes the job, emits Respond with ResolvedMembersList
6. Detail panel shows the final computed member list with effective access levels

#### Flow: View delegation chain
1. In the resolved member list, a user "bob" has "IsMember" access
2. User clicks on bob's entry
3. Detail panel shows: "bob gets IsMember via: team → Arb2/shared → direct member"
4. This shows the access composition path

### 2.7 Simulating Remote Resolution (The Key Feature)

When a space delegates to a remote space, the real server would do an HTTP fetch.
In the simulator, the JS orchestrator handles this:

```
User action: FetchMembers on space "team" (has RemoteSpace member Arb2/shared)
  ↓
Engine: Queues job, emits SendMessage({
  arbiterDid: "did:example:arb2",
  spaceKey: "shared",
  kind: FetchMembers
})
  ↓
Orchestrator: Receives SendMessage
  ↓
Orchestrator: Calls engine.getSpaceMembers("did:example:arb2", "shared")
  ↓
Engine: Returns unresolved member list for "shared" space
  ↓
Orchestrator: Calls engine.resolveMemberList(unresolvedList)
  ↓
Engine: Recursively resolves any local-space references within "shared"
  ↓
Orchestrator: Sends ReplyResolvedMembers back to engine
  ↓
Engine: Provides resolved members, job completes
  ↓
Orchestrator: Receives Respond effect with final ResolvedMembersList
  ↓
UI: Displays the computed member list with effective access levels
```

The orchestrator also handles **recursive delegation chains**: if "shared" space
on Arb2 itself delegates to another remote space, the orchestrator repeats the
process (with a depth limit).

### 2.8 Notifications and Error Display

- Success/error toasts for operations (e.g., "Space created", "Permission denied")
- Show the actual `ArbiterError` messages from the engine when an operation fails
- Visual indicators for queued jobs (spinner or badge on the space)
- Expandable log panel showing the sequence of effects for each operation

## Phase 3 — Polish and Deployment

### 3.1 Access Level Explorer

A dedicated section that helps users understand the access level hierarchy:
- Visual chart showing: `ReadMemberList < IsMember < AddMembers < RemoveMembers
  < ConfigureSpace < CreateSpaces < RemoveSpace < Owner`
- Explanation: "Each level includes all levels below it"
- Tooltip on each access level explaining what it allows

### 3.2 Delegation Chain Visualization

When viewing a resolved member list, show the delegation chain for each user:
```
alice → Owner
  (direct member in $admin)

bob → IsMember
  (via: team → Arb2/shared → direct member)
  
carol → ReadMemberList
  (via: team → Arb2/shared → public space)
```

### 3.3 Invariant Checking

The simulator can run the arbiter invariants and display any violations:
- "Arbiter has at least one owner" ✓
- "Admin space exists" ✓
- "No remote space references local arbiter" ✓

### 3.4 Config Export/Import

Allow users to:
- Export the current configuration as JSON (all arbiters, spaces, members)
- Import a configuration to reproduce a scenario
- Share configurations (e.g., via URL hash)

### 3.5 Deployment

- Build with `pnpm build` → outputs to `arbiter-simulator/dist/`
- Can be deployed to any static hosting (GitHub Pages, Netlify, etc.)
- The wasm binary is loaded dynamically by Vite's wasm plugin

## Files to Create

### crates/arbiter-wasm/

| File | Action |
|------|--------|
| `crates/arbiter-wasm/Cargo.toml` | New: wasm crate with arbiter-core dep |
| `crates/arbiter-wasm/src/lib.rs` | New: ArbiterEngine wasm-bindgen API |

### arbiter-simulator/

| File | Action |
|------|--------|
| `arbiter-simulator/package.json` | New: Svelte 5 + Vite + wasm deps |
| `arbiter-simulator/vite.config.ts` | New: Vite config with wasm plugin |
| `arbiter-simulator/svelte.config.js` | New: Svelte config |
| `arbiter-simulator/tsconfig.json` | New: TypeScript config |
| `arbiter-simulator/index.html` | New: Entry HTML |
| `arbiter-simulator/src/main.ts` | New: App initialization |
| `arbiter-simulator/src/App.svelte` | New: Main layout component |
| `arbiter-simulator/src/app.css` | New: Global styles |
| `arbiter-simulator/src/lib/types.ts` | New: TypeScript type definitions |
| `arbiter-simulator/src/lib/simulator.ts` | New: Simulation orchestrator |
| `arbiter-simulator/src/lib/simulation-store.ts` | New: Svelte stores |
| `arbiter-simulator/src/lib/utils.ts` | New: Utility functions |
| `arbiter-simulator/src/components/Toolbar.svelte` | New: Top toolbar |
| `arbiter-simulator/src/components/Sidebar.svelte` | New: Left sidebar |
| `arbiter-simulator/src/components/UserList.svelte` | New: User management |
| `arbiter-simulator/src/components/ActionPanel.svelte` | New: Action buttons |
| `arbiter-simulator/src/components/Canvas.svelte` | New: Graph visualization |
| `arbiter-simulator/src/components/ArbiterNode.svelte` | New: Arbiter display |
| `arbiter-simulator/src/components/SpaceNode.svelte` | New: Space display |
| `arbiter-simulator/src/components/DelegationEdge.svelte` | New: Edge rendering |
| `arbiter-simulator/src/components/DetailPanel.svelte` | New: Detail view |
| `arbiter-simulator/src/components/Notification.svelte` | New: Toast notifications |

## Test Criteria

### Phase 1 (wasm)

- [ ] `wasm-pack build` succeeds in `crates/arbiter-wasm/`
- [ ] `ArbiterEngine::new()` creates an empty engine
- [ ] `handleMessage` with `CreateArbiter` returns effects including `ArbiterChanged`
- [ ] `handleMessage` with `FetchMembers` on a fresh arbiter returns member list
- [ ] `getState` returns valid JSON matching the view types
- [ ] `resolveMemberList` can resolve local-space references
- [ ] Multiple operations in sequence produce correct state
- [ ] Wasm binary size is reasonable (< 500KB gzipped)

### Phase 2 (SPA)

- [ ] App initializes and loads wasm module
- [ ] Can create users, arbiters, spaces
- [ ] Can add/remove members with different access levels
- [ ] Fetching members shows correct resolved list
- [ ] Remote space delegation auto-resolves via the orchestrator
- [ ] Delegation chains of depth 2+ work correctly
- [ ] Error cases are displayed (permission denied, etc.)
- [ ] Access level legend/tooltips provide useful information
- [ ] Export/import config works
- [ ] Canvas updates reactively when state changes

### Phase 3 (Polish)

- [ ] Delegation chain visualization is clear and informative
- [ ] Invariant checking displays meaningful results
- [ ] Responsive layout works on different screen sizes
- [ ] App is deployable to static hosting

## Future Enhancements (Not in Initial Scope)

- **Timeline/History** — Show the sequence of operations as a timeline, with the
  ability to step forward/backward through state changes
- **Multi-server simulation** — Separate `Server` instances in wasm to more
  faithfully simulate the network topology (vs. the unified Server in one engine)
- **Job queue visualization** — Show the job queue for each arbiter with timeouts
- **Scenario library** — Pre-built scenarios demonstrating different auth patterns
- **Performance benchmarks** — Measure resolution time with many spaces/members
