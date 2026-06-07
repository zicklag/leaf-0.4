/**
 * Arbiter XRPC helpers for the arbiter-manager UI.
 *
 * Provides a single `arbiter` object with methods to:
 *  - Check whether a DID has an `#arbiter` service endpoint.
 *  - Obtain a service auth token for the arbiter.
 *  - Fetch / save the arbiter config (which contains the Rego policy).
 *  - List / create / update / delete spaces on the arbiter.
 *  - Get / set / remove space members.
 */

import { PUBLIC_ARBITER_URL } from '$env/static/public';
import { xrpc } from '@atproto/lex';
import { XrpcResponseError } from '@atproto/lex';
import type { AtprotoDid } from '@atcute/lexicons/syntax';
import * as town from '$lib/lexicons/town';
import * as com from '$lib/lexicons/com';
import { auth } from '$lib/auth.svelte';
import { didResolver } from '$lib/resolver';

const ARBITER_SERVICE_ID = 'arbiter';

/** Minimal DID document shape we care about */
interface MinimalDidDoc {
  service?: { id?: string; type?: string; serviceEndpoint?: string }[];
}

export const arbiter = {
  /**
   * Check whether a DID document advertises an `#arbiter` service endpoint.
   */
  async hasService(did: string): Promise<boolean> {
    try {
      const doc = (await didResolver.resolve(did as AtprotoDid)) as MinimalDidDoc;
      const services = doc.service ?? [];
      return services.some((s) => {
        const id = typeof s.id === 'string' ? s.id.replace(/^#/, '') : '';
        return id === ARBITER_SERVICE_ID && typeof s.serviceEndpoint === 'string';
      });
    } catch {
      return false;
    }
  },

  /**
   * Obtain a service auth token for the arbiter of the given community DID.
   *
   * The token is a JWT signed by the user's PDS that authorizes the caller
   * to act on behalf of the authenticated account when talking to the arbiter.
   */
  async getServiceAuth(did: string, lxm?: string): Promise<string> {
    if (!auth.client) throw new Error('Not authenticated');

    const params: Record<string, unknown> = {
      aud: did as AtprotoDid,
    };
    if (lxm) params.lxm = lxm;

    const resp = await auth.client.xrpc(com.atproto.server.getServiceAuth, {
      params: params as any,
    });
    return (resp.body as { token: string }).token;
  },

  // ─── Arbiter config (policy) ───────────────────────────────────────

  /**
   * Fetch the arbiter configuration object for a given DID.
   *
   * Returns the full config object from the arbiter (which includes a `policy`
   * field containing the Rego source).
   *
   * Throws on network / permission errors.
   */
  async getConfig(did: string): Promise<Record<string, unknown>> {
    const token = await this.getServiceAuth(
      did,
      'town.muni.arbiter.getArbiterConfig',
    );

    const res = await xrpc(
      PUBLIC_ARBITER_URL,
      town.muni.arbiter.getArbiterConfig,
      {
        params: { arbiterDid: did as AtprotoDid },
        headers: {
          'atproto-proxy': `${did}#arbiter`,
          Authorization: `Bearer ${token}`,
        },
      },
    );
    return res.body.config as Record<string, unknown>;
  },

  /**
   * Save (replace) the arbiter configuration object.
   *
   * The `config` object should include the `policy` field and any other fields
   * that were present in the fetched config.
   */
  async setConfig(
    did: string,
    config: Record<string, unknown>,
  ): Promise<void> {
    const token = await this.getServiceAuth(
      did,
      'town.muni.arbiter.setArbiterConfig',
    );

    await xrpc(
      PUBLIC_ARBITER_URL,
      town.muni.arbiter.setArbiterConfig,
      {
        body: { arbiterDid: did as AtprotoDid, config: config as any },
        headers: {
          'atproto-proxy': `${did}#arbiter`,
          Authorization: `Bearer ${token}`,
        },
      },
    );
  },

  // ─── Space operations ──────────────────────────────────────────────

  /**
   * List all spaces on an arbiter.
   */
  async listSpaces(did: string): Promise<{ spaceKey: string; spaceType: string; config?: Record<string, unknown> }[]> {
    const token = await this.getServiceAuth(did, 'town.muni.arbiter.listSpaces');

    const res = await xrpc(
      PUBLIC_ARBITER_URL,
      town.muni.arbiter.listSpaces,
      {
        params: { arbiterDid: did as AtprotoDid },
        headers: {
          'atproto-proxy': `${did}#arbiter`,
          Authorization: `Bearer ${token}`,
        },
      },
    );
    return (res.body as any).spaces ?? [];
  },

  /**
   * Create a new space on an arbiter.
   */
  async createSpace(
    did: string,
    spaceKey: string,
    spaceType: string,
    config?: Record<string, unknown>,
  ): Promise<void> {
    const token = await this.getServiceAuth(did, 'town.muni.arbiter.createSpace');

    await xrpc(
      PUBLIC_ARBITER_URL,
      town.muni.arbiter.createSpace,
      {
        body: {
          arbiterDid: did as AtprotoDid,
          spaceKey,
          spaceType: spaceType as any,
          config: (config ?? {}) as any,
        },
        headers: {
          'atproto-proxy': `${did}#arbiter`,
          Authorization: `Bearer ${token}`,
        },
      },
    );
  },

  /**
   * Fetch a space's configuration.
   */
  async getSpaceConfig(did: string, spaceKey: string): Promise<Record<string, unknown>> {
    const token = await this.getServiceAuth(did, 'town.muni.arbiter.getSpaceConfig');

    const res = await xrpc(
      PUBLIC_ARBITER_URL,
      town.muni.arbiter.getSpaceConfig,
      {
        params: { arbiterDid: did as AtprotoDid, spaceKey },
        headers: {
          'atproto-proxy': `${did}#arbiter`,
          Authorization: `Bearer ${token}`,
        },
      },
    );
    return (res.body as any).config ?? {};
  },

  /**
   * Update a space's configuration.
   */
  async setSpaceConfig(
    did: string,
    spaceKey: string,
    spaceType: string,
    config: Record<string, unknown>,
  ): Promise<void> {
    const token = await this.getServiceAuth(did, 'town.muni.arbiter.setSpaceConfig');

    await xrpc(
      PUBLIC_ARBITER_URL,
      town.muni.arbiter.setSpaceConfig,
      {
        body: {
          arbiterDid: did as AtprotoDid,
          spaceKey,
          spaceType: spaceType as any,
          config: config as any,
        },
        headers: {
          'atproto-proxy': `${did}#arbiter`,
          Authorization: `Bearer ${token}`,
        },
      },
    );
  },

  /**
   * Delete a space from an arbiter.
   */
  async deleteSpace(did: string, spaceKey: string): Promise<void> {
    const token = await this.getServiceAuth(did, 'town.muni.arbiter.deleteSpace');

    await xrpc(
      PUBLIC_ARBITER_URL,
      town.muni.arbiter.deleteSpace,
      {
        body: { arbiterDid: did as AtprotoDid, spaceKey },
        headers: {
          'atproto-proxy': `${did}#arbiter`,
          Authorization: `Bearer ${token}`,
        },
      },
    );
  },

  // ─── Member operations ─────────────────────────────────────────────

  /**
   * Get the direct (non-resolved) members of a space.
   */
  async getSpaceMembers(
    did: string,
    spaceKey: string,
  ): Promise<{ member: Record<string, unknown>; access: Record<string, unknown> }[]> {
    const token = await this.getServiceAuth(did, 'town.muni.arbiter.getSpaceMembers');

    const res = await xrpc(
      PUBLIC_ARBITER_URL,
      town.muni.arbiter.getSpaceMembers,
      {
        params: { arbiterDid: did as AtprotoDid, spaceKey },
        headers: {
          'atproto-proxy': `${did}#arbiter`,
          Authorization: `Bearer ${token}`,
        },
      },
    );
    return (res.body as any).members ?? [];
  },

  /**
   * Get the flattened resolved members of a space (DIDs only, with access).
   */
  async resolveSpaceMembers(
    did: string,
    spaceKey: string,
  ): Promise<{ did: string; access: Record<string, unknown> }[]> {
    const token = await this.getServiceAuth(did, 'town.muni.arbiter.resolveSpaceMembers');

    const res = await xrpc(
      PUBLIC_ARBITER_URL,
      town.muni.arbiter.resolveSpaceMembers,
      {
        params: { arbiterDid: did as AtprotoDid, spaceKey },
        headers: {
          'atproto-proxy': `${did}#arbiter`,
          Authorization: `Bearer ${token}`,
        },
      },
    );
    return (res.body as any).members ?? [];
  },

  /**
   * Set (add or update) a member's access in a space.
   */
  async setSpaceMemberAccess(
    did: string,
    spaceKey: string,
    member: Record<string, unknown>,
    access: Record<string, unknown>,
  ): Promise<void> {
    const token = await this.getServiceAuth(did, 'town.muni.arbiter.setSpaceMemberAccess');

    await xrpc(
      PUBLIC_ARBITER_URL,
      town.muni.arbiter.setSpaceMemberAccess,
      {
        body: {
          arbiterDid: did as AtprotoDid,
          spaceKey,
          member: member as any,
          access: access as any,
        },
        headers: {
          'atproto-proxy': `${did}#arbiter`,
          Authorization: `Bearer ${token}`,
        },
      },
    );
  },

  /**
   * Remove a member from a space.
   */
  async removeSpaceMember(
    did: string,
    spaceKey: string,
    member: Record<string, unknown>,
  ): Promise<void> {
    const token = await this.getServiceAuth(did, 'town.muni.arbiter.removeSpaceMember');

    await xrpc(
      PUBLIC_ARBITER_URL,
      town.muni.arbiter.removeSpaceMember,
      {
        body: {
          arbiterDid: did as AtprotoDid,
          spaceKey,
          member: member as any,
          access: {} as any,
        },
        headers: {
          'atproto-proxy': `${did}#arbiter`,
          Authorization: `Bearer ${token}`,
        },
      },
    );
  },

  /**
   * Extract a user-friendly message from an XRPC error.
   */
  formatError(err: unknown): string {
    if (err instanceof XrpcResponseError) {
      const code = err.error;
      const msg = err.message;
      return `Request failed (${err.status}): ${msg || code || 'unknown error'}`;
    }
    if (err instanceof Error) return err.message;
    return String(err);
  },
};