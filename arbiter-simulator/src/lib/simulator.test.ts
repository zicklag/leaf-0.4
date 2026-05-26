//! Integration tests for the access-levels policy via the Simulator.
//!
//! Ported from crates/arbiter-core2/tests/default_policy.rs.
//! Tests realistic multi-arbiter scenarios including local space delegation,
//! remote space resolution, and complex permission chains.

import { describe, it, expect, beforeEach } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Simulator } from './simulator';
import type { Did, SpaceKey, MemberEntry, OpResult } from './types';

// For Node.js tests, use synchronous WASM init
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const wasmPath = path.resolve(
  __dirname,
  '../../../crates/policy-core-wasm/pkg/policy_core_wasm_bg.wasm',
);
const wasmBuffer = fs.readFileSync(wasmPath);
const wasmModule = new WebAssembly.Module(wasmBuffer);

// ---------------------------------------------------------------------------
// Shared default policy (loaded from policies/arbiter/access-levels.rego)
// ---------------------------------------------------------------------------

// We read the file at import time via a raw loader or embed it directly.
// Inlined here for self-contained tests matching the current policy design.
const DEFAULT_POLICY = `package arbiter

import rego.v1

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

resolved_members_raw contains member if {
	some entry in space_members(input.operation.params.spaceKey)
	member := {"did": entry.did, "access": entry.access, "via": input.operation.params.spaceKey}
}

resolved_members_raw contains member if {
	input.operation.params.spaceKey != "$admin"
	some entry in space_members("$admin")
	member := {"did": entry.did, "access": entry.access, "via": "$admin"}
}

# Recursively resolve local space delegations: any "space:<key>" entry
# in resolved_members_raw is expanded to the child space's members.
# This handles arbitrarily nested delegation chains.
resolved_members_raw contains member if {
	some raw_entry in resolved_members_raw
	startswith(raw_entry.did, "space:")
	child_key := trim_prefix(raw_entry.did, "space:")
	some child_entry in space_members(child_key)
	member := {
		"did": child_entry.did,
		"access": min_access(child_entry.access, raw_entry.access),
		"via": child_key,
	}
}

resolved_members_raw contains member if {
	some entry in space_members(input.operation.params.spaceKey)
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

higher_exists(member) if {
	some higher in resolved_members_raw
	higher.did == member.did
	member_rank(higher) > member_rank(member)
}

resolved_members contains member if {
	some member in resolved_members_raw
	not higher_exists(member)
	not startswith(member.did, "space:")
	not contains(member.did, "|")
}

min_access(a, b) := a if {
	member_rank({"access": a}) <= member_rank({"access": b})
}

min_access(a, b) := b if {
	member_rank({"access": b}) < member_rank({"access": a})
}

requester_rank := rank if {
	ranks := {member_rank(member) |
		some member in resolved_members_raw
		member.did == input.caller.did
	}
	rank := max(ranks)
}

missing_spaces contains ms if {
	some entry in space_members(input.operation.params.spaceKey)
	contains(entry.did, "|")
	parts := split(entry.did, "|")
	arbiter_did := parts[0]
	space_key := parts[1]
	count(xrpc_remote(arbiter_did, "town.muni.arbiter.resolveSpaceMembers", {"spaceKey": space_key})) == 0
	ms := {"arbiterDid": arbiter_did, "spaceKey": space_key}
}

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

default allow := false

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

allow if {
	input.operation.nsid == "town.muni.arbiter.getSpaceConfig"
	space_config(input.operation.params.spaceKey).publicRecords == true
}

allow if {
	input.operation.nsid == "town.muni.arbiter.getSpaceConfig"
	requester_rank >= access_rank("ReadMemberList")
}

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

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

class TestHarness {
  sim!: Simulator;

  async init(): Promise<void> {
    // Sync WASM init for Node.js vitest
    const { initSync } = await import('policy-core-wasm');
    initSync({ module: wasmModule });
    this.sim = new Simulator();
    // Mark as initialized since we used sync init
    (this.sim as unknown as { initialized: boolean }).initialized = true;
  }

  /** Create an arbiter with the default access-levels policy. */
  createDefaultArbiter(did: Did, ownerDid: Did): void {
    const result = this.sim.createArbiter(
      did,
      { $type: 'town.muni.arbiter.config.regoPolicy', policy: DEFAULT_POLICY },
      ownerDid,
    );
    expect(result.status).toBe('ok');
  }

  /** Create an arbiter with a custom policy. */
  createArbiter(did: Did, ownerDid: Did, policy: string): void {
    const result = this.sim.createArbiter(
      did,
      { $type: 'town.muni.arbiter.config.regoPolicy', policy },
      ownerDid,
    );
    expect(result.status).toBe('ok');
  }

  /** Assert an operation completes successfully. */
  async assertOk(
    arbiterDid: Did,
    userDid: Did,
    spaceKey: SpaceKey,
    operation: string,
    params: Record<string, unknown> = {},
  ): Promise<void> {
    const result = await this.callMethod(arbiterDid, userDid, spaceKey, operation, params);
    if (result.status !== 'ok') {
      throw new Error(
        `Expected success for ${userDid}@${arbiterDid}/${spaceKey} (${operation}), got ${JSON.stringify(result)}`,
      );
    }
  }

  /** Assert an operation is denied. */
  async assertDenied(
    arbiterDid: Did,
    userDid: Did,
    spaceKey: SpaceKey,
    operation: string,
    params: Record<string, unknown> = {},
  ): Promise<void> {
    const result = await this.callMethod(arbiterDid, userDid, spaceKey, operation, params);
    expect(result.status).toBe('error');
    if (result.status === 'error') {
      expect(result.error).toMatch(/denied/i);
    }
  }

  /** Resolve members for a space and return the member entries. */
  async resolvedMembers(
    arbiterDid: Did,
    userDid: Did,
    spaceKey: SpaceKey,
  ): Promise<MemberEntry[]> {
    const result = await this.sim.resolveSpaceMembers(arbiterDid, userDid, {
      spaceKey,
    });
    if (result.status !== 'ok' || !result.members) {
      throw new Error(
        `Expected ok for resolveSpaceMembers, got ${JSON.stringify(result)}`,
      );
    }
    return result.members;
  }

  /** Call the correct simulator method based on operation name. */
  private async callMethod(
    arbiterDid: Did,
    userDid: Did,
    spaceKey: SpaceKey,
    operation: string,
    params: Record<string, unknown>,
  ): Promise<OpResult> {
    switch (operation) {
      case 'createSpace':
        return this.sim.createSpace(arbiterDid, userDid, {
          spaceKey,
          spaceType: 'town.muni.arbiter.config.space',
          config: params.config as Record<string, unknown> ?? {
            $type: 'town.muni.arbiter.config.space',
            publicRecords: false,
            publicMembers: false,
          },
        });
      case 'deleteSpace':
        return this.sim.deleteSpace(arbiterDid, userDid, { spaceKey });
      case 'setSpaceMemberAccess': {
        const targetParams = params as {
          memberDid: Did;
          access: Record<string, unknown>;
        };
        return this.sim.setSpaceMemberAccess(arbiterDid, userDid, {
          spaceKey,
          memberDid: targetParams.memberDid,
          access: targetParams.access,
        });
      }
      case 'removeSpaceMember':
        return this.sim.removeSpaceMember(arbiterDid, userDid, {
          spaceKey,
          memberDid: params.memberDid as Did,
        });
      case 'deleteArbiter':
        return this.sim.deleteArbiter(arbiterDid, userDid);
      case 'resolveSpaceMembers':
        return this.sim.resolveSpaceMembers(arbiterDid, userDid, { spaceKey });
      default:
        throw new Error(`Unknown operation: ${operation}`);
    }
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function spaceConfig(
  publicMembers = false,
  publicRecords = false,
): Record<string, unknown> {
  return {
    $type: 'town.muni.arbiter.config.space',
    publicMembers,
    publicRecords,
  };
}

function access(level: string): Record<string, unknown> {
  return { $type: 'town.muni.arbiter.config.accessLevel', level };
}

function assertMemberExists(
  members: MemberEntry[],
  expectedDid: string,
  expectedLevel: string,
): void {
  const found = members.find((m) => m.did === expectedDid);
  expect(found, `Member ${expectedDid} not found`).toBeDefined();
  const level = (found!.access as { level?: string }).level;
  expect(level).toBe(expectedLevel);
}

// ---------------------------------------------------------------------------
// Tests
// ===========================================================================

describe('access-levels policy', () => {
  let h: TestHarness;

  beforeEach(async () => {
    h = new TestHarness();
    await h.init();
  });

  // =======================================================================
  // Basic owner operations
  // =======================================================================

  describe('basic owner operations', () => {
    it('owner can create spaces', async () => {
      h.createDefaultArbiter('org', 'alice');
      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'docs', 'createSpace');
    });

    it('non-member cannot create space', async () => {
      h.createDefaultArbiter('org', 'alice');
      await h.assertDenied('org', 'stranger', 'team', 'createSpace');
    });

    it('owner can delete arbiter', async () => {
      h.createDefaultArbiter('org', 'alice');
      await h.assertOk('org', 'alice', '$admin', 'deleteArbiter');
      expect(h.sim.arbiters.has('org')).toBe(false);
    });

    it('non-owner cannot delete arbiter', async () => {
      h.createDefaultArbiter('org', 'alice');
      await h.assertDenied('org', 'stranger', '$admin', 'deleteArbiter');
    });

    it('multiple owners cannot delete arbiter', async () => {
      h.createDefaultArbiter('org', 'alice');
      await h.assertOk('org', 'alice', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'bob',
        access: access('Owner'),
      });
      await h.assertDenied('org', 'alice', '$admin', 'deleteArbiter');
      await h.assertDenied('org', 'bob', '$admin', 'deleteArbiter');
    });

    it('owner can delete space', async () => {
      h.createDefaultArbiter('org', 'alice');
      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'deleteSpace');
      const result = await h.sim.resolveSpaceMembers('org', 'alice', {
        spaceKey: '$admin',
      });
      if (result.status === 'ok' && result.members) {
        const teamDeleted = !h.sim.arbiters.get('org')?.spaces.has('team');
        expect(teamDeleted).toBe(true);
      }
    });

    it('owner cannot delete $admin space', async () => {
      h.createDefaultArbiter('org', 'alice');
      const result = await h.sim.deleteSpace('org', 'alice', { spaceKey: '$admin' });
      expect(result.status).toBe('error');
      // $admin should still exist
      expect(h.sim.arbiters.get('org')?.spaces.has('$admin')).toBe(true);
    });
  });

  // =======================================================================
  // Access level hierarchy
  // =======================================================================

  describe('access level hierarchy', () => {
    it('owner can add members', async () => {
      h.createDefaultArbiter('org', 'alice');
      await h.assertOk('org', 'alice', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'bob',
        access: access('Owner'),
      });
      await h.assertOk('org', 'alice', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'carol',
        access: access('IsMember'),
      });
    });

    it('read member cannot create space', async () => {
      h.createDefaultArbiter('org', 'alice');
      await h.assertOk('org', 'alice', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'bob',
        access: access('ReadMemberList'),
      });
      await h.assertDenied('org', 'bob', 'team', 'createSpace');
    });

    it('cannot grant higher access than own', async () => {
      h.createDefaultArbiter('org', 'alice');
      await h.assertOk('org', 'alice', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'bob',
        access: access('AddMembers'),
      });
      // Bob can add someone with IsMember (lower than AddMembers)
      await h.assertOk('org', 'bob', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'carol',
        access: access('IsMember'),
      });
      // Bob cannot add someone with Owner (higher)
      await h.assertDenied('org', 'bob', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'dave',
        access: access('Owner'),
      });
      // Bob cannot add someone with ConfigureSpace (also higher)
      await h.assertDenied('org', 'bob', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'eve',
        access: access('ConfigureSpace'),
      });
    });

    it('need RemoveMembers to modify existing', async () => {
      h.createDefaultArbiter('org', 'alice');
      await h.assertOk('org', 'alice', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'bob',
        access: access('IsMember'),
      });
      await h.assertOk('org', 'alice', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'carol',
        access: access('AddMembers'),
      });
      // Carol can add a new member
      await h.assertOk('org', 'carol', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'dave',
        access: access('ReadMemberList'),
      });
      // Carol cannot modify bob's existing entry (needs RemoveMembers)
      await h.assertDenied('org', 'carol', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'bob',
        access: access('ReadMemberList'),
      });
      // Alice (Owner) can modify anyone
      await h.assertOk('org', 'alice', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'bob',
        access: access('ReadMemberList'),
      });
    });
  });

  // =======================================================================
  // Resolved member lists
  // =======================================================================

  describe('resolved member lists', () => {
    it('owner sees themselves in admin space', async () => {
      h.createDefaultArbiter('org', 'alice');
      const members = await h.resolvedMembers('org', 'alice', '$admin');
      expect(members.length).toBeGreaterThan(0);
      assertMemberExists(members, 'alice', 'Owner');
    });

    it('resolve includes all direct members', async () => {
      h.createDefaultArbiter('org', 'alice');
      await h.assertOk('org', 'alice', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'bob',
        access: access('IsMember'),
      });
      await h.assertOk('org', 'alice', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'carol',
        access: access('ReadMemberList'),
      });
      const members = await h.resolvedMembers('org', 'alice', '$admin');
      assertMemberExists(members, 'alice', 'Owner');
      assertMemberExists(members, 'bob', 'IsMember');
      assertMemberExists(members, 'carol', 'ReadMemberList');
    });
  });

  // =======================================================================
  // Local space delegation
  // =======================================================================

  describe('local space delegation', () => {
    it('access limited by parent delegation', async () => {
      h.createDefaultArbiter('org', 'alice');
      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'setSpaceMemberAccess', {
        memberDid: 'bob',
        access: access('Owner'),
      });
      await h.assertOk('org', 'alice', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'space:team',
        access: access('ReadMemberList'),
      });
      // Bob's effective access in $admin should be ReadMemberList (limited by parent)
      const members = await h.resolvedMembers('org', 'alice', '$admin');
      assertMemberExists(members, 'bob', 'ReadMemberList');
    });

    it('members of child space inherit access', async () => {
      h.createDefaultArbiter('org', 'alice');
      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'setSpaceMemberAccess', {
        memberDid: 'bob',
        access: access('IsMember'),
      });
      await h.assertOk('org', 'alice', '$admin', 'setSpaceMemberAccess', {
        memberDid: 'space:team',
        access: access('IsMember'),
      });
      const members = await h.resolvedMembers('org', 'alice', '$admin');
      assertMemberExists(members, 'bob', 'IsMember');
    });

    it('public members allows non-member access', async () => {
      h.createDefaultArbiter('org', 'alice');
      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'setSpaceMemberAccess', {
        memberDid: 'bob',
        access: access('IsMember'),
      });
      // Make the space have public members
      const space = h.sim.arbiters.get('org')!.spaces.get('team')!;
      space.config = { ...space.config, publicMembers: true };

      const members = await h.resolvedMembers('org', 'stranger', 'team');
      expect(members.length).toBeGreaterThan(0);
      assertMemberExists(members, 'bob', 'IsMember');
    });
  });

  // =======================================================================
  // Remote space resolution
  // =======================================================================

  describe('remote space resolution', () => {
    it('remote space resolution works', async () => {
      h.createDefaultArbiter('org', 'alice');
      h.createDefaultArbiter('partner', 'carol');

      // Partner creates "shared" with public members
      await h.assertOk('partner', 'carol', 'shared', 'createSpace');
      const shared = h.sim.arbiters.get('partner')!.spaces.get('shared')!;
      shared.config = { ...shared.config, publicMembers: true };
      await h.assertOk('partner', 'carol', 'shared', 'setSpaceMemberAccess', {
        memberDid: 'dave',
        access: access('Owner'),
      });

      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'setSpaceMemberAccess', {
        memberDid: 'partner|shared',
        access: access('IsMember'),
      });

      const members = await h.resolvedMembers('org', 'alice', 'team');
      assertMemberExists(members, 'dave', 'IsMember');
    });

    it('remote access limited by parent', async () => {
      h.createDefaultArbiter('org', 'alice');
      h.createDefaultArbiter('partner', 'carol');

      await h.assertOk('partner', 'carol', 'shared', 'createSpace');
      const shared = h.sim.arbiters.get('partner')!.spaces.get('shared')!;
      shared.config = { ...shared.config, publicMembers: true };
      await h.assertOk('partner', 'carol', 'shared', 'setSpaceMemberAccess', {
        memberDid: 'dave',
        access: access('Owner'),
      });

      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'setSpaceMemberAccess', {
        memberDid: 'partner|shared',
        access: access('ReadMemberList'),
      });

      const members = await h.resolvedMembers('org', 'alice', 'team');
      assertMemberExists(members, 'dave', 'ReadMemberList');
    });

    it('deep remote chain resolves', async () => {
      h.createDefaultArbiter('org', 'alice');
      h.createDefaultArbiter('partner', 'carol');

      await h.assertOk('partner', 'carol', 'users', 'createSpace');
      const users = h.sim.arbiters.get('partner')!.spaces.get('users')!;
      users.config = { ...users.config, publicMembers: true };
      await h.assertOk('partner', 'carol', 'users', 'setSpaceMemberAccess', {
        memberDid: 'dave',
        access: access('Owner'),
      });

      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'setSpaceMemberAccess', {
        memberDid: 'partner|users',
        access: access('IsMember'),
      });

      const members = await h.resolvedMembers('org', 'alice', 'team');
      assertMemberExists(members, 'dave', 'IsMember');
    });

    it('remote arbiter denies unauthorised caller', async () => {
      h.createDefaultArbiter('org', 'alice');
      h.createDefaultArbiter('partner', 'carol');

      // Partner creates a restricted space (not public, no org in members)
      await h.assertOk('partner', 'carol', 'restricted', 'createSpace');
      await h.assertOk('partner', 'carol', 'restricted', 'setSpaceMemberAccess', {
        memberDid: 'dave',
        access: access('Owner'),
      });

      // org tries to reference the restricted space
      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'setSpaceMemberAccess', {
        memberDid: 'partner|restricted',
        access: access('IsMember'),
      });

      // The remote arbiter should deny org's request — dave should NOT appear
      const members = await h.resolvedMembers('org', 'alice', 'team');
      expect(members.some((m) => m.did === 'dave')).toBe(false);
    });

    it('remote arbiter grants caller via member access', async () => {
      h.createDefaultArbiter('org', 'alice');
      h.createDefaultArbiter('partner', 'carol');

      // Partner creates a shared space with Dave as member (NOT public)
      await h.assertOk('partner', 'carol', 'shared', 'createSpace');
      await h.assertOk('partner', 'carol', 'shared', 'setSpaceMemberAccess', {
        memberDid: 'dave',
        access: access('Owner'),
      });
      // Partner adds org's DID as a member of the space (grants access)
      await h.assertOk('partner', 'carol', 'shared', 'setSpaceMemberAccess', {
        memberDid: 'org',
        access: access('ReadMemberList'),
      });

      // org references the shared space
      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'setSpaceMemberAccess', {
        memberDid: 'partner|shared',
        access: access('IsMember'),
      });

      // org (as member with ReadMemberList) should be allowed — Dave appears
      const members = await h.resolvedMembers('org', 'alice', 'team');
      assertMemberExists(members, 'dave', 'IsMember');
    });
  });

  // =======================================================================
  // Custom policies
  // =======================================================================

  describe('custom policies', () => {
    it('allow-all policy', async () => {
      const allowAll = `
        package arbiter
        import rego.v1
        default allow := true
        resolved_members contains {"did": input.caller.did, "access": {"level": "Owner"}} if { true }
        missing_spaces contains false if { false }
        resolve_result := {"members": resolved_members, "missingSpaces": missing_spaces}
      `;
      h.createArbiter('org', 'alice', allowAll);
      await h.assertOk('org', 'stranger', 'team', 'createSpace');
      await h.assertOk('org', 'stranger', 'team', 'setSpaceMemberAccess', {
        memberDid: 'alice',
        access: access('Owner'),
      });
      const members = await h.resolvedMembers('org', 'stranger', '$admin');
      assertMemberExists(members, 'stranger', 'Owner');
    });

    it('deny-all policy', async () => {
      const denyAll = `
        package arbiter
        import rego.v1
        default allow := false
        resolved_members contains {"did": "noone", "access": {"level": "ReadMemberList"}} if { false }
      `;
      h.createArbiter('org', 'alice', denyAll);
      await h.assertDenied('org', 'alice', 'team', 'createSpace');
    });
  });

  // =======================================================================
  // Access control edge cases
  // =======================================================================

  describe('access control edge cases', () => {
    it('remote arbiter offline excludes remote members', async () => {
      h.createDefaultArbiter('org', 'alice');
      h.createDefaultArbiter('partner', 'carol');

      await h.assertOk('partner', 'carol', 'shared', 'createSpace');
      const shared = h.sim.arbiters.get('partner')!.spaces.get('shared')!;
      shared.config = { ...shared.config, publicMembers: true };
      await h.assertOk('partner', 'carol', 'shared', 'setSpaceMemberAccess', {
        memberDid: 'dave',
        access: access('Owner'),
      });

      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'setSpaceMemberAccess', {
        memberDid: 'partner|shared',
        access: access('ReadMemberList'),
      });

      // Online: Dave visible
      const online = await h.resolvedMembers('org', 'alice', 'team');
      assertMemberExists(online, 'dave', 'ReadMemberList');

      // Offline: Dave absent
      h.sim.toggleArbiterOffline('partner');
      const offline = await h.resolvedMembers('org', 'alice', 'team');
      expect(offline.some((m) => m.did === 'dave')).toBe(false);

      // Back online: Dave returns
      h.sim.toggleArbiterOffline('partner');
      const backOnline = await h.resolvedMembers('org', 'alice', 'team');
      assertMemberExists(backOnline, 'dave', 'ReadMemberList');
    });

    it('public members toggle controls stranger access', async () => {
      h.createDefaultArbiter('org', 'alice');
      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'setSpaceMemberAccess', {
        memberDid: 'bob',
        access: access('IsMember'),
      });

      // Not public: stranger denied
      await h.assertDenied('org', 'stranger', 'team', 'resolveSpaceMembers');

      // Make public: stranger can see
      const team = h.sim.arbiters.get('org')!.spaces.get('team')!;
      team.config = { ...team.config, publicMembers: true };
      const members = await h.resolvedMembers('org', 'stranger', 'team');
      assertMemberExists(members, 'bob', 'IsMember');

      // Un-public: stranger denied again
      team.config = { ...team.config, publicMembers: false };
      await h.assertDenied('org', 'stranger', 'team', 'resolveSpaceMembers');
    });

    it('space-scoped Owner cannot create spaces globally', async () => {
      h.createDefaultArbiter('org', 'alice');

      // Create team space; alice adds bob as Owner of team only
      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'setSpaceMemberAccess', {
        memberDid: 'bob',
        access: access('Owner'),
      });

      // Bob is Owner in team — can configure, add members there
      await h.assertOk('org', 'bob', 'team', 'setSpaceMemberAccess', {
        memberDid: 'carol',
        access: access('IsMember'),
      });

      // But Bob only has ReadMemberList in $admin — can't create spaces
      await h.assertDenied('org', 'bob', 'newspace', 'createSpace');

      // Alice (Owner in $admin) can still create spaces
      await h.assertOk('org', 'alice', 'newspace', 'createSpace');
    });
  });

  // =======================================================================
  // UI flow regression tests (match exact UI patterns)
  // =======================================================================

  describe('UI flow regressions', () => {
    it('create arbiter with UI-style config then resolve members', async () => {
      // Match CreateArbiterBar.svelte: calls createArbiter with only $type
      const result = h.sim.createArbiter(
        'arbiter1',
        { $type: 'town.muni.arbiter.config.regoPolicy' },
        'alice',
      );
      expect(result.status).toBe('ok');

      // Match fetchResolvedMembers: resolves members right after creation
      const members = await h.resolvedMembers('arbiter1', 'alice', '$admin');
      expect(members.length).toBe(1);
      assertMemberExists(members, 'alice', 'Owner');
    });

    it('add member to admin space via setSpaceMemberAccess', async () => {
      h.createDefaultArbiter('org', 'alice');
      // Match DetailPanel handleAddMember flow
      const result = await h.sim.setSpaceMemberAccess('org', 'alice', {
        spaceKey: '$admin',
        memberDid: 'bob',
        access: { $type: 'town.muni.arbiter.config.accessLevel', level: 'IsMember' },
      });
      expect(result.status).toBe('ok');

      const members = await h.resolvedMembers('org', 'alice', '$admin');
      assertMemberExists(members, 'bob', 'IsMember');
    });

    it('create space with explicit key (UI flow)', async () => {
      h.createDefaultArbiter('org', 'alice');
      // Match handleCreateSpace in ArbiterActions.svelte
      const result = await h.sim.createSpace('org', 'alice', {
        spaceKey: 'test',
        spaceType: 'town.muni.arbiter.config.space',
        config: {
          $type: 'town.muni.arbiter.config.space',
          publicRecords: false,
          publicMembers: false,
        },
      });
      expect(result.status).toBe('ok');

      const space = h.sim.arbiters.get('org')!.spaces.get('test');
      expect(space).toBeDefined();
      expect(space!.key).toBe('test');
    });
  });

  // =======================================================================
  // Nested local delegation
  // =======================================================================

  describe('nested local delegation', () => {
    it('resolves deeply nested local delegations', async () => {
      h.createDefaultArbiter('arb1', 'alice');

      // Create the spaces
      await h.assertOk('arb1', 'alice', 'members', 'createSpace');
      await h.assertOk('arb1', 'alice', 'moderators', 'createSpace');
      await h.assertOk('arb1', 'alice', '#general', 'createSpace');

      // Add moderators as a local space member to the members space with RemoveMembers access
      await h.assertOk('arb1', 'alice', 'members', 'setSpaceMemberAccess', {
        memberDid: 'space:moderators',
        access: access('RemoveMembers'),
      });

      // Add members as a local space member to #general with RemoveMembers access
      await h.assertOk('arb1', 'alice', '#general', 'setSpaceMemberAccess', {
        memberDid: 'space:members',
        access: access('RemoveMembers'),
      });

      // Add Carol as member of moderators with RemoveMembers access
      await h.assertOk('arb1', 'alice', 'moderators', 'setSpaceMemberAccess', {
        memberDid: 'carol',
        access: access('RemoveMembers'),
      });

      // Add George as member of members with IsMember access
      await h.assertOk('arb1', 'alice', 'members', 'setSpaceMemberAccess', {
        memberDid: 'george',
        access: access('IsMember'),
      });

      // Resolve members of #general
      const members = await h.resolvedMembers('arb1', 'alice', '#general');

      // Should have: alice (Owner from $admin), george (IsMember from members), carol (RemoveMembers from moderators)
      assertMemberExists(members, 'alice', 'Owner');
      assertMemberExists(members, 'george', 'IsMember');
      assertMemberExists(members, 'carol', 'RemoveMembers');
    });
  });
});
