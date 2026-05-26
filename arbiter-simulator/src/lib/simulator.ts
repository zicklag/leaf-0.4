// ---------------------------------------------------------------------------
// Simulator — multi-arbiter simulation using policy-core-wasm for auth.
//
// Each arbiter stores its own state and policy. Before any mutation or data
// access, the policy is evaluated. If the policy suspends with an XRPC query
// request (xrpc_local / xrpc_remote), the simulator resolves it by calling
// the corresponding query method and resumes the policy.
//
// Policy XRPC requests are ALWAYS queries (read-only), never procedures.
// Evaluating a policy must never change system state.
// ---------------------------------------------------------------------------

import init, { PolicySession, PolicyResult } from 'policy-core-wasm';
import type {
  Did,
  SpaceKey,
  ArbiterState,
  Space,
  MemberEntry,
  ServerSnapshot,
  ArbiterSnapshot,
  SpaceSnapshot,
  OpResult,
  SpaceRef,
  SpaceSummary,
  PolicyCheckLog,
} from './types';
import { NSID, nsidType } from './types';

const DEFAULT_POLICY = `package arbiter
import rego.v1
default allow := false
allow if { input.caller.access.level == "Owner" }
allow if {
    input.operation.type == "query"
    input.caller.access.level in {"ReadMemberList", "IsMember", "AddMembers", "RemoveMembers", "ConfigureSpace", "CreateSpaces", "RemoveSpace"}
}
`;

export class Simulator {
  private initialized = false;

  /** All arbiters keyed by DID. */
  arbiters: Map<Did, ArbiterState> = new Map();

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
  // Policy evaluation (core auth logic)
  // -----------------------------------------------------------------------

  /**
   * Evaluate an arbiter's policy for a given entry point.
   * Handles the resolution loop for xrpc_local / xrpc_remote suspensions.
   * Returns the completed policy value.
   */
  private async evaluateEntryPoint(
    arbiter: ArbiterState,
    callerDid: Did,
    nsid: string,
    params: Record<string, unknown>,
    entryPoint: string,
    log?: PolicyCheckLog,
  ): Promise<{ value: unknown; error?: string }> {
    const data = this.arbiterToData(arbiter);
    const opInfo = {
      nsid,
      type: nsidType(nsid),
      params,
    };

    const session = new PolicySession(
      arbiter.policy ?? this.defaultPolicy,
      data,
      { caller: { did: callerDid }, operation: opInfo },
      [entryPoint],
    );

    let result: PolicyResult;
    try {
      result = session.start();
    } catch (e) {
      return { value: undefined, error: String(e) };
    }

    while (result.status === 'suspended') {
      const request = result.request!;

      if (request.kind === 'xrpc_local') {
        log?.steps.push(`xrpc_local(${request.path})`);
        const resolved = this.resolveLocalQuery(arbiter.did, request.path, request.input);
        try {
          result = session.resume(resolved);
        } catch (e) {
          return { value: undefined, error: String(e) };
        }
      } else if (request.kind === 'xrpc_remote') {
        log?.steps.push(`xrpc_remote(${request.did}, ${request.path})`);
        const resolved = this.resolveRemoteQuery(request.did!, request.path, request.input);
        try {
          result = session.resume(resolved);
        } catch (e) {
          return { value: undefined, error: String(e) };
        }
      } else {
        return { value: undefined, error: 'Unknown suspension kind' };
      }
    }

    return { value: result.value };
  }

  /**
   * Convenience: evaluate the `data.arbiter.allow` entry point to check
   * whether an operation is permitted. Returns allow/deny.
   */
  private async checkPolicy(
    arbiter: ArbiterState,
    callerDid: Did,
    nsid: string,
    params: Record<string, unknown>,
    log?: PolicyCheckLog,
  ): Promise<{ allowed: boolean; error?: string }> {
    const { value, error } = await this.evaluateEntryPoint(
      arbiter, callerDid, nsid, params, 'data.arbiter.allow', log,
    );
    if (error) return { allowed: false, error };
    const allowed = value === true;
    if (log) {
      log.result = value;
      log.allowed = allowed;
    }
    return { allowed };
  }

  /** Resolve an xrpc_local / xrpc_remote query from the policy (server-to-server, no policy check). */
  private resolveLocalQuery(arbiterDid: Did, path: string, input: unknown): unknown {
    return this.executeQuery(arbiterDid, path, (input ?? {}) as Record<string, unknown>);
  }

  private resolveRemoteQuery(remoteDid: Did, path: string, input: unknown): unknown {
    return this.executeQuery(remoteDid, path, (input ?? {}) as Record<string, unknown>);
  }

  /**
   * Execute a query-type XRPC method (read-only, no policy check — server-to-server).
   * Returns the result value directly (not wrapped in OpResult).
   */
  private executeQuery(arbiterDid: Did, path: string, params: Record<string, unknown>): unknown {
    const arbiter = this.arbiters.get(arbiterDid);
    if (!arbiter) return null;

    switch (path) {
      case NSID.getArbiterConfig:
        return arbiter.config;

      case NSID.getSpaceConfig: {
        const sk = params.spaceKey as string | undefined;
        return sk ? arbiter.spaces.get(sk)?.config ?? null : null;
      }

      case NSID.getSpaceMembers: {
        const sk = params.spaceKey as string | undefined;
        return sk ? arbiter.spaces.get(sk)?.members ?? [] : [];
      }

      case NSID.resolveSpaceMembers: {
        // When the policy queries resolveSpaceMembers via xrpc_remote, return
        // the raw direct members of the target space. The calling policy handles
        // delegation, min_access, and deduplication.
        const sk = params.spaceKey as string | undefined;
        if (!sk) return [];
        const space = arbiter.spaces.get(sk);
        return space ? space.members.map((m) => ({ did: m.did, access: m.access })) : [];
      }

      case NSID.listSpaces:
        return Array.from(arbiter.spaces.values()).map((s) => ({
          key: s.key, spaceType: s.spaceType,
        }));

      default:
        return null;
    }
  }

  // -----------------------------------------------------------------------
  // XRPC method implementations
  // -----------------------------------------------------------------------

  /** Create an arbiter (bypasses policy — identity bootstrap). */
  createArbiter(arbiterDid: Did, config: Record<string, unknown>, ownerDid: Did): OpResult {
    if (this.arbiters.has(arbiterDid)) {
      return { status: 'error', error: 'ErrArbiterAlreadyExists' };
    }
    const policy = typeof config.policy === 'string' ? config.policy : this.defaultPolicy;
    const adminSpace: Space = {
      key: '$admin',
      spaceType: 'town.muni.arbiter.config.adminSpace',
      config: {},
      members: [{ did: ownerDid, access: { level: 'Owner' } }],
    };
    this.arbiters.set(arbiterDid, {
      did: arbiterDid, version: 1, config, policy,
      spaces: new Map([[adminSpace.key, adminSpace]]),
    });
    this.time++;
    return { status: 'ok' };
  }

  async getArbiterConfig(
    arbiterDid: Did, callerDid: Did, _params: { resolverDepth?: number }, log?: PolicyCheckLog,
  ): Promise<OpResult> {
    const arbiter = this.arbiters.get(arbiterDid);
    if (!arbiter) return { status: 'error', error: 'ErrArbiterNotExists' };
    const ok = await this.checkPolicy(arbiter, callerDid, NSID.getArbiterConfig, { spaceKey: '$admin' }, log);
    if (!ok.allowed) return { status: 'error', error: ok.error ?? 'ErrPermissionDenied' };
    return { status: 'ok', config: { ...arbiter.config } };
  }

  async setArbiterConfig(
    arbiterDid: Did, callerDid: Did, params: { config: Record<string, unknown> }, log?: PolicyCheckLog,
  ): Promise<OpResult> {
    const arbiter = this.arbiters.get(arbiterDid);
    if (!arbiter) return { status: 'error', error: 'ErrArbiterNotExists' };
    const ok = await this.checkPolicy(arbiter, callerDid, NSID.setArbiterConfig, params, log);
    if (!ok.allowed) return { status: 'error', error: ok.error ?? 'ErrPermissionDenied' };
    arbiter.config = { ...params.config };
    arbiter.version++;
    this.time++;
    return { status: 'ok' };
  }

  async deleteArbiter(arbiterDid: Did, callerDid: Did, log?: PolicyCheckLog): Promise<OpResult> {
    const arbiter = this.arbiters.get(arbiterDid);
    if (!arbiter) return { status: 'error', error: 'ErrArbiterNotExists' };
    const ok = await this.checkPolicy(arbiter, callerDid, NSID.deleteArbiter, { spaceKey: '$admin' }, log);
    if (!ok.allowed) return { status: 'error', error: ok.error ?? 'ErrPermissionDenied' };
    this.arbiters.delete(arbiterDid);
    this.time++;
    return { status: 'ok' };
  }

  async createSpace(
    arbiterDid: Did, callerDid: Did,
    params: { spaceKey: SpaceKey; spaceType: string; config: Record<string, unknown> },
    log?: PolicyCheckLog,
  ): Promise<OpResult> {
    const arbiter = this.arbiters.get(arbiterDid);
    if (!arbiter) return { status: 'error', error: 'ErrArbiterNotExists' };
    if (arbiter.spaces.has(params.spaceKey)) return { status: 'error', error: 'ErrSpaceExists' };
    const ok = await this.checkPolicy(arbiter, callerDid, NSID.createSpace, params, log);
    if (!ok.allowed) return { status: 'error', error: ok.error ?? 'ErrPermissionDenied' };
    arbiter.spaces.set(params.spaceKey, {
      key: params.spaceKey, spaceType: params.spaceType,
      config: { ...params.config }, members: [],
    });
    arbiter.version++;
    this.time++;
    return { status: 'ok' };
  }

  async getSpaceConfig(
    arbiterDid: Did, callerDid: Did, params: { spaceKey: SpaceKey }, log?: PolicyCheckLog,
  ): Promise<OpResult> {
    const arbiter = this.arbiters.get(arbiterDid);
    if (!arbiter) return { status: 'error', error: 'ErrArbiterNotExists' };
    const space = arbiter.spaces.get(params.spaceKey);
    if (!space) return { status: 'error', error: 'ErrSpaceNotExists' };
    const ok = await this.checkPolicy(arbiter, callerDid, NSID.getSpaceConfig, params, log);
    if (!ok.allowed) return { status: 'error', error: ok.error ?? 'ErrPermissionDenied' };
    return { status: 'ok', config: { ...space.config } };
  }

  async setSpaceConfig(
    arbiterDid: Did, callerDid: Did,
    params: { spaceKey: SpaceKey; config: Record<string, unknown> },
    log?: PolicyCheckLog,
  ): Promise<OpResult> {
    const arbiter = this.arbiters.get(arbiterDid);
    if (!arbiter) return { status: 'error', error: 'ErrArbiterNotExists' };
    const space = arbiter.spaces.get(params.spaceKey);
    if (!space) return { status: 'error', error: 'ErrSpaceNotExists' };
    const ok = await this.checkPolicy(arbiter, callerDid, NSID.setSpaceConfig, params, log);
    if (!ok.allowed) return { status: 'error', error: ok.error ?? 'ErrPermissionDenied' };
    space.config = { ...params.config };
    arbiter.version++;
    this.time++;
    return { status: 'ok' };
  }

  async deleteSpace(
    arbiterDid: Did, callerDid: Did, params: { spaceKey: SpaceKey }, log?: PolicyCheckLog,
  ): Promise<OpResult> {
    const arbiter = this.arbiters.get(arbiterDid);
    if (!arbiter) return { status: 'error', error: 'ErrArbiterNotExists' };
    if (!arbiter.spaces.has(params.spaceKey)) return { status: 'error', error: 'ErrSpaceNotExists' };
    const ok = await this.checkPolicy(arbiter, callerDid, NSID.deleteSpace, params, log);
    if (!ok.allowed) return { status: 'error', error: ok.error ?? 'ErrPermissionDenied' };
    arbiter.spaces.delete(params.spaceKey);
    arbiter.version++;
    this.time++;
    return { status: 'ok' };
  }

  async listSpaces(arbiterDid: Did, callerDid: Did, log?: PolicyCheckLog): Promise<OpResult> {
    const arbiter = this.arbiters.get(arbiterDid);
    if (!arbiter) return { status: 'error', error: 'ErrArbiterNotExists' };
    const ok = await this.checkPolicy(arbiter, callerDid, NSID.listSpaces, { spaceKey: '$admin' }, log);
    if (!ok.allowed) return { status: 'error', error: ok.error ?? 'ErrPermissionDenied' };
    const spaces: SpaceSummary[] = Array.from(arbiter.spaces.values()).map((s) => ({
      key: s.key, spaceType: s.spaceType,
    }));
    return { status: 'ok', spaces };
  }

  async getSpaceMembers(
    arbiterDid: Did, callerDid: Did, params: { spaceKey: SpaceKey }, log?: PolicyCheckLog,
  ): Promise<OpResult> {
    const arbiter = this.arbiters.get(arbiterDid);
    if (!arbiter) return { status: 'error', error: 'ErrArbiterNotExists' };
    const space = arbiter.spaces.get(params.spaceKey);
    if (!space) return { status: 'error', error: 'ErrSpaceNotExists' };
    const ok = await this.checkPolicy(arbiter, callerDid, NSID.getSpaceMembers, params, log);
    if (!ok.allowed) return { status: 'error', error: ok.error ?? 'ErrPermissionDenied' };
    return { status: 'ok', members: [...space.members] };
  }

  /** Resolve members — queries the policy's `resolved_members` rule. */
  async resolveSpaceMembers(
    arbiterDid: Did,
    callerDid: Did,
    params: { spaceKey: SpaceKey; resolverDepth?: number },
    log?: PolicyCheckLog,
  ): Promise<OpResult> {
    const arbiter = this.arbiters.get(arbiterDid);
    if (!arbiter) return { status: 'error', error: 'ErrArbiterNotExists' };
    const space = arbiter.spaces.get(params.spaceKey);
    if (!space) return { status: 'error', error: 'ErrSpaceNotExists' };

    // Auth check
    const auth = await this.checkPolicy(arbiter, callerDid, NSID.resolveSpaceMembers, params, log);
    if (!auth.allowed) return { status: 'error', error: auth.error ?? 'ErrPermissionDenied' };

    // Query the policy's resolve_result for the computed member data
    const result = await this.evaluateEntryPoint(
      arbiter, callerDid, NSID.resolveSpaceMembers, params, 'data.arbiter.resolve_result', log,
    );
    if (result.error) return { status: 'error', error: result.error };

    const data = result.value as Record<string, unknown> | undefined;
    return {
      status: 'ok',
      members: Array.isArray(data?.members) ? (data!.members as MemberEntry[]) : [],
      missingSpaces: Array.isArray(data?.missingSpaces) ? (data!.missingSpaces as SpaceRef[]) : [],
    };
  }

  async setSpaceMemberAccess(
    arbiterDid: Did, callerDid: Did,
    params: { spaceKey: SpaceKey; memberDid: Did; access: Record<string, unknown> },
    log?: PolicyCheckLog,
  ): Promise<OpResult> {
    const arbiter = this.arbiters.get(arbiterDid);
    if (!arbiter) return { status: 'error', error: 'ErrArbiterNotExists' };
    const space = arbiter.spaces.get(params.spaceKey);
    if (!space) return { status: 'error', error: 'ErrSpaceNotExists' };
    const ok = await this.checkPolicy(arbiter, callerDid, NSID.setSpaceMemberAccess, params, log);
    if (!ok.allowed) return { status: 'error', error: ok.error ?? 'ErrPermissionDenied' };
    const idx = space.members.findIndex((m) => m.did === params.memberDid);
    if (idx >= 0) {
      space.members[idx] = { did: params.memberDid, access: { ...params.access } };
    } else {
      space.members.push({ did: params.memberDid, access: { ...params.access } });
    }
    arbiter.version++;
    this.time++;
    return { status: 'ok' };
  }

  async removeSpaceMember(
    arbiterDid: Did, callerDid: Did,
    params: { spaceKey: SpaceKey; memberDid: Did },
    log?: PolicyCheckLog,
  ): Promise<OpResult> {
    const arbiter = this.arbiters.get(arbiterDid);
    if (!arbiter) return { status: 'error', error: 'ErrArbiterNotExists' };
    const space = arbiter.spaces.get(params.spaceKey);
    if (!space) return { status: 'error', error: 'ErrSpaceNotExists' };
    const ok = await this.checkPolicy(arbiter, callerDid, NSID.removeSpaceMember, params, log);
    if (!ok.allowed) return { status: 'error', error: ok.error ?? 'ErrPermissionDenied' };
    space.members = space.members.filter((m) => m.did !== params.memberDid);
    arbiter.version++;
    this.time++;
    return { status: 'ok' };
  }

  // -----------------------------------------------------------------------
  // Internal helpers
  // -----------------------------------------------------------------------

  /** Convert an arbiter's state to a plain JS object for the policy data doc. */
  private arbiterToData(arbiter: ArbiterState): Record<string, unknown> {
    const spaces: Record<string, unknown> = {};
    for (const [key, space] of arbiter.spaces) {
      spaces[key] = {
        spaceType: space.spaceType,
        config: space.config,
        members: space.members.map((m) => ({ did: m.did, access: m.access })),
      };
    }
    return { arbiter: { config: arbiter.config, spaces } };
  }

  // -----------------------------------------------------------------------
  // Serialisation / snapshot
  // -----------------------------------------------------------------------

  snapshot(): ServerSnapshot {
    const arbiters: ArbiterSnapshot[] = [];
    for (const [did, arb] of this.arbiters) {
      const spaces: SpaceSnapshot[] = [];
      for (const [key, space] of arb.spaces) {
        spaces.push({
          key, spaceType: space.spaceType,
          config: { ...space.config },
          members: space.members.map((m) => ({ did: m.did, access: { ...m.access } })),
        });
      }
      arbiters.push({ did, version: arb.version, config: { ...arb.config }, policy: arb.policy, spaces });
    }
    return { arbiters };
  }

  loadSnapshot(snapshot: ServerSnapshot): void {
    this.arbiters.clear();
    for (const a of snapshot.arbiters) {
      const spaces = new Map<SpaceKey, Space>();
      for (const s of a.spaces) {
        spaces.set(s.key, {
          key: s.key, spaceType: s.spaceType,
          config: { ...s.config },
          members: s.members.map((m) => ({ did: m.did, access: { ...m.access } })),
        });
      }
      this.arbiters.set(a.did, {
        did: a.did, version: a.version,
        config: { ...a.config }, policy: a.policy,
        spaces,
      });
    }
  }

  applyPolicyToAll(policy: string): void {
    for (const arbiter of this.arbiters.values()) {
      arbiter.policy = policy;
    }
  }

  createDefaultArbiter(did: Did, ownerDid: Did): OpResult {
    return this.createArbiter(did, {}, ownerDid);
  }

  getDefaultPolicy(): string {
    return this.defaultPolicy;
  }
}
