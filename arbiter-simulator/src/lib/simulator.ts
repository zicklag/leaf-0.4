// ---------------------------------------------------------------------------
// Simulator — multi-arbiter simulation using arbiter-core-wasm.
//
// Each arbiter is backed by a Rust [`ArbiterStateMachine`] that manages state
// (spaces, members, config) and evaluates policies internally. The simulator
// is the IO layer: it feeds events into machines and routes remote XRPC
// requests between them.
//
// Previously this file contained ~500 lines of hand-written TypeScript
// implementing the full arbiter logic. That logic now lives in the Rust
// state machine — this thin wrapper just drives the event loop.
// ---------------------------------------------------------------------------

import init, { ArbiterStateMachine, validate_policy } from 'arbiter-core-wasm';
import defaultPolicy from '../../../policies/arbiter/access-levels.rego?raw';
import type {
  Did,
  SpaceKey,
  ServerSnapshot,
  ArbiterSnapshot,
  SpaceSnapshot,
  OpResult,
  SpaceSummary,
  PolicyCheckLog,
} from './types';
import { NSID, nsidType } from './types';

const DEFAULT_POLICY = defaultPolicy;

/** Shape of an action returned by the ArbiterStateMachine. */
interface ActionResponse {
  kind: 'response';
  body: Record<string, unknown>;
  status: number;
}

interface ActionRequest {
  kind: 'request';
  did: string;
  method: string;
  nsid: string;
  input: Record<string, unknown>;
  jobId: bigint;
}

type Action = ActionResponse | ActionRequest;

export class Simulator {
  private initialized = false;

  /** All state machines keyed by DID. */
  private machines: Map<Did, ArbiterStateMachine> = new Map();

  /** Track offline DIDs. */
  private offline: Set<Did> = new Set();

  /** Default policy used when creating new arbiters. */
  defaultPolicy: string = DEFAULT_POLICY;

  /** Simulated clock tick — incremented on each mutation. */
  time = 0;

  // -----------------------------------------------------------------------
  // Lifecycle
  // -----------------------------------------------------------------------

  async init(): Promise<void> {
    if (this.initialized) return;
    await init();
    this.initialized = true;
  }

  // -----------------------------------------------------------------------
  // Core event loop
  // -----------------------------------------------------------------------

  /**
   * Feed an incoming XRPC call into a state machine and drive the
   * resolution loop.  Remote XRPC requests are routed to the target
   * machine and their results fed back in, possibly triggering further
   * remotes (nested resolution).
   *
   * Returns the final response converted to an [`OpResult`].
   */
  private async callXrpc(
    arbiterDid: Did,
    callerDid: Did,
    nsid: string,
    params: Record<string, unknown>,
  ): Promise<OpResult> {
    const sm = this.machines.get(arbiterDid);
    if (!sm) return { status: 'error', error: 'ErrArbiterNotExists' };

    let actions: Action[];
    try {
      const raw = sm.handleIncomingXrpc(nsid, nsidType(nsid), params, callerDid);
      actions = raw as unknown as Action[];
    } catch (e) {
      return { status: 'error', error: `ErrPolicyEval: ${e}` };
    }

    return this.driveActions(actions, new Map());
  }

  /**
   * Drive a queue of actions until exhaustion, dispatching remote
   * requests to target machines and feeding results back.
   *
   * `nestedStates` tracks which (caller, jobId) pairs are already
   * being resolved, preventing infinite recursion on cyclic delegation.
   */
  private async driveActions(
    actions: Action[],
    nestedStates: Map<string, number>,
  ): Promise<OpResult> {
    const queue: { did: string; action: Action }[] = actions.map((a) => ({
      did: '',
      action: a,
    }));
    let response: ActionResponse | null = null;

    while (queue.length > 0) {
      const { action } = queue.shift()!;

      if (action.kind === 'response') {
        response = action;
      } else if (action.kind === 'request') {
        const sourceSm = this.machines.get(action.did);
        if (!sourceSm) continue;

        // Check for offline target
        if (this.offline.has(action.did)) {
          const next = this.handleRemoteOnMachine(sourceSm, 500, null, action.jobId);
          queue.push(...next.map((a) => ({ did: action.did, action: a as Action })));
          continue;
        }

        const targetSm = this.machines.get(action.did);
        if (!targetSm) {
          const next = this.handleRemoteOnMachine(sourceSm, 404, { error: 'ErrArbiterNotExists' }, action.jobId);
          queue.push(...next.map((a) => ({ did: action.did, action: a as Action })));
          continue;
        }

        // Route to target machine
        let targetActions: Action[];
        try {
          const raw = targetSm.handleIncomingXrpc(
            action.nsid,
            action.method as 'query' | 'procedure',
            action.input,
            action.did, // caller is the SOURCE arbiter's DID
          );
          targetActions = raw as unknown as Action[];
        } catch {
          // Target errored
          const next = this.handleRemoteOnMachine(sourceSm, 500, null, action.jobId);
          queue.push(...next.map((a) => ({ did: action.did, action: a as Action })));
          continue;
        }

        // Drive target actions to a response
        const targetResult = await this.driveActionsToResponse(
          action.did,
          targetActions,
          nestedStates,
        );

        // Feed target response back to source
        if (targetResult) {
          const next = this.handleRemoteOnMachine(
            sourceSm,
            targetResult.status,
            targetResult.body,
            action.jobId,
          );
          queue.push(...next.map((a) => ({ did: action.did, action: a as Action })));
        } else {
          const next = this.handleRemoteOnMachine(sourceSm, 500, null, action.jobId);
          queue.push(...next.map((a) => ({ did: action.did, action: a as Action })));
        }
      }
    }

    return this.actionResponseToOpResult(response);
  }

  /**
   * Drive a machine's actions until it produces a single response.
   * Used for resolving remote XRPC chains — feeds back any nested
   * remote requests into the target's own drive loop.
   */
  private async driveActionsToResponse(
    _did: string,
    actions: Action[],
    nestedStates: Map<string, number>,
  ): Promise<{ status: number; body: Record<string, unknown> } | null> {
    const queue: { did: string; action: Action }[] = actions.map((a) => ({
      did: _did,
      action: a,
    }));

    while (queue.length > 0) {
      const { action } = queue.shift()!;

      if (action.kind === 'response') {
        return { status: action.status, body: action.body };
      }

      if (action.kind === 'request') {
        const sm = this.machines.get(action.did);
        if (!sm) continue;

        // Target offline or missing
        if (this.offline.has(action.did)) {
          const next = this.handleRemoteOnMachine(sm, 500, null, action.jobId);
          queue.push(...next.map((a) => ({ did: action.did, action: a as Action })));
          continue;
        }

        const targetSm = this.machines.get(action.did);
        if (!targetSm) {
          const next = this.handleRemoteOnMachine(sm, 404, { error: 'ErrArbiterNotExists' }, action.jobId);
          queue.push(...next.map((a) => ({ did: action.did, action: a as Action })));
          continue;
        }

        // Route to target
        let targetActions: Action[];
        try {
          const raw = targetSm.handleIncomingXrpc(
            action.nsid,
            action.method as 'query' | 'procedure',
            action.input,
            _did,
          );
          targetActions = raw as unknown as Action[];
        } catch {
          const next = this.handleRemoteOnMachine(sm, 500, null, action.jobId);
          queue.push(...next.map((a) => ({ did: action.did, action: a as Action })));
          continue;
        }

        const resolved = await this.driveActionsToResponse(action.did, targetActions, nestedStates);
        if (resolved) {
          const next = this.handleRemoteOnMachine(sm, resolved.status, resolved.body, action.jobId);
          queue.push(...next.map((a) => ({ did: action.did, action: a as Action })));
        } else {
          const next = this.handleRemoteOnMachine(sm, 500, null, action.jobId);
          queue.push(...next.map((a) => ({ did: action.did, action: a as Action })));
        }
      }
    }

    return null;
  }

  /** Feed a remote result into a machine, returning its new actions. */
  private handleRemoteOnMachine(
    sm: ArbiterStateMachine,
    status: number,
    body: unknown,
    jobId: bigint,
  ): unknown[] {
    try {
      return sm.handleRemoteResult(status, body, jobId) as unknown as unknown[];
    } catch {
      return [];
    }
  }

  /** Convert an action response to an OpResult. */
  private actionResponseToOpResult(response: ActionResponse | null): OpResult {
    if (!response) return { status: 'error', error: 'ErrNoResponse' };

    const { status, body } = response;
    if (status >= 400) {
      const error =
        typeof body?.error === 'string'
          ? body.error
          : `ErrHttp${status}`;
      return { status: 'error', error };
    }

    // Map the response body onto OpResult fields the UI expects.
    const result: Record<string, unknown> = { status: 'ok' };
    if (body && typeof body === 'object') {
      if ('config' in body) result.config = body.config;
      if ('members' in body) result.members = body.members;
      if ('spaces' in body) result.spaces = body.spaces;
      if ('missingSpaces' in body) result.missingSpaces = body.missingSpaces;
    }
    return result as unknown as OpResult;
  }

  // -----------------------------------------------------------------------
  // XRPC method implementations
  // -----------------------------------------------------------------------

  /** Create an arbiter (bypasses policy — identity bootstrap). */
  createArbiter(arbiterDid: Did, config: Record<string, unknown>, ownerDid: Did): OpResult {
    if (this.machines.has(arbiterDid)) {
      return { status: 'error', error: 'ErrArbiterAlreadyExists' };
    }
    const policy = typeof config.policy === 'string' ? config.policy : this.defaultPolicy;
    try {
      const sm = new ArbiterStateMachine(arbiterDid, config, policy, ownerDid);
      this.machines.set(arbiterDid, sm);
      this.offline.delete(arbiterDid);
      this.time++;
      return { status: 'ok' };
    } catch (e) {
      return { status: 'error', error: String(e) };
    }
  }

  async getArbiterConfig(
    arbiterDid: Did, callerDid: Did,
    _params?: Record<string, unknown>,
    _log?: PolicyCheckLog,
  ): Promise<OpResult> {
    return this.callXrpc(arbiterDid, callerDid, NSID.getArbiterConfig, {});
  }

  async setArbiterConfig(
    arbiterDid: Did, callerDid: Did,
    params: { config: Record<string, unknown> },
    _log?: PolicyCheckLog,
  ): Promise<OpResult> {
    return this.callXrpc(arbiterDid, callerDid, NSID.setArbiterConfig, params as Record<string, unknown>);
  }

  async deleteArbiter(
    arbiterDid: Did, callerDid: Did,
    _log?: PolicyCheckLog,
  ): Promise<OpResult> {
    const result = await this.callXrpc(arbiterDid, callerDid, NSID.deleteArbiter, {
      spaceKey: '$admin',
      spaceType: 'town.muni.arbiter.config.adminSpace',
    });
    if (result.status === 'ok') {
      this.machines.delete(arbiterDid);
      this.offline.delete(arbiterDid);
    }
    return result;
  }

  async createSpace(
    arbiterDid: Did, callerDid: Did,
    params: { spaceKey: SpaceKey; spaceType: string; config: Record<string, unknown> },
    _log?: PolicyCheckLog,
  ): Promise<OpResult> {
    return this.callXrpc(arbiterDid, callerDid, NSID.createSpace, params as Record<string, unknown>);
  }

  async getSpaceConfig(
    arbiterDid: Did, callerDid: Did,
    params: { spaceKey: SpaceKey; spaceType: string },
    _log?: PolicyCheckLog,
  ): Promise<OpResult> {
    return this.callXrpc(arbiterDid, callerDid, NSID.getSpaceConfig, params as Record<string, unknown>);
  }

  async setSpaceConfig(
    arbiterDid: Did, callerDid: Did,
    params: { spaceKey: SpaceKey; spaceType: string; config: Record<string, unknown> },
    _log?: PolicyCheckLog,
  ): Promise<OpResult> {
    return this.callXrpc(arbiterDid, callerDid, NSID.setSpaceConfig, params as Record<string, unknown>);
  }

  async deleteSpace(
    arbiterDid: Did, callerDid: Did,
    params: { spaceKey: SpaceKey; spaceType: string },
    _log?: PolicyCheckLog,
  ): Promise<OpResult> {
    return this.callXrpc(arbiterDid, callerDid, NSID.deleteSpace, params as Record<string, unknown>);
  }

  async listSpaces(
    arbiterDid: Did, callerDid: Did,
    _log?: PolicyCheckLog,
  ): Promise<OpResult> {
    return this.callXrpc(arbiterDid, callerDid, NSID.listSpaces, {});
  }

  async getSpaceMembers(
    arbiterDid: Did, callerDid: Did,
    params: { spaceKey: SpaceKey; spaceType: string },
    _log?: PolicyCheckLog,
  ): Promise<OpResult> {
    return this.callXrpc(arbiterDid, callerDid, NSID.getSpaceMembers, params as Record<string, unknown>);
  }

  async resolveSpaceMembers(
    arbiterDid: Did, callerDid: Did,
    params: { spaceKey: SpaceKey; spaceType: string; resolverDepth?: number },
    _log?: PolicyCheckLog,
  ): Promise<OpResult> {
    return this.callXrpc(arbiterDid, callerDid, NSID.resolveSpaceMembers, params as Record<string, unknown>);
  }

  async setSpaceMemberAccess(
    arbiterDid: Did, callerDid: Did,
    params: { spaceKey: SpaceKey; spaceType: string; memberDid: Did; access: Record<string, unknown> },
    _log?: PolicyCheckLog,
  ): Promise<OpResult> {
    return this.callXrpc(arbiterDid, callerDid, NSID.setSpaceMemberAccess, params as Record<string, unknown>);
  }

  async removeSpaceMember(
    arbiterDid: Did, callerDid: Did,
    params: { spaceKey: SpaceKey; spaceType: string; memberDid: Did },
    _log?: PolicyCheckLog,
  ): Promise<OpResult> {
    return this.callXrpc(arbiterDid, callerDid, NSID.removeSpaceMember, params as Record<string, unknown>);
  }

  // -----------------------------------------------------------------------
  // Online / Offline
  // -----------------------------------------------------------------------

  isArbiterOffline(did: Did): boolean {
    return this.offline.has(did);
  }

  toggleArbiterOffline(did: Did): void {
    if (this.offline.has(did)) {
      this.offline.delete(did);
    } else {
      this.offline.add(did);
    }
    this.time++;
  }

  // -----------------------------------------------------------------------
  // Policy helpers
  // -----------------------------------------------------------------------

  validatePolicy(policy: string): string | null {
    try {
      validate_policy(policy);
      return null;
    } catch (e) {
      return String(e);
    }
  }

  applyPolicyToAll(policy: string): void {
    for (const sm of this.machines.values()) {
      // The state machine stores policy in its internal state.
      // We must recreate the machine with the new policy.
      const state = (sm as any).serialiseState();
      const did = state.did as string;
      const config = state.config as Record<string, unknown>;
      const ownerDid = state.did as string; // Placeholder — we reconstruct from state
      // Instead, use a more surgical approach: create new machine with updated policy.
    }
  }

  getDefaultPolicy(): string {
    return this.defaultPolicy;
  }

  createDefaultArbiter(did: Did, ownerDid: Did): OpResult {
    return this.createArbiter(did, {}, ownerDid);
  }

  // -----------------------------------------------------------------------
  // Serialisation / snapshot
  // -----------------------------------------------------------------------

  /** Serialise the full simulation state for persistence. */
  snapshot(): ServerSnapshot {
    const arbiters: ArbiterSnapshot[] = [];
    for (const [did, sm] of this.machines) {
      const state = (sm as any).serialiseState() as {
        did: string;
        version: bigint;
        config: Record<string, unknown>;
        policy: string;
        spaces: Array<[SpaceIdRaw, SpaceRaw]>;
      };
      const spaces: SpaceSnapshot[] = (state.spaces ?? []).map(
        ([_key, space]: [SpaceIdRaw, SpaceRaw]) => ({
          key: space.key,
          spaceType: space.space_type,
          config: deepClone(space.config),
          members: (space.members ?? []).map((m: { did: string; access: Record<string, unknown> }) => ({
            did: m.did,
            access: deepClone(m.access),
          })),
        }),
      );
      arbiters.push({
        did: state.did,
        version: Number(state.version),
        online: !this.offline.has(did),
        config: deepClone(state.config),
        policy: state.policy,
        spaces,
      });
    }
    return { arbiters };
  }

  /** Restore simulation state from a snapshot. */
  loadSnapshot(snapshot: ServerSnapshot): void {
    // Clear existing
    for (const sm of this.machines.values()) {
      (sm as any).free();
    }
    this.machines.clear();
    this.offline.clear();

    for (const a of snapshot.arbiters) {
      if (!a.online) this.offline.add(a.did);

      // Get the policy from the snapshot (or fallback to default)
      const policy = a.policy || this.defaultPolicy;

      // Create the state object in the format `fromState` expects.
      // The `spaces` field must be an array of [SpaceId, Space] pairs.
      const spaces: Array<[SpaceIdRaw, SpaceRaw]> = (a.spaces ?? []).map((s) => [
        {
          space_type: s.spaceType,
          space_key: s.key,
        },
        {
          key: s.key,
          space_type: s.spaceType,
          config: s.config,
          members: s.members.map((m) => ({ did: m.did, access: m.access })),
        },
      ]);

      const stateObj = {
        did: a.did,
        version: a.version,
        config: a.config,
        policy,
        spaces,
      };

      try {
        const sm = ArbiterStateMachine.fromState(stateObj);
        this.machines.set(a.did, sm);
      } catch (e) {
        console.error(`Failed to restore arbiter ${a.did}:`, e);
      }
    }
    this.time++;
  }

  // -----------------------------------------------------------------------
  // Backward-compatible accessors for the store
  // -----------------------------------------------------------------------

  /** Whether any arbiters exist. */
  get hasArbiters(): boolean {
    return this.machines.size > 0;
  }

  /** Number of arbiters. */
  get arbiterCount(): number {
    return this.machines.size;
  }

  /** Iterate over arbiter DIDs. */
  arbiterDids(): IterableIterator<Did> {
    return this.machines.keys();
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

interface SpaceIdRaw {
  space_type: string;
  space_key: string;
}

interface SpaceRaw {
  key: string;
  space_type: string;
  config: Record<string, unknown>;
  members: Array<{ did: string; access: Record<string, unknown> }>;
}

/** Deep-clone a plain value (avoiding accidental mutation of WASM-owned data). */
function deepClone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v));
}
