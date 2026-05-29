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
  '../../../crates/arbiter-core-wasm/pkg/arbiter_core_wasm_bg.wasm',
);
const wasmBuffer = fs.readFileSync(wasmPath);
const wasmModule = new WebAssembly.Module(wasmBuffer);

// ---------------------------------------------------------------------------
// Shared default policy (loaded from policies/arbiter/access-levels.rego)
// ---------------------------------------------------------------------------

// We read the file at import time via a raw loader or embed it directly.
// Inlined here for self-contained tests matching the current policy design.
import defaultPolicy from '../../../policies/arbiter/access-levels.rego?raw';

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

class TestHarness {
  sim!: Simulator;

  async init(): Promise<void> {
    // Sync WASM init for Node.js vitest
    const { initSync } = await import('arbiter-core-wasm');
    initSync({ module: wasmModule });
    this.sim = new Simulator();
    // Mark as initialized since we used sync init
    (this.sim as unknown as { initialized: boolean }).initialized = true;
  }

  /** Create an arbiter with the default access-levels policy. */
  createDefaultArbiter(did: Did, ownerDid: Did): void {
    const result = this.sim.createArbiter(
      did,
      { $type: 'town.muni.arbiter.config.regoPolicy', policy: defaultPolicy },
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
    const spaceType = spaceKey === '$admin'
      ? 'town.muni.arbiter.config.adminSpace'
      : 'town.muni.arbiter.config.space';
    const result = await this.sim.resolveSpaceMembers(arbiterDid, userDid, {
      spaceKey, spaceType,
    });
    if (result.status !== 'ok' || !result.members) {
      throw new Error(
        `Expected ok for resolveSpaceMembers, got ${JSON.stringify(result)}`,
      );
    }
    return result.members;
  }

  /** Set a space's config to make members public — uses the owner as caller. */
  async setSpacePublic(arbiterDid: Did, ownerDid: Did, spaceKey: SpaceKey): Promise<void> {
    const spaceType = 'town.muni.arbiter.config.space';
    const config = { $type: 'town.muni.arbiter.config.space', publicMembers: true, publicRecords: false };
    const result = await this.sim.setSpaceConfig(arbiterDid, ownerDid, { spaceKey, spaceType, config });
    if (result.status !== 'ok') {
      throw new Error(`setSpacePublic failed: ${JSON.stringify(result)}`);
    }
  }

  /** Call the correct simulator method based on operation name. */
  private async callMethod(
    arbiterDid: Did,
    userDid: Did,
    spaceKey: SpaceKey,
    operation: string,
    params: Record<string, unknown>,
  ): Promise<OpResult> {
    const autoSpaceType = spaceKey === '$admin'
      ? 'town.muni.arbiter.config.adminSpace'
      : 'town.muni.arbiter.config.space';
    switch (operation) {
      case 'createSpace':
        return this.sim.createSpace(arbiterDid, userDid, {
          spaceKey,
          spaceType: autoSpaceType,
          config: params.config as Record<string, unknown> ?? {
            $type: 'town.muni.arbiter.config.space',
            publicRecords: false,
            publicMembers: false,
          },
        });
      case 'deleteSpace':
        return this.sim.deleteSpace(arbiterDid, userDid, { spaceKey, spaceType: 'town.muni.arbiter.config.space' });
      case 'setSpaceMemberAccess': {
        const targetParams = params as {
          memberDid: Did;
          access: Record<string, unknown>;
        };
        return this.sim.setSpaceMemberAccess(arbiterDid, userDid, {
          spaceKey,
          spaceType: autoSpaceType,
          memberDid: targetParams.memberDid,
          access: targetParams.access,
        });
      }
      case 'removeSpaceMember':
        return this.sim.removeSpaceMember(arbiterDid, userDid, {
          spaceKey,
          spaceType: autoSpaceType,
          memberDid: params.memberDid as Did,
        });
      case 'deleteArbiter':
        return this.sim.deleteArbiter(arbiterDid, userDid);
      case 'resolveSpaceMembers':
        return this.sim.resolveSpaceMembers(arbiterDid, userDid, { spaceKey, spaceType: autoSpaceType });
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
      expect(h.sim.hasArbiter('org')).toBe(false);
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
        spaceType: 'town.muni.arbiter.config.adminSpace',
      });
      if (result.status === 'ok' && result.members) {
        const teamDeleted = h.sim.getSpaceInfo('org', 'team', 'town.muni.arbiter.config.space') === null;
        expect(teamDeleted).toBe(true);
      }
    });

    it('owner cannot delete $admin space', async () => {
      h.createDefaultArbiter('org', 'alice');
      const result = await h.sim.deleteSpace('org', 'alice', { spaceKey: '$admin', spaceType: 'town.muni.arbiter.config.adminSpace' });
      expect(result.status).toBe('error');
      // $admin should still exist
      expect(h.sim.getSpaceInfo('org', '$admin', 'town.muni.arbiter.config.adminSpace')).not.toBeNull();
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
        memberDid: 'space:town.muni.arbiter.config.space/team',
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
        memberDid: 'space:town.muni.arbiter.config.space/team',
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
      await h.setSpacePublic('org', 'alice', 'team');

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
      await h.setSpacePublic('partner', 'carol', 'shared');
      await h.assertOk('partner', 'carol', 'shared', 'setSpaceMemberAccess', {
        memberDid: 'dave',
        access: access('Owner'),
      });

      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'setSpaceMemberAccess', {
        memberDid: 'partner|town.muni.arbiter.config.space|shared',
        access: access('IsMember'),
      });

      const members = await h.resolvedMembers('org', 'alice', 'team');
      assertMemberExists(members, 'dave', 'IsMember');
    });

    it('remote access limited by parent', async () => {
      h.createDefaultArbiter('org', 'alice');
      h.createDefaultArbiter('partner', 'carol');

      await h.assertOk('partner', 'carol', 'shared', 'createSpace');
      await h.setSpacePublic('partner', 'carol', 'shared');
      await h.assertOk('partner', 'carol', 'shared', 'setSpaceMemberAccess', {
        memberDid: 'dave',
        access: access('Owner'),
      });

      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'setSpaceMemberAccess', {
        memberDid: 'partner|town.muni.arbiter.config.space|shared',
        access: access('ReadMemberList'),
      });

      const members = await h.resolvedMembers('org', 'alice', 'team');
      assertMemberExists(members, 'dave', 'ReadMemberList');
    });

    it('deep remote chain resolves', async () => {
      h.createDefaultArbiter('org', 'alice');
      h.createDefaultArbiter('partner', 'carol');

      await h.assertOk('partner', 'carol', 'users', 'createSpace');
      await h.setSpacePublic('partner', 'carol', 'users');
      await h.assertOk('partner', 'carol', 'users', 'setSpaceMemberAccess', {
        memberDid: 'dave',
        access: access('Owner'),
      });

      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'setSpaceMemberAccess', {
        memberDid: 'partner|town.muni.arbiter.config.space|users',
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
        memberDid: 'partner|town.muni.arbiter.config.space|restricted',
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
        memberDid: 'partner|town.muni.arbiter.config.space|shared',
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

        # Delegate every operation to xrpc_local — allows everything.
        response := xrpc_local(input.operation.method, input.operation.nsid, input.operation.params)
      `;
      h.createArbiter('org', 'alice', allowAll);
      await h.assertOk('org', 'stranger', 'team', 'createSpace');
      await h.assertOk('org', 'stranger', 'team', 'setSpaceMemberAccess', {
        memberDid: 'alice',
        access: access('Owner'),
      });
      // allow-all policy just delegates to xrpc_local which returns raw
      // members (without the caller injection logic of the default policy).
      // Verify that the creator appears via the raw $admin space members.
      const members = await h.resolvedMembers('org', 'alice', '$admin');
      expect(members.length).toBeGreaterThanOrEqual(1);
    });

    it('deny-all policy', async () => {
      const denyAll = `
        package arbiter
        import rego.v1

        # Always deny.
        response := {"status": 403, "body": {"error": "ErrPermissionDenied"}}
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
      await h.setSpacePublic('partner', 'carol', 'shared');
      await h.assertOk('partner', 'carol', 'shared', 'setSpaceMemberAccess', {
        memberDid: 'dave',
        access: access('Owner'),
      });

      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'setSpaceMemberAccess', {
        memberDid: 'partner|town.muni.arbiter.config.space|shared',
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
      await h.setSpacePublic('org', 'alice', 'team');
      const members = await h.resolvedMembers('org', 'stranger', 'team');
      assertMemberExists(members, 'bob', 'IsMember');

      // Un-public: stranger denied again
      await h.sim.setSpaceConfig('org', 'alice', { spaceKey: 'team', spaceType: 'town.muni.arbiter.config.space', config: { $type: 'town.muni.arbiter.config.space', publicMembers: false, publicRecords: false } });
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
        spaceKey: '$admin', spaceType: 'town.muni.arbiter.config.adminSpace',
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

      const spaceInfo = h.sim.getSpaceInfo('org', 'test', 'town.muni.arbiter.config.space');
      expect(spaceInfo).not.toBeNull();
      expect(spaceInfo!.key).toBe('test');
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
        memberDid: 'space:town.muni.arbiter.config.space/moderators',
        access: access('RemoveMembers'),
      });

      // Add members as a local space member to #general with RemoveMembers access
      await h.assertOk('arb1', 'alice', '#general', 'setSpaceMemberAccess', {
        memberDid: 'space:town.muni.arbiter.config.space/members',
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

  // =======================================================================
  // Cross-arbiter remote delegation
  // =======================================================================

  describe('cross-arbiter remote delegation', () => {
    it('resolves members across arbiter boundaries with nested delegations', async () => {
      // Set up muni-town with nested delegation
      h.createDefaultArbiter('muni-town', 'alice');
      await h.assertOk('muni-town', 'alice', 'members', 'createSpace');
      await h.assertOk('muni-town', 'alice', 'moderators', 'createSpace');

      // muni-town/members is public so remote arbiters can read its members
      await h.sim.setSpaceConfig('muni-town', 'alice', {
        spaceKey: 'members',
        spaceType: 'town.muni.arbiter.config.space',
        config: { $type: 'town.muni.arbiter.config.space', publicMembers: true, publicRecords: false },
      });

      // muni-town/members delegates to moderators (local) and has george
      await h.assertOk('muni-town', 'alice', 'members', 'setSpaceMemberAccess', {
        memberDid: 'space:town.muni.arbiter.config.space/moderators',
        access: access('RemoveMembers'),
      });
      await h.assertOk('muni-town', 'alice', 'members', 'setSpaceMemberAccess', {
        memberDid: 'george',
        access: access('IsMember'),
      });

      // muni-town/moderators has carol
      await h.assertOk('muni-town', 'alice', 'moderators', 'setSpaceMemberAccess', {
        memberDid: 'carol',
        access: access('RemoveMembers'),
      });

      // Set up spicy-lobster with remote delegation to muni-town/members
      h.createDefaultArbiter('spicy-lobster', 'bob');
      await h.assertOk('spicy-lobster', 'bob', 'members', 'createSpace');
      await h.assertOk('spicy-lobster', 'bob', '#general', 'createSpace');

      // spicy-lobster/members has mary (direct) and muni-town|members (remote)
      await h.assertOk('spicy-lobster', 'bob', 'members', 'setSpaceMemberAccess', {
        memberDid: 'mary',
        access: access('IsMember'),
      });
      await h.assertOk('spicy-lobster', 'bob', 'members', 'setSpaceMemberAccess', {
        memberDid: 'muni-town|town.muni.arbiter.config.space|members',
        access: access('IsMember'),
      });

      // spicy-lobster/#general delegates to members
      await h.assertOk('spicy-lobster', 'bob', '#general', 'setSpaceMemberAccess', {
        memberDid: 'space:town.muni.arbiter.config.space/members',
        access: access('RemoveMembers'),
      });

      // Resolve members of spicy-lobster/#general as bob
      const members = await h.resolvedMembers('spicy-lobster', 'bob', '#general');

      // Should include:
      // - bob (Owner from $admin)
      // - mary (IsMember from members via #general)
      // - alice (IsMember, from muni-town via remote delegation, capped by min_access)
      // - george (IsMember from muni-town/members via remote delegation)
      // - carol (IsMember from muni-town/moderators via remote delegation, capped)
      assertMemberExists(members, 'bob', 'Owner');
      assertMemberExists(members, 'mary', 'IsMember');
      assertMemberExists(members, 'alice', 'IsMember');
      assertMemberExists(members, 'george', 'IsMember');
      assertMemberExists(members, 'carol', 'IsMember');
    });
  });

  // =======================================================================
  // Snapshot roundtrip
  // =======================================================================

  describe('snapshot roundtrip', () => {
    it('can serialise and restore state', async () => {
      h.createDefaultArbiter('org', 'alice');
      await h.assertOk('org', 'alice', 'team', 'createSpace');
      await h.assertOk('org', 'alice', 'team', 'setSpaceMemberAccess', {
        memberDid: 'bob',
        access: access('IsMember'),
      });

      // Snapshot
      const snap = h.sim.snapshot();
      expect(snap.arbiters).toHaveLength(1);
      expect(snap.arbiters[0].spaces).toHaveLength(2); // $admin + team

      // Restore into a fresh simulator
      const sim2 = new Simulator();
      await sim2.init();
      sim2.loadSnapshot(snap);

      // Verify restored state
      const members = await sim2.resolveSpaceMembers('org', 'alice', {
        spaceKey: '$admin',
        spaceType: 'town.muni.arbiter.config.adminSpace',
      });
      expect(members).toHaveProperty('members');
      expect((members as any).members?.length).toBeGreaterThan(0);

      const teamMembers = await sim2.resolveSpaceMembers('org', 'alice', {
        spaceKey: 'team',
        spaceType: 'town.muni.arbiter.config.space',
      });
      expect(teamMembers).toHaveProperty('members');
      assertMemberExists((teamMembers as any).members as MemberEntry[], 'bob', 'IsMember');
    });
  });
});
