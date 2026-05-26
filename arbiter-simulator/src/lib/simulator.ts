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

import init, { PolicySession, PolicyResult, validate_policy } from 'policy-core-wasm';
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

const DEFAULT_POLICY = `# Default access-level authorization policy for the Muni Town Arbiter.
#
# This policy evaluates whether a given XRPC operation is allowed. It
# receives:
#
#   data.arbiter             — the arbiter's full state (config + spaces)
#   data.arbiter.spaces[key] — a space with its config and members
#   input.caller.did         — the requester's DID
#   input.caller.access      — the requester's pre-computed access level
#   input.operation.nsid     — the XRPC method NSID
#   input.operation.params   — the method parameters
#
# The policy can request additional data via two host built-ins:
#   xrpc_local(path, params)   — query the local arbiter
#   xrpc_remote(did, path, params) — query a remote arbiter
#
# XRPC queries from the policy are ALWAYS read-only (never procedures).

package arbiter

import rego.v1

# ---------------------------------------------------------------------------
# Access level helpers
# ---------------------------------------------------------------------------

access_level(obj) := obj.level if is_object(obj)
access_level(obj) := obj if not is_object(obj)

access_rank("ReadMemberList") := 0
access_rank("IsMember") := 1
access_rank("AddMembers") := 2
access_rank("RemoveMembers") := 3
access_rank("ConfigureSpace") := 4
access_rank("CreateSpaces") := 5
access_rank("RemoveSpace") := 6
access_rank("Owner") := 7

member_rank(member) := access_rank(access_level(member.access))

# ---------------------------------------------------------------------------
# Space data access helpers
# ---------------------------------------------------------------------------

space_members(space_key) := members if {
	data.arbiter.spaces[space_key]
	members := data.arbiter.spaces[space_key].members
}

space_members(space_key) := [] if {
	not data.arbiter.spaces[space_key]
}

space_config(space_key) := config if {
	data.arbiter.spaces[space_key]
	config := data.arbiter.spaces[space_key].config
}

space_config(space_key) := {} if {
	not data.arbiter.spaces[space_key]
}

# ---------------------------------------------------------------------------
# Resolved members via local/remote delegation
# ---------------------------------------------------------------------------

# Direct member in the target space
resolved_members_raw contains member if {
	some entry in space_members(input.operation.params.spaceKey)
	member := {"did": entry.did, "access": entry.access, "via": input.operation.params.spaceKey}
}

# Direct member inherited from $admin space
resolved_members_raw contains member if {
	input.operation.params.spaceKey != "$admin"
	some entry in space_members("$admin")
	member := {"did": entry.did, "access": entry.access, "via": "$admin"}
}

# Delegated from a local space: the member DID is "space:<key>"
resolved_members_raw contains member if {
	some entry in space_members(input.operation.params.spaceKey)
	startswith(entry.did, "space:")
	child_key := trim_prefix(entry.did, "space:")
	some child_entry in space_members(child_key)
	member := {
		"did": child_entry.did,
		"access": min_access(child_entry.access, entry.access),
		"via": child_key,
	}
}

# Delegated from a local space in the admin space
resolved_members_raw contains member if {
	input.operation.params.spaceKey != "$admin"
	some entry in space_members("$admin")
	startswith(entry.did, "space:")
	child_key := trim_prefix(entry.did, "space:")
	some child_entry in space_members(child_key)
	member := {
		"did": child_entry.did,
		"access": min_access(child_entry.access, entry.access),
		"via": child_key,
	}
}

# Delegated from a remote space: the member DID is "<arbiterDid>|<spaceKey>"
resolved_members_raw contains member if {
	some entry in space_members(input.operation.params.spaceKey)
	contains(entry.did, "|")
	parts := split(entry.did, "|")
	arbiter_did := parts[0]
	space_key := parts[1]

	# This resolves asynchronously via xrpc_remote → __builtin_host_await
	some remote_entry in xrpc_remote(arbiter_did, "town.muni.arbiter.resolveSpaceMembers", {"spaceKey": space_key})
	member := {
		"did": remote_entry.did,
		"access": min_access(remote_entry.access, entry.access),
		"via": concat("|", [arbiter_did, space_key]),
	}
}

# Delegated from a remote space in the admin space
resolved_members_raw contains member if {
	input.operation.params.spaceKey != "$admin"
	some entry in space_members("$admin")
	contains(entry.did, "|")
	parts := split(entry.did, "|")
	arbiter_did := parts[0]
	space_key := parts[1]
	some remote_entry in xrpc_remote(arbiter_did, "town.muni.arbiter.resolveSpaceMembers", {"spaceKey": space_key})
	member := {
		"did": remote_entry.did,
		"access": min_access(remote_entry.access, entry.access),
		"via": concat("|", [arbiter_did, space_key]),
	}
}

# ---------------------------------------------------------------------------
# Deduplicated: each DID appears once with their highest access
# ---------------------------------------------------------------------------

higher_exists(member) if {
	some higher in resolved_members_raw
	higher.did == member.did
	member_rank(higher) > member_rank(member)
}

resolved_members contains member if {
	some member in resolved_members_raw
	not higher_exists(member)
}

# ---------------------------------------------------------------------------
# min_access
# ---------------------------------------------------------------------------

min_access(a, b) := a if {
	member_rank({"access": a}) <= member_rank({"access": b})
}

min_access(a, b) := b if {
	member_rank({"access": b}) < member_rank({"access": a})
}

# ---------------------------------------------------------------------------
# Requester info
# ---------------------------------------------------------------------------

requester_rank := rank if {
	ranks := {member_rank(member) |
		some member in resolved_members_raw
		member.did == input.caller.did
	}
	rank := max(ranks)
}

# ---------------------------------------------------------------------------
# Missing spaces: remote references that resolved to empty
# ---------------------------------------------------------------------------

missing_spaces contains ms if {
	some entry in space_members(input.operation.params.spaceKey)
	contains(entry.did, "|")
	parts := split(entry.did, "|")
	arbiter_did := parts[0]
	space_key := parts[1]
	count(xrpc_remote(arbiter_did, "town.muni.arbiter.resolveSpaceMembers", {"spaceKey": space_key})) == 0
	ms := {"arbiterDid": arbiter_did, "spaceKey": space_key}
}

# ---------------------------------------------------------------------------
# Target member helpers (for set/remove)
# ---------------------------------------------------------------------------

target_exists_in_raw if {
	some entry in space_members(input.operation.params.spaceKey)
	entry.did == input.operation.params.memberDid
}

raw_target_rank := rank if {
	some entry in space_members(input.operation.params.spaceKey)
	entry.did == input.operation.params.memberDid
	rank := member_rank(entry)
}

resolve_result := {"members": resolved_members, "missingSpaces": missing_spaces}

# ---------------------------------------------------------------------------
# Authorization rules
# ---------------------------------------------------------------------------

default allow := false

# --- Reads ---

# Public member list: anyone can read
allow if {
	input.operation.nsid in {"town.muni.arbiter.resolveSpaceMembers", "town.muni.arbiter.getSpaceMembers"}
	space_config(input.operation.params.spaceKey).publicMembers == true
}

allow if {
	input.operation.nsid in {"town.muni.arbiter.resolveSpaceMembers", "town.muni.arbiter.getSpaceMembers"}
	requester_rank >= access_rank("ReadMemberList")
}

allow if {
	input.operation.nsid in {"town.muni.arbiter.getArbiterConfig", "town.muni.arbiter.listSpaces"}
	requester_rank >= access_rank("ReadMemberList")
}

# Public records: anyone can read space config
allow if {
	input.operation.nsid == "town.muni.arbiter.getSpaceConfig"
	space_config(input.operation.params.spaceKey).publicRecords == true
}

allow if {
	input.operation.nsid == "town.muni.arbiter.getSpaceConfig"
	requester_rank >= access_rank("ReadMemberList")
}

# --- Writes ---

allow if {
	input.operation.nsid == "town.muni.arbiter.createSpace"
	requester_rank >= access_rank("CreateSpaces")
}

allow if {
	input.operation.nsid == "town.muni.arbiter.setSpaceConfig"
	requester_rank >= access_rank("ConfigureSpace")
}

allow if {
	input.operation.nsid == "town.muni.arbiter.deleteSpace"
	input.operation.params.spaceKey != "$admin"
	requester_rank >= access_rank("RemoveSpace")
}

allow if {
	input.operation.nsid == "town.muni.arbiter.setArbiterConfig"
	requester_rank >= access_rank("Owner")
}

allow if {
	input.operation.nsid == "town.muni.arbiter.setSpaceMemberAccess"
	requester_rank >= access_rank("AddMembers")
	access_rank(input.operation.params.access.level) <= requester_rank
	not target_exists_in_raw
}

allow if {
	input.operation.nsid == "town.muni.arbiter.setSpaceMemberAccess"
	requester_rank >= access_rank("AddMembers")
	access_rank(input.operation.params.access.level) <= requester_rank
	target_exists_in_raw
	requester_rank >= access_rank("RemoveMembers")
	raw_target_rank <= requester_rank
}

allow if {
	input.operation.nsid == "town.muni.arbiter.removeSpaceMember"
	requester_rank >= access_rank("RemoveMembers")
	target_exists_in_raw
	raw_target_rank <= requester_rank
}

allow if {
	input.operation.nsid == "town.muni.arbiter.deleteArbiter"
	requester_rank >= access_rank("Owner")
	count(space_members("$admin")) == 1
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
    if (!this.arbiters.get(remoteDid)?.online) {
      return null;
    }
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
      online: true,
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
      arbiters.push({ did, version: arb.version, online: arb.online, config: { ...arb.config }, policy: arb.policy, spaces });
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
        online: a.online ?? true,
        spaces,
      });
    }
  }

  /** Validate a Rego policy string, returning null on success or an error message. */
  isArbiterOffline(did: Did): boolean {
    const arbiter = this.arbiters.get(did);
    return arbiter ? !arbiter.online : true;
  }

  toggleArbiterOffline(did: Did): void {
    const arbiter = this.arbiters.get(did);
    if (arbiter) {
      arbiter.online = !arbiter.online;
    }
  }

  validatePolicy(policy: string): string | null {
    try {
      validate_policy(policy);
      return null;
    } catch (e) {
      return String(e);
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
